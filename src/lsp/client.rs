use crate::{
    buffer::rope::{BufferEdit, EditorBuffer},
    lsp::{
        servers::ServerDefinition,
        sync::incremental_change,
        transport::{notification, read_message, request, response_id, write_message},
        types::{
            CodeActionItem, CompletionItem, Diagnostic, FoldRange, HoverInfo, InlayHintItem,
            Location, Position, Range, SignatureHelpInfo, Symbol,
        },
    },
};
use anyhow::{anyhow, Context, Result};
use lsp_types as lsp;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
};
use tokio::{
    io::{BufReader, BufWriter},
    process::{Child, ChildStdin, Command},
    sync::{mpsc, oneshot, Mutex},
    task::JoinHandle,
};

#[derive(Debug)]
pub struct LspClient {
    root: PathBuf,
    server: String,
    diagnostics: Vec<Diagnostic>,
    state: Option<LspHandle>,
    event_rx: Option<mpsc::UnboundedReceiver<LspEvent>>,
    initialized: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
pub struct LspRequestHandle {
    tx: mpsc::UnboundedSender<OutboundMessage>,
    pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    next_id: Arc<AtomicI64>,
}

type LspHandle = LspRequestHandle;

#[derive(Debug)]
enum OutboundMessage {
    Json(Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LspEvent {
    PublishDiagnostics {
        uri: String,
        diagnostics: Vec<Diagnostic>,
        version: Option<i32>,
    },
    ShowMessage {
        typ: &'static str,
        message: String,
    },
    LogMessage {
        typ: &'static str,
        message: String,
    },
    ServerRequest {
        id: i64,
        method: String,
        params: Value,
    },
    Notification {
        method: String,
        params: Value,
    },
}

pub struct RunningLspClient {
    root: PathBuf,
    server: String,
    child: Child,
    handle: LspHandle,
    event_rx: mpsc::UnboundedReceiver<LspEvent>,
    reader_task: JoinHandle<Result<()>>,
    writer_task: JoinHandle<Result<()>>,
}

impl LspClient {
    pub fn new(root: PathBuf, server: impl Into<String>) -> Self {
        Self {
            root,
            server: server.into(),
            diagnostics: Vec::new(),
            state: None,
            event_rx: None,
            initialized: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn update_diagnostics(&mut self, params: lsp::PublishDiagnosticsParams) {
        self.diagnostics = params.diagnostics.into_iter().map(Into::into).collect();
    }

    pub fn update_diagnostics_from_json(&mut self, params: Value) -> Result<()> {
        self.update_diagnostics(serde_json::from_value(params)?);
        Ok(())
    }

    pub fn try_recv_event(&mut self) -> Option<LspEvent> {
        self.event_rx.as_mut()?.try_recv().ok()
    }

    pub fn request_handle(&self) -> Option<LspRequestHandle> {
        self.state.clone()
    }

    pub fn start(&mut self) -> Result<()> {
        if self.state.is_some() {
            return Ok(());
        }

        tokio::runtime::Handle::try_current()
            .context("starting the LSP client requires a running Tokio runtime")?;

        if !binary_exists(&self.server) {
            return Err(anyhow!(
                "{} not found in PATH; install the server or start jet with --no-lsp",
                self.server
            ));
        }

        let (tx, mut rx) = mpsc::unbounded_channel::<OutboundMessage>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<LspEvent>();
        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_for_reader = Arc::clone(&pending);

        let mut child = Command::new(&self.server)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn LSP server {}", self.server))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("{} stdout unavailable", self.server))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("{} stdin unavailable", self.server))?;

        let reader_task: JoinHandle<Result<()>> = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_message(&mut reader).await? {
                route_inbound_message(message, &pending_for_reader, &event_tx).await?;
            }
            Ok(())
        });

        let writer_task: JoinHandle<Result<()>> = tokio::spawn(async move {
            let mut writer = BufWriter::new(stdin);
            while let Some(message) = rx.recv().await {
                match message {
                    OutboundMessage::Json(value) => write_message(&mut writer, &value).await?,
                }
            }
            Ok(())
        });

        let handle = LspHandle {
            tx,
            pending,
            next_id: Arc::new(AtomicI64::new(1)),
        };
        self.state = Some(handle.clone());
        self.event_rx = Some(event_rx);

        let root = self.root.clone();
        let initialized = Arc::clone(&self.initialized);
        tokio::spawn(async move {
            if initialize_handle(&handle, &root).await.is_ok() {
                initialized.store(true, Ordering::SeqCst);
            }
        });

        tokio::spawn(async move {
            let _ = reader_task.await;
            let _ = writer_task.await;
        });

        Ok(())
    }

    pub async fn start_from_definition(
        root: PathBuf,
        definition: &ServerDefinition,
    ) -> Result<RunningLspClient> {
        RunningLspClient::spawn(root, definition).await
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.state
            .as_ref()
            .ok_or_else(|| anyhow!("LSP client is not started"))?
            .request(method, params)
            .await
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.state
            .as_ref()
            .ok_or_else(|| anyhow!("LSP client is not started"))?
            .notify(method, params)
    }

    pub fn is_started(&self) -> bool {
        self.state.is_some()
    }

    pub fn did_open(
        &self,
        path: &Path,
        language_id: &str,
        text: String,
        version: i32,
    ) -> Result<()> {
        let uri = path_to_url(path)?;
        self.notify(
            "textDocument/didOpen",
            serde_json::to_value(lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem {
                    uri,
                    language_id: language_id.to_string(),
                    version,
                    text,
                },
            })?,
        )
    }

    pub fn did_change_full(&self, path: &Path, text: String, version: i32) -> Result<()> {
        self.state
            .as_ref()
            .ok_or_else(|| anyhow!("LSP client is not started"))?
            .did_change_full(path, text, version)
    }

    pub fn did_change_incremental(
        &self,
        path: &Path,
        version: i32,
        edit: &BufferEdit,
        buffer: &EditorBuffer,
    ) -> Result<()> {
        self.state
            .as_ref()
            .ok_or_else(|| anyhow!("LSP client is not started"))?
            .did_change_incremental(path, version, edit, buffer)
    }

    pub fn did_save(&self, path: &Path, text: Option<String>) -> Result<()> {
        self.state
            .as_ref()
            .ok_or_else(|| anyhow!("LSP client is not started"))?
            .did_save(path, text)
    }
}

impl RunningLspClient {
    async fn spawn(root: PathBuf, definition: &ServerDefinition) -> Result<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<OutboundMessage>();
        let (event_tx, event_rx) = mpsc::unbounded_channel::<LspEvent>();
        let pending: Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_for_reader = Arc::clone(&pending);

        let mut child = Command::new(definition.binary)
            .args(definition.args)
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn LSP server {}", definition.binary))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("{} stdout unavailable", definition.binary))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("{} stdin unavailable", definition.binary))?;

        let reader_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_message(&mut reader).await? {
                route_inbound_message(message, &pending_for_reader, &event_tx).await?;
            }
            Ok(())
        });

        let writer_task = tokio::spawn(async move { write_loop(stdin, &mut rx).await });

        let handle = LspHandle {
            tx,
            pending,
            next_id: Arc::new(AtomicI64::new(1)),
        };

        let mut client = Self {
            root,
            server: definition.binary.to_string(),
            child,
            handle,
            event_rx,
            reader_task,
            writer_task,
        };
        client.initialize().await?;
        Ok(client)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn server(&self) -> &str {
        &self.server
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.handle.request(method, params).await
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.handle.notify(method, params)
    }

    pub fn try_recv_event(&mut self) -> Option<LspEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn did_open(
        &self,
        path: &Path,
        language_id: &str,
        text: String,
        version: i32,
    ) -> Result<()> {
        let uri = path_to_url(path)?;
        self.notify(
            "textDocument/didOpen",
            serde_json::to_value(lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem {
                    uri,
                    language_id: language_id.to_string(),
                    version,
                    text,
                },
            })?,
        )
    }

    pub fn did_change_full(&self, path: &Path, text: String, version: i32) -> Result<()> {
        let uri = path_to_url(path)?;
        self.notify(
            "textDocument/didChange",
            serde_json::to_value(lsp::DidChangeTextDocumentParams {
                text_document: lsp::VersionedTextDocumentIdentifier { uri, version },
                content_changes: vec![lsp::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            })?,
        )
    }

    pub fn did_save(&self, path: &Path, text: Option<String>) -> Result<()> {
        self.notify(
            "textDocument/didSave",
            serde_json::to_value(lsp::DidSaveTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
                text,
            })?,
        )
    }

    pub async fn completion(&self, path: &Path, position: Position) -> Result<Vec<CompletionItem>> {
        let params = lsp::CompletionParams {
            text_document_position: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
            context: None,
        };
        let result = response_result(
            self.request("textDocument/completion", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_completion_result(result)
    }

    pub async fn hover(&self, path: &Path, position: Position) -> Result<Option<HoverInfo>> {
        let params = lsp::HoverParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request("textDocument/hover", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_hover_result(result)
    }

    pub async fn signature_help(
        &self,
        path: &Path,
        position: Position,
        trigger_character: Option<char>,
    ) -> Result<Option<SignatureHelpInfo>> {
        let context = trigger_character.map(|ch| lsp::SignatureHelpContext {
            trigger_kind: lsp::SignatureHelpTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(ch.to_string()),
            is_retrigger: false,
            active_signature_help: None,
        });
        let params = lsp::SignatureHelpParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            context,
        };
        let result = response_result(
            self.request("textDocument/signatureHelp", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_signature_help_result(result)
    }

    pub async fn goto_definition(&self, path: &Path, position: Position) -> Result<Vec<Location>> {
        self.goto_request("textDocument/definition", path, position)
            .await
    }

    pub async fn goto_type_definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        self.goto_request("textDocument/typeDefinition", path, position)
            .await
    }

    pub async fn goto_implementation(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        self.goto_request("textDocument/implementation", path, position)
            .await
    }

    pub async fn references(
        &self,
        path: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<Location>> {
        let params = lsp::ReferenceParams {
            text_document_position: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
            context: lsp::ReferenceContext {
                include_declaration,
            },
        };
        let result = response_result(
            self.request("textDocument/references", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_locations_result(result)
    }

    pub async fn document_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        let params = lsp::DocumentSymbolParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(
            self.request("textDocument/documentSymbol", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_document_symbols_result(result)
    }

    pub async fn workspace_symbols(&self, query: String) -> Result<Value> {
        let params = lsp::WorkspaceSymbolParams {
            partial_result_params: lsp::PartialResultParams::default(),
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            query,
        };
        response_result(
            self.request("workspace/symbol", serde_json::to_value(params)?)
                .await?,
        )
    }

    pub async fn format_document(
        &self,
        path: &Path,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Vec<lsp::TextEdit>> {
        let params = lsp::DocumentFormattingParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            options: lsp::FormattingOptions {
                tab_size,
                insert_spaces,
                ..Default::default()
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request("textDocument/formatting", serde_json::to_value(params)?)
                .await?,
        )?;
        Ok(if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        })
    }

    pub async fn format_range(
        &self,
        path: &Path,
        range: Range,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Vec<lsp::TextEdit>> {
        let params = lsp::DocumentRangeFormattingParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            range: range.into(),
            options: lsp::FormattingOptions {
                tab_size,
                insert_spaces,
                ..Default::default()
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request(
                "textDocument/rangeFormatting",
                serde_json::to_value(params)?,
            )
            .await?,
        )?;
        Ok(if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        })
    }

    pub async fn rename(&self, path: &Path, position: Position, new_name: String) -> Result<Value> {
        let params = lsp::RenameParams {
            text_document_position: text_document_position(path, position)?,
            new_name,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        response_result(
            self.request("textDocument/rename", serde_json::to_value(params)?)
                .await?,
        )
    }

    async fn goto_request(
        &self,
        method: &str,
        path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        let params = lsp::GotoDefinitionParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(self.request(method, serde_json::to_value(params)?).await?)?;
        parse_goto_result(result)
    }

    async fn initialize(&mut self) -> Result<()> {
        let root_uri = path_to_file_uri(&self.root);
        let response = self
            .request(
                "initialize",
                serde_json::json!({
                    "processId": std::process::id(),
                    "rootUri": root_uri,
                    "capabilities": {
                        "textDocument": {
                            "synchronization": { "dynamicRegistration": false },
                            "completion": { "completionItem": { "documentationFormat": ["markdown", "plaintext"] } },
                            "hover": { "contentFormat": ["markdown", "plaintext"] },
                            "signatureHelp": {
                                "contextSupport": true,
                                "signatureInformation": {
                                    "documentationFormat": ["markdown", "plaintext"],
                                    "parameterInformationFormat": ["markdown", "plaintext"]
                                }
                            },
                            "definition": { "linkSupport": true },
                            "typeDefinition": { "linkSupport": true },
                            "implementation": { "linkSupport": true },
                            "references": {},
                            "rename": {},
                            "formatting": {},
                            "rangeFormatting": {},
                            "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": ["quickfix", "refactor", "source"] } } },
                            "documentSymbol": {},
                            "publishDiagnostics": {},
                            "semanticTokens": {}
                        },
                        "workspace": { "applyEdit": true, "workspaceEdit": { "documentChanges": true }, "symbol": {} }
                    }
                }),
            )
            .await?;
        if response.get("error").is_some() {
            return Err(anyhow!("LSP initialize failed: {response}"));
        }
        self.notify("initialized", serde_json::json!({}))?;
        Ok(())
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.request("shutdown", serde_json::json!(null)).await;
        let _ = self.notify("exit", serde_json::json!(null));
        let _ = self.child.kill().await;
        let _ = self.reader_task.await;
        let _ = self.writer_task.await;
        Ok(())
    }
}

impl LspRequestHandle {
    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.tx
            .send(OutboundMessage::Json(request(id, method, params)))
            .map_err(|_| anyhow!("LSP writer task stopped"))?;
        Ok(rx.await?)
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.tx
            .send(OutboundMessage::Json(notification(method, params)))
            .map_err(|_| anyhow!("LSP writer task stopped"))
    }

    pub fn did_open(
        &self,
        path: &Path,
        language_id: &str,
        text: String,
        version: i32,
    ) -> Result<()> {
        let uri = path_to_url(path)?;
        self.notify(
            "textDocument/didOpen",
            serde_json::to_value(lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem {
                    uri,
                    language_id: language_id.to_string(),
                    version,
                    text,
                },
            })?,
        )
    }

    pub fn did_change_full(&self, path: &Path, text: String, version: i32) -> Result<()> {
        let uri = path_to_url(path)?;
        self.notify(
            "textDocument/didChange",
            serde_json::to_value(lsp::DidChangeTextDocumentParams {
                text_document: lsp::VersionedTextDocumentIdentifier { uri, version },
                content_changes: vec![lsp::TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text,
                }],
            })?,
        )
    }

    pub fn did_change_incremental(
        &self,
        path: &Path,
        version: i32,
        edit: &BufferEdit,
        buffer: &EditorBuffer,
    ) -> Result<()> {
        let uri = path_to_url(path)?;
        self.notify(
            "textDocument/didChange",
            serde_json::to_value(lsp::DidChangeTextDocumentParams {
                text_document: lsp::VersionedTextDocumentIdentifier { uri, version },
                content_changes: vec![incremental_change(buffer, edit)],
            })?,
        )
    }

    pub fn did_save(&self, path: &Path, text: Option<String>) -> Result<()> {
        self.notify(
            "textDocument/didSave",
            serde_json::to_value(lsp::DidSaveTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
                text,
            })?,
        )
    }

    pub async fn completion(&self, path: &Path, position: Position) -> Result<Vec<CompletionItem>> {
        let params = lsp::CompletionParams {
            text_document_position: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
            context: None,
        };
        let result = response_result(
            self.request("textDocument/completion", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_completion_result(result)
    }

    pub async fn resolve_completion_item(&self, item: &CompletionItem) -> Result<CompletionItem> {
        let raw = &item.raw;
        if raw.is_null() {
            return Ok(item.clone());
        }
        let result = response_result(self.request("completionItem/resolve", raw.clone()).await?)?;
        let resolved: lsp::CompletionItem = serde_json::from_value(result)?;
        let label = resolved.label.clone();
        let detail = resolved.detail.clone().or_else(|| item.detail.clone());
        let documentation = resolved.documentation.as_ref().map(markup_to_string);
        let documentation = documentation.or_else(|| item.documentation.clone());
        let insert_text = resolved
            .insert_text
            .clone()
            .or_else(|| item.insert_text.clone());
        let kind = resolved.kind.map(|kind| completion_kind_code(&kind));
        let raw = serde_json::to_value(&resolved).unwrap_or_else(|_| item.raw.clone());
        Ok(CompletionItem {
            label,
            detail,
            documentation,
            insert_text,
            kind,
            raw,
        })
    }

    pub async fn hover(&self, path: &Path, position: Position) -> Result<Option<HoverInfo>> {
        let params = lsp::HoverParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request("textDocument/hover", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_hover_result(result)
    }

    pub async fn signature_help(
        &self,
        path: &Path,
        position: Position,
        trigger_character: Option<char>,
    ) -> Result<Option<SignatureHelpInfo>> {
        let context = trigger_character.map(|ch| lsp::SignatureHelpContext {
            trigger_kind: lsp::SignatureHelpTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(ch.to_string()),
            is_retrigger: false,
            active_signature_help: None,
        });
        let params = lsp::SignatureHelpParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            context,
        };
        let result = response_result(
            self.request("textDocument/signatureHelp", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_signature_help_result(result)
    }

    pub async fn goto_definition(&self, path: &Path, position: Position) -> Result<Vec<Location>> {
        self.goto_request("textDocument/definition", path, position)
            .await
    }

    pub async fn goto_type_definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        self.goto_request("textDocument/typeDefinition", path, position)
            .await
    }

    pub async fn goto_implementation(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        self.goto_request("textDocument/implementation", path, position)
            .await
    }

    pub async fn references(
        &self,
        path: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<Location>> {
        let params = lsp::ReferenceParams {
            text_document_position: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
            context: lsp::ReferenceContext {
                include_declaration,
            },
        };
        let result = response_result(
            self.request("textDocument/references", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_locations_result(result)
    }

    pub async fn document_symbols(&self, path: &Path) -> Result<Vec<Symbol>> {
        let params = lsp::DocumentSymbolParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(
            self.request("textDocument/documentSymbol", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_document_symbols_result(result)
    }

    pub async fn workspace_symbols(&self, query: String) -> Result<Value> {
        let params = lsp::WorkspaceSymbolParams {
            partial_result_params: lsp::PartialResultParams::default(),
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            query,
        };
        response_result(
            self.request("workspace/symbol", serde_json::to_value(params)?)
                .await?,
        )
    }

    pub async fn format_document(
        &self,
        path: &Path,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Vec<lsp::TextEdit>> {
        let params = lsp::DocumentFormattingParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            options: lsp::FormattingOptions {
                tab_size,
                insert_spaces,
                ..Default::default()
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request("textDocument/formatting", serde_json::to_value(params)?)
                .await?,
        )?;
        Ok(if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        })
    }

    pub async fn format_range(
        &self,
        path: &Path,
        range: Range,
        tab_size: u32,
        insert_spaces: bool,
    ) -> Result<Vec<lsp::TextEdit>> {
        let params = lsp::DocumentRangeFormattingParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            range: range.into(),
            options: lsp::FormattingOptions {
                tab_size,
                insert_spaces,
                ..Default::default()
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request(
                "textDocument/rangeFormatting",
                serde_json::to_value(params)?,
            )
            .await?,
        )?;
        Ok(if result.is_null() {
            Vec::new()
        } else {
            serde_json::from_value(result)?
        })
    }

    pub async fn rename(&self, path: &Path, position: Position, new_name: String) -> Result<Value> {
        let params = lsp::RenameParams {
            text_document_position: text_document_position(path, position)?,
            new_name,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        response_result(
            self.request("textDocument/rename", serde_json::to_value(params)?)
                .await?,
        )
    }

    pub async fn code_actions(
        &self,
        path: &Path,
        range: Range,
        diagnostics: Vec<Diagnostic>,
    ) -> Result<Vec<CodeActionItem>> {
        let params = lsp::CodeActionParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            range: range.into(),
            context: lsp::CodeActionContext {
                diagnostics: diagnostics.into_iter().map(Into::into).collect(),
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(
            self.request("textDocument/codeAction", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_code_actions_result(result)
    }

    pub async fn document_highlight(&self, path: &Path, position: Position) -> Result<Vec<Range>> {
        let params = lsp::DocumentHighlightParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(
            self.request(
                "textDocument/documentHighlight",
                serde_json::to_value(params)?,
            )
            .await?,
        )?;
        parse_document_highlight_result(result)
    }

    pub async fn inlay_hints(&self, path: &Path) -> Result<Vec<InlayHintItem>> {
        let params = lsp::InlayHintParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            range: lsp::Range {
                start: lsp::Position {
                    line: 0,
                    character: 0,
                },
                end: lsp::Position {
                    line: u32::MAX / 2,
                    character: 0,
                },
            },
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
        };
        let result = response_result(
            self.request("textDocument/inlayHint", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_inlay_hints_result(result)
    }

    pub async fn semantic_tokens_full(&self, path: &Path) -> Result<Vec<u32>> {
        let params = lsp::SemanticTokensParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(
            self.request(
                "textDocument/semanticTokens/full",
                serde_json::to_value(params)?,
            )
            .await?,
        )?;
        parse_semantic_tokens_data(result)
    }

    pub async fn folding_ranges(&self, path: &Path) -> Result<Vec<FoldRange>> {
        let params = lsp::FoldingRangeParams {
            text_document: lsp::TextDocumentIdentifier::new(path_to_url(path)?),
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(
            self.request("textDocument/foldingRange", serde_json::to_value(params)?)
                .await?,
        )?;
        parse_folding_ranges_result(result)
    }

    async fn goto_request(
        &self,
        method: &str,
        path: &Path,
        position: Position,
    ) -> Result<Vec<Location>> {
        let params = lsp::GotoDefinitionParams {
            text_document_position_params: text_document_position(path, position)?,
            work_done_progress_params: lsp::WorkDoneProgressParams::default(),
            partial_result_params: lsp::PartialResultParams::default(),
        };
        let result = response_result(self.request(method, serde_json::to_value(params)?).await?)?;
        parse_goto_result(result)
    }
}

async fn initialize_handle(handle: &LspHandle, root: &Path) -> Result<()> {
    let response = handle
        .request(
            "initialize",
            serde_json::json!({
                "processId": std::process::id(),
                "rootUri": path_to_file_uri(root),
                "capabilities": {
                    "textDocument": {
                        "synchronization": { "dynamicRegistration": false },
                        "completion": { "completionItem": { "documentationFormat": ["markdown", "plaintext"] } },
                        "hover": { "contentFormat": ["markdown", "plaintext"] },
                        "signatureHelp": {
                            "contextSupport": true,
                            "signatureInformation": {
                                "documentationFormat": ["markdown", "plaintext"],
                                "parameterInformationFormat": ["markdown", "plaintext"]
                            }
                        },
                        "definition": { "linkSupport": true },
                        "typeDefinition": { "linkSupport": true },
                        "implementation": { "linkSupport": true },
                        "references": {},
                        "rename": {},
                        "formatting": {},
                        "rangeFormatting": {},
                        "codeAction": { "codeActionLiteralSupport": { "codeActionKind": { "valueSet": ["quickfix", "refactor", "source"] } } },
                        "documentSymbol": {},
                        "publishDiagnostics": {},
                        "semanticTokens": {}
                    },
                    "workspace": { "applyEdit": true, "workspaceEdit": { "documentChanges": true }, "symbol": {} }
                }
            }),
        )
        .await?;
    if response.get("error").is_some() {
        return Err(anyhow!("LSP initialize failed: {response}"));
    }
    handle.notify("initialized", serde_json::json!({}))?;
    Ok(())
}

async fn write_loop(
    stdin: ChildStdin,
    rx: &mut mpsc::UnboundedReceiver<OutboundMessage>,
) -> Result<()> {
    let mut writer = BufWriter::new(stdin);
    while let Some(message) = rx.recv().await {
        match message {
            OutboundMessage::Json(value) => write_message(&mut writer, &value).await?,
        }
    }
    Ok(())
}

async fn route_inbound_message(
    message: Value,
    pending: &Arc<Mutex<HashMap<i64, oneshot::Sender<Value>>>>,
    event_tx: &mpsc::UnboundedSender<LspEvent>,
) -> Result<()> {
    let method = message
        .get("method")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(method) = method {
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        if let Some(id) = response_id(&message)? {
            let _ = event_tx.send(LspEvent::ServerRequest { id, method, params });
        } else if let Some(event) = parse_lsp_notification(&method, params)? {
            let _ = event_tx.send(event);
        }
        return Ok(());
    }

    if let Some(id) = response_id(&message)? {
        if let Some(waiter) = pending.lock().await.remove(&id) {
            let _ = waiter.send(message);
        }
    }
    Ok(())
}

pub fn parse_lsp_notification(method: &str, params: Value) -> Result<Option<LspEvent>> {
    match method {
        "textDocument/publishDiagnostics" => {
            let params: lsp::PublishDiagnosticsParams = serde_json::from_value(params)?;
            Ok(Some(LspEvent::PublishDiagnostics {
                uri: params.uri.to_string(),
                diagnostics: params.diagnostics.into_iter().map(Into::into).collect(),
                version: params.version,
            }))
        }
        "window/showMessage" => {
            let params: lsp::ShowMessageParams = serde_json::from_value(params)?;
            Ok(Some(LspEvent::ShowMessage {
                typ: message_type_name(&params.typ),
                message: params.message,
            }))
        }
        "window/logMessage" => {
            let params: lsp::LogMessageParams = serde_json::from_value(params)?;
            Ok(Some(LspEvent::LogMessage {
                typ: message_type_name(&params.typ),
                message: params.message,
            }))
        }
        _ => Ok(Some(LspEvent::Notification {
            method: method.to_string(),
            params,
        })),
    }
}

fn binary_exists(binary: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let direct = dir.join(binary);
                let exe = dir.join(format!("{binary}.exe"));
                direct.is_file() || exe.is_file()
            })
        })
        .unwrap_or(false)
}

fn path_to_file_uri(path: &Path) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    if path.starts_with('/') {
        format!("file://{path}")
    } else {
        format!("file:///{path}")
    }
}

pub fn path_to_url(path: &Path) -> Result<lsp::Url> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    lsp::Url::from_file_path(&path)
        .map_err(|_| anyhow!("could not convert path to file URI: {}", path.display()))
}

fn text_document_position(
    path: &Path,
    position: Position,
) -> Result<lsp::TextDocumentPositionParams> {
    Ok(lsp::TextDocumentPositionParams::new(
        lsp::TextDocumentIdentifier::new(path_to_url(path)?),
        position.into(),
    ))
}

fn response_result(response: Value) -> Result<Value> {
    if let Some(error) = response.get("error") {
        return Err(anyhow!("LSP request failed: {error}"));
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

pub fn parse_completion_result(result: Value) -> Result<Vec<CompletionItem>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let response: lsp::CompletionResponse = serde_json::from_value(result)?;
    let items = match response {
        lsp::CompletionResponse::Array(items) => items,
        lsp::CompletionResponse::List(list) => list.items,
    };
    Ok(items
        .into_iter()
        .map(|item| {
            let documentation = item.documentation.as_ref().map(markup_to_string);
            CompletionItem {
                label: item.label.clone(),
                detail: item.detail.clone(),
                documentation,
                insert_text: item.insert_text.clone(),
                kind: item.kind.map(|kind| completion_kind_code(&kind)),
                raw: serde_json::to_value(item).unwrap_or(Value::Null),
            }
        })
        .collect())
}

pub fn parse_hover_result(result: Value) -> Result<Option<HoverInfo>> {
    if result.is_null() {
        return Ok(None);
    }
    let hover: lsp::Hover = serde_json::from_value(result)?;
    Ok(Some(HoverInfo {
        markdown: hover_contents_to_string(&hover.contents),
        range: hover.range.map(Into::into),
    }))
}

pub fn parse_signature_help_result(result: Value) -> Result<Option<SignatureHelpInfo>> {
    if result.is_null() {
        return Ok(None);
    }
    let help: lsp::SignatureHelp = serde_json::from_value(result)?;
    let active_signature = help.active_signature.unwrap_or(0) as usize;
    let signature = help
        .signatures
        .get(active_signature)
        .or_else(|| help.signatures.first());
    let Some(signature) = signature else {
        return Ok(None);
    };
    let active_parameter = help
        .active_parameter
        .or(signature.active_parameter)
        .map(|index| index as usize);
    let parameters = signature
        .parameters
        .as_ref()
        .map(|params| {
            params
                .iter()
                .map(|param| parameter_label(&param.label))
                .collect()
        })
        .unwrap_or_default();
    Ok(Some(SignatureHelpInfo {
        label: signature.label.clone(),
        documentation: signature
            .documentation
            .as_ref()
            .map(documentation_to_string),
        active_parameter,
        parameters,
    }))
}

pub fn parse_goto_result(result: Value) -> Result<Vec<Location>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let response: lsp::GotoDefinitionResponse = serde_json::from_value(result)?;
    Ok(match response {
        lsp::GotoDefinitionResponse::Scalar(location) => vec![location.into()],
        lsp::GotoDefinitionResponse::Array(locations) => {
            locations.into_iter().map(Into::into).collect()
        }
        lsp::GotoDefinitionResponse::Link(links) => links
            .into_iter()
            .map(|link| Location {
                uri: link.target_uri.to_string(),
                range: link.target_selection_range.into(),
            })
            .collect(),
    })
}

pub fn parse_code_actions_result(result: Value) -> Result<Vec<CodeActionItem>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let response: lsp::CodeActionResponse = serde_json::from_value(result)?;
    Ok(response
        .into_iter()
        .map(|item| match item {
            lsp::CodeActionOrCommand::Command(command) => CodeActionItem {
                title: command.title.clone(),
                kind: None,
                raw: serde_json::to_value(command).unwrap_or(Value::Null),
            },
            lsp::CodeActionOrCommand::CodeAction(action) => CodeActionItem {
                title: action.title.clone(),
                kind: action.kind.as_ref().map(|kind| kind.as_str().to_string()),
                raw: serde_json::to_value(action).unwrap_or(Value::Null),
            },
        })
        .collect())
}

fn parse_locations_result(result: Value) -> Result<Vec<Location>> {
    if result.is_null() {
        Ok(Vec::new())
    } else {
        Ok(serde_json::from_value::<Vec<lsp::Location>>(result)?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}

pub fn parse_document_symbols_result(result: Value) -> Result<Vec<Symbol>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let response: lsp::DocumentSymbolResponse = serde_json::from_value(result)?;
    let mut symbols = Vec::new();
    match response {
        lsp::DocumentSymbolResponse::Flat(flat) => {
            symbols.extend(flat.into_iter().map(|symbol| Symbol {
                name: symbol.name,
                kind: symbol_kind_code(&symbol.kind),
                range: symbol.location.range.into(),
                selection_range: symbol.location.range.into(),
            }));
        }
        lsp::DocumentSymbolResponse::Nested(nested) => {
            for symbol in nested {
                flatten_document_symbol(symbol, &mut symbols);
            }
        }
    }
    Ok(symbols)
}

fn flatten_document_symbol(symbol: lsp::DocumentSymbol, out: &mut Vec<Symbol>) {
    out.push(Symbol {
        name: symbol.name,
        kind: symbol_kind_code(&symbol.kind),
        range: symbol.range.into(),
        selection_range: symbol.selection_range.into(),
    });
    if let Some(children) = symbol.children {
        for child in children {
            flatten_document_symbol(child, out);
        }
    }
}

fn hover_contents_to_string(contents: &lsp::HoverContents) -> String {
    match contents {
        lsp::HoverContents::Scalar(marked) => marked_string_to_string(marked),
        lsp::HoverContents::Array(items) => items
            .iter()
            .map(marked_string_to_string)
            .collect::<Vec<_>>()
            .join("\n\n"),
        lsp::HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn parameter_label(label: &lsp::ParameterLabel) -> String {
    match label {
        lsp::ParameterLabel::Simple(text) => text.clone(),
        lsp::ParameterLabel::LabelOffsets([start, end]) => format!("{start}..{end}"),
    }
}

fn documentation_to_string(documentation: &lsp::Documentation) -> String {
    markup_to_string(documentation)
}

fn markup_to_string(markup: &lsp::Documentation) -> String {
    match markup {
        lsp::Documentation::String(text) => text.clone(),
        lsp::Documentation::MarkupContent(markup) => markup.value.clone(),
    }
}

fn marked_string_to_string(marked: &lsp::MarkedString) -> String {
    match marked {
        lsp::MarkedString::String(text) => text.clone(),
        lsp::MarkedString::LanguageString(language) => {
            format!("```{}\n{}\n```", language.language, language.value)
        }
    }
}

fn completion_kind_code(kind: &lsp::CompletionItemKind) -> u32 {
    if *kind == lsp::CompletionItemKind::TEXT {
        1
    } else if *kind == lsp::CompletionItemKind::METHOD {
        2
    } else if *kind == lsp::CompletionItemKind::FUNCTION {
        3
    } else if *kind == lsp::CompletionItemKind::CONSTRUCTOR {
        4
    } else if *kind == lsp::CompletionItemKind::FIELD {
        5
    } else if *kind == lsp::CompletionItemKind::VARIABLE {
        6
    } else if *kind == lsp::CompletionItemKind::CLASS {
        7
    } else if *kind == lsp::CompletionItemKind::INTERFACE {
        8
    } else if *kind == lsp::CompletionItemKind::MODULE {
        9
    } else if *kind == lsp::CompletionItemKind::PROPERTY {
        10
    } else {
        0
    }
}

fn symbol_kind_code(kind: &lsp::SymbolKind) -> u32 {
    if *kind == lsp::SymbolKind::FILE {
        1
    } else if *kind == lsp::SymbolKind::MODULE {
        2
    } else if *kind == lsp::SymbolKind::NAMESPACE {
        3
    } else if *kind == lsp::SymbolKind::PACKAGE {
        4
    } else if *kind == lsp::SymbolKind::CLASS {
        5
    } else if *kind == lsp::SymbolKind::METHOD {
        6
    } else if *kind == lsp::SymbolKind::PROPERTY {
        7
    } else if *kind == lsp::SymbolKind::FIELD {
        8
    } else if *kind == lsp::SymbolKind::FUNCTION {
        12
    } else if *kind == lsp::SymbolKind::VARIABLE {
        13
    } else if *kind == lsp::SymbolKind::STRUCT {
        23
    } else {
        0
    }
}

fn message_type_name(typ: &lsp::MessageType) -> &'static str {
    if *typ == lsp::MessageType::ERROR {
        "error"
    } else if *typ == lsp::MessageType::WARNING {
        "warning"
    } else if *typ == lsp::MessageType::INFO {
        "info"
    } else {
        "log"
    }
}

pub fn parse_semantic_tokens_data(result: Value) -> Result<Vec<u32>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    let Some(data) = result.get("data").and_then(|value| value.as_array()) else {
        return Ok(Vec::new());
    };
    Ok(data
        .iter()
        .filter_map(|value| value.as_u64().map(|number| number as u32))
        .collect())
}

pub fn parse_document_highlight_result(result: Value) -> Result<Vec<Range>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    Ok(
        serde_json::from_value::<Vec<lsp::DocumentHighlight>>(result)?
            .into_iter()
            .map(|highlight| highlight.range.into())
            .collect(),
    )
}

pub fn parse_inlay_hints_result(result: Value) -> Result<Vec<InlayHintItem>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_value::<Vec<lsp::InlayHint>>(result)?
        .into_iter()
        .map(|hint| InlayHintItem {
            line: hint.position.line,
            character: hint.position.character,
            label: inlay_hint_label(&hint.label),
        })
        .collect())
}

pub fn parse_folding_ranges_result(result: Value) -> Result<Vec<FoldRange>> {
    if result.is_null() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_value::<Vec<lsp::FoldingRange>>(result)?
        .into_iter()
        .map(|range| FoldRange {
            start_line: range.start_line,
            end_line: range.end_line,
            kind: range.kind.as_ref().map(|kind| format!("{kind:?}")),
        })
        .collect())
}

fn inlay_hint_label(label: &lsp::InlayHintLabel) -> String {
    match label {
        lsp::InlayHintLabel::String(text) => text.clone(),
        other => format!("{other:?}"),
    }
}

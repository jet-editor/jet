use crate::{
    buffer::crdt::TextOperation,
    buffer::{
        history::{CursorSnapshot, History, HistoryEntry, HistoryNode},
        rope::{BufferEdit, EditorBuffer},
    },
    collab::{
        protocol::CollabMessage,
        session::CollaborationSession,
        transport::{CollabLink, CollaborationTransport, TcpTransport, WebSocketTransport},
        ui as collab_ui,
    },
    config::{
        self,
        keybindings::{chord_from_event, BindingMatch, BindingTable, KeyChord},
        schema::JetConfig,
        watch::ConfigWatcher,
    },
    editor::{
        actions,
        buffers::{BufferId, BufferManager},
        commands,
        cursor::Cursor,
        grep, indent, lsp_ui,
        mode::Mode,
        motions::{self, CharSearchMode, Jump, JumpList},
        picker,
        registers::{RegisterBank, RegisterId},
        search,
        selection::Selection,
        splits::{SplitDirection, SplitManager},
        surround, textobjects,
        view::{View, CHROME_ROWS},
        word_wrap,
    },
    git::{self, LineStatus},
    highlight::{
        grammars::{self, GrammarManager},
        semantic::{self, SemanticToken},
        theme::{Theme, ThemeRegistry},
        treesitter::{HighlightSpan, Language, TreeSitterHighlighter},
    },
    lsp::{
        client::{LspClient, LspEvent, LspRequestHandle},
        servers::{self, ServerDefinition},
        types::{
            CodeActionItem, CompletionItem, Diagnostic, DiagnosticSeverity, FoldRange, HoverInfo,
            InlayHintItem, Location, Position, Range, SignatureHelpInfo, Symbol,
        },
    },
    plugin::{
        api::{BufferSnapshot, PluginEvent},
        manager::PluginManager,
        runtime::wasm_runtime_available,
    },
    terminal::TerminalPanel,
    ui::{
        renderer::FrameRenderer,
        widgets::{
            bufferline::{self, BufferTab},
            diagnostics,
            filetree::{self, FileTreeItem},
            fuzzy::fuzzy_match,
            gutter, plugin_ui, tutor, whichkey,
            whichkey::WhichKeyEntry,
        },
    },
    util::clipboard,
};
use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, Clear, ClearType},
};
use lsp_types as lsp;
use serde_json::Value;
use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    env,
    hash::{Hash, Hasher},
    io::{stdout, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePicker {
    Files,
    Buffers,
    Diagnostics,
    Themes,
    Symbols,
    Grep,
    Locations,
    CodeActions,
    GitDiff,
    Help,
    Plugins,
}

#[derive(Debug, Clone)]
struct AppPickerItem {
    label: String,
    path: Option<PathBuf>,
    buffer_id: Option<BufferId>,
    row: Option<usize>,
    col: Option<usize>,
    code_action: Option<CodeActionItem>,
}

impl AppPickerItem {
    fn plain(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            path: None,
            buffer_id: None,
            row: None,
            col: None,
            code_action: None,
        }
    }

    fn path(label: impl Into<String>, path: PathBuf) -> Self {
        Self {
            label: label.into(),
            path: Some(path),
            buffer_id: None,
            row: None,
            col: None,
            code_action: None,
        }
    }

    fn buffer(label: impl Into<String>, id: BufferId) -> Self {
        Self {
            label: label.into(),
            path: None,
            buffer_id: Some(id),
            row: None,
            col: None,
            code_action: None,
        }
    }

    fn location(label: impl Into<String>, path: Option<PathBuf>, row: usize, col: usize) -> Self {
        Self {
            label: label.into(),
            path,
            buffer_id: None,
            row: Some(row),
            col: Some(col),
            code_action: None,
        }
    }

    fn code_action(action: CodeActionItem) -> Self {
        Self {
            label: action.title.clone(),
            path: None,
            buffer_id: None,
            row: None,
            col: None,
            code_action: Some(action),
        }
    }
}

#[derive(Debug, Default)]
struct LspUiState {
    completions: Vec<CompletionItem>,
    completion_filter: Vec<usize>,
    completion_visible: bool,
    completion_selected: usize,
    completion_word_start: usize,
    hover: Option<HoverInfo>,
    signature_help: Option<SignatureHelpInfo>,
}

#[derive(Debug)]
enum AppAsyncEvent {
    Lsp(LspUiEvent),
    TreeSitter(TreeSitterEvent),
}

#[derive(Debug)]
enum LspUiEvent {
    Completion(Result<Vec<CompletionItem>, String>),
    CompletionResolve(Result<CompletionItem, String>),
    Hover(Result<Option<HoverInfo>, String>),
    SignatureHelp(Result<Option<SignatureHelpInfo>, String>),
    Locations {
        action: &'static str,
        result: Result<Vec<Location>, String>,
    },
    Symbols(Result<Vec<Symbol>, String>),
    Rename(Result<Value, String>),
    Formatting(Result<Vec<lsp::TextEdit>, String>),
    CodeActions(Result<Vec<CodeActionItem>, String>),
    Auxiliary {
        highlights: Result<Vec<Range>, String>,
        inlays: Result<Vec<InlayHintItem>, String>,
        folds: Result<Vec<FoldRange>, String>,
        semantic: Result<Vec<u32>, String>,
    },
}

#[derive(Debug)]
struct TreeSitterEvent {
    generation: u64,
    tree: Option<tree_sitter::Tree>,
}

#[derive(Debug, Clone)]
enum LastEdit {
    Insert { text: String },
    Delete { len: usize },
    PasteAfter,
    PasteBefore,
    JoinLines,
    ToggleCase,
    Indent,
    Dedent,
    DuplicateDown,
}

pub struct App {
    pub buffer: EditorBuffer,
    pub filepath: Option<PathBuf>,
    pub cursor: Cursor,
    pub view: View,
    pub read_only: bool,
    pub should_quit: bool,
    pub dirty: bool,
    pub status: String,
    pub mode: Mode,
    pub command_buffer: String,
    command_history: Vec<String>,
    command_history_index: Option<usize>,
    command_history_draft: String,
    undo_tree_visible: bool,
    pub history: History,
    register: String,
    buffers: BufferManager,
    splits: SplitManager,
    session_read_only: bool,
    picker_kind: Option<ActivePicker>,
    picker_items: Vec<AppPickerItem>,
    picker_selected: usize,
    picker_root: PathBuf,
    renderer: FrameRenderer,
    config: JetConfig,
    config_normal_bindings: BindingTable,
    config_space_bindings: BindingTable,
    config_chord_buffer: Vec<KeyChord>,
    config_watcher: Option<ConfigWatcher>,
    themes: ThemeRegistry,
    enable_lsp: bool,
    enable_highlight: bool,
    language: Language,
    highlighter: Option<TreeSitterHighlighter>,
    grammar_manager: GrammarManager,
    lsp_server: Option<&'static ServerDefinition>,
    lsp_client: Option<LspClient>,
    lsp_diagnostics: Vec<Diagnostic>,
    lsp_ui: LspUiState,
    document_version: i32,
    async_tx: mpsc::UnboundedSender<AppAsyncEvent>,
    async_rx: mpsc::UnboundedReceiver<AppAsyncEvent>,
    tree_parse_generation: u64,
    tree_parse_in_flight: bool,
    tree_parse_dirty: bool,
    plugin_manager: PluginManager,
    plugins_discovered: bool,
    collab_session: Option<CollaborationSession>,
    collab_listener: Option<TcpListener>,
    collab_clients: Vec<TcpTransport>,
    collab_client: Option<CollabLink>,
    collab_suppress_echo: bool,
    collab_last_ping: Option<Instant>,
    collab_join_addr: Option<String>,
    collab_reconnect_after: Option<Instant>,
    collab_disconnect_notified: bool,
    tutor_visible: bool,
    registers: RegisterBank,
    active_register: RegisterId,
    register_select_pending: bool,
    jump_list: JumpList,
    last_edit_position: usize,
    last_edit: Option<LastEdit>,
    pending_insert_text: String,
    char_search: Option<CharSearchState>,
    match_pending: Option<MatchPending>,
    search_forward: bool,
    search_matches: Vec<usize>,
    search_match_index: usize,
    search_pattern: String,
    search_hl_visible: bool,
    lsp_start_pending: bool,
    lsp_document_open: Option<PathBuf>,
    chord_leader: Option<char>,
    git_marks: HashMap<usize, LineStatus>,
    git_hunks: Vec<git::GitHunk>,
    git_blame_visible: bool,
    git_blame: HashMap<usize, git::LineBlame>,
    git_branch: Option<String>,
    auto_pairs: bool,
    collab_pending: Vec<CollabMessage>,
    lsp_document_highlights: Vec<Range>,
    lsp_inlay_hints: Vec<InlayHintItem>,
    lsp_fold_ranges: Vec<FoldRange>,
    lsp_folded_starts: HashMap<u32, bool>,
    lsp_semantic_tokens: Vec<SemanticToken>,
    terminal_panel: Option<TerminalPanel>,
    terminal_visible: bool,
    terminal_focused: bool,
    file_tree_visible: bool,
    file_tree_focused: bool,
    file_tree_items: Vec<FileTreeItem>,
    file_tree_lines: Vec<String>,
    file_tree_selected: usize,
    prefix_mode_since: Option<Instant>,
    count_buffer: String,
    which_key_visible: bool,
    pending_format_on_save: Option<PathBuf>,
    wrap_continuation: Vec<bool>,
}

#[derive(Debug, Clone)]
struct CharSearchState {
    target: char,
    backward: bool,
    mode: CharSearchMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchPending {
    ForwardInclusive,
    ForwardExclusive,
    BackwardInclusive,
    BackwardExclusive,
}

enum ConfigBindingStep {
    Ignored,
    Consumed,
    Execute(String),
}

fn match_config_chord(
    buffer: &mut Vec<KeyChord>,
    key: KeyEvent,
    table: &BindingTable,
) -> ConfigBindingStep {
    if table.is_empty() {
        return ConfigBindingStep::Ignored;
    }
    buffer.push(chord_from_event(key));
    match table.match_sequence(buffer) {
        BindingMatch::Complete(action) => {
            buffer.clear();
            ConfigBindingStep::Execute(action.to_string())
        }
        BindingMatch::Prefix => ConfigBindingStep::Consumed,
        BindingMatch::None => {
            buffer.clear();
            ConfigBindingStep::Ignored
        }
    }
}

impl App {
    pub fn from_args(
        files: Vec<PathBuf>,
        line: Option<usize>,
        read_only: bool,
        _keymap: Option<String>,
        theme: Option<String>,
        enable_lsp: bool,
        enable_highlight: bool,
    ) -> Result<Self> {
        let mut config = config::load().unwrap_or_default();
        let auto_pairs = config.auto_pairs;
        if let Some(theme) = theme {
            config.theme = theme;
        }
        config.lsp &= enable_lsp;
        config.highlight &= enable_highlight;
        let mut themes = ThemeRegistry::new();
        if let Some(dir) = config::themes_dir() {
            let _ = themes.load_dir(&dir);
        }
        let _ = themes.set_active(&config.theme);
        let mut buffers = BufferManager::new(64);
        let mut first_id = None;
        if files.is_empty() {
            first_id = Some(buffers.new_scratch());
        } else {
            for path in files {
                let id = buffers.open(path)?;
                first_id.get_or_insert(id);
            }
        }
        if let Some(id) = first_id {
            buffers.switch_to(id);
        }

        let (width, height) = terminal::size().unwrap_or((80, 24));
        let current = buffers.current().expect("app always has a buffer");
        let current_id = current.id;
        let filepath = current.path.clone();
        let buffer = current.buffer.clone();
        let cursor = current.cursor;
        let dirty = current.modified;
        let effective_read_only = read_only || current.read_only;
        let data_dir = default_data_dir();
        let (async_tx, async_rx) = mpsc::unbounded_channel();
        let config_watcher = ConfigWatcher::try_new().ok().flatten();
        let config_normal_bindings = BindingTable::from_config(&config, "normal");
        let config_space_bindings = BindingTable::from_config(&config, "space");
        let mut app = Self {
            buffer,
            filepath,
            cursor,
            view: View::new(
                width as usize,
                height.saturating_sub(CHROME_ROWS as u16) as usize,
            ),
            read_only: effective_read_only,
            should_quit: false,
            dirty,
            status: String::from("i insert  : command  space menu  Ctrl-Q quit"),
            mode: Mode::Normal,
            command_buffer: String::new(),
            command_history: Vec::new(),
            command_history_index: None,
            command_history_draft: String::new(),
            undo_tree_visible: false,
            history: History::new(),
            register: String::new(),
            buffers,
            splits: SplitManager::new(
                current_id,
                width as usize,
                height.saturating_sub(CHROME_ROWS as u16) as usize,
            ),
            session_read_only: read_only,
            picker_kind: None,
            picker_items: Vec::new(),
            picker_selected: 0,
            picker_root: env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            renderer: FrameRenderer::new(width, height.saturating_sub(CHROME_ROWS as u16)),
            config,
            config_normal_bindings,
            config_space_bindings,
            config_chord_buffer: Vec::new(),
            config_watcher,
            themes,
            enable_lsp: false,
            enable_highlight: false,
            language: Language::PlainText,
            highlighter: None,
            grammar_manager: GrammarManager::new(data_dir.join("grammars")),
            lsp_server: None,
            lsp_client: None,
            lsp_diagnostics: Vec::new(),
            lsp_ui: LspUiState::default(),
            document_version: 0,
            async_tx,
            async_rx,
            tree_parse_generation: 0,
            tree_parse_in_flight: false,
            tree_parse_dirty: false,
            plugin_manager: PluginManager::new(data_dir.join("plugins")),
            plugins_discovered: false,
            collab_session: None,
            collab_listener: None,
            collab_clients: Vec::new(),
            collab_client: None,
            collab_suppress_echo: false,
            collab_last_ping: None,
            collab_join_addr: None,
            collab_reconnect_after: None,
            collab_disconnect_notified: false,
            tutor_visible: false,
            registers: RegisterBank::default(),
            active_register: RegisterId::Unnamed,
            register_select_pending: false,
            jump_list: JumpList::new(128),
            last_edit_position: 0,
            last_edit: None,
            pending_insert_text: String::new(),
            char_search: None,
            match_pending: None,
            search_forward: true,
            search_matches: Vec::new(),
            search_match_index: 0,
            search_pattern: String::new(),
            search_hl_visible: true,
            lsp_start_pending: false,
            lsp_document_open: None,
            chord_leader: None,
            git_marks: HashMap::new(),
            git_hunks: Vec::new(),
            git_blame_visible: false,
            git_blame: HashMap::new(),
            git_branch: None,
            auto_pairs,
            collab_pending: Vec::new(),
            lsp_document_highlights: Vec::new(),
            lsp_inlay_hints: Vec::new(),
            lsp_fold_ranges: Vec::new(),
            lsp_folded_starts: HashMap::new(),
            lsp_semantic_tokens: Vec::new(),
            terminal_panel: None,
            terminal_visible: false,
            terminal_focused: false,
            file_tree_visible: false,
            file_tree_focused: false,
            file_tree_items: Vec::new(),
            file_tree_lines: Vec::new(),
            file_tree_selected: 0,
            prefix_mode_since: None,
            count_buffer: String::new(),
            which_key_visible: false,
            pending_format_on_save: None,
            wrap_continuation: Vec::new(),
        };
        app.enable_lsp = app.config.lsp;
        app.enable_highlight = app.config.highlight;
        app.view.scrolloff = app.config.editor.scrolloff;
        app.refresh_subsystems_for_current_buffer();

        if let Some(line) = line {
            app.cursor.row = line
                .saturating_sub(1)
                .min(app.buffer.len_lines().saturating_sub(1));
        }

        Ok(app)
    }

    pub fn run(&mut self) -> Result<()> {
        let mut stdout = stdout();
        execute!(stdout, Hide, Clear(ClearType::All))?;

        while !self.should_quit {
            self.drain_lsp_events();
            self.drain_async_events();
            self.tick_lsp_lifecycle();
            self.tick_tree_sitter();
            self.tick_which_key();
            self.tick_collab();
            self.tick_config_watch();
            self.draw()?;
            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key)?,
                    Event::Resize(width, height) => {
                        let editor_height = height.saturating_sub(CHROME_ROWS as u16) as usize;
                        self.view.resize(width as usize, editor_height);
                        self.splits
                            .focused_mut()
                            .view
                            .resize(width as usize, editor_height);
                        self.renderer
                            .resize(width, height.saturating_sub(CHROME_ROWS as u16));
                    }
                    _ => {}
                }
            }
        }

        self.persist_persistent_history();
        execute!(stdout, Show)?;
        Ok(())
    }

    fn draw(&mut self) -> Result<()> {
        self.apply_plugin_edits();
        let mut stdout = stdout();
        let frame_start = Instant::now();
        self.install_ready_tree_sitter(frame_start);
        self.view.ensure_cursor_visible(&self.buffer, &self.cursor);
        let visible = if self.mode == Mode::Picker {
            self.picker_lines()
        } else {
            let lines = self.highlight_visible_lines();
            let folded = self.apply_line_folding(lines);
            if self.config.editor.word_wrap && self.view.width > 0 {
                let (wrapped, continuation) = word_wrap::wrap_lines(folded, self.view.width);
                self.wrap_continuation = continuation;
                wrapped
            } else {
                self.wrap_continuation.clear();
                folded
            }
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_collab_carets(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_git_blame_annotations(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_diagnostic_inline(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_plugin_virtual_text(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_gutter(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_lsp_overlays(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_tutor_overlay(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_which_key_overlay(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_undo_tree_overlay(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_document_highlights(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_search_highlights(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_cursorline_highlight(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_inlay_hints(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_file_tree_sidebar(visible)
        };
        let visible = if self.mode == Mode::Picker {
            visible
        } else {
            self.with_terminal_overlay(visible)
        };
        let frame = self
            .renderer
            .render_to_string(visible.iter().map(String::as_str));
        stdout.write_all(frame.as_bytes())?;

        let bufferline = self.bufferline_line();
        execute!(
            stdout,
            MoveTo(0, self.view.height as u16),
            Clear(ClearType::CurrentLine)
        )?;
        write!(stdout, "{bufferline}")?;

        let status = self.status_line();
        execute!(
            stdout,
            MoveTo(0, self.view.height.saturating_add(1) as u16),
            Clear(ClearType::CurrentLine)
        )?;
        write!(stdout, "{}", status)?;
        execute!(
            stdout,
            MoveTo(
                self.cursor.col.saturating_sub(self.view.left_col) as u16,
                self.cursor.row.saturating_sub(self.view.top_line) as u16
            )
        )?;
        stdout.flush()?;
        Ok(())
    }

    fn status_line(&self) -> String {
        if self.mode == Mode::Command {
            let mut line = format!(":{}", self.command_buffer);
            if let Some(hints) = self.command_mode_hints() {
                line.push_str("  [");
                line.push_str(&hints);
                line.push(']');
            }
            return line;
        }
        if self.mode == Mode::Search {
            let dir = if self.search_forward { '/' } else { '?' };
            let count = self.search_matches.len();
            return format!(
                "{dir}{}  {}/{} matches",
                self.command_buffer,
                self.search_match_index.saturating_add(1).min(count),
                count
            );
        }
        if self.mode == Mode::Goto {
            return "goto: g top  e end  h start  l end  . edit  d def  y type  r refs  i impl  c comment".to_string();
        }
        if self.mode == Mode::Match {
            return "match: enter target character".to_string();
        }
        if self.mode == Mode::Picker {
            let kind = match self.picker_kind {
                Some(ActivePicker::Files) => "files",
                Some(ActivePicker::Buffers) => "buffers",
                Some(ActivePicker::Diagnostics) => "diagnostics",
                Some(ActivePicker::Themes) => "themes",
                Some(ActivePicker::Symbols) => "symbols",
                Some(ActivePicker::Grep) => "grep",
                Some(ActivePicker::Locations) => "locations",
                Some(ActivePicker::CodeActions) => "code-actions",
                Some(ActivePicker::GitDiff) => "git-diff",
                Some(ActivePicker::Help) => "help",
                Some(ActivePicker::Plugins) => "plugins",
                None => "picker",
            };
            return format!(
                "{} picker: {}  {}/{}",
                kind,
                self.command_buffer,
                self.picker_selected
                    .saturating_add(1)
                    .min(self.picker_items.len()),
                self.picker_items.len()
            );
        }
        let name = self
            .filepath
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "[No Name]".to_string());
        let dirty = if self.dirty { " modified" } else { "" };
        let read_only = if self.read_only { " readonly" } else { "" };
        let splits = if self.splits.splits().len() > 1 {
            format!("  {} splits", self.splits.splits().len())
        } else {
            String::new()
        };
        let diagnostics = self.diagnostics_summary();
        let language = self.language_name();
        let plugins = if self.plugin_manager.plugin_count() > 0 {
            format!("  plug:{}", self.plugin_manager.plugin_count())
        } else {
            String::new()
        };
        let collab = self.collab_status();
        let git = self.git_status_suffix();
        let selections = if self.buffer.selections().selections().len() > 1 {
            format!("  sel:{}", self.buffer.selections().selections().len())
        } else {
            String::new()
        };
        let mode = format!(
            "{}{}\x1b[0m",
            self.themes.active().ansi_status_mode(),
            self.mode.name()
        );
        format!(
            "{} {}{}{}  {}:{}{}  {}{}{}{}{}{}  {}",
            mode,
            selections,
            name,
            dirty,
            read_only,
            self.cursor.row + 1,
            self.cursor.col + 1,
            splits,
            language,
            diagnostics,
            plugins,
            collab,
            git,
            self.status
        )
    }

    pub fn current_path(&self) -> Option<&std::path::Path> {
        self.filepath.as_deref()
    }

    pub fn split_count(&self) -> usize {
        self.splits.splits().len()
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.code == KeyCode::Char('q') && key.modifiers == KeyModifiers::CONTROL {
            self.should_quit = true;
            return Ok(());
        }
        if key.code == KeyCode::Char('s') && key.modifiers == KeyModifiers::CONTROL {
            self.save()?;
            return Ok(());
        }
        if self.terminal_focused {
            return self.handle_terminal_key(key);
        }
        if self.file_tree_focused && self.file_tree_visible {
            return self.handle_file_tree_key(key);
        }

        let before = self.mode;
        let result = match self.mode {
            Mode::Insert => self.handle_insert_key(key),
            Mode::Normal | Mode::Select => self.handle_normal_key(key),
            Mode::Command => self.handle_command_key(key),
            Mode::Picker => self.handle_picker_key(key),
            Mode::Space => self.handle_space_key(key),
            Mode::View => self.handle_view_key(key),
            Mode::Goto => self.handle_goto_key(key),
            Mode::Match => self.handle_match_key(key),
            Mode::Search => self.handle_search_key(key),
        };
        self.maybe_dispatch_mode_change(before);
        result
    }

    fn handle_insert_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.lsp_ui.completion_visible && self.handle_completion_key(key)? {
            return Ok(());
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.dismiss_completion();
                if !self.pending_insert_text.is_empty() {
                    self.last_edit = Some(LastEdit::Insert {
                        text: self.pending_insert_text.clone(),
                    });
                    self.pending_insert_text.clear();
                }
                self.history.boundary();
                self.mode = Mode::Normal;
                self.sync_selection_to_cursor();
            }
            (KeyCode::Up, _) => self.cursor.move_up(&self.buffer),
            (KeyCode::Down, _) => self.cursor.move_down(&self.buffer),
            (KeyCode::Left, _) => self.cursor.move_left(&self.buffer),
            (KeyCode::Right, _) => self.cursor.move_right(&self.buffer),
            (KeyCode::Home, _) => self.cursor.col = 0,
            (KeyCode::End, _) => self.cursor.col = self.buffer.line_len(self.cursor.row),
            (KeyCode::Backspace, _) => {
                self.backspace();
                self.refresh_completion_filter();
            }
            (KeyCode::Enter, _) => {
                let line = self.buffer.line_string(self.cursor.row);
                let extra = indent::indent_after_newline(&line);
                self.insert("\n");
                if !extra.is_empty() {
                    self.insert(&extra);
                }
            }
            (KeyCode::Tab, _) if self.lsp_ui.completion_visible => {
                self.cycle_completion(1);
            }
            (KeyCode::Tab, _) => self.insert("    "),
            (KeyCode::Char(' '), KeyModifiers::CONTROL) => self.trigger_completion(),
            (KeyCode::Char(ch), modifiers)
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                self.insert_char(ch);
                if matches!(ch, '.' | ':') {
                    self.trigger_completion();
                } else {
                    self.refresh_completion_filter();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_completion_key(&mut self, key: KeyEvent) -> Result<bool> {
        if !self.lsp_ui.completion_visible {
            return Ok(false);
        }
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.dismiss_completion();
                Ok(true)
            }
            (KeyCode::Enter, _) => {
                self.accept_completion();
                Ok(true)
            }
            (KeyCode::Tab, KeyModifiers::SHIFT)
            | (KeyCode::Up, _)
            | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.cycle_completion(-1);
                Ok(true)
            }
            (KeyCode::Tab, _)
            | (KeyCode::Down, _)
            | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.cycle_completion(1);
                Ok(true)
            }
            (KeyCode::Char(ch), modifiers)
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                let text = ch.to_string();
                self.insert(&text);
                self.refresh_completion_filter();
                Ok(true)
            }
            (KeyCode::Backspace, _) => {
                self.backspace();
                self.refresh_completion_filter();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) -> Result<()> {
        match match_config_chord(
            &mut self.config_chord_buffer,
            key,
            &self.config_normal_bindings,
        ) {
            ConfigBindingStep::Execute(action) => {
                self.execute_command(&action)?;
                return Ok(());
            }
            ConfigBindingStep::Consumed => return Ok(()),
            ConfigBindingStep::Ignored => {}
        }
        if self.handle_chord_key(key)? {
            return Ok(());
        }
        if self.register_select_pending {
            if let KeyCode::Char(ch) = key.code {
                if let Some(id) = RegisterId::from_char(ch) {
                    self.active_register = id;
                    self.register_select_pending = false;
                    self.status = format!("register: {ch}");
                    return Ok(());
                }
            }
            self.register_select_pending = false;
        }

        // Number prefix / count repeater: accumulate digits
        if let KeyCode::Char(ch) = key.code {
            if ch.is_ascii_digit() {
                if ch == '0' && self.count_buffer.is_empty() {
                    // 0 with no prefix - go to start of line
                    self.count_buffer.clear();
                    self.move_selection_line_start();
                    return Ok(());
                }
                self.count_buffer.push(ch);
                self.status = format!("count: {}", self.count_buffer);
                return Ok(());
            }
        }
        // Consume any accumulated count before motion/action
        let count: usize = self.count_buffer.parse().unwrap_or(1);
        self.count_buffer.clear();

        match (key.code, key.modifiers) {
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.scroll_view_up(self.view.height / 2)
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                self.scroll_view_down(self.view.height / 2)
            }
            (KeyCode::Char('f'), KeyModifiers::CONTROL) => self.scroll_view_down(self.view.height),
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => self.scroll_view_up(self.view.height),
            (KeyCode::Char('e'), KeyModifiers::CONTROL) => self.scroll_view_down(1),
            (KeyCode::Char('y'), KeyModifiers::CONTROL) => self.scroll_view_up(1),
            (KeyCode::Char('l'), KeyModifiers::CONTROL) => self.toggle_search_highlights(),
            (KeyCode::Char('a'), KeyModifiers::CONTROL) => self.increment_number_at_cursor(1),
            (KeyCode::Char('x'), KeyModifiers::CONTROL) => self.increment_number_at_cursor(-1),
            (KeyCode::Char('o'), KeyModifiers::CONTROL) => self.jump_backward(),
            (KeyCode::Char('i'), KeyModifiers::CONTROL) => self.jump_forward(),
            (KeyCode::Char('i'), _) => {
                self.pending_insert_text.clear();
                self.history.boundary();
                self.mode = Mode::Insert;
            }
            (KeyCode::Char(':'), _) => {
                self.command_buffer.clear();
                self.mode = Mode::Command;
            }
            (KeyCode::Char(' '), _) => {
                if self.config_chord_buffer.is_empty() {
                    self.enter_prefix_mode(Mode::Space);
                    self.status = self.which_key_text();
                }
            }
            (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                self.enter_prefix_mode(Mode::View);
                self.status = "s split  v vsplit  w next  q close  o only".to_string();
            }
            (KeyCode::Char('a'), _) => {
                self.cursor.move_right(&self.buffer);
                self.sync_selection_to_cursor();
                self.pending_insert_text.clear();
                self.history.boundary();
                self.mode = Mode::Insert;
            }
            (KeyCode::Char('v'), _) => {
                self.mode = Mode::Select;
                self.status = "extend selection (C adds cursor)".to_string();
            }
            (KeyCode::Char('C'), _) if self.mode == Mode::Select => {
                self.add_selection_from_primary();
            }
            (KeyCode::Esc, _) => {
                self.tutor_visible = false;
                self.undo_tree_visible = false;
                self.config_chord_buffer.clear();
                self.mode = Mode::Normal;
                self.sync_selection_to_cursor();
            }
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => self.perform_redo(),
            (KeyCode::Char('h'), _) | (KeyCode::Left, _) => self.move_selection_left(false, count),
            (KeyCode::Char('j'), _) | (KeyCode::Down, _) => self.move_selection_down(false, count),
            (KeyCode::Char('k'), _) | (KeyCode::Up, _) => self.move_selection_up(false, count),
            (KeyCode::Char('l'), _) | (KeyCode::Right, _) => {
                self.move_selection_right(false, count)
            }
            (KeyCode::Char('H'), _) => self.move_selection_left(true, count),
            (KeyCode::Char('J'), KeyModifiers::SHIFT) => self.join_selection_lines(),
            (KeyCode::Char('J'), _) => self.move_selection_down(true, count),
            (KeyCode::Char('K'), KeyModifiers::SHIFT) => self.lsp_status_action("hover"),
            (KeyCode::Char('K'), _) => self.move_selection_up(true, count),
            (KeyCode::Char('L'), _) => self.move_selection_right(true, count),
            (KeyCode::Char('w'), mods) => {
                self.apply_motion(motions::word_forward, mods == KeyModifiers::SHIFT, count)
            }
            (KeyCode::Char('b'), mods) => {
                self.apply_motion(motions::word_backward, mods == KeyModifiers::SHIFT, count)
            }
            (KeyCode::Char('e'), mods) => {
                self.apply_motion(motions::word_end, mods == KeyModifiers::SHIFT, count)
            }
            (KeyCode::Char('W'), mods) => self.apply_motion(
                motions::big_word_forward,
                mods == KeyModifiers::SHIFT,
                count,
            ),
            (KeyCode::Char('B'), mods) => self.apply_motion(
                motions::big_word_backward,
                mods == KeyModifiers::SHIFT,
                count,
            ),
            (KeyCode::Char('E'), mods) => {
                self.apply_motion(motions::big_word_end, mods == KeyModifiers::SHIFT, count)
            }
            (KeyCode::Char('{'), mods) => {
                self.move_selection_paragraph_backward(mods == KeyModifiers::SHIFT, count)
            }
            (KeyCode::Char('}'), mods) => {
                self.move_selection_paragraph_forward(mods == KeyModifiers::SHIFT, count)
            }
            (KeyCode::Char('x'), _) => self.select_line(),
            (KeyCode::Char('X'), _) => self.select_line_bounds(),
            (KeyCode::Char('%'), mods) => {
                if self.find_matching_bracket_pos().is_some() {
                    self.move_to_matching_bracket(mods == KeyModifiers::SHIFT);
                } else {
                    self.select_file();
                }
            }
            (KeyCode::Char(','), _) if self.char_search.is_some() => self.repeat_char_search(false),
            (KeyCode::Char(','), _) => {
                self.buffer.selections_mut().collapse_to_primary();
                self.selection_to_cursor();
            }
            (KeyCode::Char(')'), _) => {
                self.buffer.selections_mut().rotate_forward();
                self.selection_to_cursor();
            }
            (KeyCode::Char('('), _) => {
                self.buffer.selections_mut().rotate_backward();
                self.selection_to_cursor();
            }
            (KeyCode::Char('"'), _) => {
                self.register_select_pending = true;
                self.status = "select register".to_string();
            }
            (KeyCode::Char('u'), _) => self.perform_undo(),
            (KeyCode::Char('g'), _) => {
                self.enter_prefix_mode(Mode::Goto);
                self.status =
                    "goto: g top e end h start l end . edit d def y type r refs i impl".to_string();
            }
            (KeyCode::Char(']'), _) => {
                self.chord_leader = Some(']');
                self.status = "chord ]".to_string();
            }
            (KeyCode::Char('['), _) => {
                self.chord_leader = Some('[');
                self.status = "chord [".to_string();
            }
            (KeyCode::Char('/'), _) => self.begin_search(true),
            (KeyCode::Char('?'), _) => self.begin_search(false),
            (KeyCode::Char('n'), _) => self.goto_search_match(true),
            (KeyCode::Char('N'), _) => self.goto_search_match(false),
            (KeyCode::Char('*'), _) => self.search_word_under_cursor(),
            (KeyCode::Char('f'), _) => {
                self.match_pending = Some(MatchPending::ForwardInclusive);
                self.enter_prefix_mode(Mode::Match);
            }
            (KeyCode::Char('t'), _) => {
                self.match_pending = Some(MatchPending::ForwardExclusive);
                self.enter_prefix_mode(Mode::Match);
            }
            (KeyCode::Char('F'), _) => {
                self.match_pending = Some(MatchPending::BackwardInclusive);
                self.enter_prefix_mode(Mode::Match);
            }
            (KeyCode::Char('T'), _) => {
                self.match_pending = Some(MatchPending::BackwardExclusive);
                self.enter_prefix_mode(Mode::Match);
            }
            (KeyCode::Char(';'), _) => self.repeat_char_search(true),
            (KeyCode::Char('.'), _) => self.repeat_last_edit(),
            (KeyCode::Char('d'), _) => self.delete_selection(false),
            (KeyCode::Char('c'), _) => {
                self.delete_selection(false);
                self.pending_insert_text.clear();
                self.history.boundary();
                self.mode = Mode::Insert;
            }
            (KeyCode::Char('y'), _) => self.yank_selection(),
            (KeyCode::Char('p'), _) => self.paste_after(),
            (KeyCode::Char('P'), _) => self.paste_before(),
            (KeyCode::Char('~'), _) => self.toggle_case_selection(),
            (KeyCode::Char('>'), _) => self.indent_selection(true),
            (KeyCode::Char('<'), _) => self.dedent_selection(),
            (KeyCode::Char('C'), KeyModifiers::SHIFT) => self.duplicate_selection_down(),
            (KeyCode::Char('o'), KeyModifiers::ALT) => self.expand_selection_textobject(),
            _ => {}
        }
        Ok(())
    }

    fn handle_goto_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.which_key_visible = false;
                self.prefix_mode_since = None;
            }
            KeyCode::Char('g') => {
                self.push_jump();
                self.goto_char_index(0);
                self.mode = Mode::Normal;
            }
            KeyCode::Char('e') => {
                self.push_jump();
                self.goto_char_index(motions::file_end(&self.buffer));
                self.mode = Mode::Normal;
            }
            KeyCode::Char('h') => {
                self.push_jump();
                let current = self.buffer.selections().primary().head;
                self.goto_char_index(motions::line_start(&self.buffer, current));
                self.mode = Mode::Normal;
            }
            KeyCode::Char('l') => {
                self.push_jump();
                let current = self.buffer.selections().primary().head;
                self.goto_char_index(motions::line_end(&self.buffer, current));
                self.mode = Mode::Normal;
            }
            KeyCode::Char('.') => {
                self.push_jump();
                self.goto_char_index(self.last_edit_position);
                self.mode = Mode::Normal;
            }
            KeyCode::Char('d') => {
                self.mode = Mode::Normal;
                self.lsp_status_action("definition");
            }
            KeyCode::Char('y') => {
                self.mode = Mode::Normal;
                self.lsp_status_action("type definition");
            }
            KeyCode::Char('r') => {
                self.mode = Mode::Normal;
                self.lsp_status_action("references");
            }
            KeyCode::Char('i') => {
                self.mode = Mode::Normal;
                self.lsp_status_action("implementation");
            }
            KeyCode::Char('c') => {
                self.mode = Mode::Normal;
                self.toggle_comment_selection();
            }
            _ => self.mode = Mode::Normal,
        }
        Ok(())
    }

    fn handle_chord_key(&mut self, key: KeyEvent) -> Result<bool> {
        let Some(leader) = self.chord_leader.take() else {
            return Ok(false);
        };
        let handled = match (leader, key.code, key.modifiers) {
            (']', KeyCode::Char('d'), _) => {
                self.goto_diagnostic(1);
                true
            }
            (']', KeyCode::Char('c'), _) => {
                self.goto_git_hunk(1);
                true
            }
            ('[', KeyCode::Char('d'), _) => {
                self.goto_diagnostic(-1);
                true
            }
            ('[', KeyCode::Char('c'), _) => {
                self.goto_git_hunk(-1);
                true
            }
            _ => false,
        };
        if !handled {
            self.status = format!("unknown chord: {leader}");
        }
        Ok(true)
    }

    fn handle_match_key(&mut self, key: KeyEvent) -> Result<()> {
        let pending = self
            .match_pending
            .take()
            .unwrap_or(MatchPending::ForwardInclusive);
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.apply_char_search(ch, pending);
                self.mode = Mode::Normal;
            }
            _ => {
                self.match_pending = Some(pending);
            }
        }
        Ok(())
    }

    fn toggle_search_highlights(&mut self) {
        self.search_hl_visible = !self.search_hl_visible;
        self.status = if self.search_hl_visible {
            "search highlights: on".to_string()
        } else {
            "search highlights: off".to_string()
        };
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.command_buffer.clear();
                self.search_pattern.clear();
                self.search_matches.clear();
                self.search_match_index = 0;
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                if !self.search_matches.is_empty() {
                    self.search_match_index = 0;
                    self.goto_search_match(true);
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
                self.refresh_search_matches();
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.command_buffer.push(ch);
                self.refresh_search_matches();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_command_key(&mut self, key: KeyEvent) -> Result<()> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.command_buffer.clear();
                self.command_history_index = None;
                self.command_history_draft.clear();
                self.mode = Mode::Normal;
            }
            (KeyCode::Enter, _) => {
                let command = self.command_buffer.trim().to_string();
                self.command_buffer.clear();
                self.command_history_index = None;
                self.command_history_draft.clear();
                self.mode = Mode::Normal;
                self.push_command_history(&command);
                self.execute_command(&command)?;
            }
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.command_history_previous();
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.command_history_next();
            }
            (KeyCode::Tab, _) => {
                self.complete_command_buffer();
            }
            (KeyCode::Backspace, _) => {
                self.command_buffer.pop();
                self.command_history_index = None;
            }
            (KeyCode::Char(ch), modifiers)
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                self.command_buffer.push(ch);
                self.command_history_index = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn command_mode_hints(&self) -> Option<String> {
        let (head, arg) = commands::command_head(&self.command_buffer);
        match head {
            "e" | "edit" => {
                let root = self.project_root();
                let query = if arg.is_empty() { "" } else { arg };
                let items = picker::fuzzy_files(&root, query, 6, 6).ok()?;
                if items.is_empty() {
                    return None;
                }
                Some(
                    items
                        .iter()
                        .map(|item| item.display.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }
            "theme" => {
                let query = arg.to_lowercase();
                let names: Vec<_> = self
                    .themes
                    .names()
                    .into_iter()
                    .filter(|name| query.is_empty() || name.to_lowercase().contains(&query))
                    .take(6)
                    .collect();
                if names.is_empty() {
                    None
                } else {
                    Some(names.join(" "))
                }
            }
            _ => {
                let matches = commands::matching_commands(head, 4);
                if matches.is_empty() {
                    None
                } else {
                    Some(matches.join(" "))
                }
            }
        }
    }

    fn complete_command_buffer(&mut self) {
        let (head, arg) = commands::command_head(&self.command_buffer);
        match head {
            "e" | "edit" => {
                if let Ok(items) = picker::fuzzy_files(&self.project_root(), arg, 6, 1) {
                    if let Some(item) = items.first() {
                        self.command_buffer = format!("{head} {}", item.display);
                    }
                }
            }
            "theme" => {
                let query = arg.to_lowercase();
                if let Some(name) = self
                    .themes
                    .names()
                    .into_iter()
                    .find(|name| name.to_lowercase().starts_with(&query))
                {
                    self.command_buffer = format!("theme {name}");
                }
            }
            _ => {
                if let Some(command) = commands::complete_command(head) {
                    self.command_buffer = format!("{command} ");
                }
            }
        }
    }

    fn push_command_history(&mut self, command: &str) {
        if command.is_empty() {
            return;
        }
        if self
            .command_history
            .last()
            .is_some_and(|last| last == command)
        {
            return;
        }
        self.command_history.push(command.to_string());
        const MAX_HISTORY: usize = 128;
        if self.command_history.len() > MAX_HISTORY {
            let overflow = self.command_history.len() - MAX_HISTORY;
            self.command_history.drain(0..overflow);
        }
    }

    fn command_history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }
        if self.command_history_index.is_none() {
            self.command_history_draft = self.command_buffer.clone();
            self.command_history_index = Some(self.command_history.len() - 1);
        } else if let Some(index) = self.command_history_index {
            if index == 0 {
                return;
            }
            self.command_history_index = Some(index - 1);
        }
        if let Some(index) = self.command_history_index {
            self.command_buffer = self.command_history[index].clone();
        }
    }

    fn command_history_next(&mut self) {
        let Some(index) = self.command_history_index else {
            return;
        };
        if index + 1 >= self.command_history.len() {
            self.command_history_index = None;
            self.command_buffer = self.command_history_draft.clone();
            self.command_history_draft.clear();
            return;
        }
        self.command_history_index = Some(index + 1);
        self.command_buffer = self.command_history[index + 1].clone();
    }

    fn handle_space_key(&mut self, key: KeyEvent) -> Result<()> {
        match match_config_chord(
            &mut self.config_chord_buffer,
            key,
            &self.config_space_bindings,
        ) {
            ConfigBindingStep::Execute(action) => {
                self.execute_command(&action)?;
                return Ok(());
            }
            ConfigBindingStep::Consumed => return Ok(()),
            ConfigBindingStep::Ignored => {}
        }
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Char('f') => self.open_file_picker()?,
            KeyCode::Char('b') => self.open_buffer_picker(),
            KeyCode::Char('d') => self.open_diagnostics_picker(),
            KeyCode::Char('g') => self.open_grep_picker(),
            KeyCode::Char('s') if key.modifiers == KeyModifiers::SHIFT => self.open_symbol_picker(),
            KeyCode::Char('s') => {
                self.mode = Mode::Normal;
                self.save()?;
            }
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('t') => self.toggle_terminal_panel(),
            KeyCode::Char('e') => self.toggle_file_tree(),
            _ => {
                self.mode = Mode::Normal;
            }
        }
        Ok(())
    }

    fn handle_view_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('s') => {
                self.splits.split(SplitDirection::Horizontal);
                self.mode = Mode::Normal;
                self.status = format!("{} splits", self.splits.splits().len());
            }
            KeyCode::Char('v') => {
                self.splits.split(SplitDirection::Vertical);
                self.mode = Mode::Normal;
                self.status = format!("{} splits", self.splits.splits().len());
            }
            KeyCode::Char('w') => {
                self.splits.focus_next();
                self.mode = Mode::Normal;
                self.status = "focused next split".to_string();
            }
            KeyCode::Char('q') => {
                if self.splits.close_focused() {
                    self.status = format!("{} splits", self.splits.splits().len());
                } else {
                    self.status = "cannot close last split".to_string();
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('o') => {
                self.splits.close_others();
                self.mode = Mode::Normal;
                self.status = "closed other splits".to_string();
            }
            _ => self.mode = Mode::Normal,
        }
        Ok(())
    }

    fn handle_picker_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.close_picker();
            }
            KeyCode::Enter => {
                self.accept_picker_item()?;
            }
            KeyCode::Up => {
                self.picker_selected = self.picker_selected.saturating_sub(1);
            }
            KeyCode::Down if self.picker_selected + 1 < self.picker_items.len() => {
                self.picker_selected += 1;
            }
            KeyCode::Backspace => {
                self.command_buffer.pop();
                self.refresh_picker()?;
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                self.command_buffer.push(ch);
                self.refresh_picker()?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn execute_command(&mut self, command: &str) -> Result<()> {
        let command = command.trim();
        if command.is_empty() {
            return Ok(());
        }

        match command {
            "q" | "quit" => self.should_quit = true,
            "w" | "write" => self.save()?,
            "wq" | "x" => {
                self.save()?;
                self.should_quit = true;
            }
            "bn" | "bnext" => self.switch_next_buffer(),
            "bp" | "bprevious" | "bprev" => self.switch_previous_buffer(),
            "bd" | "bdelete" => self.close_current_buffer(),
            "split" | "sp" => {
                self.splits.split(SplitDirection::Horizontal);
                self.status = format!("{} splits", self.splits.splits().len());
            }
            "vsplit" | "vs" => {
                self.splits.split(SplitDirection::Vertical);
                self.status = format!("{} splits", self.splits.splits().len());
            }
            "only" => {
                self.splits.close_others();
                self.status = "closed other splits".to_string();
            }
            "buffers" | "ls" => self.open_buffer_picker(),
            "files" | "find" => self.open_file_picker()?,
            "diagnostics" | "diagnostic-list" => self.open_diagnostics_picker(),
            "git-next" | "git-hunk-next" => self.goto_git_hunk(1),
            "git-prev" | "git-hunk-prev" => self.goto_git_hunk(-1),
            "git-blame" => self.toggle_git_blame(),
            "git-stage" | "stage" => self.git_stage_current_file(),
            "git-unstage" | "unstage" => self.git_unstage_current_file(),
            "git-diff" | "diff" => self.open_git_diff_picker(),
            "symbols" | "symbol-picker" => self.open_symbol_picker(),
            "grep" => self.open_grep_picker(),
            "tutor" => self.open_tutor(),
            "terminal" => self.toggle_terminal_panel(),
            "file-tree" | "tree" => self.toggle_file_tree(),
            "textobject-function" | "af" => self.select_function_textobject(),
            "fold" | "zc" => self.toggle_fold_at_cursor(),
            "unfold" | "zo" => self.unfold_all(),
            "help" | "commands" => self.open_help_picker(),
            "lang" | "language" => {
                self.status = self.language_status();
            }
            "grammar" | "grammar-info" => {
                self.status = self.grammar_status();
            }
            "config" | "config-reload" => {
                self.reload_config();
            }
            "theme" => {
                self.open_theme_picker();
            }
            "lsp" | "lsp-info" => {
                self.status = self.lsp_status();
            }
            "lsp-start" => {
                self.start_lsp_client();
            }
            "completion" | "complete" => self.lsp_status_action("completion"),
            "hover" => self.lsp_status_action("hover"),
            "signature-help" | "signature" => self.lsp_status_action("signature help"),
            "goto-definition" | "definition" => self.lsp_status_action("definition"),
            "goto-type" => self.lsp_status_action("type definition"),
            "goto-implementation" => self.lsp_status_action("implementation"),
            "references" => self.lsp_status_action("references"),
            "rename" => self.status = "usage: :rename new_name".to_string(),
            "format" => self.lsp_status_action("format"),
            "code-actions" => self.lsp_status_action("code actions"),
            "plugins" | "plugin-list" => self.open_plugin_picker(),
            "undo-tree" | "undotree" => self.toggle_undo_tree(),
            "noh" | "nohlsearch" => {
                self.search_pattern.clear();
                self.search_matches.clear();
                self.search_match_index = 0;
                self.status = "search highlights cleared".to_string();
            }
            "collab-host" => self.collab_host(),
            "collab-leave" => self.collab_leave(),
            _ if command.starts_with("e ") || command.starts_with("edit ") => {
                let path = command
                    .split_once(' ')
                    .map(|(_, path)| path.trim())
                    .unwrap_or_default();
                if path.is_empty() {
                    self.status = "usage: :e path".to_string();
                } else {
                    self.open_path(PathBuf::from(path))?;
                }
            }
            _ if command.starts_with("theme ") => {
                let name = command
                    .split_once(' ')
                    .map(|(_, name)| name.trim())
                    .unwrap_or("");
                self.set_theme(name);
            }
            _ if command.starts_with("plugin-install ") => {
                let path = command
                    .split_once(' ')
                    .map(|(_, path)| PathBuf::from(path.trim()))
                    .unwrap_or_default();
                self.plugin_install(&path);
            }
            _ if command.starts_with("surround ") => {
                let delimiter = command
                    .split_whitespace()
                    .nth(1)
                    .and_then(|part| part.chars().next());
                if let Some(open) = delimiter {
                    self.apply_surround(open);
                } else {
                    self.status = "usage: :surround \" ( ) [ ]".to_string();
                }
            }
            "delsurround" | "ds" => self.delete_surround(),
            _ if command.starts_with("rename ") => {
                let new_name = command
                    .split_once(' ')
                    .map(|(_, name)| name.trim().to_string())
                    .unwrap_or_default();
                if new_name.is_empty() {
                    self.status = "usage: :rename new_name".to_string();
                } else {
                    self.lsp_rename_as(new_name);
                }
            }
            _ if command.starts_with("plugin-remove ") => {
                let name = command
                    .split_once(' ')
                    .map(|(_, name)| name.trim())
                    .unwrap_or("");
                self.plugin_remove(name);
            }
            _ if command.starts_with("plugin-update ") => {
                let path = command
                    .split_once(' ')
                    .map(|(_, path)| PathBuf::from(path.trim()))
                    .unwrap_or_default();
                self.plugin_update(&path);
            }
            _ if command.starts_with("collab-join ") => {
                let id = command
                    .split_once(' ')
                    .map(|(_, id)| id.trim())
                    .unwrap_or("");
                self.collab_join(id);
            }
            _ if command.starts_with("collab-chat ") => {
                let text = command
                    .split_once(' ')
                    .map(|(_, text)| text.trim().to_string())
                    .unwrap_or_default();
                self.collab_chat(text);
            }
            _ if command.starts_with("s/") || command.starts_with("%s/") => {
                let whole_file = command.starts_with('%');
                let body = command
                    .trim_start_matches('%')
                    .strip_prefix("s/")
                    .unwrap_or("");
                let first_slash = match body.find('/') {
                    Some(pos) => pos,
                    None => {
                        self.status = "usage: :s/pattern/replacement/".to_string();
                        return Ok(());
                    }
                };
                let pattern = &body[..first_slash];
                let after_pattern = &body[first_slash + 1..];
                let second_slash = after_pattern.find('/');
                let (replacement, flags) = match second_slash {
                    Some(pos) => (&after_pattern[..pos], &after_pattern[pos + 1..]),
                    None => (after_pattern, ""),
                };
                let global = flags.contains('g');
                if pattern.is_empty() {
                    self.status = "pattern required: :s/pattern/replacement/".to_string();
                } else {
                    self.execute_substitute(whole_file, pattern, replacement, global)?;
                }
            }
            _ => {
                self.status = format!("unknown command: {}", command);
            }
        }
        Ok(())
    }

    fn execute_substitute(
        &mut self,
        whole_file: bool,
        pattern: &str,
        replacement: &str,
        global: bool,
    ) -> Result<()> {
        if self.read_only {
            self.status = "read-only".to_string();
            return Ok(());
        }
        let line_indices: Vec<usize> = if whole_file {
            (0..self.buffer.len_lines()).collect()
        } else {
            vec![self.cursor.row]
        };
        let mut total = 0usize;
        let mut lines_changed = 0usize;
        let pattern_bytes = pattern.as_bytes();

        for &line_idx in &line_indices {
            if line_idx >= self.buffer.len_lines() {
                continue;
            }
            let line = self.buffer.line_string(line_idx);
            if line.is_empty() {
                continue;
            }
            let byte_matches = search::search_in_bytes(line.as_bytes(), pattern);
            if byte_matches.is_empty() {
                continue;
            }
            let matches: &[usize] = if global {
                &byte_matches
            } else {
                &byte_matches[..1]
            };
            let mut new_line = String::with_capacity(line.len());
            let mut last_end = 0;
            for &byte_start in matches {
                let byte_end = byte_start + pattern_bytes.len();
                new_line.push_str(&line[last_end..byte_start]);
                new_line.push_str(replacement);
                last_end = byte_end;
            }
            new_line.push_str(&line[last_end..]);
            let line_start_char = self.buffer.char_idx(line_idx, 0);
            let line_end_char = line_start_char + line.chars().count();
            self.buffer.remove(line_start_char..line_end_char);
            self.buffer.insert(line_start_char, &new_line);
            total += matches.len();
            lines_changed += 1;
        }
        self.dirty = true;
        if total > 0 {
            self.status = format!(
                "substituted {} occurrence{} in {} line{}",
                total,
                if total == 1 { "" } else { "s" },
                lines_changed,
                if lines_changed == 1 { "" } else { "s" }
            );
        } else {
            self.status = format!("pattern not found: {pattern}");
        }
        Ok(())
    }

    fn insert(&mut self, text: &str) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        if self.buffer.selections().selections().len() > 1 {
            self.insert_all_selections(text);
            return;
        }
        let idx = self.buffer.char_idx(self.cursor.row, self.cursor.col);
        let before = CursorSnapshot::cursor(idx);
        let edit = self.buffer.insert_with_edit(idx, text);
        for ch in text.chars() {
            if ch == '\n' {
                self.cursor.row += 1;
                self.cursor.col = 0;
            } else {
                self.cursor.col += 1;
            }
        }
        let after_idx = self.buffer.char_idx(self.cursor.row, self.cursor.col);
        self.history.push_with_cursor(
            HistoryEntry::Insert {
                idx,
                text: text.to_string(),
            },
            before,
            CursorSnapshot::cursor(after_idx),
        );
        self.sync_selection_to_cursor();
        self.dirty = true;
        self.last_edit_position = idx;
        self.after_buffer_edit(&edit);
        if text == "(" {
            self.request_signature_help(Some('('));
        } else if text == ")" {
            self.lsp_ui.signature_help = None;
        } else if text == "," && self.lsp_ui.signature_help.is_some() {
            self.request_signature_help(Some(','));
        }
        self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
    }

    fn backspace(&mut self) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let idx = self.buffer.char_idx(self.cursor.row, self.cursor.col);
        if idx == 0 {
            return;
        }
        let deleted = self.buffer.slice_chars(idx - 1, idx);
        let edit = self.buffer.remove_with_edit(idx - 1..idx);
        self.cursor.move_left(&self.buffer);
        let after_idx = self.buffer.char_idx(self.cursor.row, self.cursor.col);
        self.history.push_with_cursor(
            HistoryEntry::Delete {
                idx: idx - 1,
                text: deleted,
            },
            CursorSnapshot::cursor(idx),
            CursorSnapshot::cursor(after_idx),
        );
        self.sync_selection_to_cursor();
        self.dirty = true;
        self.last_edit_position = idx.saturating_sub(1);
        if let Some(edit) = edit {
            self.after_buffer_edit(&edit);
        }
        self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
    }

    fn save(&mut self) -> Result<()> {
        if self.read_only {
            self.status = "read-only".to_string();
            return Ok(());
        }
        if let Some(path) = self.filepath.clone() {
            if self.config.editor.format_on_save && self.pending_format_on_save.is_none() {
                if let Some(handle) = self.lsp_request_handle() {
                    if self.async_runtime_available() {
                        self.pending_format_on_save = Some(path.clone());
                        let tx = self.async_tx.clone();
                        tokio::spawn(async move {
                            let result = handle
                                .format_document(&path, 4, true)
                                .await
                                .map_err(|err| err.to_string());
                            let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Formatting(result)));
                        });
                        return Ok(());
                    }
                }
            }
            let bytes = self.buffer.save_to(&path)?;
            self.dirty = false;
            self.sync_app_into_manager();
            self.persist_persistent_history();
            self.refresh_git_gutter();
            self.status = format!("saved {} bytes", bytes);
            if let Some(client) = &self.lsp_client {
                if client.is_started() {
                    let _ = client.did_save(&path, Some(self.buffer.to_string()));
                }
            }
            self.dispatch_plugin_event(PluginEvent::BufferSave(self.buffer_snapshot()));
        } else {
            self.status = "no file path".to_string();
        }
        Ok(())
    }

    fn after_buffer_edit(&mut self, edit: &BufferEdit) {
        self.document_version += 1;
        if let Some(highlighter) = &mut self.highlighter {
            highlighter.apply_buffer_edit(&self.buffer, edit);
            self.tree_parse_dirty = true;
        }
        if let (Some(client), Some(path)) = (&self.lsp_client, self.filepath.as_deref()) {
            if client.is_started() && self.lsp_document_open.as_deref() == Some(path) {
                let _ =
                    client.did_change_incremental(path, self.document_version, edit, &self.buffer);
            }
        }
        if !self.collab_suppress_echo {
            if let Some(session) = &mut self.collab_session {
                if edit.old_text.is_empty() && !edit.new_text.is_empty() {
                    let _ = session.apply_local_insert(edit.start_char, &edit.new_text);
                } else if !edit.old_text.is_empty() && edit.new_text.is_empty() {
                    let _ = session.apply_local_delete(edit.start_char, edit.old_end_char);
                }
            }
        }
    }

    fn sync_selection_to_cursor(&mut self) {
        let idx = self.buffer.char_idx(self.cursor.row, self.cursor.col);
        self.buffer
            .selections_mut()
            .set_primary(Selection::cursor(idx));
    }

    fn selection_to_cursor(&mut self) {
        let selection = self.buffer.selections().primary();
        let (row, col) = self.buffer.char_to_line_col(selection.head);
        self.cursor.row = row;
        self.cursor.col = col;
    }

    fn update_primary_selection(&mut self, next: usize, extend: bool) {
        let max = self.buffer.len_chars();
        let next = next.min(max);
        let mut selection = self.buffer.selections().primary();
        if extend || self.mode == Mode::Select {
            selection.extend_to(next);
        } else {
            selection.move_to(next);
        }
        self.buffer.selections_mut().set_primary(selection);
        self.selection_to_cursor();
    }

    fn move_selection_left(&mut self, extend: bool, count: usize) {
        let _max = self.buffer.len_chars();
        for _ in 0..count {
            let current = self.buffer.selections().primary().head;
            if current == 0 {
                break;
            }
            self.update_primary_selection(current.saturating_sub(1), extend);
        }
    }

    fn move_selection_right(&mut self, extend: bool, count: usize) {
        let max = self.buffer.len_chars();
        for _ in 0..count {
            let current = self.buffer.selections().primary().head;
            if current >= max {
                break;
            }
            self.update_primary_selection(current.saturating_add(1), extend);
        }
    }

    fn move_selection_up(&mut self, extend: bool, count: usize) {
        let mut next = self.buffer.selections().primary().head;
        for _ in 0..count {
            let current = self.buffer.selections().primary().head;
            let (row, col) = self.buffer.char_to_line_col(current);
            if row == 0 {
                break;
            }
            next = self.buffer.char_idx(row.saturating_sub(1), col);
            self.update_primary_selection(next, extend);
        }
        self.update_primary_selection(next, extend);
    }

    fn move_selection_down(&mut self, extend: bool, count: usize) {
        let mut next = self.buffer.selections().primary().head;
        for _ in 0..count {
            let current = self.buffer.selections().primary().head;
            let (row, col) = self.buffer.char_to_line_col(current);
            let last_row = self.buffer.len_lines().saturating_sub(1);
            if row >= last_row {
                break;
            }
            next = self.buffer.char_idx(row.saturating_add(1), col);
            self.update_primary_selection(next, extend);
        }
        self.update_primary_selection(next, extend);
    }

    fn apply_motion(
        &mut self,
        motion: fn(&EditorBuffer, usize) -> usize,
        extend: bool,
        count: usize,
    ) {
        let mut pos = self.buffer.selections().primary().head;
        for _ in 0..count {
            pos = motion(&self.buffer, pos);
        }
        self.update_primary_selection(pos, extend);
    }

    fn select_line(&mut self) {
        let current = self.buffer.selections().primary().head;
        let start = motions::line_start(&self.buffer, current);
        let end = motions::line_end(&self.buffer, current);
        self.buffer
            .selections_mut()
            .set_primary(Selection::new(start, end));
        self.selection_to_cursor();
    }

    fn select_file(&mut self) {
        let len = self.buffer.len_chars();
        self.buffer
            .selections_mut()
            .set_primary(Selection::new(0, len));
        self.selection_to_cursor();
    }

    fn yank_selection(&mut self) {
        let selections = self.buffer.selections().selections().to_vec();
        let text = if selections.len() > 1 {
            selections
                .iter()
                .map(|selection| {
                    if selection.is_cursor() {
                        let (row, _) = self.buffer.char_to_line_col(selection.head);
                        self.buffer.line_string(row)
                    } else {
                        self.buffer.slice_chars(selection.start(), selection.end())
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            let selection = selections[0];
            if selection.is_cursor() {
                self.buffer.line_string(self.cursor.row)
            } else {
                self.buffer.slice_chars(selection.start(), selection.end())
            }
        };
        self.registers.yank(self.active_register, text.clone());
        self.register = text.clone();
        if clipboard::copy_osc52(&text).is_ok() {
            self.status = format!(
                "yanked {} chars (clipboard){}",
                self.register.chars().count(),
                if selections.len() > 1 {
                    format!(" from {} selections", selections.len())
                } else {
                    String::new()
                }
            );
        } else {
            self.status = format!(
                "yanked {} chars{}",
                self.register.chars().count(),
                if selections.len() > 1 {
                    format!(" from {} selections", selections.len())
                } else {
                    String::new()
                }
            );
        }
    }

    fn delete_all_selections(&mut self, blackhole: bool) {
        let mut selections = self.buffer.selections().selections().to_vec();
        selections.sort_by_key(|selection| selection.start());
        let mut deleted_total = 0usize;
        for selection in selections.into_iter().rev() {
            let range = if selection.is_cursor() {
                let start = selection.head;
                start..start.saturating_add(1).min(self.buffer.len_chars())
            } else {
                selection.range()
            };
            if range.start >= range.end {
                continue;
            }
            let deleted = self.buffer.slice_chars(range.start, range.end);
            deleted_total += deleted.chars().count();
            if !blackhole && !matches!(self.active_register, RegisterId::Blackhole) {
                self.registers.yank(self.active_register, deleted.clone());
                self.register = deleted.clone();
            }
            if let Some(edit) = self.buffer.remove_with_edit(range.clone()) {
                self.after_buffer_edit(&edit);
            }
            self.history.push_with_cursor(
                HistoryEntry::Delete {
                    idx: range.start,
                    text: deleted,
                },
                CursorSnapshot::cursor(range.end),
                CursorSnapshot::cursor(range.start),
            );
        }
        self.buffer.selections_mut().collapse_to_primary();
        self.sync_selection_to_cursor();
        self.dirty = true;
        self.status = format!("deleted {deleted_total} chars from selections");
        self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
    }

    fn add_selection_from_primary(&mut self) {
        let current = self.buffer.selections().primary();
        self.buffer.selections_mut().push_selection(current);
        let count = self.buffer.selections().selections().len();
        self.status = format!("selections: {count} (,) rotates primary");
    }

    fn delete_selection(&mut self, blackhole: bool) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }

        if self.buffer.selections().selections().len() > 1 {
            self.delete_all_selections(blackhole);
            return;
        }

        let selection = self.buffer.selections().primary();
        let range = if selection.is_cursor() {
            let start = selection.head;
            start..start.saturating_add(1).min(self.buffer.len_chars())
        } else {
            selection.range()
        };
        if range.start >= range.end {
            return;
        }
        if !blackhole {
            self.last_edit = Some(LastEdit::Delete {
                len: range.end - range.start,
            });
        }

        let deleted = self.buffer.slice_chars(range.start, range.end);
        if !blackhole && !matches!(self.active_register, RegisterId::Blackhole) {
            self.registers.yank(self.active_register, deleted.clone());
            self.registers.small_delete(deleted.clone());
            self.register = deleted.clone();
        }
        let edit = self.buffer.remove_with_edit(range.clone());
        self.history.push_with_cursor(
            HistoryEntry::Delete {
                idx: range.start,
                text: deleted,
            },
            CursorSnapshot::cursor(range.end),
            CursorSnapshot::cursor(range.start),
        );
        let (row, col) = self.buffer.char_to_line_col(range.start);
        self.cursor.row = row;
        self.cursor.col = col;
        self.sync_selection_to_cursor();
        self.dirty = true;
        if let Some(edit) = edit {
            self.after_buffer_edit(&edit);
        }
        self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
    }

    fn paste_after(&mut self) {
        self.last_edit = Some(LastEdit::PasteAfter);
        self.paste_at(self.buffer.selections().primary().end(), false);
    }

    fn paste_before(&mut self) {
        self.last_edit = Some(LastEdit::PasteBefore);
        let idx = self.buffer.selections().primary().start();
        self.paste_at(idx, true);
    }

    fn repeat_last_edit(&mut self) {
        let Some(edit) = self.last_edit.clone() else {
            self.status = "nothing to repeat".to_string();
            return;
        };
        match edit {
            LastEdit::Insert { text } => {
                self.history.boundary();
                self.insert(&text);
                self.mode = Mode::Normal;
                self.status = format!("repeated: insert {text}");
            }
            LastEdit::Delete { len } => {
                let idx = self.buffer.selections().primary().head;
                let end = idx.saturating_add(len).min(self.buffer.len_chars());
                if end <= idx {
                    return;
                }
                let range = idx..end;
                let deleted = self.buffer.slice_chars(range.start, range.end);
                self.registers.yank(RegisterId::Unnamed, deleted.clone());
                self.registers.small_delete(deleted.clone());
                let edit = self.buffer.remove_with_edit(range.clone());
                self.history.push_with_cursor(
                    HistoryEntry::Delete {
                        idx: range.start,
                        text: deleted,
                    },
                    CursorSnapshot::cursor(range.end),
                    CursorSnapshot::cursor(range.start),
                );
                let (row, col) = self.buffer.char_to_line_col(range.start);
                self.cursor.row = row;
                self.cursor.col = col;
                self.sync_selection_to_cursor();
                self.dirty = true;
                if let Some(edit) = edit {
                    self.after_buffer_edit(&edit);
                }
                self.status = format!("repeated: delete {len}");
            }
            LastEdit::PasteAfter => {
                self.paste_after();
                self.status = "repeated: paste".to_string();
            }
            LastEdit::PasteBefore => {
                self.paste_before();
                self.status = "repeated: paste before".to_string();
            }
            LastEdit::JoinLines => {
                self.join_selection_lines();
                self.status = "repeated: join lines".to_string();
            }
            LastEdit::ToggleCase => {
                self.toggle_case_selection();
                self.status = "repeated: toggle case".to_string();
            }
            LastEdit::Indent => {
                self.indent_selection(true);
                self.status = "repeated: indent".to_string();
            }
            LastEdit::Dedent => {
                self.dedent_selection();
                self.status = "repeated: dedent".to_string();
            }
            LastEdit::DuplicateDown => {
                self.duplicate_selection_down();
                self.status = "repeated: duplicate".to_string();
            }
        }
    }

    fn insert_all_selections(&mut self, text: &str) {
        let mut positions: Vec<usize> = self
            .buffer
            .selections()
            .selections()
            .iter()
            .map(|selection| selection.head)
            .collect();
        positions.sort_unstable();
        for idx in positions.into_iter().rev() {
            let edit = self.buffer.insert_with_edit(idx, text);
            self.after_buffer_edit(&edit);
        }
        self.sync_selection_to_cursor();
        self.dirty = true;
        self.status = format!(
            "inserted into {} selections",
            self.buffer.selections().selections().len()
        );
        self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
    }

    fn paste_at(&mut self, idx: usize, _before: bool) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let text = self.registers.get(self.active_register).to_string();
        if text.is_empty() {
            return;
        }
        if self.buffer.selections().selections().len() > 1 {
            let mut positions: Vec<usize> = self
                .buffer
                .selections()
                .selections()
                .iter()
                .map(|selection| selection.end())
                .collect();
            positions.sort_unstable();
            for pos in positions.into_iter().rev() {
                let edit = self.buffer.insert_with_edit(pos, &text);
                self.after_buffer_edit(&edit);
            }
            self.sync_selection_to_cursor();
            self.dirty = true;
            self.status = format!(
                "pasted into {} selections",
                self.buffer.selections().selections().len()
            );
            self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
            return;
        }
        let edit = self.buffer.insert_with_edit(idx, &text);
        let after = idx + text.chars().count();
        self.history.push_with_cursor(
            HistoryEntry::Insert {
                idx,
                text: text.clone(),
            },
            CursorSnapshot::cursor(idx),
            CursorSnapshot::cursor(after),
        );
        let (row, col) = self.buffer.char_to_line_col(after);
        self.cursor.row = row;
        self.cursor.col = col;
        self.sync_selection_to_cursor();
        self.dirty = true;
        self.last_edit_position = idx;
        self.after_buffer_edit(&edit);
        self.dispatch_plugin_event(PluginEvent::CursorMove(self.buffer_snapshot()));
    }

    fn perform_undo(&mut self) {
        let Some((entry, snapshot)) = self.history.undo_step() else {
            self.status = "undo: nothing to undo".to_string();
            return;
        };
        actions::apply_history_backward(&mut self.buffer, &entry);
        actions::restore_selections(&mut self.buffer, &snapshot.selections);
        self.selection_to_cursor();
        self.dirty = true;
        self.status = "undo".to_string();
    }

    fn perform_redo(&mut self) {
        let Some((entry, snapshot)) = self.history.redo_step() else {
            self.status = "redo: nothing to redo".to_string();
            return;
        };
        actions::apply_history_forward(&mut self.buffer, &entry);
        actions::restore_selections(&mut self.buffer, &snapshot.selections);
        self.selection_to_cursor();
        self.dirty = true;
        self.status = "redo".to_string();
    }

    fn push_jump(&mut self) {
        let position = self.buffer.selections().primary().head;
        self.jump_list.push(Jump {
            file: self.filepath.clone(),
            position,
        });
    }

    fn jump_backward(&mut self) {
        if let Some(jump) = self.jump_list.backward().cloned() {
            self.goto_jump(jump);
        }
    }

    fn jump_forward(&mut self) {
        if let Some(jump) = self.jump_list.forward().cloned() {
            self.goto_jump(jump);
        }
    }

    fn goto_jump(&mut self, jump: Jump) {
        if let Some(path) = jump.file {
            if self.filepath.as_ref() != Some(&path) {
                let _ = self.open_path(path);
            }
        }
        self.goto_char_index(jump.position);
        self.status = "jump".to_string();
    }

    fn goto_char_index(&mut self, index: usize) {
        let selection = Selection::cursor(index.min(self.buffer.len_chars()));
        self.buffer.selections_mut().set_primary(selection);
        self.selection_to_cursor();
        self.view.ensure_cursor_visible(&self.buffer, &self.cursor);
    }

    fn begin_search(&mut self, forward: bool) {
        self.search_forward = forward;
        self.command_buffer.clear();
        self.search_matches.clear();
        self.search_match_index = 0;
        self.mode = Mode::Search;
    }

    fn refresh_search_matches(&mut self) {
        self.search_pattern = self.command_buffer.clone();
        if self.search_pattern.is_empty() {
            self.search_matches.clear();
            self.search_match_index = 0;
            return;
        }
        let text = self.buffer.to_string();
        let matches: Vec<usize> = search::search_in_bytes(text.as_bytes(), &self.search_pattern)
            .into_iter()
            .map(|byte| motions::char_index_from_byte(&text, byte))
            .collect();
        self.search_matches = if self.search_forward {
            matches
        } else {
            matches.into_iter().rev().collect()
        };
        self.search_match_index = 0;
    }

    fn move_selection_paragraph_forward(&mut self, extend: bool, count: usize) {
        for _ in 0..count {
            let current = self.buffer.selections().primary().head;
            let (row, _) = self.buffer.char_to_line_col(current);
            let last = self.buffer.len_lines().saturating_sub(1);
            if row >= last {
                break;
            }
            let mut target = row + 1;
            while target < last && self.buffer.line_char_len(target) > 0 {
                target += 1;
            }
            let next = self.buffer.char_idx(target, 0);
            self.update_primary_selection(next, extend);
        }
    }

    fn move_selection_paragraph_backward(&mut self, extend: bool, count: usize) {
        for _ in 0..count {
            let current = self.buffer.selections().primary().head;
            let (row, _) = self.buffer.char_to_line_col(current);
            if row == 0 {
                break;
            }
            let mut target = row;
            while target > 0 {
                target -= 1;
                if self.buffer.line_char_len(target) == 0 {
                    break;
                }
            }
            let next = self.buffer.char_idx(target, 0);
            self.update_primary_selection(next, extend);
        }
    }

    fn find_matching_bracket_pos(&self) -> Option<usize> {
        let pos = self.buffer.selections().primary().head;
        let ch = self.buffer.slice_chars(pos, pos.saturating_add(1));
        let pair = match ch.chars().next().unwrap_or(' ') {
            '(' => ('(', ')', true),
            ')' => (')', '(', false),
            '[' => ('[', ']', true),
            ']' => (']', '[', false),
            '{' => ('{', '}', true),
            '}' => ('}', '{', false),
            _ => return None,
        };
        let (open, close, forward) = pair;
        let len = self.buffer.len_chars();
        let mut depth = 1usize;
        if forward {
            let mut i = pos.saturating_add(1);
            while i < len {
                let c = self
                    .buffer
                    .slice_chars(i, i.saturating_add(1))
                    .chars()
                    .next()
                    .unwrap_or(' ');
                if c == close {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                } else if c == open {
                    depth += 1;
                }
                i += 1;
            }
        } else {
            let mut i = pos;
            while i > 0 {
                i -= 1;
                let c = self
                    .buffer
                    .slice_chars(i, i.saturating_add(1))
                    .chars()
                    .next()
                    .unwrap_or(' ');
                if c == close {
                    depth += 1;
                } else if c == open {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    fn move_to_matching_bracket(&mut self, extend: bool) {
        if let Some(pos) = self.find_matching_bracket_pos() {
            self.update_primary_selection(pos, extend);
        }
    }

    fn scroll_view_up(&mut self, rows: usize) {
        self.view.top_line = self.view.top_line.saturating_sub(rows);
        if self.cursor.row >= self.view.top_line + self.view.height {
            self.cursor.row = self.view.top_line + self.view.height.saturating_sub(1);
            self.cursor.clamp_col(&self.buffer);
        }
    }

    fn scroll_view_down(&mut self, rows: usize) {
        let height = self.view.height.max(1);
        let max_top = self.buffer.len_lines().saturating_sub(height);
        self.view.top_line = (self.view.top_line + rows).min(max_top);
        if self.cursor.row < self.view.top_line {
            self.cursor.row = self.view.top_line;
            self.cursor.clamp_col(&self.buffer);
        }
    }

    fn toggle_comment_selection(&mut self) {
        let sel = self.buffer.selections().primary();
        let start = sel.start();
        let end = sel.end();
        let (start_row, _) = self.buffer.char_to_line_col(start);
        let (end_row, _) = self
            .buffer
            .char_to_line_col(end.saturating_sub(1).max(start));
        let mut commented = true;
        for row in start_row..=end_row {
            let line = self.buffer.line_string(row);
            if !line.trim_start().starts_with("//") {
                commented = false;
                break;
            }
        }
        self.history.boundary();
        if commented {
            for row in (start_row..=end_row).rev() {
                let line_start = self.buffer.char_idx(row, 0);
                let prefix = self
                    .buffer
                    .slice_chars(line_start, line_start.saturating_add(3));
                if prefix == "// " {
                    self.buffer.remove(line_start..line_start.saturating_add(3));
                } else {
                    self.buffer.remove(line_start..line_start.saturating_add(2));
                }
            }
        } else {
            for row in (start_row..=end_row).rev() {
                self.buffer.insert(self.buffer.char_idx(row, 0), "// ");
            }
        }
    }

    fn increment_number_at_cursor(&mut self, delta: i64) {
        let pos = self.buffer.selections().primary().head;
        let len = self.buffer.len_chars();
        let text = self.buffer.to_string();
        let text_bytes = text.as_bytes();

        // Find start of number at cursor
        let mut start = pos.min(len);
        while start > 0 {
            let b = text_bytes[start.saturating_sub(1)];
            if !b.is_ascii_digit() && b != b'-' {
                break;
            }
            start -= 1;
        }
        // If we stopped at a '-' that's not actually a negative sign, skip it
        if start > 0
            && text_bytes.get(start) == Some(&b'-')
            && !start
                .checked_sub(1)
                .is_none_or(|p| text_bytes[p].is_ascii_digit())
        {
            start += 1;
        }
        let mut end = pos;
        while end < len {
            let b = text_bytes[end];
            if !b.is_ascii_digit() {
                break;
            }
            end += 1;
        }
        if start == end || start >= len {
            return;
        }

        let num_str = &text[start..end];
        if let Ok(n) = num_str.parse::<i64>() {
            let new_n = n.saturating_add(delta);
            let new_str = new_n.to_string();
            self.buffer.remove(start..end);
            self.buffer.insert(start, &new_str);
            self.buffer
                .selections_mut()
                .set_primary(crate::editor::selection::Selection::cursor(
                    start + new_str.len(),
                ));
            self.selection_to_cursor();
        }
    }

    fn goto_search_match(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            return;
        }
        if forward {
            self.search_match_index = (self.search_match_index + 1) % self.search_matches.len();
        } else if self.search_match_index == 0 {
            self.search_match_index = self.search_matches.len().saturating_sub(1);
        } else {
            self.search_match_index -= 1;
        }
        let idx = self.search_matches[self.search_match_index];
        self.push_jump();
        self.goto_char_index(idx);
        self.buffer
            .selections_mut()
            .set_primary(Selection::cursor(idx));
    }

    fn search_word_under_cursor(&mut self) {
        let current = self.buffer.selections().primary().head;
        let Some((start, end)) = motions::word_under_cursor(&self.buffer, current) else {
            return;
        };
        self.command_buffer = self.buffer.slice_chars(start, end);
        self.search_forward = true;
        self.refresh_search_matches();
        self.mode = Mode::Search;
    }

    fn apply_char_search(&mut self, target: char, pending: MatchPending) {
        let current = self.buffer.selections().primary().head;
        let (backward, mode) = match pending {
            MatchPending::ForwardInclusive => (false, CharSearchMode::Inclusive),
            MatchPending::ForwardExclusive => (false, CharSearchMode::Exclusive),
            MatchPending::BackwardInclusive => (true, CharSearchMode::Inclusive),
            MatchPending::BackwardExclusive => (true, CharSearchMode::Exclusive),
        };
        let found = if backward {
            motions::find_char_backward(&self.buffer, current, target, mode)
        } else {
            motions::find_char_forward(&self.buffer, current, target, mode)
        };
        if let Some(next) = found {
            self.char_search = Some(CharSearchState {
                target,
                backward,
                mode,
            });
            self.update_primary_selection(next, true);
        } else {
            self.status = format!("char '{target}' not found");
        }
    }

    fn repeat_char_search(&mut self, forward: bool) {
        let Some(state) = self.char_search.clone() else {
            return;
        };
        let current = self.buffer.selections().primary().head;
        let found = if forward != state.backward {
            motions::find_char_forward(&self.buffer, current, state.target, state.mode)
        } else {
            motions::find_char_backward(&self.buffer, current, state.target, state.mode)
        };
        if let Some(next) = found {
            self.update_primary_selection(next, true);
        }
    }

    fn select_line_bounds(&mut self) {
        let current = self.buffer.selections().primary().head;
        let start = motions::line_start(&self.buffer, current);
        let end = motions::line_end(&self.buffer, current);
        self.buffer
            .selections_mut()
            .set_primary(Selection::new(start, end));
        self.selection_to_cursor();
    }

    fn move_selection_line_start(&mut self) {
        let current = self.buffer.selections().primary().head;
        let start = motions::line_start(&self.buffer, current);
        self.update_primary_selection(start, false);
    }

    fn toggle_case_selection(&mut self) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let selection = self.buffer.selections().primary();
        let range = if selection.is_cursor() {
            let head = selection.head;
            head..head.saturating_add(1).min(self.buffer.len_chars())
        } else {
            selection.range()
        };
        if range.start >= range.end {
            return;
        }
        let text = self.buffer.slice_chars(range.start, range.end);
        let toggled = actions::toggle_case(&text);
        let _ = self.buffer.remove_with_edit(range.clone());
        let edit = self.buffer.insert_with_edit(range.start, &toggled);
        self.history.push_with_cursor(
            HistoryEntry::Insert {
                idx: range.start,
                text: toggled,
            },
            CursorSnapshot::cursor(range.start),
            CursorSnapshot::cursor(range.start + text.chars().count()),
        );
        self.last_edit = Some(LastEdit::ToggleCase);
        self.last_edit_position = range.start;
        self.dirty = true;
        self.after_buffer_edit(&edit);
    }

    fn indent_selection(&mut self, indent: bool) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let selection = self.buffer.selections().primary();
        let (start_row, _) = self.buffer.char_to_line_col(selection.start());
        let (end_row, _) = self
            .buffer
            .char_to_line_col(selection.end().saturating_sub(1));
        let width = self.language_settings().tab_width.max(1);
        for row in start_row..=end_row {
            let line_start = self.buffer.char_idx(row, 0);
            let line = self.buffer.line_string(row);
            let updated = if indent {
                actions::indent_lines(&line, width)
            } else {
                actions::dedent_lines(&line, width)
            };
            if line == updated {
                continue;
            }
            let line_end = line_start + line.chars().count();
            let _ = self.buffer.remove_with_edit(line_start..line_end);
            let edit = self.buffer.insert_with_edit(line_start, &updated);
            self.after_buffer_edit(&edit);
        }
        self.last_edit = Some(if indent {
            LastEdit::Indent
        } else {
            LastEdit::Dedent
        });
        self.dirty = true;
        self.last_edit_position = self.buffer.selections().primary().head;
    }

    fn dedent_selection(&mut self) {
        self.indent_selection(false);
    }

    fn join_selection_lines(&mut self) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let selection = self.buffer.selections().primary();
        let (start_row, _) = self.buffer.char_to_line_col(selection.start());
        let (end_row, _) = self
            .buffer
            .char_to_line_col(selection.end().saturating_sub(1));
        if start_row >= end_row {
            return;
        }
        let mut parts = Vec::new();
        for row in start_row..=end_row {
            parts.push(self.buffer.line_string(row));
        }
        let replacement = parts.join(" ");
        let start = self.buffer.char_idx(start_row, 0);
        let end = self.buffer.char_idx(end_row, self.buffer.line_len(end_row));
        let _ = self.buffer.remove_with_edit(start..end + 1);
        self.last_edit = Some(LastEdit::JoinLines);
        let edit = self.buffer.insert_with_edit(start, &replacement);
        self.last_edit_position = start;
        self.dirty = true;
        self.after_buffer_edit(&edit);
    }

    fn duplicate_selection_down(&mut self) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let selection = self.buffer.selections().primary();
        let (row, col) = self.buffer.char_to_line_col(selection.head);
        let line_end = self.buffer.char_idx(row, self.buffer.line_len(row));
        let text = if selection.is_cursor() {
            format!("{}\n", self.buffer.line_string(row))
        } else {
            format!(
                "{}\n",
                self.buffer.slice_chars(selection.start(), selection.end())
            )
        };
        let edit = self.buffer.insert_with_edit(line_end, &text);
        let new_row = row.saturating_add(1);
        let new_idx = self.buffer.char_idx(new_row, col);
        self.buffer
            .selections_mut()
            .push_selection(Selection::cursor(new_idx));
        self.selection_to_cursor();
        self.last_edit = Some(LastEdit::DuplicateDown);
        self.last_edit_position = line_end;
        self.dirty = true;
        self.after_buffer_edit(&edit);
    }

    fn load_persistent_history(&mut self) {
        let Some(path) = self.filepath.as_ref() else {
            return;
        };
        let history_path = undo_history_path(path);
        if history_path.exists() {
            if let Ok(history) = History::load_from(&history_path) {
                self.history = history;
            }
        }
    }

    fn persist_persistent_history(&self) {
        let Some(path) = self.filepath.as_ref() else {
            return;
        };
        let history_path = undo_history_path(path);
        let _ = self.history.save_to(&history_path);
    }

    fn open_path(&mut self, path: PathBuf) -> Result<()> {
        self.sync_app_into_manager();
        let id = self.buffers.open(path)?;
        self.splits.focused_mut().buffer = id;
        self.load_current_from_manager();
        self.dispatch_plugin_event(PluginEvent::BufferOpen(self.buffer_snapshot()));
        self.status = "opened buffer".to_string();
        Ok(())
    }

    fn switch_next_buffer(&mut self) {
        self.sync_app_into_manager();
        if let Some(id) = self.buffers.next_buffer() {
            self.splits.focused_mut().buffer = id;
            self.load_current_from_manager();
            self.dispatch_plugin_event(PluginEvent::BufferOpen(self.buffer_snapshot()));
            self.status = "next buffer".to_string();
        }
    }

    fn switch_previous_buffer(&mut self) {
        self.sync_app_into_manager();
        if let Some(id) = self.buffers.previous_buffer() {
            self.splits.focused_mut().buffer = id;
            self.load_current_from_manager();
            self.dispatch_plugin_event(PluginEvent::BufferOpen(self.buffer_snapshot()));
            self.status = "previous buffer".to_string();
        }
    }

    fn close_current_buffer(&mut self) {
        let Some(id) = self.buffers.current().map(|entry| entry.id) else {
            return;
        };
        self.sync_app_into_manager();
        self.buffers.close(id);
        if self.buffers.current().is_none() {
            self.buffers.new_scratch();
        }
        if let Some(current) = self.buffers.current() {
            self.splits.focused_mut().buffer = current.id;
        }
        self.load_current_from_manager();
        self.dispatch_plugin_event(PluginEvent::BufferOpen(self.buffer_snapshot()));
        self.status = "closed buffer".to_string();
    }

    fn sync_app_into_manager(&mut self) {
        if let Some(entry) = self.buffers.current_mut() {
            entry.buffer = self.buffer.clone();
            entry.cursor = self.cursor;
            entry.modified = self.dirty;
        }
    }

    fn load_current_from_manager(&mut self) {
        if let Some(entry) = self.buffers.current() {
            self.buffer = entry.buffer.clone();
            self.filepath = entry.path.clone();
            self.cursor = entry.cursor;
            self.dirty = entry.modified;
            self.read_only = self.session_read_only || entry.read_only;
            self.view.ensure_cursor_visible(&self.buffer, &self.cursor);
            self.sync_selection_to_cursor();
            self.history = History::new();
            self.load_persistent_history();
            self.refresh_subsystems_for_current_buffer();
            self.refresh_git_gutter();
            if self.git_blame_visible {
                self.refresh_git_blame();
            }
        }
    }

    fn refresh_git_gutter(&mut self) {
        let Some(path) = self.filepath.clone() else {
            self.git_marks.clear();
            self.git_hunks.clear();
            self.git_branch = None;
            return;
        };
        match git::status_for_file(&self.project_root(), &path) {
            Some(status) => {
                self.git_marks = status.marks;
                self.git_hunks = status.hunks;
                self.git_branch = status.branch;
            }
            None => {
                self.git_marks.clear();
                self.git_hunks.clear();
                self.git_branch = None;
            }
        }
        if self.git_blame_visible {
            self.refresh_git_blame();
        }
    }

    fn toggle_git_blame(&mut self) {
        self.git_blame_visible = !self.git_blame_visible;
        if self.git_blame_visible {
            self.refresh_git_blame();
            let count = self.git_blame.len();
            self.status = format!("git blame on ({count} lines)");
        } else {
            self.git_blame.clear();
            self.status = "git blame off".to_string();
        }
    }

    fn refresh_git_blame(&mut self) {
        if !self.git_blame_visible {
            return;
        }
        let Some(path) = self.filepath.clone() else {
            self.git_blame.clear();
            return;
        };
        self.git_blame = git::blame_for_file(&self.project_root(), &path).unwrap_or_default();
    }

    fn with_git_blame_annotations(&self, lines: Vec<String>) -> Vec<String> {
        if !self.git_blame_visible {
            return lines;
        }
        lines
            .into_iter()
            .enumerate()
            .map(|(visible_idx, line)| {
                let line_no = self.view.top_line + visible_idx;
                let Some(blame) = self.git_blame.get(&line_no) else {
                    return line;
                };
                format!("{line}\x1b[90m  {}\x1b[0m", git::format_annotation(blame))
            })
            .collect()
    }

    fn blame_status_for_cursor(&self) -> Option<String> {
        let blame = self.git_blame.get(&self.cursor.row)?;
        Some(git::format_annotation(blame))
    }

    fn goto_git_hunk(&mut self, direction: i32) {
        let Some(hunk) = git::adjacent_hunk(&self.git_hunks, self.cursor.row, direction).copied()
        else {
            self.status = "no git hunks".to_string();
            return;
        };
        self.push_jump();
        self.cursor.row = hunk.start_line;
        self.cursor.col = 0;
        self.sync_selection_to_cursor();
        self.view.ensure_cursor_visible(&self.buffer, &self.cursor);
        let kind = match hunk.status {
            LineStatus::Added => "added",
            LineStatus::Modified => "modified",
        };
        self.status = format!(
            "git hunk {kind} {}:{}-{}",
            hunk.start_line + 1,
            hunk.start_line + 1,
            hunk.end_line + 1
        );
    }

    fn enter_prefix_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.prefix_mode_since = Some(Instant::now());
        self.which_key_visible = mode == Mode::Space;
    }

    fn tick_which_key(&mut self) {
        match self.mode {
            Mode::Space => self.which_key_visible = true,
            Mode::Goto | Mode::View | Mode::Match => {
                if let Some(since) = self.prefix_mode_since {
                    self.which_key_visible = since.elapsed() >= Duration::from_millis(500);
                }
            }
            _ => {
                self.which_key_visible = false;
                self.prefix_mode_since = None;
            }
        }
    }

    fn with_which_key_overlay(&self, mut lines: Vec<String>) -> Vec<String> {
        if !self.which_key_visible {
            return lines;
        }
        let entries = self.which_key_entries_for_mode();
        if entries.is_empty() {
            return lines;
        }
        let mut overlay = vec!["─ which-key ─".to_string(), whichkey::render(&entries)];
        let budget = self.view.height.saturating_sub(1);
        if overlay.len() + lines.len() > budget {
            let keep = budget.saturating_sub(overlay.len());
            lines.truncate(keep);
        }
        overlay.extend(lines);
        overlay
    }

    fn with_undo_tree_overlay(&self, lines: Vec<String>) -> Vec<String> {
        if !self.undo_tree_visible {
            return lines;
        }
        let tree = self.undo_tree_lines();
        let budget = lines.len();
        let mut result: Vec<String> = tree.iter().take(budget).cloned().collect();
        while result.len() < budget {
            result.push(String::new());
        }
        result
    }

    fn which_key_entries_for_mode(&self) -> Vec<WhichKeyEntry> {
        let defaults = match self.mode {
            Mode::Space => vec![
                WhichKeyEntry {
                    key: "f".into(),
                    label: "files".into(),
                },
                WhichKeyEntry {
                    key: "b".into(),
                    label: "buffers".into(),
                },
                WhichKeyEntry {
                    key: "d".into(),
                    label: "diagnostics".into(),
                },
                WhichKeyEntry {
                    key: "g".into(),
                    label: "grep".into(),
                },
                WhichKeyEntry {
                    key: "S".into(),
                    label: "symbols".into(),
                },
                WhichKeyEntry {
                    key: "s".into(),
                    label: "save".into(),
                },
                WhichKeyEntry {
                    key: "q".into(),
                    label: "quit".into(),
                },
            ],
            Mode::Goto => vec![
                WhichKeyEntry {
                    key: "g".into(),
                    label: "top".into(),
                },
                WhichKeyEntry {
                    key: "e".into(),
                    label: "end".into(),
                },
                WhichKeyEntry {
                    key: "h".into(),
                    label: "line start".into(),
                },
                WhichKeyEntry {
                    key: "l".into(),
                    label: "line end".into(),
                },
                WhichKeyEntry {
                    key: ".".into(),
                    label: "last edit".into(),
                },
                WhichKeyEntry {
                    key: "d".into(),
                    label: "definition".into(),
                },
                WhichKeyEntry {
                    key: "y".into(),
                    label: "type".into(),
                },
                WhichKeyEntry {
                    key: "r".into(),
                    label: "references".into(),
                },
                WhichKeyEntry {
                    key: "i".into(),
                    label: "implementation".into(),
                },
            ],
            Mode::View => vec![
                WhichKeyEntry {
                    key: "s".into(),
                    label: "hsplit".into(),
                },
                WhichKeyEntry {
                    key: "v".into(),
                    label: "vsplit".into(),
                },
                WhichKeyEntry {
                    key: "w".into(),
                    label: "next split".into(),
                },
                WhichKeyEntry {
                    key: "q".into(),
                    label: "close split".into(),
                },
                WhichKeyEntry {
                    key: "o".into(),
                    label: "only".into(),
                },
            ],
            Mode::Match => vec![WhichKeyEntry {
                key: "char".into(),
                label: "target".into(),
            }],
            _ => Vec::new(),
        };
        let config_mode = match self.mode {
            Mode::Space => Some("space"),
            Mode::Goto => Some("goto"),
            Mode::View => Some("view"),
            Mode::Match => Some("match"),
            _ => None,
        };
        let Some(config_mode) = config_mode else {
            return defaults;
        };
        let config_table = if self.mode == Mode::Space && !self.config_space_bindings.is_empty() {
            self.config_space_bindings.clone()
        } else {
            BindingTable::from_config(&self.config, config_mode)
        };
        if config_table.is_empty() {
            return defaults;
        }
        merge_which_key_entries(defaults, config_table.which_key_entries())
    }

    fn open_file_picker(&mut self) -> Result<()> {
        self.sync_app_into_manager();
        self.picker_kind = Some(ActivePicker::Files);
        self.picker_root = self.project_root();
        self.command_buffer.clear();
        self.refresh_file_picker()?;
        self.mode = Mode::Picker;
        Ok(())
    }

    fn open_buffer_picker(&mut self) {
        self.sync_app_into_manager();
        self.picker_kind = Some(ActivePicker::Buffers);
        self.command_buffer.clear();
        self.refresh_buffer_picker();
        self.mode = Mode::Picker;
    }

    fn open_diagnostics_picker(&mut self) {
        self.picker_kind = Some(ActivePicker::Diagnostics);
        self.command_buffer.clear();
        self.refresh_diagnostics_picker();
        self.mode = Mode::Picker;
    }

    fn open_git_diff_picker(&mut self) {
        let Some(path) = self.filepath.clone() else {
            self.status = "git diff: no file path".to_string();
            return;
        };
        match git::file_diff_lines(&self.project_root(), &path) {
            Ok(lines) if lines.is_empty() => {
                self.status = "git diff: no changes".to_string();
            }
            Ok(_) => {
                self.picker_kind = Some(ActivePicker::GitDiff);
                self.command_buffer.clear();
                self.refresh_git_diff_picker();
                self.mode = Mode::Picker;
            }
            Err(err) => self.status = format!("git diff: {err}"),
        }
    }

    fn refresh_git_diff_picker(&mut self) {
        let Some(path) = self.filepath.clone() else {
            self.picker_items.clear();
            return;
        };
        let query = self.command_buffer.to_lowercase();
        self.picker_items = git::file_diff_lines(&self.project_root(), &path)
            .unwrap_or_default()
            .into_iter()
            .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
            .map(AppPickerItem::plain)
            .collect();
        if self.picker_items.is_empty() {
            self.picker_items
                .push(AppPickerItem::plain("no diff lines match filter"));
        }
    }

    fn open_theme_picker(&mut self) {
        self.picker_kind = Some(ActivePicker::Themes);
        self.command_buffer.clear();
        self.refresh_theme_picker();
        self.mode = Mode::Picker;
    }

    fn open_symbol_picker(&mut self) {
        self.picker_kind = Some(ActivePicker::Symbols);
        self.command_buffer.clear();
        let Some(handle) = self.lsp_request_handle() else {
            self.picker_items = vec![AppPickerItem::plain(
                "document symbols require an active LSP server",
            )];
            self.mode = Mode::Picker;
            return;
        };
        let Some(path) = self.filepath.clone() else {
            self.status = "lsp: no file path for symbols".to_string();
            return;
        };
        if !self.async_runtime_available() {
            self.status = "lsp: async runtime unavailable".to_string();
            return;
        }
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let result = handle
                .document_symbols(&path)
                .await
                .map_err(|err| err.to_string());
            let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Symbols(result)));
        });
        self.status = "lsp: queued symbols".to_string();
    }

    fn open_grep_picker(&mut self) {
        self.picker_kind = Some(ActivePicker::Grep);
        self.command_buffer.clear();
        self.picker_items.clear();
        self.mode = Mode::Picker;
    }

    fn open_tutor(&mut self) {
        self.tutor_visible = true;
        self.status = "tutor: overlay shown (Esc to dismiss)".to_string();
    }

    fn open_help_picker(&mut self) {
        self.picker_kind = Some(ActivePicker::Help);
        self.command_buffer.clear();
        self.refresh_help_picker();
        self.mode = Mode::Picker;
        self.status = "command help".to_string();
    }

    fn refresh_help_picker(&mut self) {
        self.picker_items = commands::help_entries(&self.command_buffer, 64)
            .into_iter()
            .map(AppPickerItem::plain)
            .collect();
        self.picker_selected = self
            .picker_selected
            .min(self.picker_items.len().saturating_sub(1));
    }

    fn apply_surround(&mut self, open: char) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let selection = self.buffer.selections().primary();
        let start = selection.start();
        let end = selection.end();
        if start >= end {
            self.status = "surround: empty selection".to_string();
            return;
        }
        if surround::pair_chars(open).is_none() {
            self.status = format!("surround: unsupported delimiter '{open}'");
            return;
        }
        if let Some((text, replace_start, replace_end)) =
            surround::change_surrounding(&self.buffer, start, end, open)
        {
            if let Some(edit) = self.buffer.remove_with_edit(replace_start..replace_end) {
                self.after_buffer_edit(&edit);
            }
            let edit = self.buffer.insert_with_edit(replace_start, &text);
            self.after_buffer_edit(&edit);
            self.dirty = true;
            self.status = format!("surround: changed to {open}");
            return;
        }
        let Some((text, _, _)) = surround::wrap_range(&self.buffer, start, end, open) else {
            self.status = "surround: failed".to_string();
            return;
        };
        if let Some(edit) = self.buffer.remove_with_edit(start..end) {
            self.after_buffer_edit(&edit);
        }
        let edit = self.buffer.insert_with_edit(start, &text);
        self.after_buffer_edit(&edit);
        self.dirty = true;
        self.status = format!("surround: wrapped with {open}");
    }

    fn delete_surround(&mut self) {
        if self.read_only {
            self.status = "read-only".to_string();
            return;
        }
        let selection = self.buffer.selections().primary();
        let start = selection.start();
        let end = selection.end();
        if start >= end {
            self.status = "delsurround: empty selection".to_string();
            return;
        }
        let Some(pair) = surround::delete_surrounding(&self.buffer, start, end) else {
            self.status = "delsurround: no surrounding pair".to_string();
            return;
        };
        if let Some(edit) = self
            .buffer
            .remove_with_edit(pair.close_index..pair.close_index + 1)
        {
            self.after_buffer_edit(&edit);
        }
        if let Some(edit) = self
            .buffer
            .remove_with_edit(pair.open_index..pair.open_index + 1)
        {
            self.after_buffer_edit(&edit);
        }
        self.dirty = true;
        self.status = "delsurround: removed".to_string();
    }

    fn refresh_picker(&mut self) -> Result<()> {
        match self.picker_kind {
            Some(ActivePicker::Files) => self.refresh_file_picker()?,
            Some(ActivePicker::Buffers) => self.refresh_buffer_picker(),
            Some(ActivePicker::Diagnostics) => {
                self.refresh_diagnostics_picker();
            }
            Some(ActivePicker::GitDiff) => self.refresh_git_diff_picker(),
            Some(ActivePicker::Help) => self.refresh_help_picker(),
            Some(ActivePicker::Plugins) => self.refresh_plugin_picker(),
            Some(ActivePicker::Themes) => self.refresh_theme_picker(),
            Some(ActivePicker::Grep) => self.refresh_grep_picker()?,
            Some(ActivePicker::Symbols)
            | Some(ActivePicker::Locations)
            | Some(ActivePicker::CodeActions) => {}
            None => {}
        }
        self.picker_selected = self
            .picker_selected
            .min(self.picker_items.len().saturating_sub(1));
        Ok(())
    }

    fn refresh_file_picker(&mut self) -> Result<()> {
        self.picker_items = picker::fuzzy_files(
            &self.picker_root,
            &self.command_buffer,
            32,
            self.view.height.max(10),
        )?
        .into_iter()
        .map(|item| AppPickerItem::path(item.display, item.path))
        .collect();
        self.picker_selected = 0;
        Ok(())
    }

    fn refresh_buffer_picker(&mut self) {
        let query = self.command_buffer.to_lowercase();
        let mut items = Vec::new();
        for entry in self.buffers.buffers() {
            let label = entry
                .path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "[scratch]".to_string());
            if fuzzy_match(&label, &query) {
                items.push(AppPickerItem::buffer(label, entry.id));
            }
        }
        items.sort_by(|a, b| {
            a.label
                .len()
                .cmp(&b.label.len())
                .then_with(|| a.label.cmp(&b.label))
        });
        self.picker_items = items;
        self.picker_selected = 0;
    }

    fn refresh_diagnostics_picker(&mut self) {
        let theme = self.themes.active();
        let max_label = self.view.width.saturating_sub(8).max(32);
        let query = self.command_buffer.to_lowercase();
        self.picker_items = self
            .lsp_diagnostics
            .iter()
            .enumerate()
            .filter(|(_, diagnostic)| {
                query.is_empty() || diagnostic.message.to_lowercase().contains(&query)
            })
            .map(|(idx, diagnostic)| AppPickerItem {
                label: diagnostics::picker_label(diagnostic, theme, max_label),
                path: None,
                buffer_id: Some(BufferId(idx)),
                row: None,
                col: None,
                code_action: None,
            })
            .collect();
        if self.picker_items.is_empty() {
            self.picker_items
                .push(AppPickerItem::plain("no diagnostics"));
        }
        self.picker_selected = 0;
    }

    fn refresh_theme_picker(&mut self) {
        let query = self.command_buffer.to_lowercase();
        self.picker_items = self
            .themes
            .names()
            .into_iter()
            .filter(|name| name.to_lowercase().contains(&query))
            .map(AppPickerItem::plain)
            .collect();
        self.picker_selected = 0;
    }

    fn refresh_grep_picker(&mut self) -> Result<()> {
        let query = self.command_buffer.clone();
        let root = self.project_root();
        let limit = self.view.height.max(16);
        let mut items = Vec::new();
        if !query.is_empty() {
            for hit in grep::grep_project(&root, &query, limit) {
                let display = hit
                    .path
                    .strip_prefix(&root)
                    .unwrap_or(&hit.path)
                    .to_string_lossy()
                    .replace('\\', "/");
                items.push(AppPickerItem {
                    label: format!("{display}:{}: {}", hit.line + 1, hit.text.trim()),
                    path: Some(hit.path),
                    buffer_id: None,
                    row: Some(hit.line),
                    col: Some(hit.column),
                    code_action: None,
                });
            }
        }
        self.picker_items = items;
        self.picker_selected = 0;
        Ok(())
    }

    fn bufferline_line(&self) -> String {
        let current = self.buffers.current().map(|entry| entry.id);
        let tabs: Vec<BufferTab> = self
            .buffers
            .buffers()
            .iter()
            .map(|entry| BufferTab {
                label: bufferline::label_for_path(entry.path.as_deref()),
                active: current == Some(entry.id),
                modified: entry.modified,
            })
            .collect();
        bufferline::render_themed(&tabs, self.view.width, self.themes.active())
    }

    fn with_tutor_overlay(&self, lines: Vec<String>) -> Vec<String> {
        if !self.tutor_visible {
            return lines;
        }
        let mut overlay = tutor::overlay_lines(self.view.height.min(10));
        overlay.extend(lines);
        overlay.truncate(self.view.height);
        overlay
    }

    fn accept_picker_item(&mut self) -> Result<()> {
        let item = self.picker_items.get(self.picker_selected).cloned();
        let kind = self.picker_kind;
        self.close_picker();
        if let Some(item) = item {
            if kind == Some(ActivePicker::Themes) {
                self.set_theme(&item.label);
            } else if kind == Some(ActivePicker::Locations) || kind == Some(ActivePicker::Symbols) {
                if let Some(path) = item.path {
                    let _ = self.open_path(path);
                }
                if let (Some(row), Some(col)) = (item.row, item.col) {
                    self.cursor.row = row;
                    self.cursor.col = col;
                    self.sync_selection_to_cursor();
                }
            } else if kind == Some(ActivePicker::CodeActions) {
                if let Some(action) = item.code_action {
                    let edits = lsp_ui::text_edits_from_action(&action, self.filepath.as_deref());
                    if edits.is_empty() {
                        self.status = format!("code action: {} (no local edits)", action.title);
                    } else {
                        let applied = lsp_ui::apply_text_edits(&mut self.buffer, &edits);
                        for edit in &applied {
                            self.after_buffer_edit(edit);
                        }
                        self.dirty = true;
                        self.status =
                            format!("code action: {} ({} edits)", action.title, edits.len());
                    }
                }
            } else if kind == Some(ActivePicker::Diagnostics) {
                if let Some(BufferId(idx)) = item.buffer_id {
                    if let Some(diagnostic) = self.lsp_diagnostics.get(idx) {
                        self.cursor.row = diagnostic.range.start.line as usize;
                        self.cursor.col = diagnostic.range.start.character as usize;
                        self.sync_selection_to_cursor();
                    }
                }
            } else if kind == Some(ActivePicker::Help) {
                let command = item
                    .label
                    .split('—')
                    .next()
                    .unwrap_or(item.label.as_str())
                    .trim()
                    .to_string();
                if !command.is_empty() {
                    self.execute_command(&command)?;
                }
            } else if kind == Some(ActivePicker::Plugins) {
                if let Some(command) = item.label.strip_prefix('@') {
                    let command = command.split('—').next().unwrap_or(command).trim();
                    self.execute_command(command)?;
                } else {
                    self.status = format!("plugin: {}", item.label);
                }
            } else if let Some(path) = item.path {
                self.open_path(path)?;
            } else if let Some(id) = item.buffer_id {
                self.sync_app_into_manager();
                if self.buffers.switch_to(id) {
                    self.splits.focused_mut().buffer = id;
                    self.load_current_from_manager();
                    self.status = "switched buffer".to_string();
                }
            }
        }
        Ok(())
    }

    fn close_picker(&mut self) {
        self.mode = Mode::Normal;
        self.picker_kind = None;
        self.picker_items.clear();
        self.picker_selected = 0;
        self.command_buffer.clear();
    }

    fn picker_lines(&self) -> Vec<String> {
        let title = match self.picker_kind {
            Some(ActivePicker::Files) => format!("Files in {}", self.picker_root.display()),
            Some(ActivePicker::Buffers) => "Buffers".to_string(),
            Some(ActivePicker::Diagnostics) => "Diagnostics".to_string(),
            Some(ActivePicker::Themes) => "Themes".to_string(),
            Some(ActivePicker::Symbols) => "Symbols".to_string(),
            Some(ActivePicker::Grep) => "Grep".to_string(),
            Some(ActivePicker::Locations) => "Locations".to_string(),
            Some(ActivePicker::CodeActions) => "Code Actions".to_string(),
            Some(ActivePicker::GitDiff) => {
                let path = self
                    .filepath
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "buffer".to_string());
                format!("Git diff: {path}")
            }
            Some(ActivePicker::Help) => "Commands".to_string(),
            Some(ActivePicker::Plugins) => "Plugins".to_string(),
            None => "Picker".to_string(),
        };
        let preview_rows = match self.picker_kind {
            Some(ActivePicker::Files) => 6,
            _ => 0,
        };
        let list_budget = self.view.height.saturating_sub(4 + preview_rows).max(4);
        let mut lines = vec![title, format!("> {}", self.command_buffer), String::new()];
        for (idx, item) in self.picker_items.iter().take(list_budget).enumerate() {
            let marker = if idx == self.picker_selected {
                ">"
            } else {
                " "
            };
            lines.push(format!("{} {}", marker, item.label));
        }
        if preview_rows > 0 {
            if let Some(item) = self.picker_items.get(self.picker_selected) {
                if let Some(path) = &item.path {
                    lines.push(String::new());
                    lines.push("── preview ──".to_string());
                    for line in picker::preview_file_lines(path, preview_rows.saturating_sub(1)) {
                        lines.push(format!("  {line}"));
                    }
                }
            }
        }
        lines
    }

    fn project_root(&self) -> PathBuf {
        self.filepath
            .as_ref()
            .and_then(|path| path.parent().map(PathBuf::from))
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn language_settings(&self) -> config::EffectiveLanguageSettings {
        let language_name = grammars::language_spec(self.language).map(|spec| spec.name);
        self.config.effective_for_language(language_name)
    }

    fn refresh_subsystems_for_current_buffer(&mut self) {
        self.language = self.detect_current_language();
        let settings = self.language_settings();
        self.enable_lsp = settings.lsp;
        self.enable_highlight = settings.highlight;
        self.highlighter = self
            .enable_highlight
            .then(|| TreeSitterHighlighter::new(self.language));
        self.lsp_server = self
            .filepath
            .as_deref()
            .and_then(servers::server_definition_for_path)
            .filter(|_| self.enable_lsp);
        self.lsp_client = self
            .lsp_server
            .map(|server| LspClient::new(self.project_root(), server.binary));
        self.lsp_diagnostics.clear();
        self.lsp_ui = LspUiState::default();
        self.lsp_document_open = None;
        self.lsp_start_pending = self.enable_lsp && self.lsp_client.is_some();
        self.tree_parse_generation = 0;
        self.tree_parse_in_flight = false;
        self.tree_parse_dirty = self.enable_highlight && self.highlighter.is_some();
        self.dispatch_plugin_buffer_open();
    }

    fn dispatch_plugin_buffer_open(&mut self) {
        self.ensure_plugins_discovered();
        self.plugin_manager.clear_ephemeral();
        self.dispatch_plugin_event(PluginEvent::BufferOpen(self.buffer_snapshot()));
    }

    fn maybe_dispatch_mode_change(&mut self, before: Mode) {
        if before == self.mode {
            return;
        }
        self.dispatch_plugin_event(PluginEvent::ModeChange {
            from: mode_plugin_label(before).to_string(),
            to: mode_plugin_label(self.mode).to_string(),
        });
    }

    fn ensure_plugins_discovered(&mut self) {
        if self.plugins_discovered {
            return;
        }
        if self.plugin_manager.discover().is_ok() {
            self.plugins_discovered = true;
            self.rebuild_config_bindings();
        }
    }

    fn detect_current_language(&self) -> Language {
        let Some(path) = self.filepath.as_deref() else {
            return Language::PlainText;
        };
        let first_line = self.buffer.line_string(0);
        let first_line = (!first_line.is_empty()).then_some(first_line.as_str());
        TreeSitterHighlighter::detect(path, first_line)
    }

    fn highlight_visible_lines(&mut self) -> Vec<String> {
        let visible_start = self.view.top_line;
        let visible_count = self.view.height;
        let lines = self.buffer.visible_lines(visible_start, visible_count);
        if !self.enable_highlight || lines.is_empty() {
            return lines;
        }

        let Some(highlighter) = &mut self.highlighter else {
            return lines;
        };

        let overscan = TreeSitterHighlighter::viewport_overscan_lines();
        let visible_end = visible_start.saturating_add(lines.len());
        let window_start = visible_start.saturating_sub(overscan);
        let window_end = visible_end.saturating_add(overscan);
        let window_lines = self
            .buffer
            .visible_lines(window_start, window_end.saturating_sub(window_start));
        let highlighted = highlighter.highlight_visible_window(
            &window_lines,
            window_start,
            visible_start..visible_end,
            overscan,
        );

        let mut spans_by_line: Vec<Vec<HighlightSpan>> = vec![Vec::new(); lines.len()];
        for (line_idx, span) in highlighted.spans {
            if (visible_start..visible_end).contains(&line_idx) {
                spans_by_line[line_idx - visible_start].push(span);
            }
        }
        for token in &self.lsp_semantic_tokens {
            if token.line < visible_start || token.line >= visible_end {
                continue;
            }
            let idx = token.line - visible_start;
            spans_by_line[idx].push(HighlightSpan {
                start: token.start,
                end: token.start + token.len,
                group: token.group,
            });
        }

        let theme = self.themes.active().clone();
        lines
            .into_iter()
            .zip(spans_by_line.iter())
            .map(|(line, spans)| Theme::highlighted_line_with_theme(&line, spans, &theme))
            .collect()
    }

    fn with_document_highlights(&self, lines: Vec<String>) -> Vec<String> {
        if self.lsp_document_highlights.is_empty() {
            return lines;
        }
        let theme = self.themes.active();
        let mark = theme.ansi_for_theme_key("selection", false);
        let reset = "\x1b[0m";
        lines
            .into_iter()
            .enumerate()
            .map(|(visible_idx, line)| {
                let line_no = self.view.top_line + visible_idx;
                let highlighted = self.lsp_document_highlights.iter().any(|range| {
                    let start = range.start.line as usize;
                    let end = range.end.line as usize;
                    (start..=end).contains(&line_no)
                });
                if highlighted {
                    format!("{mark}{line}{reset}")
                } else {
                    line
                }
            })
            .collect()
    }

    fn with_search_highlights(&self, lines: Vec<String>) -> Vec<String> {
        if !self.search_hl_visible
            || self.search_matches.is_empty()
            || self.search_pattern.is_empty()
        {
            return lines;
        }
        let theme = self.themes.active();
        let hl = theme.ansi_for_theme_key("search.highlight", false);
        let reset = "\x1b[0m";
        let pattern_len = self.search_pattern.chars().count();
        if pattern_len == 0 {
            return lines;
        }

        // Build per-line match ranges for the visible window
        let top = self.view.top_line;
        let mut line_matches: Vec<Vec<(usize, usize)>> = vec![Vec::new(); lines.len()];
        for &start in &self.search_matches {
            let (row, col) = self.buffer.char_to_line_col(start);
            if row >= top && row < top + lines.len() {
                let end_col = (col + pattern_len).min(self.buffer.line_char_len(row));
                line_matches[row - top].push((col, end_col));
            }
        }

        lines
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                let matches = &line_matches[idx];
                if matches.is_empty() {
                    return line;
                }
                let mut sorted = matches.clone();
                sorted.sort_by_key(|m| m.0);
                let mut out = String::with_capacity(line.len() + sorted.len() * 20);
                let mut cursor = 0usize;
                for &(start, end) in &sorted {
                    if start >= line.len() || start < cursor {
                        continue;
                    }
                    out.push_str(&line[cursor..start]);
                    out.push_str(&hl);
                    out.push_str(&line[start..end]);
                    out.push_str(reset);
                    cursor = end;
                }
                out.push_str(&line[cursor..]);
                out
            })
            .collect()
    }

    fn with_cursorline_highlight(&self, lines: Vec<String>) -> Vec<String> {
        let cursor_row = self.cursor.row;
        let top = self.view.top_line;
        let bottom = top + lines.len();
        if cursor_row < top || cursor_row >= bottom {
            return lines;
        }
        let theme = self.themes.active();
        let cl = theme.ansi_background_for_theme_key("cursorline");
        let reset = "\x1b[0m";
        // Inject cursorline background after every span reset so the entire line is highlighted
        lines
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                if top + idx == cursor_row {
                    let line = format!("{cl}{line}{reset}");
                    line.replace(reset, &format!("{reset}{cl}"))
                } else {
                    line
                }
            })
            .collect()
    }

    fn with_inlay_hints(&self, lines: Vec<String>) -> Vec<String> {
        if !self.config.editor.inlay_hints || self.lsp_inlay_hints.is_empty() {
            return lines;
        }
        let theme = self.themes.active();
        let hint = theme.ansi_for_theme_key("comment", false);
        let reset = "\x1b[0m";
        lines
            .into_iter()
            .enumerate()
            .map(|(visible_idx, line)| {
                let line_no = self.view.top_line + visible_idx;
                let labels: Vec<_> = self
                    .lsp_inlay_hints
                    .iter()
                    .filter(|hint| hint.line as usize == line_no)
                    .map(|hint| hint.label.as_str())
                    .collect();
                if labels.is_empty() {
                    return line;
                }
                format!("{line} {hint} {} {reset}", labels.join(" "))
            })
            .collect()
    }

    fn with_file_tree_sidebar(&self, lines: Vec<String>) -> Vec<String> {
        if !self.file_tree_visible {
            return lines;
        }
        let width = 28usize;
        lines
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                let tree = self
                    .file_tree_lines
                    .get(idx)
                    .map(String::as_str)
                    .unwrap_or("");
                format!("{tree:<width$}│{line}")
            })
            .collect()
    }

    fn with_terminal_overlay(&self, mut lines: Vec<String>) -> Vec<String> {
        if !self.terminal_visible {
            return lines;
        }
        let term_rows = (lines.len() / 3).max(4).min(lines.len());
        let keep = lines.len().saturating_sub(term_rows);
        lines.truncate(keep);
        lines.push("─ terminal ─".to_string());
        if let Some(panel) = &self.terminal_panel {
            lines.extend(panel.visible_lines(term_rows.saturating_sub(1)));
        }
        lines.truncate(self.view.height);
        lines
    }

    fn with_gutter(&self, lines: Vec<String>) -> Vec<String> {
        let width = gutter::width(self.buffer.len_lines());
        let theme = self.themes.active();
        let mut log_line: usize = self.view.top_line;
        lines
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                let is_continuation = self.wrap_continuation.get(idx).copied().unwrap_or(false);
                if !is_continuation && idx > 0 {
                    log_line = log_line.saturating_add(1);
                }
                let line_no = log_line;
                let git_sign = gutter::git_sign(self.git_marks.get(&line_no).copied());
                let diag_sign = gutter::diagnostic_sign(self.diagnostic_severity_for_line(line_no));
                let fold_sign = if self.is_fold_header_line(line_no) {
                    gutter::GutterSign::FoldClosed
                } else {
                    gutter::GutterSign::None
                };
                let sign = gutter::merge_signs(git_sign, diag_sign);
                let sign = gutter::merge_signs(fold_sign, sign);
                let prefix = if let Some(mark) = self
                    .plugin_manager
                    .gutter_marks()
                    .iter()
                    .find(|mark| mark.line == line_no)
                {
                    let number_width = width.saturating_sub(1);
                    let marker = plugin_ui::gutter_marker(mark, theme);
                    let number = line_no + 1;
                    if line_no == self.cursor.row {
                        format!(
                            "\x1b[7m{marker}{:>number_width$}\x1b[0m ",
                            number,
                            number_width = number_width
                        )
                    } else {
                        format!(
                            "{marker}{:>number_width$} ",
                            number,
                            number_width = number_width
                        )
                    }
                } else if is_continuation {
                    let padding = " ".repeat(width);
                    format!("{padding}{line}")
                } else {
                    gutter::render_gutter(line_no, width, sign, line_no == self.cursor.row, theme)
                };
                format!("{prefix}{line}")
            })
            .collect()
    }

    fn with_plugin_virtual_text(&self, lines: Vec<String>) -> Vec<String> {
        let marks = self.plugin_manager.virtual_text();
        if marks.is_empty() {
            return lines;
        }
        let theme = self.themes.active();
        let max_suffix = self.view.width.saturating_sub(12).max(24);
        lines
            .into_iter()
            .enumerate()
            .map(|(visible_idx, line)| {
                let line_no = self.view.top_line + visible_idx;
                let Some(mark) = marks.iter().find(|mark| mark.line == line_no) else {
                    return line;
                };
                format!(
                    "{line}{}",
                    plugin_ui::virtual_text_suffix(mark, theme, max_suffix)
                )
            })
            .collect()
    }

    fn with_diagnostic_inline(&self, lines: Vec<String>) -> Vec<String> {
        if self.lsp_diagnostics.is_empty() {
            return lines;
        }
        let theme = self.themes.active();
        let max_suffix = self.view.width.saturating_sub(12).max(24);
        lines
            .into_iter()
            .enumerate()
            .map(|(visible_idx, line)| {
                let line_no = self.view.top_line + visible_idx;
                let Some(diagnostic) = self.primary_diagnostic_for_line(line_no) else {
                    return line;
                };
                format!(
                    "{line}{}",
                    diagnostics::inline_suffix(diagnostic, theme, max_suffix)
                )
            })
            .collect()
    }

    fn primary_diagnostic_for_line(&self, line: usize) -> Option<&Diagnostic> {
        self.lsp_diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.range.start.line as usize == line)
            .min_by_key(|diagnostic| diagnostic_rank(diagnostic.severity))
    }

    fn git_stage_current_file(&mut self) {
        let Some(path) = self.filepath.clone() else {
            self.status = "git stage: no file path".to_string();
            return;
        };
        match git::stage_file(&self.project_root(), &path) {
            Ok(()) => {
                self.refresh_git_gutter();
                self.status = format!("git stage: {}", path.display());
            }
            Err(err) => self.status = format!("git stage: {err}"),
        }
    }

    fn git_unstage_current_file(&mut self) {
        let Some(path) = self.filepath.clone() else {
            self.status = "git unstage: no file path".to_string();
            return;
        };
        match git::unstage_file(&self.project_root(), &path) {
            Ok(()) => {
                self.refresh_git_gutter();
                self.status = format!("git unstage: {}", path.display());
            }
            Err(err) => self.status = format!("git unstage: {err}"),
        }
    }

    fn diagnostic_severity_for_line(&self, line: usize) -> Option<DiagnosticSeverity> {
        let mut severity = None;
        for diagnostic in &self.lsp_diagnostics {
            if diagnostic.range.start.line as usize != line {
                continue;
            }
            let Some(next) = diagnostic.severity else {
                continue;
            };
            severity = Some(match (severity, next) {
                (Some(DiagnosticSeverity::Error), _) | (_, DiagnosticSeverity::Error) => {
                    DiagnosticSeverity::Error
                }
                (Some(DiagnosticSeverity::Warning), _) | (_, DiagnosticSeverity::Warning) => {
                    DiagnosticSeverity::Warning
                }
                (Some(DiagnosticSeverity::Information), _)
                | (_, DiagnosticSeverity::Information) => DiagnosticSeverity::Information,
                (current, DiagnosticSeverity::Hint) => current.unwrap_or(DiagnosticSeverity::Hint),
            });
        }
        severity
    }

    fn with_lsp_overlays(&self, lines: Vec<String>) -> Vec<String> {
        let theme = self.themes.active();
        let popup = theme.ansi_for_theme_key("popup", false);
        let selected = theme.ansi_for_theme_key("selection", false);
        let reset = theme.ansi_for_theme_key("popup", true);
        let mut overlay = Vec::new();
        if self.lsp_ui.completion_visible && !self.lsp_ui.completion_filter.is_empty() {
            overlay.push(format!("{popup}─ completion ─{reset}"));
            for (visible_idx, &item_index) in
                self.lsp_ui.completion_filter.iter().take(8).enumerate()
            {
                let item = &self.lsp_ui.completions[item_index];
                let marker = if visible_idx == self.lsp_ui.completion_selected {
                    "▸"
                } else {
                    " "
                };
                let kind = lsp_ui::completion_kind_label(item.kind);
                let detail = item.detail.as_deref().unwrap_or("");
                let color = if visible_idx == self.lsp_ui.completion_selected {
                    &selected
                } else {
                    &popup
                };
                overlay.push(format!(
                    "{color}{marker} {kind} {} {detail}{reset}",
                    item.label
                ));
            }
            if let Some(&item_index) = self
                .lsp_ui
                .completion_filter
                .get(self.lsp_ui.completion_selected)
            {
                if let Some(docs) = &self.lsp_ui.completions[item_index].documentation {
                    for line in docs.lines().take(3) {
                        overlay.push(format!("{popup}  {line}{reset}"));
                    }
                }
            }
        }
        if let Some(hover) = &self.lsp_ui.hover {
            overlay.push(format!("{popup}─ hover ─{reset}"));
            for line in hover.markdown.lines().take(4) {
                overlay.push(format!("{popup}  {}{reset}", line.trim()));
            }
        }
        if let Some(signature) = &self.lsp_ui.signature_help {
            overlay.push(format!("{popup}─ signature ─{reset}"));
            for line in lsp_ui::signature_help_lines(signature) {
                overlay.push(format!("{popup}{line}{reset}"));
            }
        }
        if overlay.is_empty() {
            return lines;
        }
        overlay.extend(lines);
        overlay.truncate(self.view.height.saturating_sub(1));
        overlay
    }

    fn language_name(&self) -> &'static str {
        grammars::language_spec(self.language)
            .map(|spec| spec.name)
            .unwrap_or("text")
    }

    fn language_status(&self) -> String {
        let settings = self.language_settings();
        format!(
            "language: {}  tab: {}  highlight: {}  lsp: {}",
            self.language_name(),
            settings.tab_width,
            if settings.highlight { "on" } else { "off" },
            if settings.lsp { "on" } else { "off" },
        )
    }

    fn grammar_status(&self) -> String {
        let language = self.language_name();
        if self.language == Language::PlainText {
            return "grammar: plain text".to_string();
        }
        if self.grammar_manager.is_available(self.language) {
            format!("grammar: {language} available")
        } else {
            format!(
                "grammar: {language} missing ({})",
                self.grammar_manager.package_path(language).display()
            )
        }
    }

    fn lsp_status(&self) -> String {
        if !self.enable_lsp {
            return "lsp: disabled".to_string();
        }
        match self.lsp_server {
            Some(server) => format!(
                "lsp: {} via {} ({})",
                server.language, server.server_name, server.install_hint
            ),
            None => "lsp: no server for this buffer".to_string(),
        }
    }

    fn start_lsp_client(&mut self) {
        self.ensure_lsp_started();
        if let Some(client) = &self.lsp_client {
            self.status = format!("lsp: {} {}", client.server(), self.lsp_status());
        } else {
            self.status = self.lsp_status();
        }
    }

    fn ensure_lsp_started(&mut self) {
        if !self.enable_lsp {
            return;
        }
        let Some(client) = &mut self.lsp_client else {
            return;
        };
        if client.is_started() {
            return;
        }
        match client.start() {
            Ok(()) => {
                self.lsp_start_pending = false;
                self.status = format!("lsp: {} starting", client.server());
            }
            Err(err) => {
                self.lsp_start_pending = false;
                self.status = format!("lsp: {err}");
            }
        }
    }

    fn tick_lsp_lifecycle(&mut self) {
        if self.lsp_start_pending {
            self.ensure_lsp_started();
        }
        self.sync_lsp_document_open();
    }

    fn sync_lsp_document_open(&mut self) {
        if !self.enable_lsp {
            return;
        }
        let Some(path) = self.filepath.clone() else {
            return;
        };
        if self.lsp_document_open.as_ref() == Some(&path) {
            return;
        }
        let Some(client) = &self.lsp_client else {
            return;
        };
        if !client.is_started() || !client.is_initialized() {
            return;
        }
        let language = self.language_name();
        let text = self.buffer.to_string();
        let version = self.document_version;
        if client.did_open(&path, language, text, version).is_ok() {
            self.lsp_document_open = Some(path.clone());
            self.status = format!("lsp: {} ready", client.server());
            self.spawn_lsp_auxiliary_views();
        }
    }

    fn trigger_completion(&mut self) {
        if !self.enable_lsp {
            return;
        }
        self.ensure_lsp_started();
        let (word_start, _) =
            lsp_ui::word_prefix_at(&self.buffer, self.cursor.row, self.cursor.col);
        self.lsp_ui.completion_word_start = word_start;
        self.lsp_status_action("completion");
    }

    fn dismiss_completion(&mut self) {
        self.lsp_ui.completion_visible = false;
        self.lsp_ui.completions.clear();
        self.lsp_ui.completion_filter.clear();
        self.lsp_ui.completion_selected = 0;
    }

    fn refresh_completion_filter(&mut self) {
        if !self.lsp_ui.completion_visible {
            return;
        }
        let (_, prefix) = lsp_ui::word_prefix_at(&self.buffer, self.cursor.row, self.cursor.col);
        self.lsp_ui.completion_filter =
            lsp_ui::filter_completions(&self.lsp_ui.completions, &prefix);
        if self.lsp_ui.completion_selected >= self.lsp_ui.completion_filter.len() {
            self.lsp_ui.completion_selected = 0;
        }
    }

    fn cycle_completion(&mut self, delta: isize) {
        if self.lsp_ui.completion_filter.is_empty() {
            return;
        }
        let len = self.lsp_ui.completion_filter.len();
        let next = self.lsp_ui.completion_selected as isize + delta;
        let wrapped = (next.rem_euclid(len as isize)) as usize;
        self.lsp_ui.completion_selected = wrapped;
        self.spawn_completion_resolve();
    }

    fn spawn_completion_resolve(&mut self) {
        let Some(&item_index) = self
            .lsp_ui
            .completion_filter
            .get(self.lsp_ui.completion_selected)
        else {
            return;
        };
        let item = &self.lsp_ui.completions[item_index];
        if item.documentation.is_some() {
            return;
        }
        let Some(handle) = self.lsp_request_handle() else {
            return;
        };
        if !self.async_runtime_available() {
            return;
        }
        let tx = self.async_tx.clone();
        let item_clone = item.clone();
        tokio::spawn(async move {
            let result = handle
                .resolve_completion_item(&item_clone)
                .await
                .map_err(|err| err.to_string());
            let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::CompletionResolve(result)));
        });
    }

    fn accept_completion(&mut self) {
        let Some(&item_index) = self
            .lsp_ui
            .completion_filter
            .get(self.lsp_ui.completion_selected)
        else {
            self.dismiss_completion();
            return;
        };
        let item = self.lsp_ui.completions[item_index].clone();
        let insert_text = item
            .insert_text
            .clone()
            .unwrap_or_else(|| item.label.clone());
        let end_col = self.cursor.col;
        let after = lsp_ui::replace_completion_range(
            &mut self.buffer,
            self.cursor.row,
            self.lsp_ui.completion_word_start,
            end_col,
            &insert_text,
        );
        let (row, col) = self.buffer.char_to_line_col(after);
        self.cursor.row = row;
        self.cursor.col = col;
        self.sync_selection_to_cursor();
        self.dirty = true;
        self.dismiss_completion();
        self.status = format!("completed: {}", item.label);
    }

    fn goto_diagnostic(&mut self, direction: i32) {
        if self.lsp_diagnostics.is_empty() {
            self.status = "no diagnostics".to_string();
            return;
        }
        let current_line = self.cursor.row as u32;
        let current_col = self.cursor.col as u32;
        let mut ordered: Vec<_> = self.lsp_diagnostics.iter().collect();
        ordered.sort_by_key(|d| (d.range.start.line, d.range.start.character));

        let next = if direction >= 0 {
            ordered
                .iter()
                .find(|d| {
                    d.range.start.line > current_line
                        || (d.range.start.line == current_line
                            && d.range.start.character > current_col)
                })
                .or_else(|| ordered.first())
        } else {
            ordered
                .iter()
                .rev()
                .find(|d| {
                    d.range.start.line < current_line
                        || (d.range.start.line == current_line
                            && d.range.start.character < current_col)
                })
                .or_else(|| ordered.last())
        };

        if let Some(diagnostic) = next.cloned() {
            let row = diagnostic.range.start.line as usize;
            let col = diagnostic.range.start.character as usize;
            let message = diagnostic.message.clone();
            self.push_jump();
            self.cursor.row = row;
            self.cursor.col = col;
            self.sync_selection_to_cursor();
            self.view.ensure_cursor_visible(&self.buffer, &self.cursor);
            self.status = format!("diagnostic: {message}");
        }
    }

    fn goto_lsp_location(&mut self, location: Location) -> Result<()> {
        self.push_jump();
        if let Some(path) = lsp_ui::file_path_from_uri(&location.uri) {
            if self.filepath.as_deref() != Some(path.as_path()) {
                self.open_path(path)?;
            }
        }
        self.cursor.row = location.range.start.line as usize;
        self.cursor.col = location.range.start.character as usize;
        self.sync_selection_to_cursor();
        self.view.ensure_cursor_visible(&self.buffer, &self.cursor);
        Ok(())
    }

    fn open_lsp_locations(&mut self, action: &'static str, locations: Vec<Location>) {
        if locations.is_empty() {
            self.status = format!("lsp {action}: no results");
            return;
        }
        if locations.len() == 1 {
            if let Err(err) = self.goto_lsp_location(locations[0].clone()) {
                self.status = format!("lsp {action}: {err}");
            } else {
                self.status = format!("lsp {action}");
            }
            return;
        }
        self.picker_kind = Some(ActivePicker::Locations);
        self.picker_items = locations
            .into_iter()
            .map(|location| {
                AppPickerItem::location(
                    lsp_ui::location_label(&location),
                    lsp_ui::file_path_from_uri(&location.uri),
                    location.range.start.line as usize,
                    location.range.start.character as usize,
                )
            })
            .collect();
        self.picker_selected = 0;
        self.mode = Mode::Picker;
        self.status = format!("lsp: {action}");
    }

    fn expand_selection_textobject(&mut self) {
        let Some(highlighter) = &self.highlighter else {
            return;
        };
        let Some(tree) = highlighter.tree() else {
            return;
        };
        let char_idx = self.buffer.selections().primary().head;
        if let Some(range) =
            textobjects::expand_to_parent(&self.buffer, self.language, tree, char_idx)
        {
            self.buffer
                .selections_mut()
                .set_primary(Selection::new(range.start, range.end));
            self.selection_to_cursor();
            self.status = "selection: parent node".to_string();
        }
    }

    fn select_function_textobject(&mut self) {
        let Some(highlighter) = &self.highlighter else {
            self.status = "textobject: highlighting disabled".to_string();
            return;
        };
        let Some(tree) = highlighter.tree() else {
            self.status = "textobject: parse tree unavailable".to_string();
            return;
        };
        let char_idx = self.buffer.selections().primary().head;
        if let Some(range) =
            textobjects::function_around(&self.buffer, self.language, tree, char_idx)
        {
            self.buffer
                .selections_mut()
                .set_primary(Selection::new(range.start, range.end));
            self.selection_to_cursor();
            self.status = "selection: function".to_string();
        } else {
            self.status = "textobject: no function".to_string();
        }
    }

    fn insert_char(&mut self, ch: char) {
        if self.auto_pairs {
            if let Some(close) = surround::pair_chars(ch) {
                let pair = format!("{ch}{close}");
                self.insert(&pair);
                if close != ch {
                    self.cursor.move_left(&self.buffer);
                    self.sync_selection_to_cursor();
                }
                return;
            }
        }
        self.insert(&ch.to_string());
    }

    fn spawn_lsp_auxiliary_views(&mut self) {
        let Some(handle) = self.lsp_request_handle() else {
            return;
        };
        if !self.async_runtime_available() {
            return;
        }
        let Some(path) = self.filepath.clone() else {
            return;
        };
        let cursor = self.cursor;
        let tx = self.async_tx.clone();
        let handle = handle.clone();
        tokio::spawn(async move {
            let highlights = handle
                .document_highlight(
                    &path,
                    Position {
                        line: cursor.row as u32,
                        character: cursor.col as u32,
                    },
                )
                .await
                .map_err(|err| err.to_string());
            let inlays = handle
                .inlay_hints(&path)
                .await
                .map_err(|err| err.to_string());
            let folds = handle
                .folding_ranges(&path)
                .await
                .map_err(|err| err.to_string());
            let semantic = handle
                .semantic_tokens_full(&path)
                .await
                .map_err(|err| err.to_string());
            let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Auxiliary {
                highlights,
                inlays,
                folds,
                semantic,
            }));
        });
    }

    fn toggle_terminal_panel(&mut self) {
        self.terminal_visible = !self.terminal_visible;
        if self.terminal_visible && self.terminal_panel.is_none() {
            let rows = ((self.view.height / 3).clamp(4, 16)) as u16;
            let cols = self.view.width.max(20) as u16;
            match TerminalPanel::spawn(rows, cols) {
                Ok(panel) => {
                    self.terminal_panel = Some(panel);
                    self.terminal_focused = true;
                    self.status = "terminal: press Esc to return to editor".to_string();
                }
                Err(err) => {
                    self.terminal_visible = false;
                    self.status = format!("terminal: {err}");
                }
            }
        } else if self.terminal_visible {
            self.terminal_focused = true;
            self.status = "terminal focused".to_string();
        } else {
            self.terminal_focused = false;
            self.status = "terminal hidden".to_string();
        }
    }

    fn toggle_file_tree(&mut self) {
        self.file_tree_visible = !self.file_tree_visible;
        if self.file_tree_visible {
            self.refresh_file_tree();
            self.file_tree_focused = true;
            self.status = "file tree: Enter open  Esc editor  Space+e hide".to_string();
        } else {
            self.file_tree_lines.clear();
            self.file_tree_items.clear();
            self.file_tree_focused = false;
            self.status = "file tree hidden".to_string();
        }
    }

    fn toggle_undo_tree(&mut self) {
        self.undo_tree_visible = !self.undo_tree_visible;
        if self.undo_tree_visible {
            self.status = format!("undo tree ({} nodes) — Esc to close", self.history.len());
        } else {
            self.status = "undo tree closed".to_string();
        }
    }

    fn refresh_file_tree(&mut self) {
        let root = self.project_root();
        self.file_tree_items = filetree::build_tree(&root, 2, 200);
        self.file_tree_lines = filetree::render_lines(&self.file_tree_items, &root, 28);
        if self.file_tree_selected >= self.file_tree_items.len() {
            self.file_tree_selected = 0;
        }
    }

    fn handle_file_tree_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.file_tree_focused = false;
                self.status = "editor".to_string();
            }
            KeyCode::Up => {
                self.file_tree_selected = self.file_tree_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                if !self.file_tree_items.is_empty() {
                    self.file_tree_selected =
                        (self.file_tree_selected + 1).min(self.file_tree_items.len() - 1);
                }
            }
            KeyCode::Enter => self.open_file_tree_selection(),
            _ => {}
        }
        Ok(())
    }

    fn open_file_tree_selection(&mut self) {
        let Some(item) = self.file_tree_items.get(self.file_tree_selected) else {
            return;
        };
        if item.is_dir {
            return;
        }
        let path = item.path.clone();
        self.file_tree_focused = false;
        if let Err(err) = self.open_path(path) {
            self.status = format!("open: {err}");
        }
    }

    fn toggle_fold_at_cursor(&mut self) {
        let line = self.cursor.row as u32;
        let Some(range) = self
            .lsp_fold_ranges
            .iter()
            .find(|range| range.start_line <= line && line <= range.end_line)
        else {
            self.status = "fold: no range".to_string();
            return;
        };
        let folded = self
            .lsp_folded_starts
            .get(&range.start_line)
            .copied()
            .unwrap_or(false);
        self.lsp_folded_starts.insert(range.start_line, !folded);
        self.status = format!(
            "fold: lines {}-{} {}",
            range.start_line + 1,
            range.end_line + 1,
            if folded { "open" } else { "closed" }
        );
    }

    fn unfold_all(&mut self) {
        self.lsp_folded_starts.clear();
        self.status = "fold: all open".to_string();
    }

    fn apply_line_folding(&self, lines: Vec<String>) -> Vec<String> {
        if self.lsp_folded_starts.is_empty() {
            return lines;
        }
        let mut out = Vec::new();
        for (visible_idx, line) in lines.into_iter().enumerate() {
            let line_no = self.view.top_line + visible_idx;
            if self.is_line_hidden_by_fold(line_no) {
                continue;
            }
            if self.is_fold_header_line(line_no) {
                out.push(format!("{line} …"));
            } else {
                out.push(line);
            }
        }
        out
    }

    fn is_line_hidden_by_fold(&self, line_no: usize) -> bool {
        self.lsp_fold_ranges.iter().any(|range| {
            let start = range.start_line as usize;
            let end = range.end_line as usize;
            let folded = self
                .lsp_folded_starts
                .get(&range.start_line)
                .copied()
                .unwrap_or(false);
            folded && line_no > start && line_no <= end
        })
    }

    fn is_fold_header_line(&self, line_no: usize) -> bool {
        self.lsp_fold_ranges.iter().any(|range| {
            let start = range.start_line as usize;
            self.lsp_folded_starts
                .get(&range.start_line)
                .copied()
                .unwrap_or(false)
                && line_no == start
        })
    }

    fn handle_terminal_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.terminal_focused = false;
                self.status = "editor".to_string();
            }
            KeyCode::Char(ch) => {
                if let Some(panel) = &mut self.terminal_panel {
                    let _ = panel.write_input(&ch.to_string());
                }
            }
            KeyCode::Enter => {
                if let Some(panel) = &mut self.terminal_panel {
                    let _ = panel.write_input("\n");
                }
            }
            KeyCode::Backspace => {
                if let Some(panel) = &mut self.terminal_panel {
                    let _ = panel.write_input("\x08");
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn lsp_status_action(&mut self, action: &str) {
        self.ensure_lsp_started();
        let Some(handle) = self.lsp_request_handle() else {
            self.status = format!("lsp: unavailable for {action}");
            return;
        };
        if !self.async_runtime_available() {
            self.status = "lsp: async runtime unavailable".to_string();
            return;
        }
        let path = self.filepath.clone();
        let cursor = self.cursor;
        let selection = self.buffer.selections().primary();
        let code_action_range = self.range_for_selection(selection);
        let tx = self.async_tx.clone();
        self.status = format!("lsp: queued {action}");

        match action {
            "completion" => {
                if let Some(path) = path {
                    let handle = handle.clone();
                    tokio::spawn(async move {
                        let result = handle
                            .completion(
                                &path,
                                Position {
                                    line: cursor.row as u32,
                                    character: cursor.col as u32,
                                },
                            )
                            .await
                            .map_err(|err| err.to_string());
                        let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Completion(result)));
                    });
                }
            }
            "hover" => {
                if let Some(path) = path {
                    let handle = handle.clone();
                    tokio::spawn(async move {
                        let result = handle
                            .hover(
                                &path,
                                Position {
                                    line: cursor.row as u32,
                                    character: cursor.col as u32,
                                },
                            )
                            .await
                            .map_err(|err| err.to_string());
                        let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Hover(result)));
                    });
                }
            }
            "signature help" => self.spawn_signature_help(handle, tx, path, cursor, None),
            "definition" => self.spawn_lsp_locations(handle, tx, path, cursor, "definition"),
            "type definition" => {
                self.spawn_lsp_locations(handle, tx, path, cursor, "type definition")
            }
            "implementation" => {
                self.spawn_lsp_locations(handle, tx, path, cursor, "implementation")
            }
            "references" => self.spawn_lsp_references(handle, tx, path, cursor),
            "rename" => self.spawn_lsp_rename(handle, tx, path, cursor, "renamed".to_string()),
            "format" => self.spawn_lsp_format(handle, tx, path),
            "code actions" => self.spawn_lsp_code_actions(handle, tx, path, code_action_range),
            _ => {}
        }
    }

    fn drain_lsp_events(&mut self) {
        let Some(client) = &mut self.lsp_client else {
            return;
        };
        while let Some(event) = client.try_recv_event() {
            match event {
                LspEvent::PublishDiagnostics { diagnostics, .. } => {
                    self.lsp_diagnostics = diagnostics;
                }
                LspEvent::ShowMessage { message, .. } | LspEvent::LogMessage { message, .. } => {
                    self.status = format!("lsp: {message}");
                }
                LspEvent::ServerRequest { method, .. } | LspEvent::Notification { method, .. } => {
                    self.status = format!("lsp: {method}");
                }
            }
        }
    }

    fn drain_async_events(&mut self) {
        while let Ok(event) = self.async_rx.try_recv() {
            match event {
                AppAsyncEvent::Lsp(event) => self.apply_lsp_ui_event(event),
                AppAsyncEvent::TreeSitter(event) => {
                    if event.generation == self.tree_parse_generation {
                        if let Some(highlighter) = &mut self.highlighter {
                            if let Some(tree) = event.tree {
                                highlighter.queue_parsed_tree(tree);
                            }
                        }
                        self.tree_parse_in_flight = false;
                    }
                }
            }
        }
    }

    fn apply_lsp_ui_event(&mut self, event: LspUiEvent) {
        match event {
            LspUiEvent::Completion(result) => match result {
                Ok(items) => {
                    self.lsp_ui.completions = items;
                    self.lsp_ui.completion_visible = true;
                    self.lsp_ui.completion_selected = 0;
                    self.refresh_completion_filter();
                    self.status = format!(
                        "lsp: {} completions ({} shown)",
                        self.lsp_ui.completions.len(),
                        self.lsp_ui.completion_filter.len()
                    );
                }
                Err(err) => self.status = format!("lsp completion: {err}"),
            },
            LspUiEvent::CompletionResolve(result) => match result {
                Ok(resolved) => {
                    if let Some(idx) = self
                        .lsp_ui
                        .completions
                        .iter()
                        .position(|c| c.label == resolved.label)
                    {
                        self.lsp_ui.completions[idx].documentation = resolved.documentation.clone();
                        self.lsp_ui.completions[idx].detail = resolved.detail.clone();
                        self.lsp_ui.completions[idx].raw = resolved.raw;
                    }
                }
                Err(err) => {
                    self.status = format!("lsp resolve: {err}");
                }
            },
            LspUiEvent::Hover(result) => match result {
                Ok(hover) => {
                    self.lsp_ui.hover = hover;
                }
                Err(err) => self.status = format!("lsp hover: {err}"),
            },
            LspUiEvent::SignatureHelp(result) => match result {
                Ok(Some(signature)) => {
                    self.lsp_ui.signature_help = Some(signature);
                    self.status = "lsp: signature help".to_string();
                }
                Ok(None) => {
                    self.lsp_ui.signature_help = None;
                    self.status = "lsp: no signature help".to_string();
                }
                Err(err) => self.status = format!("lsp signature: {err}"),
            },
            LspUiEvent::Locations { action, result } => match result {
                Ok(locations) => self.open_lsp_locations(action, locations),
                Err(err) => self.status = format!("lsp {action}: {err}"),
            },
            LspUiEvent::Symbols(result) => match result {
                Ok(symbols) => {
                    self.picker_kind = Some(ActivePicker::Symbols);
                    self.picker_items = symbols
                        .into_iter()
                        .map(|symbol| {
                            AppPickerItem::location(
                                format!(
                                    "{}:{}:{}",
                                    symbol.name,
                                    symbol.range.start.line + 1,
                                    symbol.range.start.character + 1
                                ),
                                None,
                                symbol.selection_range.start.line as usize,
                                symbol.selection_range.start.character as usize,
                            )
                        })
                        .collect();
                    self.mode = Mode::Picker;
                    self.status = format!("lsp: {} symbols", self.picker_items.len());
                }
                Err(err) => self.status = format!("lsp symbols: {err}"),
            },
            LspUiEvent::Rename(result) => match result {
                Ok(value) => {
                    let edits =
                        lsp_ui::text_edits_from_workspace_value(&value, self.filepath.as_deref());
                    if edits.is_empty() {
                        self.status = "lsp rename: no local edits".to_string();
                    } else {
                        let applied = lsp_ui::apply_text_edits(&mut self.buffer, &edits);
                        for edit in &applied {
                            self.after_buffer_edit(edit);
                        }
                        self.dirty = true;
                        self.status = format!("lsp: renamed ({} edits)", edits.len());
                    }
                }
                Err(err) => self.status = format!("lsp rename: {err}"),
            },
            LspUiEvent::Formatting(result) => match result {
                Ok(edits) => {
                    let applied = lsp_ui::apply_text_edits(&mut self.buffer, &edits);
                    for edit in &applied {
                        self.after_buffer_edit(edit);
                    }
                    if !applied.is_empty() {
                        self.dirty = true;
                    }
                    if let Some(path) = self.pending_format_on_save.take() {
                        match self.buffer.save_to(&path) {
                            Ok(bytes) => {
                                self.dirty = false;
                                self.sync_app_into_manager();
                                self.persist_persistent_history();
                                self.refresh_git_gutter();
                                self.status = format!("saved {} bytes", bytes);
                                if let Some(client) = &self.lsp_client {
                                    if client.is_started() {
                                        let _ =
                                            client.did_save(&path, Some(self.buffer.to_string()));
                                    }
                                }
                                self.dispatch_plugin_event(PluginEvent::BufferSave(
                                    self.buffer_snapshot(),
                                ));
                            }
                            Err(err) => {
                                self.status = format!("save after format: {err:#}");
                            }
                        }
                    } else {
                        self.status = "lsp: formatted".to_string();
                    }
                }
                Err(err) => {
                    if let Some(path) = self.pending_format_on_save.take() {
                        let bytes = self.buffer.save_to(&path);
                        if let Ok(bytes) = bytes {
                            self.dirty = false;
                            self.status = format!("saved {} bytes (format failed)", bytes);
                        } else {
                            self.status = format!("lsp format: {err}");
                        }
                    } else {
                        self.status = format!("lsp format: {err}");
                    }
                }
            },
            LspUiEvent::CodeActions(result) => match result {
                Ok(actions) => {
                    self.picker_kind = Some(ActivePicker::CodeActions);
                    self.picker_items = actions
                        .into_iter()
                        .map(AppPickerItem::code_action)
                        .collect();
                    self.mode = Mode::Picker;
                }
                Err(err) => self.status = format!("lsp code actions: {err}"),
            },
            LspUiEvent::Auxiliary {
                highlights,
                inlays,
                folds,
                semantic,
            } => {
                if let Ok(ranges) = highlights {
                    self.lsp_document_highlights = ranges;
                }
                if let Ok(hints) = inlays {
                    self.lsp_inlay_hints = hints;
                }
                if let Ok(ranges) = folds {
                    self.lsp_fold_ranges = ranges;
                }
                if let Ok(data) = semantic {
                    self.lsp_semantic_tokens =
                        semantic::decode_semantic_tokens(&data).tokens().to_vec();
                }
                self.status = format!(
                    "lsp: {} highlights, {} inlays, {} folds, {} semantic",
                    self.lsp_document_highlights.len(),
                    self.lsp_inlay_hints.len(),
                    self.lsp_fold_ranges.len(),
                    self.lsp_semantic_tokens.len()
                );
            }
        }
    }

    fn lsp_request_handle(&self) -> Option<LspRequestHandle> {
        self.lsp_client.as_ref().and_then(LspClient::request_handle)
    }

    fn async_runtime_available(&self) -> bool {
        tokio::runtime::Handle::try_current().is_ok()
    }

    fn lsp_rename_as(&mut self, new_name: String) {
        let Some(handle) = self.lsp_request_handle() else {
            self.status = "lsp: start first with :lsp-start for rename".to_string();
            return;
        };
        if !self.async_runtime_available() {
            self.status = "lsp: async runtime unavailable".to_string();
            return;
        }
        self.spawn_lsp_rename(
            handle,
            self.async_tx.clone(),
            self.filepath.clone(),
            self.cursor,
            new_name,
        );
        self.status = "lsp: queued rename".to_string();
    }

    fn spawn_lsp_locations(
        &self,
        handle: LspRequestHandle,
        tx: tokio::sync::mpsc::UnboundedSender<AppAsyncEvent>,
        path: Option<PathBuf>,
        cursor: Cursor,
        action: &'static str,
    ) {
        if let Some(path) = path {
            tokio::spawn(async move {
                let position = Position {
                    line: cursor.row as u32,
                    character: cursor.col as u32,
                };
                let result = match action {
                    "definition" => handle.goto_definition(&path, position).await,
                    "type definition" => handle.goto_type_definition(&path, position).await,
                    "implementation" => handle.goto_implementation(&path, position).await,
                    _ => unreachable!(),
                }
                .map_err(|err| err.to_string());
                let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Locations { action, result }));
            });
        }
    }

    fn spawn_lsp_references(
        &self,
        handle: LspRequestHandle,
        tx: tokio::sync::mpsc::UnboundedSender<AppAsyncEvent>,
        path: Option<PathBuf>,
        cursor: Cursor,
    ) {
        if let Some(path) = path {
            tokio::spawn(async move {
                let result = handle
                    .references(
                        &path,
                        Position {
                            line: cursor.row as u32,
                            character: cursor.col as u32,
                        },
                        true,
                    )
                    .await
                    .map_err(|err| err.to_string());
                let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Locations {
                    action: "references",
                    result,
                }));
            });
        }
    }

    fn spawn_lsp_rename(
        &self,
        handle: LspRequestHandle,
        tx: tokio::sync::mpsc::UnboundedSender<AppAsyncEvent>,
        path: Option<PathBuf>,
        cursor: Cursor,
        new_name: String,
    ) {
        if let Some(path) = path {
            tokio::spawn(async move {
                let result = handle
                    .rename(
                        &path,
                        Position {
                            line: cursor.row as u32,
                            character: cursor.col as u32,
                        },
                        new_name,
                    )
                    .await
                    .map_err(|err| err.to_string());
                let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Rename(result)));
            });
        }
    }

    fn request_signature_help(&mut self, trigger: Option<char>) {
        if !self.enable_lsp {
            return;
        }
        let Some(handle) = self.lsp_request_handle() else {
            return;
        };
        if !self.async_runtime_available() {
            return;
        }
        let Some(path) = self.filepath.clone() else {
            return;
        };
        self.spawn_signature_help(
            handle,
            self.async_tx.clone(),
            Some(path),
            self.cursor,
            trigger,
        );
    }

    fn spawn_signature_help(
        &self,
        handle: LspRequestHandle,
        tx: mpsc::UnboundedSender<AppAsyncEvent>,
        path: Option<PathBuf>,
        cursor: Cursor,
        trigger: Option<char>,
    ) {
        if let Some(path) = path {
            tokio::spawn(async move {
                let result = handle
                    .signature_help(
                        &path,
                        Position {
                            line: cursor.row as u32,
                            character: cursor.col as u32,
                        },
                        trigger,
                    )
                    .await
                    .map_err(|err| err.to_string());
                let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::SignatureHelp(result)));
            });
        }
    }

    fn spawn_lsp_format(
        &self,
        handle: LspRequestHandle,
        tx: tokio::sync::mpsc::UnboundedSender<AppAsyncEvent>,
        path: Option<PathBuf>,
    ) {
        if let Some(path) = path {
            tokio::spawn(async move {
                let result = handle
                    .format_document(&path, 4, true)
                    .await
                    .map_err(|err| err.to_string());
                let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::Formatting(result)));
            });
        }
    }

    fn spawn_lsp_code_actions(
        &self,
        handle: LspRequestHandle,
        tx: tokio::sync::mpsc::UnboundedSender<AppAsyncEvent>,
        path: Option<PathBuf>,
        range: Range,
    ) {
        if let Some(path) = path {
            let diagnostics = self.lsp_diagnostics.clone();
            tokio::spawn(async move {
                let result = handle
                    .code_actions(&path, range, diagnostics)
                    .await
                    .map_err(|err| err.to_string());
                let _ = tx.send(AppAsyncEvent::Lsp(LspUiEvent::CodeActions(result)));
            });
        }
    }

    fn install_ready_tree_sitter(&mut self, frame_start: Instant) {
        if let Some(highlighter) = &mut self.highlighter {
            let _ = highlighter.install_pending_tree_with_budget(frame_start);
        }
    }

    fn tick_tree_sitter(&mut self) {
        if self.tree_parse_in_flight {
            return;
        }
        if !self.tree_parse_dirty {
            return;
        }
        self.schedule_tree_sitter_parse();
    }

    fn schedule_tree_sitter_parse(&mut self) {
        if !self.enable_highlight {
            return;
        }
        let Some(highlighter) = &self.highlighter else {
            return;
        };
        if self.tree_parse_in_flight {
            return;
        }
        if !highlighter.tree_sitter_available() {
            return;
        }
        let generation = self.tree_parse_generation.wrapping_add(1);
        self.tree_parse_generation = generation;
        self.tree_parse_in_flight = true;
        self.tree_parse_dirty = false;
        let language = self.language;
        let buffer = self.buffer.clone();
        let old_tree = highlighter.editable_tree();
        let tx = self.async_tx.clone();
        tokio::spawn(async move {
            let parsed = TreeSitterHighlighter::parse_editor_buffer_snapshot(
                language,
                buffer,
                old_tree,
                TreeSitterHighlighter::background_parse_timeout(),
            );
            let _ = tx.send(AppAsyncEvent::TreeSitter(TreeSitterEvent {
                generation,
                tree: parsed,
            }));
        });
    }

    fn range_for_selection(&self, selection: Selection) -> Range {
        let (start_line, start_col) = self.buffer.char_to_line_col(selection.start());
        let (end_line, end_col) = self.buffer.char_to_line_col(selection.end());
        Range {
            start: Position {
                line: start_line as u32,
                character: start_col as u32,
            },
            end: Position {
                line: end_line as u32,
                character: end_col as u32,
            },
        }
    }

    fn diagnostics_summary(&self) -> String {
        if self.lsp_diagnostics.is_empty() {
            return String::new();
        }
        let mut errors = 0usize;
        let mut warnings = 0usize;
        for diagnostic in &self.lsp_diagnostics {
            match diagnostic.severity {
                Some(DiagnosticSeverity::Error) => errors += 1,
                Some(DiagnosticSeverity::Warning) => warnings += 1,
                _ => {}
            }
        }
        format!("  E:{} W:{}", errors, warnings)
    }

    fn rebuild_config_bindings(&mut self) {
        self.config_normal_bindings = BindingTable::from_config(&self.config, "normal");
        self.config_space_bindings = BindingTable::from_config(&self.config, "space");
        let plugin_normal = self
            .plugin_manager
            .keymaps()
            .iter()
            .filter(|keymap| keymap.mode == "normal")
            .map(|keymap| (keymap.key.clone(), keymap.command.clone()));
        self.config_normal_bindings.extend_bindings(plugin_normal);
        let plugin_space = self
            .plugin_manager
            .keymaps()
            .iter()
            .filter(|keymap| keymap.mode == "space")
            .map(|keymap| (keymap.key.clone(), keymap.command.clone()));
        self.config_space_bindings.extend_bindings(plugin_space);
    }

    fn discover_plugins(&mut self) {
        match self.plugin_manager.discover() {
            Ok(count) => {
                self.plugins_discovered = true;
                self.rebuild_config_bindings();
                self.status = format!(
                    "plugins: {} discovered, wasm runtime {}",
                    count,
                    if wasm_runtime_available() {
                        "available"
                    } else {
                        "stub"
                    }
                );
            }
            Err(err) => {
                self.status = format!("plugins: {err}");
            }
        }
    }

    fn open_plugin_picker(&mut self) {
        if !self.plugins_discovered {
            self.discover_plugins();
        }
        self.picker_kind = Some(ActivePicker::Plugins);
        self.command_buffer.clear();
        self.refresh_plugin_picker();
        self.mode = Mode::Picker;
    }

    fn refresh_plugin_picker(&mut self) {
        let query = self.command_buffer.to_lowercase();
        let mut items = Vec::new();
        for manifest in self.plugin_manager.list() {
            let label = format!("{} v{}", manifest.name, manifest.version);
            if query.is_empty() || label.to_lowercase().contains(&query) {
                items.push(AppPickerItem::plain(label));
            }
        }
        for command in self.plugin_manager.registered_commands() {
            let label = format!("@{} — {}", command.name, command.description);
            if query.is_empty() || label.to_lowercase().contains(&query) {
                items.push(AppPickerItem::plain(label));
            }
        }
        if items.is_empty() {
            items.push(AppPickerItem::plain("no plugins installed"));
        }
        self.picker_items = items;
        self.picker_selected = 0;
    }

    fn plugin_install(&mut self, path: &std::path::Path) {
        match self.plugin_manager.install_local(path) {
            Ok(name) => {
                self.plugins_discovered = true;
                self.rebuild_config_bindings();
                self.status = format!("plugin installed: {name}");
            }
            Err(err) => self.status = format!("plugin install: {err}"),
        }
    }

    fn plugin_remove(&mut self, name: &str) {
        match self.plugin_manager.remove(name) {
            Ok(true) => {
                self.rebuild_config_bindings();
                self.status = format!("plugin removed: {name}");
            }
            Ok(false) => self.status = format!("plugin not found: {name}"),
            Err(err) => self.status = format!("plugin remove: {err}"),
        }
    }

    fn plugin_update(&mut self, path: &std::path::Path) {
        match self.plugin_manager.update_local(path) {
            Ok(name) => self.status = format!("plugin updated: {name}"),
            Err(err) => self.status = format!("plugin update: {err}"),
        }
    }

    fn reload_config(&mut self) {
        match config::load() {
            Ok(config) => {
                self.enable_lsp = config.lsp;
                self.enable_highlight = config.highlight;
                self.auto_pairs = config.auto_pairs;
                self.view.scrolloff = config.editor.scrolloff;
                let theme = config.theme.clone();
                self.config = config;
                self.config_normal_bindings = BindingTable::from_config(&self.config, "normal");
                self.config_space_bindings = BindingTable::from_config(&self.config, "space");
                self.config_chord_buffer.clear();
                self.rebuild_config_bindings();
                if let Some(dir) = config::themes_dir() {
                    let _ = self.themes.load_dir(&dir);
                }
                if let Err(err) = self.themes.set_active(&theme) {
                    self.status = format!("config reloaded (theme: {err})");
                } else {
                    self.status = "config reloaded".to_string();
                }
                self.refresh_subsystems_for_current_buffer();
            }
            Err(err) => self.status = format!("config: {err}"),
        }
    }

    fn set_theme(&mut self, name: &str) {
        match self.themes.set_active(name) {
            Ok(()) => {
                self.config.theme = name.to_string();
                self.status = format!("theme: {name}");
            }
            Err(err) => self.status = format!("theme: {err}"),
        }
    }

    fn git_status_suffix(&self) -> String {
        let mut parts = String::new();
        if let Some(branch) = &self.git_branch {
            parts.push_str(&format!("  git:{branch}"));
        }
        if self.git_blame_visible {
            parts.push_str(" blame");
            if let Some(annotation) = self.blame_status_for_cursor() {
                parts.push_str(&format!(" {annotation}"));
            }
        }
        parts
    }

    fn collab_status(&self) -> String {
        let Some(session) = &self.collab_session else {
            return String::new();
        };
        let mut status = format!("  collab:{} peers:{}", session.id(), session.peer_count());
        if let Some(latency) = session.latency_ms() {
            status.push_str(&format!(" {latency}ms"));
        }
        let peers: Vec<_> = session
            .remote_presence()
            .map(|peer| peer.name.as_str())
            .collect();
        if !peers.is_empty() {
            status.push_str(&format!(" [{}]", peers.join(",")));
        }
        status
    }

    fn with_collab_carets(&self, lines: Vec<String>) -> Vec<String> {
        let Some(session) = &self.collab_session else {
            return lines;
        };
        let peers: Vec<_> = session.remote_presence().cloned().collect();
        if peers.is_empty() {
            return lines;
        }
        let lines = collab_ui::overlay_remote_selections(
            &self.buffer,
            self.view.top_line,
            self.view.left_col,
            lines,
            &peers,
        );
        collab_ui::overlay_remote_carets(
            &self.buffer,
            self.view.top_line,
            self.view.left_col,
            lines,
            &peers,
        )
    }

    fn tick_config_watch(&mut self) {
        let Some(watcher) = &mut self.config_watcher else {
            return;
        };
        if watcher.should_reload() {
            self.reload_config();
            if self.status == "config reloaded" {
                self.status = "config auto-reloaded".to_string();
            }
        }
    }

    fn tick_collab(&mut self) {
        if self.collab_session.is_none() {
            return;
        }

        if self.collab_client.is_none()
            && self.collab_listener.is_none()
            && self.collab_join_addr.is_some()
            && self
                .collab_reconnect_after
                .is_none_or(|after| Instant::now() >= after)
        {
            self.try_collab_reconnect();
        }

        if let Some(listener) = self.collab_listener.as_ref() {
            if let Ok((stream, _)) = listener.accept() {
                if let Ok(mut transport) = TcpTransport::from_stream(stream) {
                    self.collab_on_connect(&mut transport);
                    self.collab_clients.push(transport);
                }
            }
        }

        let mut inbound = Vec::new();
        if let Some(client) = &mut self.collab_client {
            loop {
                match client.try_recv() {
                    Ok(Some(message)) => inbound.push(message),
                    Ok(None) => break,
                    Err(err) => {
                        self.mark_collab_disconnected(&err);
                        break;
                    }
                }
            }
        }
        for client in &mut self.collab_clients {
            while let Ok(Some(message)) = client.try_recv() {
                inbound.push(message);
            }
        }

        let mut replies = Vec::new();
        for message in inbound {
            if let Some(reply) = self.apply_collab_message(message) {
                replies.push(reply);
            }
        }

        let mut outgoing = if let Some(session) = &mut self.collab_session {
            let selection = self.buffer.selections().primary();
            let awareness = session.awareness_message(
                vec![(selection.anchor, selection.head)],
                (self.view.top_line, self.view.height),
            );
            let mut messages = session
                .drain_outgoing()
                .into_iter()
                .chain(std::iter::once(awareness))
                .collect::<Vec<_>>();
            let has_remote_peers = !session.peers().is_empty();
            let ping_due = self
                .collab_last_ping
                .is_none_or(|sent| sent.elapsed() >= Duration::from_secs(2));
            if has_remote_peers && ping_due {
                messages.push(session.ping_message());
                self.collab_last_ping = Some(Instant::now());
            }
            messages
        } else {
            Vec::new()
        };
        outgoing.extend(replies);
        let mut pending = std::mem::take(&mut self.collab_pending);
        outgoing.append(&mut pending);

        if let Some(client) = &mut self.collab_client {
            for message in &outgoing {
                let _ = client.send(message.clone());
            }
        }
        for client in &mut self.collab_clients {
            for message in &outgoing {
                let _ = client.send(message.clone());
            }
        }
    }

    fn collab_on_connect(&mut self, transport: &mut impl CollaborationTransport) {
        let Some(session) = &self.collab_session else {
            return;
        };
        let hello = CollabMessage::Hello {
            peer_id: session.local_peer().id,
            name: session.local_peer().name.clone(),
        };
        let _ = transport.send(hello);
        let _ = transport.send(session.sync_state_message());
    }

    fn apply_collab_message(&mut self, message: CollabMessage) -> Option<CollabMessage> {
        let peer_id = message.peer_id();
        let is_remote = self
            .collab_session
            .as_ref()
            .is_some_and(|session| session.local_peer().id != peer_id);
        let pong_reply = match &message {
            CollabMessage::Ping { sent_at_ms, .. } if is_remote => self
                .collab_session
                .as_ref()
                .map(|session| session.pong_message(*sent_at_ms)),
            _ => None,
        };
        let remote_edit = match &message {
            CollabMessage::Edit { operation, .. } if is_remote => Some(operation.clone()),
            _ => None,
        };
        let needs_document_sync = matches!(
            &message,
            CollabMessage::SyncState { .. } | CollabMessage::EncodedSync { .. }
        ) && is_remote;

        let hello_from_remote = matches!(&message, CollabMessage::Hello { .. }) && is_remote;
        if let Some(session) = &mut self.collab_session {
            session.receive(message);
        }

        if hello_from_remote && self.collab_listener.is_some() {
            if let Some(session) = &self.collab_session {
                let sync = session.sync_state_message();
                for client in &mut self.collab_clients {
                    let _ = client.send(sync.clone());
                }
            }
        }

        if let Some(operation) = remote_edit {
            self.apply_remote_collab_op(&operation);
        } else if needs_document_sync {
            self.sync_buffer_from_collab_document();
        }
        pong_reply
    }

    fn apply_remote_collab_op(&mut self, operation: &TextOperation) {
        self.collab_suppress_echo = true;
        match operation {
            TextOperation::Insert { index, text, .. } => {
                let edit = self.buffer.insert_with_edit(*index, text);
                self.after_buffer_edit(&edit);
            }
            TextOperation::Delete { start, end, .. } => {
                if let Some(edit) = self.buffer.remove_with_edit(*start..*end) {
                    self.after_buffer_edit(&edit);
                }
            }
        }
        self.collab_suppress_echo = false;
        self.dirty = true;
    }

    fn sync_buffer_from_collab_document(&mut self) {
        let text = self
            .collab_session
            .as_ref()
            .map(|session| session.document().text().to_string())
            .unwrap_or_default();
        if text == self.buffer.to_string() {
            return;
        }
        self.collab_suppress_echo = true;
        let len = self.buffer.len_chars();
        if len > 0 {
            if let Some(edit) = self.buffer.remove_with_edit(0..len) {
                self.after_buffer_edit(&edit);
            }
        }
        if !text.is_empty() {
            let edit = self.buffer.insert_with_edit(0, &text);
            self.after_buffer_edit(&edit);
        }
        self.collab_suppress_echo = false;
        self.selection_to_cursor();
        self.dirty = true;
    }

    fn collab_host(&mut self) {
        let bind_addr = "127.0.0.1:3478";
        let text = self.buffer.to_string();
        let session = CollaborationSession::host("local", &text);
        let id = session.id().to_string();
        self.collab_session = Some(session);
        self.collab_listener = TcpTransport::bind(bind_addr).ok();
        self.collab_client = None;
        self.collab_clients.clear();
        if self.collab_listener.is_some() {
            self.status = format!("collab host: {id} @ {bind_addr}");
        } else {
            self.status = format!("collab host: {id} (local only, bind failed)");
        }
    }

    fn collab_join(&mut self, target: &str) {
        let session = CollaborationSession::host("local", &self.buffer.to_string());
        self.collab_session = Some(session);

        if target.starts_with("ws://") || target.starts_with("wss://") {
            self.collab_join_addr = Some(target.to_string());
            self.collab_reconnect_after = None;
            self.collab_disconnect_notified = false;
            match WebSocketTransport::connect(target) {
                Ok(mut transport) => {
                    self.collab_on_connect(&mut transport);
                    self.collab_client = Some(CollabLink::Ws(transport));
                    self.collab_listener = None;
                    self.collab_clients.clear();
                    self.status = format!("collab joined: {target}");
                }
                Err(err) => {
                    self.collab_reconnect_after = Some(Instant::now() + Duration::from_secs(2));
                    self.status = format!("collab join failed: {err}");
                }
            }
        } else if target.contains(':') {
            self.collab_join_addr = Some(target.to_string());
            self.collab_reconnect_after = None;
            self.collab_disconnect_notified = false;
            match TcpTransport::connect(target) {
                Ok(mut transport) => {
                    self.collab_on_connect(&mut transport);
                    self.collab_client = Some(CollabLink::Tcp(transport));
                    self.collab_listener = None;
                    self.collab_clients.clear();
                    self.status = format!("collab joined: {target}");
                }
                Err(err) => {
                    self.collab_reconnect_after = Some(Instant::now() + Duration::from_secs(2));
                    self.status = format!("collab join failed: {err}");
                }
            }
        } else {
            self.collab_join_addr = None;
            self.status = format!("collab joined: {target} (use host:port or ws:// for network)");
        }
    }

    fn try_collab_reconnect(&mut self) {
        let Some(addr) = self.collab_join_addr.clone() else {
            return;
        };
        let connected = if addr.starts_with("ws://") || addr.starts_with("wss://") {
            WebSocketTransport::connect(&addr).map(|mut transport| {
                self.collab_on_connect(&mut transport);
                self.collab_client = Some(CollabLink::Ws(transport));
            })
        } else {
            TcpTransport::connect(&addr).map(|mut transport| {
                self.collab_on_connect(&mut transport);
                self.collab_client = Some(CollabLink::Tcp(transport));
            })
        };
        match connected {
            Ok(()) => {
                self.collab_reconnect_after = None;
                self.collab_disconnect_notified = false;
                self.status = format!("collab reconnected: {addr}");
            }
            Err(_) => {
                self.collab_reconnect_after = Some(Instant::now() + Duration::from_secs(3));
            }
        }
    }

    fn mark_collab_disconnected(&mut self, err: &anyhow::Error) {
        if self.collab_join_addr.is_none() {
            return;
        }
        let disconnect = err
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() != std::io::ErrorKind::WouldBlock)
            || err.to_string().contains("disconnected");
        if !disconnect {
            return;
        }
        self.collab_client = None;
        self.collab_reconnect_after = Some(Instant::now() + Duration::from_secs(2));
        if !self.collab_disconnect_notified {
            self.status = "collab: disconnected, reconnecting…".to_string();
            self.collab_disconnect_notified = true;
        }
    }

    fn collab_leave(&mut self) {
        if let (Some(session), Some(client)) = (&self.collab_session, &mut self.collab_client) {
            let _ = client.send(CollabMessage::Leave {
                peer_id: session.local_peer().id,
            });
        }
        for client in &mut self.collab_clients {
            if let Some(session) = &self.collab_session {
                let _ = client.send(CollabMessage::Leave {
                    peer_id: session.local_peer().id,
                });
            }
        }
        self.collab_session = None;
        self.collab_listener = None;
        self.collab_clients.clear();
        self.collab_client = None;
        self.collab_last_ping = None;
        self.collab_join_addr = None;
        self.collab_reconnect_after = None;
        self.collab_disconnect_notified = false;
        self.status = "collab left".to_string();
    }

    fn collab_chat(&mut self, text: String) {
        if let Some(session) = &self.collab_session {
            let message = session.chat_message(text);
            self.collab_pending.push(message);
            self.status = "collab chat sent".to_string();
        } else {
            self.status = "collab: not in a session".to_string();
        }
    }

    fn which_key_text(&self) -> String {
        whichkey::render(&self.which_key_entries_for_mode())
    }

    fn undo_tree_lines(&self) -> Vec<String> {
        let nodes = self.history.nodes();
        if nodes.len() <= 1 {
            return vec![" Undo Tree — no history".to_string()];
        }
        let current_id = self.history.current_node().id;
        let max_lines = self.view.height.saturating_sub(2).max(10);
        let mut lines = Vec::new();
        lines.push(format!(
            " Undo Tree ({} nodes) — Esc to close",
            self.history.len()
        ));
        Self::traverse_tree(nodes, 0, current_id, "", &mut lines, max_lines);
        while lines.len() < max_lines.min(40) {
            lines.push(String::new());
        }
        lines
    }

    fn traverse_tree(
        nodes: &[HistoryNode],
        node_id: usize,
        current_id: usize,
        prefix: &str,
        out: &mut Vec<String>,
        max: usize,
    ) {
        if out.len() >= max {
            return;
        }
        let node = &nodes[node_id];
        let marker = if node_id == current_id { "●" } else { "○" };

        if node_id == 0 {
            out.push(format!("{}● root", prefix));
        } else {
            let label = Self::node_label(node);
            out.push(format!("{}{} {}", prefix, marker, label));
        }

        for (i, child_id) in node.children.iter().enumerate() {
            if out.len() >= max {
                break;
            }
            let bar = if i + 1 < node.children.len() {
                "│   "
            } else {
                "    "
            };
            let fork = if i + 1 < node.children.len() {
                "├── "
            } else {
                "└── "
            };
            let child_node = &nodes[*child_id];
            let child_label = Self::node_label(child_node);
            let child_marker = if *child_id == current_id {
                "●"
            } else {
                "○"
            };
            out.push(format!(
                "{}{}{} {}",
                prefix, fork, child_marker, child_label
            ));
            let gp = format!("{}{}", prefix, bar);
            for (j, grand_id) in child_node.children.iter().enumerate() {
                if out.len() >= max {
                    break;
                }
                let gfork = if j + 1 < child_node.children.len() {
                    "├── "
                } else {
                    "└── "
                };
                let gchild = &nodes[*grand_id];
                let glabel = Self::node_label(gchild);
                let gmarker = if *grand_id == current_id {
                    "●"
                } else {
                    "○"
                };
                out.push(format!("{}{}{} {}", gp, gfork, gmarker, glabel));
            }
        }
    }

    fn node_label(node: &HistoryNode) -> String {
        match &node.entry {
            None => "root".to_string(),
            Some(HistoryEntry::Insert { idx, text }) => {
                let t = if text.len() > 24 {
                    format!("{}…", &text[..24])
                } else {
                    text.clone()
                };
                format!("+ \"{}\" @{idx}", t.escape_debug())
            }
            Some(HistoryEntry::Delete { idx, text }) => {
                let t = if text.len() > 24 {
                    format!("{}…", &text[..24])
                } else {
                    text.clone()
                };
                format!("- \"{}\" @{idx}", t.escape_debug())
            }
        }
    }

    fn dispatch_plugin_event(&mut self, event: PluginEvent) {
        self.ensure_plugins_discovered();
        if self.plugin_manager.plugin_count() == 0 {
            return;
        }
        match self.plugin_manager.dispatch(&event) {
            Ok(_) => {
                if let Some(message) = self.plugin_manager.messages().last() {
                    self.status = format!("plugin: {message}");
                }
            }
            Err(err) => self.status = format!("plugin: {err}"),
        }
    }

    fn apply_plugin_edits(&mut self) {
        let edits = self.plugin_manager.drain_edits();
        for (start, end, text) in edits {
            if end > start {
                self.buffer.remove(start..end);
            }
            self.buffer.insert(start, &text);
        }
    }

    fn buffer_snapshot(&self) -> BufferSnapshot {
        BufferSnapshot {
            path: self.filepath.clone(),
            selections: self.buffer.selections().selections().to_vec(),
            visible_lines: self
                .buffer
                .visible_lines(self.view.top_line, self.view.height),
        }
    }
}

fn undo_history_path(path: &Path) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .hash(&mut hasher);
    default_data_dir()
        .join("undo")
        .join(format!("{:016x}.bin", hasher.finish()))
}

fn merge_which_key_entries(
    defaults: Vec<WhichKeyEntry>,
    config: Vec<(String, String)>,
) -> Vec<WhichKeyEntry> {
    use std::collections::HashMap;
    let mut merged: HashMap<String, String> = defaults
        .into_iter()
        .map(|entry| (entry.key, entry.label))
        .collect();
    for (key, label) in config {
        merged.insert(key, label);
    }
    let mut entries: Vec<_> = merged
        .into_iter()
        .map(|(key, label)| WhichKeyEntry { key, label })
        .collect();
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    entries
}

fn mode_plugin_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Normal => "normal",
        Mode::Insert => "insert",
        Mode::Select => "select",
        Mode::Goto => "goto",
        Mode::Match => "match",
        Mode::Space => "space",
        Mode::View => "view",
        Mode::Command => "command",
        Mode::Picker => "picker",
        Mode::Search => "search",
    }
}

fn diagnostic_rank(severity: Option<DiagnosticSeverity>) -> u8 {
    match severity {
        Some(DiagnosticSeverity::Error) => 0,
        Some(DiagnosticSeverity::Warning) => 1,
        Some(DiagnosticSeverity::Information) => 2,
        Some(DiagnosticSeverity::Hint) => 3,
        None => 4,
    }
}

fn default_data_dir() -> PathBuf {
    if let Some(dir) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir).join("jet");
    }
    if let Some(dir) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(dir).join("jet");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("jet");
    }
    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".jet")
}

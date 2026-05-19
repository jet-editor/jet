use crate::plugin::{
    api::{HostCommand, PluginEvent},
    manifest::{PluginCommandSpec, PluginManifest},
};
use anyhow::{anyhow, bail, Result};
use std::{
    fs,
    path::PathBuf,
    str,
    time::{Duration, Instant},
};
use wasmtime::{
    Caller, Config, Engine, Extern, Instance, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc,
};
type WasmtimeResult<T> = wasmtime::Result<T>;

#[derive(Debug, Clone)]
pub struct RuntimeLimits {
    pub hook_timeout: Duration,
    pub memory_limit_bytes: usize,
    pub fuel_per_hook: u64,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            hook_timeout: Duration::from_millis(50),
            memory_limit_bytes: 64 * 1024 * 1024,
            fuel_per_hook: 1_000_000,
        }
    }
}

#[derive(Debug)]
pub struct WasmPlugin {
    pub manifest: PluginManifest,
    pub wasm_path: PathBuf,
    pub limits: RuntimeLimits,
    loaded: bool,
    engine: Option<Engine>,
    module: Option<Module>,
}

impl WasmPlugin {
    pub fn new(manifest: PluginManifest, wasm_path: PathBuf, limits: RuntimeLimits) -> Self {
        Self {
            manifest,
            wasm_path,
            limits,
            loaded: false,
            engine: None,
            module: None,
        }
    }

    pub fn load(&mut self) -> Result<()> {
        if !self.wasm_path.exists() {
            bail!("plugin wasm does not exist: {}", self.wasm_path.display());
        }
        let bytes = fs::read(&self.wasm_path)?;
        if bytes.len() > self.limits.memory_limit_bytes {
            bail!(
                "plugin wasm is larger than memory limit: {} > {}",
                bytes.len(),
                self.limits.memory_limit_bytes
            );
        }
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, &bytes)
            .map_err(|err| anyhow!("compile plugin wasm {}: {err}", self.wasm_path.display()))?;
        validate_imports(&module)?;
        self.loaded = true;
        self.engine = Some(engine);
        self.module = Some(module);
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn call_hook(&mut self, event: &PluginEvent) -> Result<HookOutcome> {
        if !self.loaded {
            self.load()?;
        }
        let start = Instant::now();
        let mut commands = self.call_guest_hook(event)?;
        commands.extend(self.commands_for_event(event));
        let elapsed = start.elapsed();
        if elapsed > self.limits.hook_timeout {
            bail!(
                "plugin {} hook exceeded timeout {:?}",
                self.manifest.name,
                self.limits.hook_timeout
            );
        }
        Ok(HookOutcome { elapsed, commands })
    }

    fn call_guest_hook(&self, event: &PluginEvent) -> Result<Vec<HostCommand>> {
        let Some(engine) = &self.engine else {
            bail!("plugin {} has no initialized engine", self.manifest.name);
        };
        let Some(module) = &self.module else {
            bail!("plugin {} has no compiled module", self.manifest.name);
        };
        let mut state = PluginStoreState::new(&self.manifest, self.limits.clone());
        if let Some(lines) = event_visible_lines(event) {
            state.set_buffer_snapshot(lines);
        }
        let mut store = Store::new(engine, state);
        store.limiter(|state| &mut state.store_limits);
        store.set_fuel(self.limits.fuel_per_hook)?;
        let linker = host_linker(engine)?;
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|err| anyhow!("instantiate plugin {}: {err}", self.manifest.name))?;
        if let Some(hook) = exported_hook(&mut store, &instance, event_name(event))? {
            hook.call(&mut store, ())
                .map_err(|err| anyhow!("plugin {} hook trap: {err}", self.manifest.name))?;
        }
        Ok(store.data().commands.clone())
    }

    fn commands_for_event(&self, event: &PluginEvent) -> Vec<HostCommand> {
        let event_name = event_name(event);
        self.manifest
            .commands
            .iter()
            .filter(|command| command.event == event_name)
            .filter_map(command_from_spec)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookOutcome {
    pub elapsed: Duration,
    pub commands: Vec<HostCommand>,
}

pub fn wasm_runtime_available() -> bool {
    true
}

fn event_name(event: &PluginEvent) -> &'static str {
    match event {
        PluginEvent::BufferOpen(_) => "buffer_open",
        PluginEvent::BufferSave(_) => "buffer_save",
        PluginEvent::CursorMove(_) => "cursor_move",
        PluginEvent::ModeChange { .. } => "mode_change",
    }
}

fn event_visible_lines(event: &PluginEvent) -> Option<Vec<String>> {
    match event {
        PluginEvent::BufferOpen(snap)
        | PluginEvent::BufferSave(snap)
        | PluginEvent::CursorMove(snap) => Some(snap.visible_lines.clone()),
        PluginEvent::ModeChange { .. } => None,
    }
}

fn command_from_spec(spec: &PluginCommandSpec) -> Option<HostCommand> {
    match spec.kind.as_str() {
        "message" => Some(HostCommand::ShowMessage(spec.text.clone())),
        "virtual_text" => Some(HostCommand::SetVirtualText {
            line: spec.line,
            text: spec.text.clone(),
            group: default_group(spec),
        }),
        "gutter_mark" => Some(HostCommand::SetGutterMark {
            line: spec.line,
            mark: spec.mark.clone(),
            group: default_group(spec),
        }),
        "command" => Some(HostCommand::RegisterCommand {
            name: spec.name.clone(),
            description: spec.description.clone(),
        }),
        "keymap" => Some(HostCommand::RegisterKeymap {
            mode: spec.mode.clone(),
            key: spec.key.clone(),
            command: spec.command.clone(),
        }),
        _ => None,
    }
}

fn default_group(spec: &PluginCommandSpec) -> String {
    if spec.group.is_empty() {
        "plugin".to_string()
    } else {
        spec.group.clone()
    }
}

#[derive(Debug)]
struct PluginStoreState {
    permissions: Vec<String>,
    store_limits: StoreLimits,
    commands: Vec<HostCommand>,
    logs: Vec<String>,
    buffer_lines: Vec<String>,
}

impl PluginStoreState {
    fn new(manifest: &PluginManifest, limits: RuntimeLimits) -> Self {
        Self {
            permissions: manifest.permissions.clone(),
            store_limits: StoreLimitsBuilder::new()
                .memory_size(limits.memory_limit_bytes)
                .memories(1)
                .tables(2)
                .instances(1)
                .trap_on_grow_failure(true)
                .build(),
            commands: Vec::new(),
            logs: Vec::new(),
            buffer_lines: Vec::new(),
        }
    }

    fn set_buffer_snapshot(&mut self, lines: Vec<String>) {
        self.buffer_lines = lines;
    }

    fn can_write_ui(&self) -> bool {
        self.permissions.iter().any(|permission| {
            matches!(
                permission.as_str(),
                "ui:write" | "host:commands" | "commands:emit"
            )
        })
    }

    fn can_write_buffer(&self) -> bool {
        self.permissions
            .iter()
            .any(|p| p == "buffer:write" || p == "host:commands")
    }
}

const ALLOWED_IMPORTS: &[&str] = &[
    "emit_message",
    "emit_virtual_text",
    "emit_gutter_mark",
    "register_command",
    "register_keymap",
    "host_log",
    "host_read_line",
    "host_apply_edit",
];

fn validate_imports(module: &Module) -> Result<()> {
    for import in module.imports() {
        if import.module() != "jet" {
            bail!(
                "plugin imports unsupported module {}::{}",
                import.module(),
                import.name()
            );
        }
        if !ALLOWED_IMPORTS.contains(&import.name()) {
            bail!("plugin imports unknown function jet::{}", import.name());
        }
    }
    Ok(())
}

fn host_linker(engine: &Engine) -> Result<Linker<PluginStoreState>> {
    let mut linker = Linker::new(engine);
    linker.func_wrap(
        "jet",
        "emit_message",
        |mut caller: Caller<'_, PluginStoreState>, ptr: i32, len: i32| -> WasmtimeResult<()> {
            let text = read_guest_string(&mut caller, ptr, len)?;
            push_ui_command(&mut caller, HostCommand::ShowMessage(text))
        },
    )?;
    linker.func_wrap(
        "jet",
        "emit_virtual_text",
        |mut caller: Caller<'_, PluginStoreState>,
         line: i32,
         ptr: i32,
         len: i32|
         -> WasmtimeResult<()> {
            let text = read_guest_string(&mut caller, ptr, len)?;
            push_ui_command(
                &mut caller,
                HostCommand::SetVirtualText {
                    line: checked_line(line)?,
                    text,
                    group: "plugin".to_string(),
                },
            )
        },
    )?;
    linker.func_wrap(
        "jet",
        "emit_gutter_mark",
        |mut caller: Caller<'_, PluginStoreState>,
         line: i32,
         ptr: i32,
         len: i32|
         -> WasmtimeResult<()> {
            let mark = read_guest_string(&mut caller, ptr, len)?;
            push_ui_command(
                &mut caller,
                HostCommand::SetGutterMark {
                    line: checked_line(line)?,
                    mark,
                    group: "plugin".to_string(),
                },
            )
        },
    )?;
    linker.func_wrap(
        "jet",
        "register_command",
        |mut caller: Caller<'_, PluginStoreState>,
         name_ptr: i32,
         name_len: i32,
         description_ptr: i32,
         description_len: i32|
         -> WasmtimeResult<()> {
            let name = read_guest_string(&mut caller, name_ptr, name_len)?;
            let description = read_guest_string(&mut caller, description_ptr, description_len)?;
            push_ui_command(
                &mut caller,
                HostCommand::RegisterCommand { name, description },
            )
        },
    )?;
    linker.func_wrap(
        "jet",
        "register_keymap",
        |mut caller: Caller<'_, PluginStoreState>,
         mode_ptr: i32,
         mode_len: i32,
         key_ptr: i32,
         key_len: i32,
         command_ptr: i32,
         command_len: i32|
         -> WasmtimeResult<()> {
            let mode = read_guest_string(&mut caller, mode_ptr, mode_len)?;
            let key = read_guest_string(&mut caller, key_ptr, key_len)?;
            let command = read_guest_string(&mut caller, command_ptr, command_len)?;
            push_ui_command(
                &mut caller,
                HostCommand::RegisterKeymap { mode, key, command },
            )
        },
    )?;

    linker.func_wrap(
        "jet",
        "host_log",
        |mut caller: Caller<'_, PluginStoreState>, ptr: i32, len: i32| -> WasmtimeResult<()> {
            let text = read_guest_string(&mut caller, ptr, len)?;
            caller.data_mut().logs.push(text);
            Ok(())
        },
    )?;

    linker.func_wrap(
        "jet",
        "host_read_line",
        |mut caller: Caller<'_, PluginStoreState>,
         line: i32,
         out_ptr: i32,
         out_max: i32|
         -> WasmtimeResult<i32> {
            let line = checked_offset(line)?;
            let out_max = checked_offset(out_max)?;
            if out_max == 0 {
                return Ok(-1);
            }
            let line_str = {
                let data = caller.data();
                if line >= data.buffer_lines.len() {
                    return Ok(-1);
                }
                data.buffer_lines[line].clone()
            };
            let copy_len = line_str.len().min((out_max.saturating_sub(1)).max(0));
            let memory = match caller.get_export("memory") {
                Some(Extern::Memory(m)) => m,
                _ => wasmtime::bail!("plugin must export memory"),
            };
            let dest = checked_offset(out_ptr)?;
            memory
                .write(&mut caller, dest, &line_str.as_bytes()[..copy_len])
                .map_err(|_| wasmtime::format_err!("host_read_line write out of bounds"))?;
            memory.write(&mut caller, dest + copy_len, &[0u8]).ok();
            Ok(copy_len as i32)
        },
    )?;

    linker.func_wrap(
        "jet",
        "host_apply_edit",
        |mut caller: Caller<'_, PluginStoreState>,
         start: i32,
         end: i32,
         ptr: i32,
         len: i32|
         -> WasmtimeResult<()> {
            if !caller.data().can_write_buffer() {
                wasmtime::bail!("plugin tried to edit buffer without buffer:write permission");
            }
            let text = read_guest_string(&mut caller, ptr, len)?;
            caller.data_mut().commands.push(HostCommand::ApplyEdit {
                start: checked_offset(start)?,
                end: checked_offset(end)?,
                text,
            });
            Ok(())
        },
    )?;

    Ok(linker)
}

fn exported_hook(
    store: &mut Store<PluginStoreState>,
    instance: &Instance,
    event: &str,
) -> Result<Option<TypedFunc<(), ()>>> {
    let names = [format!("on_{event}"), event.to_string()];
    for name in names {
        if let Ok(func) = instance.get_typed_func::<(), ()>(&mut *store, &name) {
            return Ok(Some(func));
        }
    }
    Ok(None)
}

fn read_guest_string(
    caller: &mut Caller<'_, PluginStoreState>,
    ptr: i32,
    len: i32,
) -> WasmtimeResult<String> {
    let ptr = checked_offset(ptr)?;
    let len = checked_offset(len)?;
    if len > 64 * 1024 {
        wasmtime::bail!("plugin string argument exceeds 64KiB");
    }
    let memory = match caller.get_export("memory") {
        Some(Extern::Memory(memory)) => memory,
        _ => wasmtime::bail!("plugin must export memory for host string imports"),
    };
    let mut bytes = vec![0u8; len];
    memory
        .read(&*caller, ptr, &mut bytes)
        .map_err(|_| wasmtime::format_err!("plugin memory read out of bounds"))?;
    str::from_utf8(&bytes)
        .map(str::to_string)
        .map_err(|err| wasmtime::format_err!("plugin string is not UTF-8: {err}"))
}

fn push_ui_command(
    caller: &mut Caller<'_, PluginStoreState>,
    command: HostCommand,
) -> WasmtimeResult<()> {
    if !caller.data().can_write_ui() {
        wasmtime::bail!("plugin tried to emit host UI command without ui:write permission");
    }
    caller.data_mut().commands.push(command);
    Ok(())
}

fn checked_offset(value: i32) -> WasmtimeResult<usize> {
    usize::try_from(value)
        .map_err(|_| wasmtime::format_err!("plugin passed a negative pointer/length"))
}

fn checked_line(value: i32) -> WasmtimeResult<usize> {
    usize::try_from(value)
        .map_err(|_| wasmtime::format_err!("plugin passed a negative line number"))
}

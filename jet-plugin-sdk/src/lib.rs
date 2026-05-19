//! Jet Editor Plugin SDK
//!
//! This crate provides safe Rust bindings for creating Jet editor plugins.
//! Compile your plugin for the `wasm32-unknown-unknown` target.
//!
//! # Example
//!
//! ```ignore
//! use jet_plugin_sdk::{emit_message, register_command};
//!
//! #[no_mangle]
//! pub extern "C" fn on_buffer_open() {
//!     emit_message("Hello from plugin!");
//! }
//!
//! #[no_mangle]
//! pub extern "C" fn on_init() {
//!     register_command("my:command", "My custom command");
//! }
//! ```

/// Emit a status bar message visible to the user.
pub fn emit_message(msg: &str) {
    let ptr = msg.as_ptr();
    let len = msg.len();
    unsafe {
        ffi_emit_message(ptr as *const core::ffi::c_void, len as i32);
    }
}

/// Set virtual text (inline annotation) at the end of the given line.
pub fn emit_virtual_text(line: usize, text: &str) {
    let ptr = text.as_ptr();
    let len = text.len();
    unsafe {
        ffi_emit_virtual_text(line as i32, ptr as *const core::ffi::c_void, len as i32);
    }
}

/// Set a gutter marker character at the given line.
pub fn emit_gutter_mark(line: usize, mark: &str) {
    let ptr = mark.as_ptr();
    let len = mark.len();
    unsafe {
        ffi_emit_gutter_mark(line as i32, ptr as *const core::ffi::c_void, len as i32);
    }
}

/// Register a new editor command that the user can invoke via `:command`.
pub fn register_command(name: &str, description: &str) {
    let name_ptr = name.as_ptr();
    let name_len = name.len();
    let desc_ptr = description.as_ptr();
    let desc_len = description.len();
    unsafe {
        ffi_register_command(
            name_ptr as *const core::ffi::c_void,
            name_len as i32,
            desc_ptr as *const core::ffi::c_void,
            desc_len as i32,
        );
    }
}

/// Register a keybinding for a given mode (e.g. "normal", "insert").
pub fn register_keymap(mode: &str, key: &str, command: &str) {
    let mode_ptr = mode.as_ptr();
    let mode_len = mode.len();
    let key_ptr = key.as_ptr();
    let key_len = key.len();
    let cmd_ptr = command.as_ptr();
    let cmd_len = command.len();
    unsafe {
        ffi_register_keymap(
            mode_ptr as *const core::ffi::c_void,
            mode_len as i32,
            key_ptr as *const core::ffi::c_void,
            key_len as i32,
            cmd_ptr as *const core::ffi::c_void,
            cmd_len as i32,
        );
    }
}

/// Append a debug log message to the editor's status line.
/// Always available, no permission required.
pub fn host_log(msg: &str) {
    let ptr = msg.as_ptr();
    let len = msg.len();
    unsafe {
        ffi_host_log(ptr as *const core::ffi::c_void, len as i32);
    }
}

/// Read a line from the current editor buffer snapshot.
/// Returns `None` if the line number is out of range.
/// The line text is capped at 4096 bytes.
pub fn host_read_line(line: usize) -> Option<String> {
    let mut buf = [0u8; 4096];
    let len = unsafe { ffi_host_read_line(line as i32, buf.as_mut_ptr() as i32, 4096) };
    if len < 0 {
        return None;
    }
    let len = len as usize;
    if len > buf.len() {
        return None;
    }
    Some(String::from_utf8_lossy(&buf[..len]).into_owned())
}

/// Request an edit in the current buffer.
/// Requires the `buffer:write` or `host:commands` permission.
pub fn host_apply_edit(start: usize, end: usize, text: &str) {
    let ptr = text.as_ptr();
    let len = text.len();
    unsafe {
        ffi_host_apply_edit(
            start as i32,
            end as i32,
            ptr as *const core::ffi::c_void,
            len as i32,
        );
    }
}

// On wasm32, the functions come from the host via import module "jet".
// On other platforms, provide no-op stubs so the crate compiles for testing.
#[cfg(target_arch = "wasm32")]
mod ffi {
    #[link(wasm_import_module = "jet")]
    extern "C" {
        pub fn ffi_emit_message(ptr: *const core::ffi::c_void, len: i32);
        pub fn ffi_emit_virtual_text(line: i32, ptr: *const core::ffi::c_void, len: i32);
        pub fn ffi_emit_gutter_mark(line: i32, ptr: *const core::ffi::c_void, len: i32);
        pub fn ffi_register_command(
            name_ptr: *const core::ffi::c_void,
            name_len: i32,
            desc_ptr: *const core::ffi::c_void,
            desc_len: i32,
        );
        pub fn ffi_register_keymap(
            mode_ptr: *const core::ffi::c_void,
            mode_len: i32,
            key_ptr: *const core::ffi::c_void,
            key_len: i32,
            cmd_ptr: *const core::ffi::c_void,
            cmd_len: i32,
        );
        pub fn ffi_host_log(ptr: *const core::ffi::c_void, len: i32);
        pub fn ffi_host_read_line(line: i32, out_ptr: i32, out_max: i32) -> i32;
        pub fn ffi_host_apply_edit(start: i32, end: i32, ptr: *const core::ffi::c_void, len: i32);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod ffi {
    pub unsafe extern "C" fn ffi_emit_message(_ptr: *const core::ffi::c_void, _len: i32) {}
    pub unsafe extern "C" fn ffi_emit_virtual_text(
        _line: i32,
        _ptr: *const core::ffi::c_void,
        _len: i32,
    ) {
    }
    pub unsafe extern "C" fn ffi_emit_gutter_mark(
        _line: i32,
        _ptr: *const core::ffi::c_void,
        _len: i32,
    ) {
    }
    pub unsafe extern "C" fn ffi_register_command(
        _name_ptr: *const core::ffi::c_void,
        _name_len: i32,
        _desc_ptr: *const core::ffi::c_void,
        _desc_len: i32,
    ) {
    }
    pub unsafe extern "C" fn ffi_register_keymap(
        _mode_ptr: *const core::ffi::c_void,
        _mode_len: i32,
        _key_ptr: *const core::ffi::c_void,
        _key_len: i32,
        _cmd_ptr: *const core::ffi::c_void,
        _cmd_len: i32,
    ) {
    }
    pub unsafe extern "C" fn ffi_host_log(_ptr: *const core::ffi::c_void, _len: i32) {}
    pub unsafe extern "C" fn ffi_host_read_line(_line: i32, _out_ptr: i32, _out_max: i32) -> i32 {
        -1
    }
    pub unsafe extern "C" fn ffi_host_apply_edit(
        _start: i32,
        _end: i32,
        _ptr: *const core::ffi::c_void,
        _len: i32,
    ) {
    }
}

use ffi::*;

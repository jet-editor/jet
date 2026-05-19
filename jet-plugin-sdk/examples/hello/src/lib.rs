// Example Jet plugin that shows a greeting on buffer open.
// Compile with: cargo build --target wasm32-unknown-unknown --release
// Then copy plugin.toml and .wasm to ~/.config/jet/plugins/hello/

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::format;
use jet_plugin_sdk::{emit_message, register_command};

#[no_mangle]
pub extern "C" fn on_buffer_open() {
    emit_message("Hello from Jet plugin!");
}

#[no_mangle]
pub extern "C" fn on_cursor_move() {
    // This is called on every cursor move (if enabled in manifest).
    // Be careful with performance.
}

#[no_mangle]
pub extern "C" fn on_buffer_save() {
    emit_message("Buffer saved!");
}

#[no_mangle]
pub extern "C" fn on_init() {
    register_command("hello:greet", "Emit a greeting message");
}

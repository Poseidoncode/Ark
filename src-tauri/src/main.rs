//!
//! Application entry point.
//! Starts the Rust backend, providing Tauri command registration and component setup.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ark_lib::run()
}

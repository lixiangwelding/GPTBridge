// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod startup_path;

fn main() {
    // Before Tauri, Tokio, worker initialization or any application threads.
    startup_path::bootstrap();
    if let Some(code) = coding_tools_personal_runtime::worker::run_from_args() {
        std::process::exit(code);
    }
    if let Some(code) = coding_tools_mcp_desktop_lib::personal_cli() {
        std::process::exit(code);
    }
    coding_tools_mcp_desktop_lib::run()
}

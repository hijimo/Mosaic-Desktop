pub mod auth;
pub mod commands;
pub mod config;
pub mod core;
pub mod exec;
pub mod execpolicy;
pub mod file_search;
pub mod netproxy;
pub mod protocol;
pub mod provider;
pub mod pty;
pub mod responses_api_proxy;
pub mod secrets;
pub mod shell_command;
pub mod shell_escalation;
pub mod state;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use commands::AppState;
use config::ConfigLayerStack;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load ~/.codex/config.toml as the User layer
    let mut config = ConfigLayerStack::new();
    if let Some(home) = std::env::var_os("HOME") {
        let path = std::path::Path::new(&home).join(".codex/config.toml");
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(parsed) = config::deserialize_toml(&content) {
                config.add_layer(config::ConfigLayer::User, parsed);
            }
        }
    }

    let app_state = AppState {
        threads: Arc::new(Mutex::new(HashMap::new())),
        config: Arc::new(Mutex::new(config)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::submit_op,
            commands::thread_start,
            commands::thread_list,
            commands::thread_archive,
            commands::thread_fork,
            commands::fuzzy_file_search,
            commands::get_config,
            commands::update_config,
            commands::get_cwd,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

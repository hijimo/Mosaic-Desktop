pub mod auth;
pub mod commands;
pub mod config;
pub mod core;
pub mod exec;
pub mod execpolicy;
pub mod file_search;
pub mod mosaic_api;
pub mod mosaic_client;
pub mod netproxy;
pub mod protocol;
pub mod provider;
pub mod pty;
pub mod responses_api_proxy;
pub mod rmcp_client;
pub mod secrets;
pub mod shell_command;
pub mod shell_escalation;
pub mod state;
pub mod stream_parser;

#[cfg(test)]
mod frontend_compat_tests;

#[cfg(test)]
mod e2e_smoke_tests;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use commands::AppState;
use config::ConfigLayerStack;
use core::state_db::StateDb;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Load ~/.codex/config.toml as the User layer
    let mut config = ConfigLayerStack::new();
    if let Some(home) = std::env::var_os("HOME") {
        let path = std::path::Path::new(&home).join(".codex/config.toml");
        if let Ok(content) = std::fs::read_to_string(&path) {
            // Strip sections that cause parse errors (e.g. shell_environment_policy
            // with scalar value instead of expected array, mcp_servers with inline tables)
            let mut skip = false;
            let mut cleaned = Vec::new();
            for line in content.lines() {
                if line.starts_with("[shell_environment_policy")
                    || line.starts_with("[mcp_servers")
                {
                    skip = true;
                    continue;
                }
                if skip {
                    if line.starts_with('[')
                        && !line.starts_with("[shell_environment_policy")
                        && !line.starts_with("[mcp_servers")
                    {
                        skip = false;
                    } else {
                        continue;
                    }
                }
                cleaned.push(line);
            }
            if let Ok(parsed) = config::deserialize_toml(&cleaned.join("\n")) {
                config.add_layer(config::ConfigLayer::User, parsed);
            }
        }
    }

    let mosaic_home = dirs::home_dir()
        .map(|h| h.join(".mosaic"))
        .unwrap_or_else(|| std::path::PathBuf::from(".mosaic"));
    let db = StateDb::open(&mosaic_home.join("state.db"))
        .expect("failed to open state database");

    let app_state = AppState {
        threads: Arc::new(Mutex::new(HashMap::new())),
        thread_meta: Arc::new(Mutex::new(HashMap::new())),
        config: Arc::new(Mutex::new(config)),
        db,
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::submit_op,
            commands::thread_start,
            commands::thread_list,
            commands::thread_get_info,
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

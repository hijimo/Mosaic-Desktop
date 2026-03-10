pub mod commands;
pub mod config;
pub mod core;
pub mod exec;
pub mod execpolicy;
pub mod netproxy;
pub mod protocol;
pub mod provider;
pub mod secrets;
pub mod shell_command;
pub mod state;

use std::sync::Arc;
use tokio::sync::Mutex;

use commands::AppState;
use config::ConfigLayerStack;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (sq_tx, sq_rx) = async_channel::unbounded();
    let (eq_tx, eq_rx) = async_channel::unbounded();

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
        sq_tx,
        eq_rx: Arc::new(Mutex::new(eq_rx)),
        config: Arc::new(Mutex::new(config.clone())),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::submit_op,
            commands::poll_events,
            commands::get_config,
            commands::update_config,
            commands::get_cwd,
        ])
        .setup(move |_app| {
            let cwd = std::env::current_dir().unwrap_or_default();
            let codex = core::codex::Codex::new(sq_rx, eq_tx, config, cwd);
            tauri::async_runtime::spawn(async move {
                let _ = codex.run().await;
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

mod app_state;
mod commands;
mod config_store;
mod core_runtime;
mod core_update;
mod domain;
mod models;
mod system_proxy;

use app_state::SharedState;
use config_store::ConfigStore;
use core_runtime::RuntimeManager;
use core_update::CorePaths;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let store = ConfigStore::bootstrap(app.handle())?;
            let core_paths = CorePaths {
                bin_root: std::path::PathBuf::from(store.paths().bin.clone()),
            };

            app.manage(SharedState {
                store,
                core_paths,
                runtime: RuntimeManager::new(),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::save_app_config,
            commands::import_share_links,
            commands::save_subscription,
            commands::refresh_subscription,
            commands::generate_config_preview,
            commands::check_core_assets,
            commands::download_core_asset,
            commands::start_core,
            commands::stop_core,
            commands::enable_system_proxy,
            commands::disable_system_proxy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

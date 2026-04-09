mod app_state;
mod commands;
mod config_store;
mod core_runtime;
mod core_update;
mod domain;
mod models;
mod network_probe;
mod system_proxy;
mod tun;

use app_state::SharedState;
use config_store::ConfigStore;
use core_runtime::RuntimeManager;
use core_update::CorePaths;
use std::{sync::Mutex, thread, time::Duration};
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
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
            let core_status_cache = crate::core_update::list_local_core_statuses(&core_paths)?;

            app.manage(SharedState {
                store,
                core_paths,
                runtime: RuntimeManager::new(),
                core_status_cache: Mutex::new(core_status_cache),
                subscription_refresh_lock: Mutex::new(()),
            });

            start_subscription_scheduler(app.handle().clone());
            start_core_status_scheduler(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::get_app_status_light,
            commands::save_app_config,
            commands::initialize_builtin_routing,
            commands::import_routing_template_url,
            commands::save_routing_item,
            commands::remove_routing_item,
            commands::set_default_routing_item,
            commands::import_routing_rules,
            commands::export_routing_rules,
            commands::move_routing_rule,
            commands::import_share_links,
            commands::preview_import_result,
            commands::import_full_config,
            commands::save_subscription,
            commands::remove_subscription,
            commands::refresh_subscription,
            commands::refresh_all_subscriptions,
            commands::refresh_all_subscriptions_in_background,
            commands::remove_profile,
            commands::select_profile,
            commands::generate_config_preview,
            commands::check_core_assets,
            commands::download_core_asset,
            commands::start_core,
            commands::stop_core,
            commands::restart_core,
            commands::enable_system_proxy,
            commands::disable_system_proxy,
            commands::probe_current_outbound,
            commands::get_clash_proxy_groups,
            commands::get_clash_proxy_providers,
            commands::select_clash_proxy,
            commands::update_clash_rule_mode,
            commands::reload_clash_config,
            commands::close_clash_connection,
            commands::refresh_clash_proxy_provider,
            commands::get_clash_connections,
            commands::test_clash_proxy_delay,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
            cleanup_runtime(app_handle);
        }
    });
}

fn cleanup_runtime(app: &tauri::AppHandle) {
    let state = app.state::<SharedState>();
    let _ = state.runtime.stop();

    if let Ok(config) = state.store.load() {
        if config.proxy.use_system_proxy {
            #[cfg(target_os = "macos")]
            {
                let _ = system_proxy::clear_macos_proxy();
            }
        }
    }
}

fn start_subscription_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(60));
        let state = app.state::<SharedState>();
        match commands::auto_refresh_due_subscriptions(&state) {
            Ok(true) => {
                let _ = app.emit("app-state-changed", "subscription_auto_refresh");
            }
            Ok(false) => {}
            Err(error) => {
                log::warn!("自动刷新订阅失败: {error}");
            }
        }
    });
}

fn start_core_status_scheduler(app: tauri::AppHandle) {
    thread::spawn(move || loop {
        let state = app.state::<SharedState>();
        match crate::core_update::list_core_statuses(&app, &state.core_paths) {
            Ok(statuses) => {
                if let Ok(mut cache) = state.core_status_cache.lock() {
                    *cache = statuses;
                }
                let _ = app.emit("app-state-changed", "core_status_cache_updated");
            }
            Err(error) => {
                log::warn!("刷新核心状态缓存失败: {error}");
            }
        }
        thread::sleep(Duration::from_secs(1800));
    });
}

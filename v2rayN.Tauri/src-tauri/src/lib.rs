pub mod app_state;
pub mod commands;
pub mod config_store;
pub mod core_runtime;
pub mod core_update;
pub mod domain;
pub mod events;
pub mod http_server;
pub mod macos_app_bundle;
pub mod models;
pub mod network_probe;
pub mod system_proxy;
pub mod tun;

use app_state::SharedState;
use config_store::ConfigStore;
use core_runtime::RuntimeManager;
use core_update::CorePaths;
use events::{AppEvent, EventSender};
use std::{sync::{Arc, Mutex}, thread, time::Duration};
use tauri::{Emitter, Manager};
use tokio::sync::broadcast;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let event_sender = EventSender::new();

    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(move |app| {
            let store = ConfigStore::bootstrap(app.handle())?;
            let core_paths = CorePaths {
                bin_root: std::path::PathBuf::from(store.paths().bin.clone()),
            };
            let core_status_cache = crate::core_update::list_local_core_statuses(&core_paths)?;

            let shared = SharedState {
                store,
                core_paths,
                runtime: Arc::new(RuntimeManager::new()),
                core_status_cache: Arc::new(Mutex::new(core_status_cache)),
                subscription_refresh_lock: Arc::new(Mutex::new(())),
                event_sender: event_sender.clone(),
            };

            app.manage(shared);

            // 桥接：将广播事件转发给 Tauri 前端
            let mut tauri_rx = event_sender.subscribe();
            let app_handle = app.handle().clone();
            thread::spawn(move || {
                loop {
                    match tauri_rx.blocking_recv() {
                        Ok(event) => forward_to_tauri(&app_handle, &event),
                        Err(broadcast::error::RecvError::Closed) => break,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    }
                }
            });

            // 启动 HTTP 服务器（端口 7393）
            let http_shared = app.state::<SharedState>().inner().clone();
            thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("创建 Tokio 运行时失败");
                rt.block_on(http_server::serve(http_shared, 7393));
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
            commands::resolve_macos_app_bundle,
            commands::list_applications,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
            cleanup_runtime(app_handle);
        }
    });
}

fn forward_to_tauri(app: &tauri::AppHandle, event: &AppEvent) {
    match event {
        AppEvent::CoreLog(payload) => { let _ = app.emit("core-log", payload); }
        AppEvent::AppStateChanged(reason) => { let _ = app.emit("app-state-changed", reason); }
        AppEvent::BackgroundTaskFinished(payload) => { let _ = app.emit("background-task-finished", payload); }
    }
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
                state.event_sender.emit_app_state_changed("subscription_auto_refresh");
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
        match crate::core_update::list_core_statuses(&state.core_paths) {
            Ok(statuses) => {
                if let Ok(mut cache) = state.core_status_cache.lock() {
                    *cache = statuses;
                }
                state.event_sender.emit_app_state_changed("core_status_cache_updated");
            }
            Err(error) => {
                log::warn!("刷新核心状态缓存失败: {error}");
            }
        }
        thread::sleep(Duration::from_secs(1800));
    });
}

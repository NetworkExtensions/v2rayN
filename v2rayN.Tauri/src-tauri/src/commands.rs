use crate::{
    app_state::SharedState,
    domain,
    models::{AppConfig, AppStatus, CoreAssetStatus, CoreType, RunningStatus, Subscription},
    system_proxy,
};
use anyhow::{Context, Error, Result};
use reqwest::blocking::Client;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_app_status(app: AppHandle, state: State<'_, SharedState>) -> Result<AppStatus, String> {
    let paths = state.core_paths.clone();
    let store = state.store.clone();
    let runtime = state.runtime.status();

    tauri::async_runtime::spawn_blocking(move || {
        let config = store.load()?;
        let core_assets = crate::core_update::list_core_statuses(&app, &paths)?;
        Ok::<AppStatus, Error>(AppStatus {
            paths: store.paths(),
            config,
            runtime,
            core_assets,
        })
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(format_anyhow)
}

#[tauri::command]
pub fn save_app_config(config: AppConfig, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    state.store.save(&config).map_err(error_to_string)?;
    state.store.load().map_err(error_to_string)
}

#[tauri::command]
pub fn import_share_links(
    core_type: CoreType,
    raw: String,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let profiles = domain::import_share_links(&raw, core_type).map_err(error_to_string)?;
    let imported = domain::merge_imported_profiles(&mut config, profiles);
    if imported == 0 {
        return Err("未导入任何可识别的分享链接".into());
    }
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn save_subscription(subscription: Subscription, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    if let Some(existing) = config
        .subscriptions
        .iter_mut()
        .find(|item| item.id == subscription.id)
    {
        *existing = subscription;
    } else {
        config.subscriptions.push(subscription);
    }
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub async fn refresh_subscription(
    subscription_id: String,
    core_type: CoreType,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let store = state.store.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut config = store.load()?;
        let subscription_index = config
            .subscriptions
            .iter()
            .position(|item| item.id == subscription_id)
            .context("未找到订阅")?;

        let subscription_url = config.subscriptions[subscription_index].url.clone();

        let raw = Client::builder()
            .user_agent("v2rayN-tauri")
            .build()?
            .get(&subscription_url)
            .send()?
            .error_for_status()?
            .text()?;

        let profiles = domain::import_share_links(&raw, core_type)?;
        domain::merge_imported_profiles(&mut config, profiles);
        domain::apply_subscription_result(&mut config.subscriptions[subscription_index]);
        store.save(&config)?;
        Ok::<AppConfig, Error>(config)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(format_anyhow)
}

#[tauri::command]
pub fn generate_config_preview(state: State<'_, SharedState>) -> Result<String, String> {
    let config = state.store.load().map_err(error_to_string)?;
    let preview = domain::generate_core_config(&config).map_err(error_to_string)?;
    serde_json::to_string_pretty(&preview).map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_core_assets(app: AppHandle, state: State<'_, SharedState>) -> Result<Vec<CoreAssetStatus>, String> {
    let paths = state.core_paths.clone();
    tauri::async_runtime::spawn_blocking(move || crate::core_update::list_core_statuses(&app, &paths))
        .await
        .map_err(|error| error.to_string())?
        .map_err(format_anyhow)
}

#[tauri::command]
pub async fn download_core_asset(
    app: AppHandle,
    core_type: CoreType,
    state: State<'_, SharedState>,
) -> Result<CoreAssetStatus, String> {
    let paths = state.core_paths.clone();
    tauri::async_runtime::spawn_blocking(move || crate::core_update::download_core(&app, &paths, core_type))
        .await
        .map_err(|error| error.to_string())?
        .map_err(format_anyhow)
}

#[tauri::command]
pub fn start_core(app: AppHandle, state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    state
        .runtime
        .start(&app, &state.store, &state.core_paths)
        .map_err(error_to_string)
}

#[tauri::command]
pub fn stop_core(state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    state.runtime.stop().map_err(error_to_string)
}

#[tauri::command]
pub fn enable_system_proxy(state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    #[cfg(target_os = "macos")]
    {
        system_proxy::set_macos_proxy(
            "127.0.0.1",
            config.proxy.http_port,
            config.proxy.socks_port,
            &config.proxy.bypass_domains,
        )
        .map_err(error_to_string)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        return Err("当前仅实现 macOS 系统代理切换".into());
    }

    config.proxy.use_system_proxy = true;
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn disable_system_proxy(state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    #[cfg(target_os = "macos")]
    {
        system_proxy::clear_macos_proxy().map_err(error_to_string)?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        return Err("当前仅实现 macOS 系统代理切换".into());
    }

    config.proxy.use_system_proxy = false;
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

fn error_to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn format_anyhow(error: Error) -> String {
    let parts = error.chain().map(|item| item.to_string()).collect::<Vec<_>>();
    parts.join("\ncaused by: ")
}

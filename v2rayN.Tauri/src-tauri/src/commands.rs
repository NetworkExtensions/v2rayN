use crate::{
    app_state::SharedState,
    domain,
    models::{AppConfig, AppStatus, CoreAssetStatus, CoreType, ProxyProbe, RunningStatus, Subscription},
    network_probe,
    system_proxy,
};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use tauri::{AppHandle, State};

fn load_status(app: &AppHandle, state: &SharedState) -> Result<AppStatus> {
    let config = state.store.load()?;
    let core_assets = crate::core_update::list_core_statuses(app, &state.core_paths)?;
    let proxy_probe = if state.runtime.status().running {
        network_probe::probe_proxy(config.proxy.socks_port).ok()
    } else {
        network_probe::probe_direct().ok()
    };
    Ok(AppStatus {
        paths: state.store.paths(),
        config,
        runtime: state.runtime.status(),
        core_assets,
        proxy_probe,
    })
}

#[tauri::command]
pub fn get_app_status(app: AppHandle, state: State<'_, SharedState>) -> Result<AppStatus, String> {
    load_status(&app, &state).map_err(error_to_string)
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
pub fn remove_subscription(subscription_id: String, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    config.subscriptions.retain(|item| item.id != subscription_id);
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn refresh_subscription(
    subscription_id: String,
    core_type: CoreType,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let subscription_index = config
        .subscriptions
        .iter()
        .position(|item| item.id == subscription_id)
        .context("未找到订阅")
        .map_err(error_to_string)?;

    let subscription_url = config.subscriptions[subscription_index].url.clone();

    let raw = Client::builder()
        .user_agent("v2rayN-tauri")
        .build()
        .and_then(|client| client.get(&subscription_url).send())
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .map_err(|error| error.to_string())?;

    let profiles = domain::import_share_links(&raw, core_type).map_err(error_to_string)?;
    domain::merge_imported_profiles(&mut config, profiles);
    domain::apply_subscription_result(&mut config.subscriptions[subscription_index]);
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn refresh_all_subscriptions(core_type: CoreType, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let client = Client::builder()
        .user_agent("v2rayN-tauri")
        .build()
        .map_err(|error| error.to_string())?;

    for index in 0..config.subscriptions.len() {
        let subscription = &config.subscriptions[index];
        if !subscription.enabled || subscription.url.trim().is_empty() {
            continue;
        }

        let raw = client
            .get(&subscription.url)
            .send()
            .and_then(|response| response.error_for_status())
            .and_then(|response| response.text())
            .map_err(|error| error.to_string())?;

        let profiles = domain::import_share_links(&raw, core_type.clone()).map_err(error_to_string)?;
        domain::merge_imported_profiles(&mut config, profiles);
        domain::apply_subscription_result(&mut config.subscriptions[index]);
    }

    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn remove_profile(profile_id: String, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    config.profiles.retain(|profile| profile.id != profile_id);

    if config.profiles.is_empty() {
        let profile = crate::models::Profile::default();
        config.selected_profile_id = Some(profile.id.clone());
        config.profiles.push(profile);
    } else if config.selected_profile_id.as_ref() == Some(&profile_id) {
        config.selected_profile_id = config.profiles.first().map(|profile| profile.id.clone());
    }

    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn select_profile(profile_id: String, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    if config.profiles.iter().any(|profile| profile.id == profile_id) {
        config.selected_profile_id = Some(profile_id);
        state.store.save(&config).map_err(error_to_string)?;
        Ok(config)
    } else {
        Err("未找到节点".into())
    }
}

#[tauri::command]
pub fn generate_config_preview(state: State<'_, SharedState>) -> Result<String, String> {
    let config = state.store.load().map_err(error_to_string)?;
    let preview = domain::generate_preview(&config).map_err(error_to_string)?;
    serde_json::to_string_pretty(&preview).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn check_core_assets(app: AppHandle, state: State<'_, SharedState>) -> Result<Vec<CoreAssetStatus>, String> {
    crate::core_update::list_core_statuses(&app, &state.core_paths).map_err(error_to_string)
}

#[tauri::command]
pub fn download_core_asset(
    app: AppHandle,
    core_type: CoreType,
    state: State<'_, SharedState>,
) -> Result<CoreAssetStatus, String> {
    crate::core_update::download_core(&app, &state.core_paths, core_type).map_err(error_to_string)
}

#[tauri::command]
pub fn start_core(app: AppHandle, state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    let status = state
        .runtime
        .start(&app, &state.store, &state.core_paths)
        .map_err(error_to_string)?;

    let config = state.store.load().map_err(error_to_string)?;
    if config.proxy.use_system_proxy {
        #[cfg(target_os = "macos")]
        {
            system_proxy::set_macos_proxy(
                "127.0.0.1",
                config.proxy.socks_port,
                &config.proxy.bypass_domains,
            )
            .map_err(error_to_string)?;
        }
    }

    Ok(status)
}

#[tauri::command]
pub fn stop_core(state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    let status = state.runtime.stop().map_err(error_to_string)?;
    let config = state.store.load().map_err(error_to_string)?;
    if config.proxy.use_system_proxy {
        #[cfg(target_os = "macos")]
        {
            system_proxy::clear_macos_proxy().map_err(error_to_string)?;
        }
    }
    Ok(status)
}

#[tauri::command]
pub fn restart_core(app: AppHandle, state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    let _ = stop_core(state.clone());
    start_core(app, state)
}

#[tauri::command]
pub fn enable_system_proxy(state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    #[cfg(target_os = "macos")]
    {
        system_proxy::set_macos_proxy(
            "127.0.0.1",
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

#[tauri::command]
pub fn probe_current_outbound(state: State<'_, SharedState>) -> Result<ProxyProbe, String> {
    let config = state.store.load().map_err(error_to_string)?;
    if state.runtime.status().running {
        network_probe::probe_proxy(config.proxy.socks_port).map_err(error_to_string)
    } else {
        network_probe::probe_direct().map_err(error_to_string)
    }
}

fn error_to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

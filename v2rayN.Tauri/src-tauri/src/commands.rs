use crate::{
    app_state::SharedState,
    domain,
    models::{
        AppConfig, AppStatus, BackgroundTaskEvent, ClashConnection, ClashProxyGroup,
        ClashProxyProvider, CoreAssetStatus, CoreType, ProxyProbe, RoutingItem, RoutingRule,
        RunningStatus, Subscription,
    },
    network_probe,
    system_proxy,
};
use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::Proxy;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::thread;
use tauri::State;

// ── 内部辅助函数 ─────────────────────────────────────────────────────────────

pub fn load_status(state: &SharedState, core_assets: Vec<CoreAssetStatus>, include_probe: bool) -> Result<AppStatus> {
    let config = state.store.load()?;
    let proxy_probe = if include_probe && state.runtime.status().running {
        network_probe::probe_proxy(config.proxy.socks_port).ok()
    } else if include_probe {
        network_probe::probe_direct().ok()
    } else {
        None
    };
    Ok(AppStatus {
        paths: state.store.paths(),
        config,
        runtime: state.runtime.status(),
        core_assets,
        proxy_probe,
    })
}

pub fn build_client(user_agent: &str, proxy_url: Option<&str>) -> Result<Client> {
    let mut builder = Client::builder().user_agent(user_agent);
    if let Some(proxy_url) = proxy_url {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    builder.build().context("创建 HTTP 客户端失败")
}

pub fn load_cached_core_assets(state: &SharedState) -> Result<Vec<CoreAssetStatus>> {
    let cached = state
        .core_status_cache
        .lock()
        .map_err(|_| anyhow::anyhow!("核心状态缓存不可用"))?
        .clone();
    if cached.is_empty() {
        crate::core_update::list_local_core_statuses(&state.core_paths)
    } else {
        Ok(cached)
    }
}

pub fn update_core_status_cache(state: &SharedState, statuses: &[CoreAssetStatus]) -> Result<()> {
    let mut cache = state
        .core_status_cache
        .lock()
        .map_err(|_| anyhow::anyhow!("核心状态缓存不可用"))?;
    *cache = statuses.to_vec();
    Ok(())
}

fn download_text(url: &str, user_agent: &str, proxy_url: Option<&str>) -> Result<String> {
    build_client(user_agent, proxy_url)?
        .get(url)
        .send()
        .and_then(|response| response.error_for_status())
        .and_then(|response| response.text())
        .map_err(Into::into)
}

fn refresh_subscription_impl(
    config: &mut AppConfig,
    subscription_index: usize,
    fallback_core_type: CoreType,
    socks_port: u16,
    import_storage_dir: &PathBuf,
) -> Result<()> {
    let subscription = config
        .subscriptions
        .get(subscription_index)
        .cloned()
        .context("未找到订阅")?;
    let user_agent = subscription.user_agent.trim();
    let user_agent = if user_agent.is_empty() { "v2rayN-tauri" } else { user_agent };
    let proxy_url = format!("socks5h://127.0.0.1:{socks_port}");

    let mut segments = vec![download_subscription_with_fallback(&subscription.url, user_agent, subscription.use_proxy_on_refresh, &proxy_url)?];
    if subscription.convert_core_target.is_none() {
        for more_url in subscription.more_urls.iter().map(String::as_str).map(str::trim).filter(|url| !url.is_empty()) {
            segments.push(download_subscription_with_fallback(more_url, user_agent, subscription.use_proxy_on_refresh, &proxy_url)?);
        }
    }

    let raw = segments.join("\n");
    let core_type = subscription.convert_core_target.unwrap_or(fallback_core_type);
    let import_format = domain::detect_import_format(&raw);
    let profiles = match import_format {
        domain::ImportFormat::ShareLinks => domain::import_share_links(&raw, core_type),
        domain::ImportFormat::SingBoxJson
        | domain::ImportFormat::XrayJson
        | domain::ImportFormat::ClashYaml => domain::import_full_config(&raw, import_storage_dir),
        domain::ImportFormat::Unknown => Err(anyhow::anyhow!("订阅内容无法识别")),
    }?;
    let mut profiles = if matches!(import_format, domain::ImportFormat::ShareLinks) {
        domain::filter_profiles(profiles, subscription.filter.as_deref())?
    } else {
        profiles
    };
    for profile in &mut profiles {
        profile.source_subscription_id = Some(subscription.id.clone());
    }
    domain::merge_profiles(config, profiles, Some(&subscription.id));
    domain::apply_subscription_result(&mut config.subscriptions[subscription_index]);
    Ok(())
}

fn download_subscription_with_fallback(
    url: &str,
    user_agent: &str,
    use_proxy: bool,
    proxy_url: &str,
) -> Result<String> {
    if use_proxy {
        match download_text(url, user_agent, Some(proxy_url)) {
            Ok(body) if !body.trim().is_empty() => return Ok(body),
            Ok(_) | Err(_) => {}
        }
    }
    download_text(url, user_agent, None)
}

pub fn auto_refresh_due_subscriptions(state: &SharedState) -> Result<bool> {
    let _guard = state
        .subscription_refresh_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("订阅刷新锁不可用"))?;

    let mut config = state.store.load()?;
    let import_dir = PathBuf::from(state.store.paths().bin_configs).join("imported");
    let socks_port = config.proxy.socks_port;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let mut changed = false;

    for index in 0..config.subscriptions.len() {
        let Some(interval_secs) = config.subscriptions[index].auto_update_interval_secs else {
            continue;
        };
        if interval_secs == 0 {
            continue;
        }
        let interval_window = interval_secs.saturating_mul(60);

        let last_checked = config.subscriptions[index]
            .last_checked_at
            .as_deref()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        if now.saturating_sub(last_checked) < interval_window {
            continue;
        }

        if config.subscriptions[index].enabled && !config.subscriptions[index].url.trim().is_empty() {
            if let Err(error) =
                refresh_subscription_impl(&mut config, index, CoreType::SingBox, socks_port, &import_dir)
            {
                domain::apply_subscription_error(&mut config.subscriptions[index], error.to_string());
            }
        } else {
            domain::apply_subscription_checked(&mut config.subscriptions[index]);
        }

        changed = true;
        thread::sleep(std::time::Duration::from_secs(1));
    }

    if changed {
        state.store.save(&config)?;
    }

    Ok(changed)
}

pub fn refresh_all_subscriptions_impl(core_type: CoreType, state: &SharedState) -> Result<AppConfig> {
    let _guard = state
        .subscription_refresh_lock
        .lock()
        .map_err(|_| anyhow::anyhow!("订阅刷新锁不可用"))?;
    let mut config = state.store.load()?;
    let import_dir = PathBuf::from(state.store.paths().bin_configs).join("imported");
    let socks_port = config.proxy.socks_port;

    for index in 0..config.subscriptions.len() {
        let subscription = &config.subscriptions[index];
        if !subscription.enabled || subscription.url.trim().is_empty() {
            continue;
        }
        if let Err(error) = refresh_subscription_impl(&mut config, index, core_type.clone(), socks_port, &import_dir) {
            domain::apply_subscription_error(&mut config.subscriptions[index], error.to_string());
        }
    }

    state.store.save(&config)?;
    Ok(config)
}

// ── Tauri 命令 ───────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_app_status(state: State<'_, SharedState>) -> Result<AppStatus, String> {
    let core_assets = crate::core_update::list_core_statuses(&state.core_paths).map_err(error_to_string)?;
    update_core_status_cache(&state, &core_assets).map_err(error_to_string)?;
    load_status(&state, core_assets, true).map_err(error_to_string)
}

#[tauri::command]
pub fn get_app_status_light(state: State<'_, SharedState>) -> Result<AppStatus, String> {
    let core_assets = load_cached_core_assets(&state).map_err(error_to_string)?;
    load_status(&state, core_assets, false).map_err(error_to_string)
}

#[tauri::command]
pub fn save_app_config(config: AppConfig, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = config;
    domain::ensure_routing_items(&mut config.routing);
    state.store.save(&config).map_err(error_to_string)?;
    state.store.load().map_err(error_to_string)
}

#[tauri::command]
pub fn initialize_builtin_routing(advanced_only: bool, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    if advanced_only {
        let template = crate::models::RoutingTemplate {
            version: "V4".into(),
            routing_items: domain::builtin_routing_items(),
        };
        let _ = domain::apply_routing_template(&mut config.routing, template, true).map_err(error_to_string)?;
    } else {
        config.routing.items = domain::builtin_routing_items();
        domain::ensure_routing_items(&mut config.routing);
    }
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn import_routing_template_url(
    url: String,
    advanced_only: bool,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let template_raw = download_text(&url, "v2rayN-tauri", None).map_err(error_to_string)?;
    let template = domain::routing_template_from_raw(&template_raw).map_err(error_to_string)?;
    let _ = domain::apply_routing_template(&mut config.routing, template, advanced_only).map_err(error_to_string)?;
    config.routing.template_source_url = Some(url);
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn save_routing_item(routing_item: RoutingItem, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let mut routing_item = routing_item;
    if routing_item.id.is_empty() {
        routing_item.id = crate::models::new_id("routing");
    }
    for rule in &mut routing_item.rule_set {
        if rule.id.is_empty() {
            rule.id = crate::models::new_id("routing-rule");
        }
    }
    routing_item.rule_num = routing_item.rule_set.len();

    if let Some(existing) = config.routing.items.iter_mut().find(|item| item.id == routing_item.id) {
        *existing = routing_item.clone();
    } else {
        routing_item.sort = config.routing.items.iter().map(|item| item.sort).max().unwrap_or(0) + 1;
        config.routing.items.push(routing_item.clone());
    }
    if routing_item.is_active {
        domain::set_active_routing_item(&mut config.routing, &routing_item.id);
    } else {
        domain::ensure_routing_items(&mut config.routing);
    }
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn remove_routing_item(routing_id: String, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    config.routing.items.retain(|item| item.id != routing_id);
    domain::ensure_routing_items(&mut config.routing);
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn set_default_routing_item(routing_id: String, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    if !config.routing.items.iter().any(|item| item.id == routing_id) {
        return Err("未找到路由集".into());
    }
    domain::set_active_routing_item(&mut config.routing, &routing_id);
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn import_routing_rules(
    routing_id: String,
    raw: String,
    replace_existing: bool,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let imported_rules = domain::parse_routing_rules_json(&raw).map_err(error_to_string)?;
    let item = config
        .routing
        .items
        .iter_mut()
        .find(|item| item.id == routing_id)
        .ok_or_else(|| "未找到路由集".to_string())?;
    if replace_existing {
        item.rule_set = imported_rules;
    } else {
        item.rule_set.extend(imported_rules);
    }
    item.rule_num = item.rule_set.len();
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn export_routing_rules(
    routing_id: String,
    rule_ids: Option<Vec<String>>,
    state: State<'_, SharedState>,
) -> Result<String, String> {
    let config = state.store.load().map_err(error_to_string)?;
    let item = config
        .routing
        .items
        .iter()
        .find(|item| item.id == routing_id)
        .ok_or_else(|| "未找到路由集".to_string())?;
    let rules: Vec<RoutingRule> = match rule_ids {
        Some(ids) if !ids.is_empty() => item
            .rule_set
            .iter()
            .filter(|rule| ids.iter().any(|id| id == &rule.id))
            .cloned()
            .collect(),
        _ => item.rule_set.clone(),
    };
    domain::export_routing_rules_json(&rules).map_err(error_to_string)
}

#[tauri::command]
pub fn move_routing_rule(
    routing_id: String,
    rule_id: String,
    direction: String,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let item = config
        .routing
        .items
        .iter_mut()
        .find(|item| item.id == routing_id)
        .ok_or_else(|| "未找到路由集".to_string())?;
    let index = item
        .rule_set
        .iter()
        .position(|rule| rule.id == rule_id)
        .ok_or_else(|| "未找到路由规则".to_string())?;

    let new_index = match direction.as_str() {
        "top" => 0,
        "up" => index.saturating_sub(1),
        "down" => (index + 1).min(item.rule_set.len().saturating_sub(1)),
        "bottom" => item.rule_set.len().saturating_sub(1),
        _ => return Err("不支持的移动方向".into()),
    };
    if index != new_index {
        let rule = item.rule_set.remove(index);
        item.rule_set.insert(new_index, rule);
    }
    item.rule_num = item.rule_set.len();
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn import_share_links(
    core_type: CoreType,
    raw: String,
    state: State<'_, SharedState>,
) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let profiles = domain::import_share_links(&raw, core_type).map_err(error_to_string)?;
    let selected_id = profiles.last().map(|profile| profile.id.clone());
    let imported = domain::merge_imported_profiles(&mut config, profiles);
    if imported == 0 {
        return Err("未导入任何可识别的分享链接".into());
    }
    if let Some(selected_id) = selected_id {
        config.selected_profile_id = Some(selected_id);
    }
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn preview_import_result(raw: String, core_type: CoreType) -> Result<domain::ImportPreview, String> {
    domain::preview_import(&raw, core_type).map_err(error_to_string)
}

#[tauri::command]
pub fn import_full_config(raw: String, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    let import_dir = PathBuf::from(state.store.paths().bin_configs).join("imported");
    let profiles = domain::import_full_config(&raw, &import_dir).map_err(error_to_string)?;
    let selected_id = profiles.last().map(|profile| profile.id.clone());
    let imported = domain::merge_profiles(&mut config, profiles, None);
    if imported == 0 {
        return Err("未导入任何完整配置".into());
    }
    if let Some(selected_id) = selected_id {
        config.selected_profile_id = Some(selected_id);
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
    let _guard = state
        .subscription_refresh_lock
        .lock()
        .map_err(|_| "订阅刷新锁不可用".to_string())?;
    let mut config = state.store.load().map_err(error_to_string)?;
    let subscription_index = config
        .subscriptions
        .iter()
        .position(|item| item.id == subscription_id)
        .context("未找到订阅")
        .map_err(error_to_string)?;
    let socks_port = config.proxy.socks_port;

    let import_dir = PathBuf::from(state.store.paths().bin_configs).join("imported");
    if let Err(error) = refresh_subscription_impl(
        &mut config,
        subscription_index,
        core_type,
        socks_port,
        &import_dir,
    ) {
        domain::apply_subscription_error(&mut config.subscriptions[subscription_index], error.to_string());
        state.store.save(&config).map_err(error_to_string)?;
        return Err(error_to_string(error));
    }
    state.store.save(&config).map_err(error_to_string)?;
    Ok(config)
}

#[tauri::command]
pub fn refresh_all_subscriptions(core_type: CoreType, state: State<'_, SharedState>) -> Result<AppConfig, String> {
    refresh_all_subscriptions_impl(core_type, &state).map_err(error_to_string)
}

#[tauri::command]
pub fn refresh_all_subscriptions_in_background(
    core_type: CoreType,
    state: State<'_, SharedState>,
) -> Result<(), String> {
    let state_clone = (*state).clone();
    thread::spawn(move || {
        let result = refresh_all_subscriptions_impl(core_type, &state_clone);
        let message = match &result {
            Ok(_) => "全部订阅刷新完成".to_string(),
            Err(error) => error.to_string(),
        };
        state_clone.event_sender.emit_background_task(BackgroundTaskEvent {
            task: "subscription-refresh-all".into(),
            success: result.is_ok(),
            message,
        });
        if result.is_ok() {
            state_clone.event_sender.emit_app_state_changed("subscription_refresh_all");
        }
    });
    Ok(())
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
    domain::generate_preview(&config).map_err(error_to_string)
}

#[tauri::command]
pub fn check_core_assets(state: State<'_, SharedState>) -> Result<Vec<CoreAssetStatus>, String> {
    let statuses = crate::core_update::list_core_statuses(&state.core_paths).map_err(error_to_string)?;
    update_core_status_cache(&state, &statuses).map_err(error_to_string)?;
    Ok(statuses)
}

#[tauri::command]
pub fn download_core_asset(
    core_type: CoreType,
    state: State<'_, SharedState>,
) -> Result<CoreAssetStatus, String> {
    let status = crate::core_update::download_core(&state.core_paths, core_type).map_err(error_to_string)?;
    let mut cache = load_cached_core_assets(&state).map_err(error_to_string)?;
    if let Some(existing) = cache.iter_mut().find(|asset| asset.core_type == status.core_type) {
        *existing = status.clone();
    } else {
        cache.push(status.clone());
    }
    update_core_status_cache(&state, &cache).map_err(error_to_string)?;
    Ok(status)
}

#[tauri::command]
pub fn start_core(state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    let status = state
        .runtime
        .start(&state.event_sender, &state.store, &state.core_paths)
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
pub fn restart_core(state: State<'_, SharedState>) -> Result<RunningStatus, String> {
    let _ = stop_core(state.clone());
    start_core(state)
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

pub fn parse_clash_proxy_groups(config: &AppConfig) -> Result<Vec<ClashProxyGroup>, String> {
    let value = clash_api_get(config, "/proxies").map_err(error_to_string)?;
    let mut groups = vec![];
    if let Some(proxies) = value.get("proxies").and_then(Value::as_object) {
        for (name, proxy) in proxies {
            let all = proxy
                .get("all")
                .and_then(Value::as_array)
                .map(|items| {
                    items.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if all.is_empty() {
                continue;
            }
            groups.push(ClashProxyGroup {
                name: name.clone(),
                proxy_type: proxy
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown")
                    .to_string(),
                now: proxy.get("now").and_then(Value::as_str).map(str::to_string),
                all,
                last_delay_ms: latest_delay_ms(proxy.as_object()),
            });
        }
    }
    Ok(groups)
}

#[tauri::command]
pub fn get_clash_proxy_groups(state: State<'_, SharedState>) -> Result<Vec<ClashProxyGroup>, String> {
    let config = state.store.load().map_err(error_to_string)?;
    parse_clash_proxy_groups(&config)
}

#[tauri::command]
pub fn select_clash_proxy(group_name: String, proxy_name: String, state: State<'_, SharedState>) -> Result<(), String> {
    let config = state.store.load().map_err(error_to_string)?;
    clash_api_put(
        &config,
        &format!("/proxies/{}", urlencoding::encode(&group_name)),
        json!({ "name": proxy_name }),
    )
    .map_err(error_to_string)
}

pub fn parse_clash_connections(config: &AppConfig) -> Result<Vec<ClashConnection>, String> {
    let value = clash_api_get(config, "/connections").map_err(error_to_string)?;
    let mut connections = vec![];
    for item in value
        .get("connections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let metadata = item.get("metadata").and_then(Value::as_object);
        let destination = match (
            metadata.and_then(|meta| meta.get("destinationIP")).and_then(Value::as_str),
            metadata.and_then(|meta| meta.get("destinationPort")).and_then(Value::as_u64),
        ) {
            (Some(host), Some(port)) => Some(format!("{host}:{port}")),
            _ => None,
        };
        connections.push(ClashConnection {
            id: item
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            network: item.get("metadata").and_then(|meta| meta.get("network")).and_then(Value::as_str).map(str::to_string),
            r#type: metadata.and_then(|meta| meta.get("type")).and_then(Value::as_str).map(str::to_string),
            rule: item.get("rule").and_then(Value::as_str).map(str::to_string),
            chains: item
                .get("chains")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
            upload: item.get("upload").and_then(Value::as_u64),
            download: item.get("download").and_then(Value::as_u64),
            host: metadata.and_then(|meta| meta.get("host")).and_then(Value::as_str).map(str::to_string),
            destination,
            start: item.get("start").and_then(Value::as_str).map(str::to_string),
        });
    }
    Ok(connections)
}

#[tauri::command]
pub fn get_clash_connections(state: State<'_, SharedState>) -> Result<Vec<ClashConnection>, String> {
    let config = state.store.load().map_err(error_to_string)?;
    parse_clash_connections(&config)
}

#[tauri::command]
pub fn test_clash_proxy_delay(group_name: String, state: State<'_, SharedState>) -> Result<u64, String> {
    let config = state.store.load().map_err(error_to_string)?;
    clash_api_delay_test(&config, &group_name).map_err(error_to_string)
}

pub fn parse_clash_proxy_providers(config: &AppConfig) -> Result<Vec<ClashProxyProvider>, String> {
    let value = clash_api_get(config, "/providers/proxies").map_err(error_to_string)?;
    let mut providers = vec![];
    if let Some(items) = value.get("providers").and_then(Value::as_object) {
        for (name, provider) in items {
            providers.push(ClashProxyProvider {
                name: name.clone(),
                provider_type: provider
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("Unknown")
                    .to_string(),
                vehicle_type: provider.get("vehicleType").and_then(Value::as_str).map(str::to_string),
                updated_at: provider.get("updatedAt").and_then(Value::as_str).map(str::to_string),
                proxies: provider
                    .get("proxies")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.get("name").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
            });
        }
    }
    Ok(providers)
}

#[tauri::command]
pub fn get_clash_proxy_providers(state: State<'_, SharedState>) -> Result<Vec<ClashProxyProvider>, String> {
    let config = state.store.load().map_err(error_to_string)?;
    parse_clash_proxy_providers(&config)
}

#[tauri::command]
pub fn update_clash_rule_mode(rule_mode: String, state: State<'_, SharedState>) -> Result<(), String> {
    let mut config = state.store.load().map_err(error_to_string)?;
    clash_api_patch(&config, "/configs", json!({ "mode": rule_mode })).map_err(error_to_string)?;
    config.clash.rule_mode = rule_mode;
    state.store.save(&config).map_err(error_to_string)?;
    Ok(())
}

#[tauri::command]
pub fn reload_clash_config(state: State<'_, SharedState>) -> Result<(), String> {
    let config = state.store.load().map_err(error_to_string)?;
    let path = state.runtime.status().config_path.ok_or_else(|| "当前没有运行中的 mihomo 配置".to_string())?;
    clash_api_put_with_query(&config, "/configs?force=true", json!({ "path": path })).map_err(error_to_string)
}

#[tauri::command]
pub fn close_clash_connection(connection_id: String, state: State<'_, SharedState>) -> Result<(), String> {
    let config = state.store.load().map_err(error_to_string)?;
    let path = if connection_id.is_empty() {
        "/connections".to_string()
    } else {
        format!("/connections/{}", urlencoding::encode(&connection_id))
    };
    clash_api_delete(&config, &path).map_err(error_to_string)
}

#[tauri::command]
pub fn refresh_clash_proxy_provider(provider_name: String, state: State<'_, SharedState>) -> Result<(), String> {
    let config = state.store.load().map_err(error_to_string)?;
    clash_api_put(
        &config,
        &format!("/providers/proxies/{}/healthcheck", urlencoding::encode(&provider_name)),
        json!({}),
    )
    .map_err(error_to_string)
}

// ── Clash API 辅助 ───────────────────────────────────────────────────────────

pub fn clash_api_get(config: &AppConfig, path: &str) -> Result<Value> {
    let client = build_client("v2rayN-tauri", None)?;
    let url = format!("http://127.0.0.1:{}{}", config.clash.external_controller_port, path);
    let response = clash_request(client.get(url), config)?.error_for_status()?;
    Ok(response.json()?)
}

pub fn clash_api_put(config: &AppConfig, path: &str, body: Value) -> Result<()> {
    let client = build_client("v2rayN-tauri", None)?;
    let url = format!("http://127.0.0.1:{}{}", config.clash.external_controller_port, path);
    clash_request(client.put(url).json(&body), config)?.error_for_status()?;
    Ok(())
}

pub fn clash_api_put_with_query(config: &AppConfig, path: &str, body: Value) -> Result<()> {
    let client = build_client("v2rayN-tauri", None)?;
    let url = format!("http://127.0.0.1:{}{}", config.clash.external_controller_port, path);
    clash_request(client.put(url).json(&body), config)?.error_for_status()?;
    Ok(())
}

pub fn clash_api_patch(config: &AppConfig, path: &str, body: Value) -> Result<()> {
    let client = build_client("v2rayN-tauri", None)?;
    let url = format!("http://127.0.0.1:{}{}", config.clash.external_controller_port, path);
    clash_request(client.patch(url).json(&body), config)?.error_for_status()?;
    Ok(())
}

pub fn clash_api_delete(config: &AppConfig, path: &str) -> Result<()> {
    let client = build_client("v2rayN-tauri", None)?;
    let url = format!("http://127.0.0.1:{}{}", config.clash.external_controller_port, path);
    clash_request(client.delete(url), config)?.error_for_status()?;
    Ok(())
}

pub fn clash_api_delay_test(config: &AppConfig, group_name: &str) -> Result<u64> {
    let client = build_client("v2rayN-tauri", None)?;
    let url = format!(
        "http://127.0.0.1:{}/proxies/{}/delay?timeout=10000&url={}",
        config.clash.external_controller_port,
        urlencoding::encode(group_name),
        urlencoding::encode(&config.clash.proxies_auto_delay_test_url)
    );
    let response = clash_request(client.get(url), config)?.error_for_status()?;
    let payload: Value = response.json()?;
    payload
        .get("delay")
        .and_then(Value::as_u64)
        .context("测速结果缺少 delay 字段")
}

fn clash_request(
    request: reqwest::blocking::RequestBuilder,
    config: &AppConfig,
) -> Result<reqwest::blocking::Response> {
    let request = if let Some(secret) = config.clash.secret.as_ref().filter(|value| !value.is_empty()) {
        request.header("Authorization", format!("Bearer {secret}"))
    } else {
        request
    };
    Ok(request.send()?)
}

fn latest_delay_ms(proxy: Option<&serde_json::Map<String, Value>>) -> Option<u64> {
    proxy
        .and_then(|proxy| proxy.get("history"))
        .and_then(Value::as_array)
        .and_then(|history| history.iter().rev().find_map(|entry| entry.get("delay").and_then(Value::as_u64)))
}

pub fn error_to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

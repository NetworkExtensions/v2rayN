/// 本地 HTTP + WebSocket 服务器（端口 7393）
///
/// 暴露与 Tauri commands 对等的 REST API，使浏览器可以直接控制代理核心。
/// 事件（核心日志、状态变更）通过 WebSocket 实时推送。
use crate::{
    app_state::SharedState,
    commands,
    domain,
    events::AppEvent,
    models::{
        AppConfig, ClashConnection, ClashProxyGroup, ClashProxyProvider, CoreType, RoutingItem,
        RoutingRule, Subscription,
    },
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tokio::task::spawn_blocking;
use tower_http::cors::{Any, CorsLayer};

// ── 错误类型 ─────────────────────────────────────────────────────────────────

struct ApiError(String);

impl From<String> for ApiError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        Self(e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0).into_response()
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

// ── 服务器入口 ────────────────────────────────────────────────────────────────

pub async fn serve(state: SharedState, port: u16) {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // 状态
        .route("/api/status", get(handle_get_status_light))
        .route("/api/status/full", get(handle_get_status_full))
        // 配置
        .route("/api/config", post(handle_save_config))
        // 路由集
        .route("/api/routing/init", post(handle_routing_init))
        .route("/api/routing/template-url", post(handle_routing_template_url))
        .route("/api/routing/item", post(handle_save_routing_item))
        .route("/api/routing/item/{id}", delete(handle_remove_routing_item))
        .route("/api/routing/item/{id}/default", post(handle_set_default_routing_item))
        .route("/api/routing/item/{id}/rules", post(handle_import_routing_rules))
        .route("/api/routing/item/{id}/rules", get(handle_export_routing_rules))
        .route("/api/routing/item/{id}/rules/{rule_id}/move", post(handle_move_routing_rule))
        // 导入
        .route("/api/import/share-links", post(handle_import_share_links))
        .route("/api/import/preview", post(handle_preview_import))
        .route("/api/import/full", post(handle_import_full_config))
        // 订阅
        .route("/api/subscriptions", post(handle_save_subscription))
        .route("/api/subscriptions/{id}", delete(handle_remove_subscription))
        .route("/api/subscriptions/{id}/refresh", post(handle_refresh_subscription))
        .route("/api/subscriptions/refresh-all", post(handle_refresh_all_subscriptions))
        .route("/api/subscriptions/refresh-background", post(handle_refresh_all_background))
        // 节点
        .route("/api/profiles/{id}", delete(handle_remove_profile))
        .route("/api/profiles/{id}/select", post(handle_select_profile))
        // 配置预览
        .route("/api/preview", get(handle_generate_preview))
        // 核心管理
        .route("/api/cores", get(handle_check_core_assets))
        .route("/api/cores/{core_type}/download", post(handle_download_core))
        .route("/api/core/start", post(handle_start_core))
        .route("/api/core/stop", post(handle_stop_core))
        .route("/api/core/restart", post(handle_restart_core))
        // 系统代理
        .route("/api/proxy/enable", post(handle_enable_proxy))
        .route("/api/proxy/disable", post(handle_disable_proxy))
        // 出口探测
        .route("/api/probe", get(handle_probe))
        // Clash API
        .route("/api/clash/proxy-groups", get(handle_clash_proxy_groups))
        .route("/api/clash/proxy-groups/{name}/select", post(handle_clash_select_proxy))
        .route("/api/clash/connections", get(handle_clash_connections))
        .route("/api/clash/connections", delete(handle_clash_close_all_connections))
        .route("/api/clash/connections/{id}", delete(handle_clash_close_connection))
        .route("/api/clash/proxy-delay/{name}", get(handle_clash_proxy_delay))
        .route("/api/clash/providers", get(handle_clash_providers))
        .route("/api/clash/providers/{name}/refresh", post(handle_clash_refresh_provider))
        .route("/api/clash/rule-mode", post(handle_clash_rule_mode))
        .route("/api/clash/reload", post(handle_clash_reload))
        // WebSocket 事件流
        .route("/api/events", get(handle_ws_upgrade))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    log::info!("HTTP 服务器启动：http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await.expect("绑定 HTTP 端口失败");
    axum::serve(listener, app).await.expect("HTTP 服务器运行失败");
}

// ── WebSocket 事件流 ──────────────────────────────────────────────────────────

async fn handle_ws_upgrade(ws: WebSocketUpgrade, State(state): State<SharedState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: SharedState) {
    let mut rx = state.event_sender.subscribe();
    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Ok(json) = serde_json::to_string(&event) {
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
        }
    }
}

// ── 宏：在 spawn_blocking 中执行同步操作 ──────────────────────────────────────

/// 在 spawn_blocking 中执行同步操作，结果自动包装为 Json<T>
macro_rules! blocking {
    ($state:expr, $body:expr) => {{
        let s = $state.clone();
        spawn_blocking(move || -> Result<_, String> {
            let state = s;
            $body
        })
        .await
        .map_err(ApiError::from)?
        .map_err(ApiError::from)
        .map(Json)
    }};
}

// ── 状态 ──────────────────────────────────────────────────────────────────────

async fn handle_get_status_light(State(state): State<SharedState>) -> ApiResult<serde_json::Value> {
    blocking!(state, {
        let core_assets = commands::load_cached_core_assets(&state).map_err(|e| e.to_string())?;
        commands::load_status(&state, core_assets, false)
            .map(|s| serde_json::to_value(s).unwrap_or_default())
            .map_err(|e| e.to_string())
    })
}

async fn handle_get_status_full(State(state): State<SharedState>) -> ApiResult<serde_json::Value> {
    blocking!(state, {
        let core_assets = crate::core_update::list_core_statuses(&state.core_paths)
            .map_err(|e| e.to_string())?;
        commands::update_core_status_cache(&state, &core_assets).map_err(|e| e.to_string())?;
        commands::load_status(&state, core_assets, true)
            .map(|s| serde_json::to_value(s).unwrap_or_default())
            .map_err(|e| e.to_string())
    })
}

// ── 配置 ──────────────────────────────────────────────────────────────────────

async fn handle_save_config(State(state): State<SharedState>, Json(config): Json<AppConfig>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = config;
        domain::ensure_routing_items(&mut config.routing);
        state.store.save(&config).map_err(|e| e.to_string())?;
        state.store.load().map_err(|e| e.to_string())
    })
}

// ── 路由集 ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct InitRoutingBody {
    advanced_only: Option<bool>,
}

async fn handle_routing_init(State(state): State<SharedState>, body: Option<Json<InitRoutingBody>>) -> ApiResult<AppConfig> {
    let advanced_only = body.and_then(|b| b.advanced_only).unwrap_or(false);
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        if advanced_only {
            let template = crate::models::RoutingTemplate {
                version: "V4".into(),
                routing_items: domain::builtin_routing_items(),
            };
            domain::apply_routing_template(&mut config.routing, template, true).map_err(|e| e.to_string())?;
        } else {
            config.routing.items = domain::builtin_routing_items();
            domain::ensure_routing_items(&mut config.routing);
        }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok::<_, String>(config)
    })
}

#[derive(Deserialize)]
struct TemplateUrlBody {
    url: String,
    advanced_only: Option<bool>,
}

async fn handle_routing_template_url(State(state): State<SharedState>, Json(body): Json<TemplateUrlBody>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        let raw = commands::build_client("v2rayN-tauri", None)
            .and_then(|c| c.get(&body.url).send()?.error_for_status()?.text().map_err(Into::into))
            .map_err(|e| e.to_string())?;
        let template = domain::routing_template_from_raw(&raw).map_err(|e| e.to_string())?;
        domain::apply_routing_template(&mut config.routing, template, body.advanced_only.unwrap_or(false))
            .map_err(|e| e.to_string())?;
        config.routing.template_source_url = Some(body.url);
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_save_routing_item(State(state): State<SharedState>, Json(mut item): Json<RoutingItem>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        if item.id.is_empty() {
            item.id = crate::models::new_id("routing");
        }
        for rule in &mut item.rule_set {
            if rule.id.is_empty() {
                rule.id = crate::models::new_id("routing-rule");
            }
        }
        item.rule_num = item.rule_set.len();
        if let Some(existing) = config.routing.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item.clone();
        } else {
            item.sort = config.routing.items.iter().map(|i| i.sort).max().unwrap_or(0) + 1;
            config.routing.items.push(item.clone());
        }
        if item.is_active {
            domain::set_active_routing_item(&mut config.routing, &item.id);
        } else {
            domain::ensure_routing_items(&mut config.routing);
        }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_remove_routing_item(State(state): State<SharedState>, Path(id): Path<String>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        config.routing.items.retain(|item| item.id != id);
        domain::ensure_routing_items(&mut config.routing);
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_set_default_routing_item(State(state): State<SharedState>, Path(id): Path<String>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        if !config.routing.items.iter().any(|item| item.id == id) {
            return Err("未找到路由集".to_string());
        }
        domain::set_active_routing_item(&mut config.routing, &id);
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

#[derive(Deserialize)]
struct ImportRulesBody {
    raw: String,
    replace_existing: Option<bool>,
}

async fn handle_import_routing_rules(State(state): State<SharedState>, Path(id): Path<String>, Json(body): Json<ImportRulesBody>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        let imported = domain::parse_routing_rules_json(&body.raw).map_err(|e| e.to_string())?;
        let item = config.routing.items.iter_mut().find(|i| i.id == id)
            .ok_or_else(|| "未找到路由集".to_string())?;
        if body.replace_existing.unwrap_or(false) {
            item.rule_set = imported;
        } else {
            item.rule_set.extend(imported);
        }
        item.rule_num = item.rule_set.len();
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_export_routing_rules(State(state): State<SharedState>, Path(id): Path<String>) -> Result<String, ApiError> {
    spawn_blocking(move || -> Result<String, String> {
        let config = state.store.load().map_err(|e| e.to_string())?;
        let item = config.routing.items.iter().find(|i| i.id == id)
            .ok_or_else(|| "未找到路由集".to_string())?;
        domain::export_routing_rules_json(&item.rule_set).map_err(|e| e.to_string())
    })
    .await
    .map_err(ApiError::from)?
    .map_err(ApiError::from)
}

#[derive(Deserialize)]
struct MoveRuleBody {
    direction: String,
}

async fn handle_move_routing_rule(
    State(state): State<SharedState>,
    Path((routing_id, rule_id)): Path<(String, String)>,
    Json(body): Json<MoveRuleBody>,
) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        let item = config.routing.items.iter_mut().find(|i| i.id == routing_id)
            .ok_or_else(|| "未找到路由集".to_string())?;
        let index = item.rule_set.iter().position(|r| r.id == rule_id)
            .ok_or_else(|| "未找到规则".to_string())?;
        let new_index = match body.direction.as_str() {
            "top" => 0,
            "up" => index.saturating_sub(1),
            "down" => (index + 1).min(item.rule_set.len().saturating_sub(1)),
            "bottom" => item.rule_set.len().saturating_sub(1),
            _ => return Err("不支持的方向".to_string()),
        };
        if index != new_index {
            let rule = item.rule_set.remove(index);
            item.rule_set.insert(new_index, rule);
        }
        item.rule_num = item.rule_set.len();
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

// ── 导入 ──────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ShareLinksBody {
    raw: String,
    core_type: CoreType,
}

async fn handle_import_share_links(State(state): State<SharedState>, Json(body): Json<ShareLinksBody>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        let profiles = domain::import_share_links(&body.raw, body.core_type).map_err(|e| e.to_string())?;
        let selected_id = profiles.last().map(|p| p.id.clone());
        let imported = domain::merge_imported_profiles(&mut config, profiles);
        if imported == 0 {
            return Err("未导入任何可识别的分享链接".to_string());
        }
        if let Some(id) = selected_id { config.selected_profile_id = Some(id); }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

#[derive(Deserialize)]
struct PreviewBody {
    raw: String,
    core_type: CoreType,
}

async fn handle_preview_import(Json(body): Json<PreviewBody>) -> ApiResult<domain::ImportPreview> {
    spawn_blocking(move || domain::preview_import(&body.raw, body.core_type).map_err(|e| e.to_string()))
        .await
        .map_err(ApiError::from)?
        .map(Json)
        .map_err(ApiError::from)
}

#[derive(Deserialize)]
struct RawBody {
    raw: String,
}

async fn handle_import_full_config(State(state): State<SharedState>, Json(body): Json<RawBody>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        let import_dir = std::path::PathBuf::from(state.store.paths().bin_configs).join("imported");
        let profiles = domain::import_full_config(&body.raw, &import_dir).map_err(|e| e.to_string())?;
        let selected_id = profiles.last().map(|p| p.id.clone());
        let imported = domain::merge_profiles(&mut config, profiles, None);
        if imported == 0 {
            return Err("未导入任何完整配置".to_string());
        }
        if let Some(id) = selected_id { config.selected_profile_id = Some(id); }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

// ── 订阅 ──────────────────────────────────────────────────────────────────────

async fn handle_save_subscription(State(state): State<SharedState>, Json(subscription): Json<Subscription>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        if let Some(existing) = config.subscriptions.iter_mut().find(|s| s.id == subscription.id) {
            *existing = subscription;
        } else {
            config.subscriptions.push(subscription);
        }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_remove_subscription(State(state): State<SharedState>, Path(id): Path<String>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        config.subscriptions.retain(|s| s.id != id);
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

#[derive(Deserialize)]
struct RefreshSubBody {
    core_type: CoreType,
}

async fn handle_refresh_subscription(State(state): State<SharedState>, Path(id): Path<String>, Json(body): Json<RefreshSubBody>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let _guard = state.subscription_refresh_lock.lock()
            .map_err(|_| "订阅刷新锁不可用".to_string())?;
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        let index = config.subscriptions.iter().position(|s| s.id == id)
            .ok_or_else(|| "未找到订阅".to_string())?;
        let socks_port = config.proxy.socks_port;
        let import_dir = std::path::PathBuf::from(state.store.paths().bin_configs).join("imported");

        let result = refresh_sub_impl(&mut config, index, body.core_type, socks_port, &import_dir);
        if let Err(e) = result {
            domain::apply_subscription_error(&mut config.subscriptions[index], e.to_string());
            state.store.save(&config).map_err(|e| e.to_string())?;
            return Err(e.to_string());
        }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

fn refresh_sub_impl(
    config: &mut AppConfig,
    index: usize,
    fallback_core: CoreType,
    socks_port: u16,
    import_dir: &std::path::PathBuf,
) -> anyhow::Result<()> {
    let sub = config.subscriptions.get(index).cloned().ok_or_else(|| anyhow::anyhow!("未找到订阅"))?;
    let ua = if sub.user_agent.trim().is_empty() { "v2rayN-tauri" } else { sub.user_agent.trim() };
    let proxy_url = format!("socks5h://127.0.0.1:{socks_port}");
    let body = download_with_fallback(&sub.url, ua, sub.use_proxy_on_refresh, &proxy_url)?;
    let raw = body;
    let core_type = sub.convert_core_target.unwrap_or(fallback_core);
    let fmt = domain::detect_import_format(&raw);
    let profiles = match fmt {
        domain::ImportFormat::ShareLinks => domain::import_share_links(&raw, core_type),
        _ => domain::import_full_config(&raw, import_dir),
    }?;
    let mut profiles = if matches!(fmt, domain::ImportFormat::ShareLinks) {
        domain::filter_profiles(profiles, sub.filter.as_deref())?
    } else { profiles };
    for p in &mut profiles { p.source_subscription_id = Some(sub.id.clone()); }
    domain::merge_profiles(config, profiles, Some(&sub.id));
    domain::apply_subscription_result(&mut config.subscriptions[index]);
    Ok(())
}

fn download_with_fallback(url: &str, ua: &str, use_proxy: bool, proxy_url: &str) -> anyhow::Result<String> {
    if use_proxy {
        if let Ok(body) = commands::build_client(ua, Some(proxy_url))
            .and_then(|c| c.get(url).send()?.error_for_status()?.text().map_err(Into::into))
        {
            if !body.trim().is_empty() { return Ok(body); }
        }
    }
    commands::build_client(ua, None)
        .and_then(|c| c.get(url).send()?.error_for_status()?.text().map_err(Into::into))
}

#[derive(Deserialize)]
struct CoreTypeBody {
    core_type: CoreType,
}

async fn handle_refresh_all_subscriptions(State(state): State<SharedState>, Json(body): Json<CoreTypeBody>) -> ApiResult<AppConfig> {
    blocking!(state, {
        commands::refresh_all_subscriptions_impl(body.core_type, &state).map_err(|e| e.to_string())
    })
}

async fn handle_refresh_all_background(State(state): State<SharedState>, Json(body): Json<CoreTypeBody>) -> ApiResult<Value> {
    let state_clone = state.clone();
    tokio::spawn(async move {
        let core_type = body.core_type;
        let result = spawn_blocking(move || {
            commands::refresh_all_subscriptions_impl(core_type, &state_clone)
        }).await;
        let success = result.as_ref().map(|r| r.is_ok()).unwrap_or(false);
        let message = match result {
            Ok(Ok(_)) => "全部订阅刷新完成".to_string(),
            Ok(Err(e)) => e.to_string(),
            Err(e) => e.to_string(),
        };
        state.event_sender.emit_background_task(crate::models::BackgroundTaskEvent {
            task: "subscription-refresh-all".into(),
            success,
            message,
        });
        if success {
            state.event_sender.emit_app_state_changed("subscription_refresh_all");
        }
    });
    Ok(Json(json!({ "ok": true })))
}

// ── 节点 ──────────────────────────────────────────────────────────────────────

async fn handle_remove_profile(State(state): State<SharedState>, Path(id): Path<String>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        config.profiles.retain(|p| p.id != id);
        if config.profiles.is_empty() {
            let p = crate::models::Profile::default();
            config.selected_profile_id = Some(p.id.clone());
            config.profiles.push(p);
        } else if config.selected_profile_id.as_ref() == Some(&id) {
            config.selected_profile_id = config.profiles.first().map(|p| p.id.clone());
        }
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_select_profile(State(state): State<SharedState>, Path(id): Path<String>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        if config.profiles.iter().any(|p| p.id == id) {
            config.selected_profile_id = Some(id);
            state.store.save(&config).map_err(|e| e.to_string())?;
            Ok(config)
        } else {
            Err("未找到节点".to_string())
        }
    })
}

// ── 配置预览 ──────────────────────────────────────────────────────────────────

async fn handle_generate_preview(State(state): State<SharedState>) -> Result<String, ApiError> {
    spawn_blocking(move || -> Result<String, String> {
        let config = state.store.load().map_err(|e| e.to_string())?;
        domain::generate_preview(&config).map_err(|e| e.to_string())
    })
    .await
    .map_err(ApiError::from)?
    .map_err(ApiError::from)
}

// ── 核心管理 ──────────────────────────────────────────────────────────────────

async fn handle_check_core_assets(State(state): State<SharedState>) -> ApiResult<Vec<crate::models::CoreAssetStatus>> {
    blocking!(state, {
        let statuses = crate::core_update::list_core_statuses(&state.core_paths)
            .map_err(|e| e.to_string())?;
        commands::update_core_status_cache(&state, &statuses).map_err(|e| e.to_string())?;
        Ok(statuses)
    })
}

async fn handle_download_core(State(state): State<SharedState>, Path(core_type_str): Path<String>) -> ApiResult<crate::models::CoreAssetStatus> {
    let core_type: CoreType = serde_json::from_value(json!(core_type_str))
        .map_err(|_| ApiError("无效的核心类型".to_string()))?;
    blocking!(state, {
        let status = crate::core_update::download_core(&state.core_paths, core_type)
            .map_err(|e| e.to_string())?;
        let mut cache = commands::load_cached_core_assets(&state).map_err(|e| e.to_string())?;
        if let Some(existing) = cache.iter_mut().find(|a| a.core_type == status.core_type) {
            *existing = status.clone();
        } else {
            cache.push(status.clone());
        }
        commands::update_core_status_cache(&state, &cache).map_err(|e| e.to_string())?;
        Ok(status)
    })
}

async fn handle_start_core(State(state): State<SharedState>) -> ApiResult<crate::models::RunningStatus> {
    blocking!(state, {
        let status = state.runtime.start(&state.event_sender, &state.store, &state.core_paths)
            .map_err(|e| e.to_string())?;
        let config = state.store.load().map_err(|e| e.to_string())?;
        if config.proxy.use_system_proxy {
            #[cfg(target_os = "macos")]
            crate::system_proxy::set_macos_proxy("127.0.0.1", config.proxy.socks_port, &config.proxy.bypass_domains)
                .map_err(|e| e.to_string())?;
        }
        state.event_sender.emit_app_state_changed("core_started");
        Ok(status)
    })
}

async fn handle_stop_core(State(state): State<SharedState>) -> ApiResult<crate::models::RunningStatus> {
    blocking!(state, {
        let status = state.runtime.stop().map_err(|e| e.to_string())?;
        let config = state.store.load().map_err(|e| e.to_string())?;
        if config.proxy.use_system_proxy {
            #[cfg(target_os = "macos")]
            crate::system_proxy::clear_macos_proxy().map_err(|e| e.to_string())?;
        }
        state.event_sender.emit_app_state_changed("core_stopped");
        Ok(status)
    })
}

async fn handle_restart_core(State(state): State<SharedState>) -> ApiResult<crate::models::RunningStatus> {
    blocking!(state, {
        let _ = state.runtime.stop();
        let status = state.runtime.start(&state.event_sender, &state.store, &state.core_paths)
            .map_err(|e| e.to_string())?;
        state.event_sender.emit_app_state_changed("core_restarted");
        Ok(status)
    })
}

// ── 系统代理 ──────────────────────────────────────────────────────────────────

async fn handle_enable_proxy(State(state): State<SharedState>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        crate::system_proxy::set_macos_proxy("127.0.0.1", config.proxy.socks_port, &config.proxy.bypass_domains)
            .map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "macos"))]
        return Err("当前仅实现 macOS 系统代理切换".to_string());
        config.proxy.use_system_proxy = true;
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

async fn handle_disable_proxy(State(state): State<SharedState>) -> ApiResult<AppConfig> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        crate::system_proxy::clear_macos_proxy().map_err(|e| e.to_string())?;
        #[cfg(not(target_os = "macos"))]
        return Err("当前仅实现 macOS 系统代理切换".to_string());
        config.proxy.use_system_proxy = false;
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok(config)
    })
}

// ── 出口探测 ──────────────────────────────────────────────────────────────────

async fn handle_probe(State(state): State<SharedState>) -> ApiResult<crate::models::ProxyProbe> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        if state.runtime.status().running {
            crate::network_probe::probe_proxy(config.proxy.socks_port)
        } else {
            crate::network_probe::probe_direct()
        }
        .map_err(|e| e.to_string())
    })
}

// ── Clash API ─────────────────────────────────────────────────────────────────

async fn handle_clash_proxy_groups(State(state): State<SharedState>) -> ApiResult<Vec<ClashProxyGroup>> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::parse_clash_proxy_groups(&config)
    })
}

#[derive(Deserialize)]
struct SelectProxyBody {
    name: String,
}

async fn handle_clash_select_proxy(
    State(state): State<SharedState>,
    Path(group_name): Path<String>,
    Json(body): Json<SelectProxyBody>,
) -> ApiResult<Value> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::clash_api_put(&config, &format!("/proxies/{}", urlencoding::encode(&group_name)), json!({ "name": body.name }))
            .map_err(|e| e.to_string())?;
        Ok(json!({ "ok": true }))
    })
}

async fn handle_clash_connections(State(state): State<SharedState>) -> ApiResult<Vec<ClashConnection>> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::parse_clash_connections(&config)
    })
}

async fn handle_clash_close_all_connections(State(state): State<SharedState>) -> ApiResult<Value> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::clash_api_delete(&config, "/connections").map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "ok": true }))
    })
}

async fn handle_clash_close_connection(State(state): State<SharedState>, Path(id): Path<String>) -> ApiResult<Value> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::clash_api_delete(&config, &format!("/connections/{}", urlencoding::encode(&id)))
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "ok": true }))
    })
}

async fn handle_clash_proxy_delay(State(state): State<SharedState>, Path(name): Path<String>) -> ApiResult<Value> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        let delay = commands::clash_api_delay_test(&config, &name).map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "delay": delay }))
    })
}

async fn handle_clash_providers(State(state): State<SharedState>) -> ApiResult<Vec<ClashProxyProvider>> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::parse_clash_proxy_providers(&config)
    })
}

async fn handle_clash_refresh_provider(State(state): State<SharedState>, Path(name): Path<String>) -> ApiResult<Value> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        commands::clash_api_put(&config, &format!("/providers/proxies/{}/healthcheck", urlencoding::encode(&name)), json!({}))
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "ok": true }))
    })
}

#[derive(Deserialize)]
struct RuleModeBody {
    rule_mode: String,
}

async fn handle_clash_rule_mode(State(state): State<SharedState>, Json(body): Json<RuleModeBody>) -> ApiResult<Value> {
    blocking!(state, {
        let mut config = state.store.load().map_err(|e| e.to_string())?;
        commands::clash_api_patch(&config, "/configs", json!({ "mode": body.rule_mode }))
            .map_err(|e| e.to_string())?;
        config.clash.rule_mode = body.rule_mode;
        state.store.save(&config).map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "ok": true }))
    })
}

async fn handle_clash_reload(State(state): State<SharedState>) -> ApiResult<Value> {
    blocking!(state, {
        let config = state.store.load().map_err(|e| e.to_string())?;
        let path = state.runtime.status().config_path
            .ok_or_else(|| "当前没有运行中的 mihomo 配置".to_string())?;
        commands::clash_api_put_with_query(&config, "/configs?force=true", json!({ "path": path }))
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(json!({ "ok": true }))
    })
}

use crate::models::{
    AppConfig, CoreType, DnsSettings, ExternalConfigFormat, MuxOverride, Profile,
    ProfileConfigType, ProfileProtocol, ProxySettings, RoutingItem, RoutingRule, RoutingRuleType,
    RoutingSettings, RoutingTemplate, Subscription, TunSettings,
};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const BUILTIN_ROUTE_WHITE: &str = include_str!("../../../v2rayN/ServiceLib/Sample/custom_routing_white");
const BUILTIN_ROUTE_BLACK: &str = include_str!("../../../v2rayN/ServiceLib/Sample/custom_routing_black");
const BUILTIN_ROUTE_GLOBAL: &str = include_str!("../../../v2rayN/ServiceLib/Sample/custom_routing_global");

#[derive(Debug, Clone)]
pub struct ConfigArtifact {
    pub file_name: String,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct HelperConfig {
    pub core_type: CoreType,
    pub artifact: ConfigArtifact,
}

#[derive(Debug, Clone)]
pub struct RuntimeBundle {
    pub main_core_type: CoreType,
    pub main_artifact: ConfigArtifact,
    pub helper: Option<HelperConfig>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    ShareLinks,
    SingBoxJson,
    XrayJson,
    ClashYaml,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportPreview {
    pub format: ImportFormat,
    pub profile_names: Vec<String>,
    pub profile_count: usize,
    pub stores_as_external: bool,
    pub external_format: Option<ExternalConfigFormat>,
    pub message: Option<String>,
}

pub fn ensure_profile(config: &AppConfig) -> Result<&Profile> {
    let selected = config
        .selected_profile_id
        .as_ref()
        .context("当前没有选中的节点")?;

    config
        .profiles
        .iter()
        .find(|profile| &profile.id == selected)
        .context("未找到选中的节点")
}

pub fn ensure_routing_items(routing: &mut RoutingSettings) {
    if routing.items.is_empty() {
        routing.items = builtin_routing_items();
    }
    normalize_routing_items(routing);
}

pub fn builtin_routing_items() -> Vec<RoutingItem> {
    [
        ("V4-绕过大陆(Whitelist)", BUILTIN_ROUTE_WHITE),
        ("V4-黑名单(Blacklist)", BUILTIN_ROUTE_BLACK),
        ("V4-全局(Global)", BUILTIN_ROUTE_GLOBAL),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (remarks, raw))| RoutingItem {
        id: crate::models::new_id("routing"),
        remarks: remarks.into(),
        url: String::new(),
        rule_set: parse_routing_rules_json(raw).unwrap_or_default(),
        rule_num: parse_routing_rules_json(raw).map(|rules| rules.len()).unwrap_or(0),
        enabled: true,
        locked: false,
        custom_icon: None,
        custom_ruleset_path_4_singbox: None,
        domain_strategy: None,
        domain_strategy_4_singbox: None,
        sort: index + 1,
        is_active: index == 0,
    })
    .collect()
}

pub fn normalize_routing_items(routing: &mut RoutingSettings) {
    if routing.domain_strategy.is_empty() {
        routing.domain_strategy = "AsIs".into();
    }

    if routing.items.is_empty() {
        return;
    }

    for (index, item) in routing.items.iter_mut().enumerate() {
        if item.id.is_empty() {
            item.id = crate::models::new_id("routing");
        }
        if item.sort == 0 {
            item.sort = index + 1;
        }
        for rule in &mut item.rule_set {
            if rule.id.is_empty() {
                rule.id = crate::models::new_id("routing-rule");
            }
        }
        item.rule_num = item.rule_set.len();
    }

    let active_id = routing
        .items
        .iter()
        .find(|item| item.is_active)
        .map(|item| item.id.clone())
        .or_else(|| routing.routing_index_id.clone())
        .or_else(|| routing.items.first().map(|item| item.id.clone()));

    for item in &mut routing.items {
        item.is_active = Some(&item.id) == active_id.as_ref();
    }
    routing.routing_index_id = active_id;
    routing.items.sort_by_key(|item| item.sort);
}

pub fn parse_routing_rules_json(raw: &str) -> Result<Vec<RoutingRule>> {
    let mut rules = serde_json::from_str::<Vec<RoutingRule>>(raw).context("路由规则 JSON 解析失败")?;
    for rule in &mut rules {
        if rule.id.is_empty() {
            rule.id = crate::models::new_id("routing-rule");
        }
        if !rule.enabled {
            continue;
        }
    }
    Ok(rules)
}

pub fn export_routing_rules_json(rules: &[RoutingRule]) -> Result<String> {
    serde_json::to_string_pretty(rules).context("导出路由规则 JSON 失败")
}

pub fn active_routing_item(routing: &RoutingSettings) -> Option<&RoutingItem> {
    routing
        .routing_index_id
        .as_ref()
        .and_then(|id| routing.items.iter().find(|item| &item.id == id))
        .or_else(|| routing.items.iter().find(|item| item.is_active))
        .or_else(|| routing.items.first())
}

pub fn set_active_routing_item(routing: &mut RoutingSettings, routing_id: &str) {
    for item in &mut routing.items {
        item.is_active = item.id == routing_id;
    }
    routing.routing_index_id = Some(routing_id.to_string());
}

pub fn routing_template_from_raw(raw: &str) -> Result<RoutingTemplate> {
    serde_json::from_str::<RoutingTemplate>(raw).context("路由模板解析失败")
}

pub fn apply_routing_template(
    routing: &mut RoutingSettings,
    template: RoutingTemplate,
    advanced_only: bool,
) -> Result<usize> {
    let prefix = template.version;
    if !advanced_only && routing.items.iter().any(|item| item.remarks.starts_with(&prefix)) {
        return Ok(0);
    }

    let mut max_sort = routing.items.iter().map(|item| item.sort).max().unwrap_or(0);
    let mut imported = 0usize;
    for (index, mut item) in template.routing_items.into_iter().enumerate() {
        let rules = if !item.rule_set.is_empty() {
            item.rule_set
        } else {
            Vec::new()
        };
        item.id = crate::models::new_id("routing");
        item.remarks = format!("{prefix}-{}", item.remarks);
        item.enabled = true;
        item.is_active = !advanced_only && index == 0 && imported == 0;
        item.sort = {
            max_sort += 1;
            max_sort
        };
        item.rule_set = rules
            .into_iter()
            .map(|mut rule| {
                rule.id = crate::models::new_id("routing-rule");
                rule
            })
            .collect();
        item.rule_num = item.rule_set.len();
        routing.items.push(item);
        imported += 1;
    }

    normalize_routing_items(routing);
    Ok(imported)
}

pub fn import_share_links(raw: &str, core_type: CoreType) -> Result<Vec<Profile>> {
    let mut profiles = Vec::new();
    let mut seen = HashSet::new();
    let candidates = expand_subscription_body(raw);

    for line in candidates.iter().map(String::as_str).map(str::trim).filter(|line| !line.is_empty()) {
        let mut profile = if line.starts_with("vless://") {
            parse_vless(line)?
        } else if line.starts_with("trojan://") {
            parse_trojan(line)?
        } else if line.starts_with("ss://") {
            parse_shadowsocks(line)?
        } else if line.starts_with("vmess://") {
            parse_vmess(line)?
        } else if line.starts_with("hysteria2://") || line.starts_with("hy2://") {
            parse_hysteria2(line)?
        } else if line.starts_with("tuic://") {
            parse_tuic(line)?
        } else if line.starts_with("naive://")
            || line.starts_with("naive+https://")
            || line.starts_with("naive+quic://")
        {
            parse_naive(line)?
        } else if line.starts_with("anytls://") {
            parse_anytls(line)?
        } else if line.starts_with("wireguard://") {
            parse_wireguard(line)?
        } else {
            continue;
        };

        if seen.insert(format!("{}:{}:{}", profile.server, profile.port, profile.name)) {
            profile.core_type = core_type.clone();
            profile.config_type = ProfileConfigType::Native;
            profiles.push(profile);
        }
    }

    Ok(profiles)
}

pub fn merge_imported_profiles(config: &mut AppConfig, imported: Vec<Profile>) -> usize {
    merge_profiles(config, imported, None)
}

pub fn merge_profiles(
    config: &mut AppConfig,
    imported: Vec<Profile>,
    source_subscription_id: Option<&str>,
) -> usize {
    let before = config.profiles.len();
    if let Some(source_id) = source_subscription_id {
        config
            .profiles
            .retain(|profile| profile.source_subscription_id.as_deref() != Some(source_id));
    }

    for profile in imported {
        if config
            .profiles
            .iter()
            .any(|existing| {
                existing.server == profile.server
                    && existing.port == profile.port
                    && existing.name == profile.name
                    && existing.config_type == profile.config_type
                    && existing.external_config_path == profile.external_config_path
            })
        {
            continue;
        }
        config.profiles.push(profile);
    }

    if config.selected_profile_id.is_none() {
        config.selected_profile_id = config.profiles.first().map(|profile| profile.id.clone());
    }

    config.profiles.len() - before
}

pub fn apply_subscription_result(subscription: &mut Subscription) {
    apply_subscription_checked(subscription);
    subscription.last_synced_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".into()),
    );
    subscription.last_error = None;
}

pub fn apply_subscription_error(subscription: &mut Subscription, message: impl Into<String>) {
    apply_subscription_checked(subscription);
    subscription.last_error = Some(message.into());
}

pub fn apply_subscription_checked(subscription: &mut Subscription) {
    subscription.last_checked_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".into()),
    );
}

pub fn filter_profiles(imported: Vec<Profile>, filter: Option<&str>) -> Result<Vec<Profile>> {
    let Some(filter) = filter.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(imported);
    };

    let regex = Regex::new(filter).with_context(|| format!("订阅过滤器无效: {filter}"))?;
    Ok(imported
        .into_iter()
        .filter(|profile| regex.is_match(&profile.name))
        .collect())
}

pub fn detect_import_format(raw: &str) -> ImportFormat {
    let lines = expand_subscription_body(raw);
    if lines.iter().any(|line| looks_like_share_link(line)) {
        return ImportFormat::ShareLinks;
    }

    if let Ok(value) = serde_json::from_str::<Value>(raw) {
        if json_contains_singbox_config(&value) {
            return ImportFormat::SingBoxJson;
        }
        if json_contains_xray_config(&value) {
            return ImportFormat::XrayJson;
        }
    }

    if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(raw) {
        if yaml_is_clash_config(&value) {
            return ImportFormat::ClashYaml;
        }
    }

    ImportFormat::Unknown
}

pub fn preview_import(raw: &str, core_type: CoreType) -> Result<ImportPreview> {
    match detect_import_format(raw) {
        ImportFormat::ShareLinks => {
            let profiles = import_share_links(raw, core_type)?;
            Ok(ImportPreview {
                format: ImportFormat::ShareLinks,
                profile_names: profiles.iter().map(|profile| profile.name.clone()).collect(),
                profile_count: profiles.len(),
                stores_as_external: false,
                external_format: None,
                message: Some("将作为普通分享链接导入".into()),
            })
        }
        ImportFormat::SingBoxJson => {
            let names = extract_json_outbound_names(raw, "tag")?;
            let profile_count = names.len().max(1);
            Ok(ImportPreview {
                format: ImportFormat::SingBoxJson,
                profile_names: names,
                profile_count,
                stores_as_external: true,
                external_format: Some(ExternalConfigFormat::SingBox),
                message: Some("将作为外部 sing-box 配置导入".into()),
            })
        }
        ImportFormat::XrayJson => {
            let names = extract_json_outbound_names(raw, "tag")?;
            let profile_count = names.len().max(1);
            Ok(ImportPreview {
                format: ImportFormat::XrayJson,
                profile_names: names,
                profile_count,
                stores_as_external: true,
                external_format: Some(ExternalConfigFormat::Xray),
                message: Some("将作为外部 Xray 配置导入".into()),
            })
        }
        ImportFormat::ClashYaml => {
            let names = extract_clash_proxy_names(raw)?;
            Ok(ImportPreview {
                format: ImportFormat::ClashYaml,
                profile_names: names,
                profile_count: 1,
                stores_as_external: true,
                external_format: Some(ExternalConfigFormat::Clash),
                message: Some("将作为外部 Clash YAML 导入，并使用 mihomo 运行".into()),
            })
        }
        ImportFormat::Unknown => Ok(ImportPreview {
            format: ImportFormat::Unknown,
            profile_names: vec![],
            profile_count: 0,
            stores_as_external: false,
            external_format: None,
            message: Some("未识别到支持的分享链接、JSON 或 YAML 配置".into()),
        }),
    }
}

pub fn import_full_config(raw: &str, storage_dir: &Path) -> Result<Vec<Profile>> {
    fs::create_dir_all(storage_dir)
        .with_context(|| format!("创建导入配置目录失败: {}", storage_dir.display()))?;

    match detect_import_format(raw) {
        ImportFormat::SingBoxJson => import_json_external_configs(
            raw,
            storage_dir,
            "singbox",
            CoreType::SingBox,
            ExternalConfigFormat::SingBox,
            "singbox_custom",
            extract_json_profile_name,
        ),
        ImportFormat::XrayJson => import_json_external_configs(
            raw,
            storage_dir,
            "xray",
            CoreType::Xray,
            ExternalConfigFormat::Xray,
            "v2ray_custom",
            extract_xray_profile_name,
        ),
        ImportFormat::ClashYaml => {
            let path = persist_external_config(raw, storage_dir, "clash", "yaml")?;
            Ok(vec![build_external_profile(
                "clash_custom",
                CoreType::Mihomo,
                ExternalConfigFormat::Clash,
                &path,
            )])
        }
        ImportFormat::ShareLinks => Err(anyhow!("该内容属于分享链接，请使用分享链接导入接口")),
        ImportFormat::Unknown => Err(anyhow!("未识别的完整配置格式")),
    }
}

fn persist_external_config(raw: &str, storage_dir: &Path, prefix: &str, ext: &str) -> Result<String> {
    let file_name = format!("{prefix}-{}.{}", new_timestamp_suffix(), ext);
    let path = storage_dir.join(file_name);
    fs::write(&path, raw).with_context(|| format!("写入外部配置失败: {}", path.display()))?;
    Ok(path.to_string_lossy().to_string())
}

fn build_external_profile(
    name: &str,
    core_type: CoreType,
    external_format: ExternalConfigFormat,
    path: &str,
) -> Profile {
    Profile {
        name: name.into(),
        core_type,
        config_type: ProfileConfigType::External,
        external_config_format: Some(external_format),
        external_config_path: Some(path.into()),
        network: "tcp".into(),
        security: "none".into(),
        tls: false,
        ..Profile::default()
    }
}

fn import_json_external_configs(
    raw: &str,
    storage_dir: &Path,
    prefix: &str,
    core_type: CoreType,
    external_format: ExternalConfigFormat,
    fallback_name: &str,
    name_fn: fn(&Value, usize) -> String,
) -> Result<Vec<Profile>> {
    let value = serde_json::from_str::<Value>(raw).context("JSON 配置解析失败")?;
    if let Some(items) = value.as_array() {
        let mut profiles = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let serialized = serde_json::to_string_pretty(item)?;
            let path = persist_external_config(&serialized, storage_dir, prefix, "json")?;
            profiles.push(build_external_profile(
                &name_fn(item, index),
                core_type.clone(),
                external_format.clone(),
                &path,
            ));
        }
        return Ok(profiles);
    }

    let path = persist_external_config(raw, storage_dir, prefix, "json")?;
    let derived_name = name_fn(&value, 0);
    Ok(vec![build_external_profile(
        if derived_name.trim().is_empty() { fallback_name } else { &derived_name },
        core_type,
        external_format,
        &path,
    )])
}

fn extract_json_outbound_names(raw: &str, field: &str) -> Result<Vec<String>> {
    let value = serde_json::from_str::<Value>(raw).context("JSON 配置解析失败")?;
    Ok(extract_json_items(&value)
        .into_iter()
        .flat_map(|item| {
            item.get("outbounds")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|outbound| outbound.get(field).and_then(Value::as_str).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect())
}

fn extract_clash_proxy_names(raw: &str) -> Result<Vec<String>> {
    let value = serde_yaml::from_str::<serde_yaml::Value>(raw).context("Clash YAML 解析失败")?;
    Ok(value
        .as_mapping()
        .and_then(|mapping| mapping.get(serde_yaml::Value::from("proxies")))
        .and_then(serde_yaml::Value::as_sequence)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            item.as_mapping()
                .and_then(|mapping| mapping.get(serde_yaml::Value::from("name")))
                .and_then(serde_yaml::Value::as_str)
                .map(str::to_string)
        })
        .collect())
}

fn extract_json_items<'a>(value: &'a Value) -> Vec<&'a Value> {
    value.as_array().map(|items| items.iter().collect()).unwrap_or_else(|| vec![value])
}

fn json_contains_singbox_config(value: &Value) -> bool {
    extract_json_items(value).into_iter().any(|item| {
        item.get("inbounds").is_some() && item.get("outbounds").is_some() && item.get("route").is_some() && item.get("dns").is_some()
    })
}

fn json_contains_xray_config(value: &Value) -> bool {
    extract_json_items(value)
        .into_iter()
        .any(|item| item.get("inbounds").is_some() && item.get("outbounds").is_some() && item.get("routing").is_some())
}

fn yaml_is_clash_config(value: &serde_yaml::Value) -> bool {
    let Some(mapping) = value.as_mapping() else {
        return false;
    };
    let has_rules = mapping.contains_key(serde_yaml::Value::from("rules"));
    let has_proxies = mapping.contains_key(serde_yaml::Value::from("proxies"));
    let has_port = [
        "port",
        "socks-port",
        "mixed-port",
        "redir-port",
        "tproxy-port",
    ]
    .iter()
    .any(|key| mapping.contains_key(serde_yaml::Value::from(*key)));
    has_rules && has_proxies && has_port
}

fn extract_json_profile_name(value: &Value, index: usize) -> String {
    value
        .get("remarks")
        .and_then(Value::as_str)
        .or_else(|| value.get("tag").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("singbox_custom_{}", index + 1))
}

fn extract_xray_profile_name(value: &Value, index: usize) -> String {
    value
        .get("remarks")
        .and_then(Value::as_str)
        .or_else(|| value.get("tag").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| format!("v2ray_custom_{}", index + 1))
}

fn new_timestamp_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

pub fn generate_runtime_bundle(config: &AppConfig) -> Result<RuntimeBundle> {
    let profile = ensure_profile(config)?;

    if profile.config_type == ProfileConfigType::External {
        return generate_external_runtime_bundle(profile, config);
    }

    if config.tun.enabled && matches!(profile.core_type, CoreType::Xray) {
        let tun_protect_port = portpicker::pick_unused_port().unwrap_or(30901);
        let proxy_relay_port = portpicker::pick_unused_port().unwrap_or(30902);
        let main_config = generate_xray_config(
            profile,
            &config.mux,
            &config.proxy,
            &config.tun,
            &config.dns,
            &config.routing,
            Some((tun_protect_port, proxy_relay_port)),
        )?;
        let helper_config = generate_tun_helper_sing_box_config(
            profile,
            &config.proxy,
            &config.tun,
            &config.dns,
            &config.routing,
            tun_protect_port,
            proxy_relay_port,
        );

        return Ok(RuntimeBundle {
            main_core_type: CoreType::Xray,
            main_artifact: json_artifact("config.json", &main_config)?,
            helper: Some(HelperConfig {
                core_type: CoreType::SingBox,
                artifact: json_artifact("config-helper.json", &helper_config)?,
            }),
        });
    }

    let main_config = match profile.core_type {
        CoreType::SingBox => {
            generate_sing_box_config(profile, &config.mux, &config.proxy, &config.tun, &config.dns, &config.routing)?
        }
        CoreType::Xray => {
            generate_xray_config(profile, &config.mux, &config.proxy, &config.tun, &config.dns, &config.routing, None)?
        }
        CoreType::Mihomo => {
            return Err(anyhow!("请选择导入的 Clash YAML 配置后再启动 mihomo"));
        }
    };

    Ok(RuntimeBundle {
        main_core_type: profile.core_type.clone(),
        main_artifact: json_artifact("config.json", &main_config)?,
        helper: None,
    })
}

pub fn generate_preview(config: &AppConfig) -> Result<String> {
    let bundle = generate_runtime_bundle(config)?;
    let mut sections = vec![format!(
        "# main ({})\n# file: {}\n{}",
        bundle.main_core_type.key(),
        bundle.main_artifact.file_name,
        bundle.main_artifact.content
    )];

    if let Some(helper) = bundle.helper {
        sections.push(format!(
            "# helper ({})\n# file: {}\n{}",
            helper.core_type.key(),
            helper.artifact.file_name,
            helper.artifact.content
        ));
    }

    Ok(sections.join("\n\n"))
}

fn json_artifact(file_name: &str, value: &Value) -> Result<ConfigArtifact> {
    Ok(ConfigArtifact {
        file_name: file_name.into(),
        content: serde_json::to_string_pretty(value)?,
    })
}

fn yaml_artifact(file_name: &str, content: String) -> ConfigArtifact {
    ConfigArtifact {
        file_name: file_name.into(),
        content,
    }
}

fn generate_external_runtime_bundle(profile: &Profile, config: &AppConfig) -> Result<RuntimeBundle> {
    let external_format = profile
        .external_config_format
        .clone()
        .context("外部配置缺少格式信息")?;
    let raw = load_external_config_text(profile)?;

    match (&profile.core_type, external_format) {
        (CoreType::SingBox, ExternalConfigFormat::SingBox) => {
            let parsed: Value = serde_json::from_str(&raw).context("sing-box 外部配置不是合法 JSON")?;
            Ok(RuntimeBundle {
                main_core_type: CoreType::SingBox,
                main_artifact: json_artifact("config.json", &parsed)?,
                helper: None,
            })
        }
        (CoreType::Xray, ExternalConfigFormat::Xray) => {
            let parsed: Value = serde_json::from_str(&raw).context("Xray 外部配置不是合法 JSON")?;
            Ok(RuntimeBundle {
                main_core_type: CoreType::Xray,
                main_artifact: json_artifact("config.json", &parsed)?,
                helper: None,
            })
        }
        (CoreType::Mihomo, ExternalConfigFormat::Clash) => {
            let patched = patch_mihomo_config(&raw, config)?;
            Ok(RuntimeBundle {
                main_core_type: CoreType::Mihomo,
                main_artifact: yaml_artifact("config.yaml", patched),
                helper: None,
            })
        }
        _ => Err(anyhow!("外部配置格式与核心类型不匹配")),
    }
}

fn load_external_config_text(profile: &Profile) -> Result<String> {
    let path = profile
        .external_config_path
        .as_deref()
        .context("外部配置缺少文件路径")?;
    fs::read_to_string(path).with_context(|| format!("读取外部配置失败: {path}"))
}

fn patch_mihomo_config(raw: &str, config: &AppConfig) -> Result<String> {
    let mut yaml = serde_yaml::from_str::<serde_yaml::Value>(raw).context("Clash YAML 解析失败")?;
    let root = yaml
        .as_mapping_mut()
        .context("Clash YAML 根节点必须是对象")?;

    root.insert(serde_yaml::Value::from("mixed-port"), serde_yaml::Value::from(config.proxy.socks_port));
    root.insert(
        serde_yaml::Value::from("external-controller"),
        serde_yaml::Value::from(format!("{}:{}", config.clash.bind_address, clash_external_controller_port(config))),
    );
    root.insert(serde_yaml::Value::from("allow-lan"), serde_yaml::Value::from(config.clash.allow_lan));
    root.insert(serde_yaml::Value::from("bind-address"), serde_yaml::Value::from(config.clash.bind_address.clone()));
    root.insert(serde_yaml::Value::from("ipv6"), serde_yaml::Value::from(config.clash.enable_ipv6));
    root.insert(
        serde_yaml::Value::from("mode"),
        serde_yaml::Value::from(clash_mode(
            if config.clash.rule_mode == "unchanged" {
                &config.routing.mode
            } else {
                &config.clash.rule_mode
            },
        )),
    );
    root.insert(serde_yaml::Value::from("log-level"), serde_yaml::Value::from("warning"));
    if let Some(secret) = config.clash.secret.as_ref().filter(|value| !value.is_empty()) {
        root.insert(serde_yaml::Value::from("secret"), serde_yaml::Value::from(secret.clone()));
    }
    if config.tun.enabled {
        root.insert(
            serde_yaml::Value::from("tun"),
            serde_yaml::to_value(serde_json::json!({
                "enable": true,
                "stack": config.tun.stack,
                "auto-route": config.tun.auto_route,
                "auto-detect-interface": true,
                "dns-hijack": ["any:53"],
            }))
            .context("生成 Clash TUN 片段失败")?,
        );
    }
    if config.clash.enable_mixin_content && !config.clash.mixin_content.trim().is_empty() {
        let mixin = serde_yaml::from_str::<serde_yaml::Value>(&config.clash.mixin_content)
            .context("Clash mixin YAML 解析失败")?;
        merge_yaml_mapping(root, mixin);
    }

    serde_yaml::to_string(&yaml).context("生成 mihomo 运行配置失败")
}

fn clash_external_controller_port(config: &AppConfig) -> u16 {
    if config.clash.external_controller_port > 0 {
        config.clash.external_controller_port
    } else {
        config.proxy.socks_port.saturating_add(5)
    }
}

fn clash_mode(routing_mode: &str) -> &'static str {
    match routing_mode {
        "global" => "global",
        "direct" => "direct",
        _ => "rule",
    }
}

fn merge_yaml_mapping(root: &mut serde_yaml::Mapping, mixin: serde_yaml::Value) {
    let Some(mixin_map) = mixin.as_mapping() else {
        return;
    };
    for (key, value) in mixin_map {
        if let Some(name) = key.as_str() {
            if let Some(target_key) = name.strip_prefix("append-") {
                merge_yaml_sequence(root, target_key, value.clone(), false);
                continue;
            }
            if let Some(target_key) = name.strip_prefix("prepend-") {
                merge_yaml_sequence(root, target_key, value.clone(), true);
                continue;
            }
            if let Some(target_key) = name.strip_prefix("removed-") {
                remove_yaml_sequence_items(root, target_key, value.clone());
                continue;
            }
        }
        root.insert(key.clone(), value.clone());
    }
}

fn merge_yaml_sequence(
    root: &mut serde_yaml::Mapping,
    key: &str,
    value: serde_yaml::Value,
    prepend: bool,
) {
    let target_key = serde_yaml::Value::from(key);
    let Some(source_items) = value.as_sequence().cloned() else {
        root.insert(target_key, value);
        return;
    };
    let target = root
        .entry(target_key)
        .or_insert_with(|| serde_yaml::Value::Sequence(vec![]));
    let Some(items) = target.as_sequence_mut() else {
        *target = serde_yaml::Value::Sequence(source_items);
        return;
    };
    if prepend {
        let mut merged = source_items;
        merged.extend(items.clone());
        *items = merged;
    } else {
        items.extend(source_items);
    }
}

fn remove_yaml_sequence_items(root: &mut serde_yaml::Mapping, key: &str, value: serde_yaml::Value) {
    let Some(remove_items) = value.as_sequence() else {
        return;
    };
    let Some(existing) = root
        .get_mut(serde_yaml::Value::from(key))
        .and_then(serde_yaml::Value::as_sequence_mut)
    else {
        return;
    };
    existing.retain(|item| {
        let current = serde_yaml::to_string(item).unwrap_or_default();
        !remove_items.iter().any(|remove| current.starts_with(&serde_yaml::to_string(remove).unwrap_or_default()))
    });
}

fn generate_sing_box_config(
    profile: &Profile,
    mux: &crate::models::MuxSettings,
    proxy: &ProxySettings,
    tun: &TunSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
) -> Result<Value> {
    let outbound = build_singbox_outbound(profile, mux);
    let (route_rules, route_rule_sets) = singbox_route_rules(tun, routing);

    let mut inbounds = vec![
        json!({
            "type": "mixed",
            "tag": "socks",
            "listen": "127.0.0.1",
            "listen_port": proxy.socks_port,
        }),
    ];

    if tun.enabled {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": resolve_tun_interface_name(tun),
            "address": ["172.18.0.1/30", "fdfe:dcba:9876::1/126"],
            "mtu": tun.mtu,
            "auto_route": tun.auto_route,
            "strict_route": tun.strict_route,
            "stack": tun.stack,
        }));
    }

    let mut route = json!({
        "auto_detect_interface": true,
        "final": singbox_final_outbound(routing),
        "rules": route_rules,
        "default_domain_resolver": {
            "server": "bootstrap",
            "strategy": active_singbox_domain_strategy(routing),
        }
    });
    route["rule_set"] = Value::Array(singbox_rule_set_entries(&route_rule_sets));

    Ok(json!({
        "log": {
            "level": "warn",
            "timestamp": true,
        },
        "dns": singbox_dns_block(dns, routing),
        "inbounds": inbounds,
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" },
            { "type": "block", "tag": "block" }
        ],
        "route": route
    }))
}

fn build_singbox_outbound(profile: &Profile, mux: &crate::models::MuxSettings) -> Value {
    let mut ob = json!({
        "tag": "proxy",
        "server": profile.server,
        "server_port": profile.port,
    });

    match profile.protocol {
        ProfileProtocol::Vless => {
            ob["type"] = json!("vless");
            ob["uuid"] = json!(profile.uuid.clone().unwrap_or_default());
            if let Some(ref flow) = profile.flow {
                if !flow.is_empty() {
                    ob["flow"] = json!(flow);
                }
            }
        }
        ProfileProtocol::Vmess => {
            ob["type"] = json!("vmess");
            ob["uuid"] = json!(profile.uuid.clone().unwrap_or_default());
            ob["security"] = json!(profile.method.clone().unwrap_or_else(|| "auto".into()));
        }
        ProfileProtocol::Trojan => {
            ob["type"] = json!("trojan");
            ob["password"] = json!(profile.password.clone().unwrap_or_default());
        }
        ProfileProtocol::Shadowsocks => {
            ob["type"] = json!("shadowsocks");
            ob["method"] = json!(profile.method.clone().unwrap_or_else(|| "aes-128-gcm".into()));
            ob["password"] = json!(profile.password.clone().unwrap_or_default());
        }
        ProfileProtocol::Hysteria2 => {
            ob["type"] = json!("hysteria2");
            ob["password"] = json!(profile.password.clone().unwrap_or_else(|| profile.uuid.clone().unwrap_or_default()));
        }
        ProfileProtocol::Tuic => {
            ob["type"] = json!("tuic");
            ob["uuid"] = json!(profile.uuid.clone().unwrap_or_default());
            ob["password"] = json!(profile.password.clone().unwrap_or_default());
            ob["congestion_control"] = json!(profile.method.clone().unwrap_or_else(|| "bbr".into()));
        }
        ProfileProtocol::Naive => {
            ob["type"] = json!("naive");
            ob["username"] = json!(profile.uuid.clone().unwrap_or_default());
            ob["password"] = json!(profile.password.clone().unwrap_or_default());
        }
        ProfileProtocol::Anytls => {
            ob["type"] = json!("anytls");
            ob["password"] = json!(profile.password.clone().unwrap_or_else(|| profile.uuid.clone().unwrap_or_default()));
        }
        ProfileProtocol::WireGuard => {
            ob["type"] = json!("wireguard");
            ob["private_key"] = json!(profile.password.clone().unwrap_or_default());
            ob["peer_public_key"] = json!(profile.reality_public_key.clone().unwrap_or_default());
            ob["local_address"] = json!(["172.19.0.2/32"]);
        }
    }

    if !matches!(profile.protocol, ProfileProtocol::Shadowsocks | ProfileProtocol::WireGuard) {
        if let Some(tls) = tls_object(profile) {
            ob["tls"] = tls;
        }
    }

    if matches!(profile.protocol, ProfileProtocol::Vless | ProfileProtocol::Vmess | ProfileProtocol::Trojan) {
        if let Some(t) = transport_object(profile) {
            ob["transport"] = t;
        }
    }

    if let Some(multiplex) = singbox_multiplex_object(profile, mux) {
        ob["multiplex"] = multiplex;
    }

    ob
}

fn mux_enabled_for_profile(profile: &Profile, global_enabled: bool) -> bool {
    match profile.mux_override {
        MuxOverride::FollowGlobal => global_enabled,
        MuxOverride::ForceEnable => true,
        MuxOverride::ForceDisable => false,
    }
}

fn singbox_multiplex_object(profile: &Profile, mux: &crate::models::MuxSettings) -> Option<Value> {
    if !mux_enabled_for_profile(profile, mux.enabled) || mux.sing_box_protocol.is_empty() {
        return None;
    }

    if matches!(profile.protocol, ProfileProtocol::Vless) && profile.flow.as_deref().is_some_and(|value| !value.is_empty()) {
        return None;
    }

    Some(json!({
        "enabled": true,
        "protocol": mux.sing_box_protocol,
        "max_connections": mux.sing_box_max_connections,
        "padding": mux.sing_box_padding,
    }))
}

fn generate_tun_helper_sing_box_config(
    profile: &Profile,
    _proxy: &ProxySettings,
    tun: &TunSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
    tun_protect_port: u16,
    proxy_relay_port: u16,
) -> Value {
    let interface_name = resolve_tun_interface_name(tun);
    let static_rule_sets = vec!["geosite-cn".to_string(), "geoip-cn".to_string()];

    json!({
        "log": {
            "level": "warn",
            "timestamp": true
        },
        "dns": singbox_dns_block(dns, routing),
        "inbounds": [
            {
                "type": "tun",
                "tag": "tun-in",
                "interface_name": interface_name,
                "address": ["172.18.0.1/30", "fdfe:dcba:9876::1/126"],
                "mtu": tun.mtu,
                "auto_route": tun.auto_route,
                "strict_route": tun.strict_route,
                "stack": tun.stack
            },
            {
                "type": "shadowsocks",
                "tag": "tun-protect-ss",
                "listen": "127.0.0.1",
                "listen_port": tun_protect_port,
                "method": "none",
                "password": "none"
            }
        ],
        "outbounds": [
            {
                "type": "shadowsocks",
                "tag": "proxy",
                "server": "127.0.0.1",
                "server_port": proxy_relay_port,
                "method": "none",
                "password": "none"
            },
            { "type": "direct", "tag": "direct" },
            { "type": "block", "tag": "block" }
        ],
        "route": {
            "auto_detect_interface": true,
            "final": singbox_final_outbound(routing),
            "default_domain_resolver": { "server": "bootstrap" },
            "rules": [
                { "inbound": ["tun-protect-ss"], "outbound": "direct" },
                { "network": "udp", "port": [135, 137, 138, 139, 5353], "action": "reject" },
                { "ip_cidr": ["224.0.0.0/3", "ff00::/8"], "action": "reject" },
                { "action": "sniff" },
                { "protocol": ["dns"], "action": "hijack-dns" },
                { "rule_set": ["geosite-cn"], "outbound": "direct" },
                { "rule_set": ["geoip-cn"], "outbound": "direct" }
            ],
            "rule_set": singbox_rule_set_entries(&static_rule_sets)
        },
        "meta": {
            "selected_profile": profile.name,
            "relay_port": proxy_relay_port,
            "tun_protect_port": tun_protect_port
        }
    })
}

fn resolve_tun_interface_name(tun: &TunSettings) -> String {
    if !tun.interface_name.is_empty() {
        return tun.interface_name.clone();
    }
    if cfg!(target_os = "macos") {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(42);
        format!("utun{}", nanos % 99)
    } else {
        "singbox_tun".to_string()
    }
}

fn singbox_final_outbound(routing: &RoutingSettings) -> &'static str {
    match routing.mode.as_str() {
        "direct" => "direct",
        _ => "proxy",
    }
}

const BOOTSTRAP_DNS_ADDRESS: &str = "223.5.5.5";

fn singbox_dns_block(dns: &DnsSettings, routing: &RoutingSettings) -> Value {
    let mut bootstrap = parse_dns_server_new_format(BOOTSTRAP_DNS_ADDRESS);
    bootstrap["tag"] = json!("bootstrap");

    let mut remote = parse_dns_server_new_format(&dns.remote_dns);
    remote["tag"] = json!("remote");
    remote["detour"] = json!("proxy");
    remote["domain_resolver"] = json!("bootstrap");

    let mut local = parse_dns_server_new_format(&dns.direct_dns);
    local["tag"] = json!("local");
    local["domain_resolver"] = json!("bootstrap");

    let mut rules = vec![
        json!({
            "rule_set": ["geosite-google"],
            "server": "remote",
            "strategy": "prefer_ipv4"
        }),
        json!({
            "rule_set": ["geosite-cn"],
            "server": "local",
            "strategy": "prefer_ipv4"
        }),
    ];

    let mut collected_rule_sets = BTreeSet::from([
        "geosite-google".to_string(),
        "geosite-cn".to_string(),
    ]);

    if let Some(active) = active_routing_item(routing) {
        for rule in &active.rule_set {
            if !rule.enabled || matches!(rule.rule_type, RoutingRuleType::Routing) {
                continue;
            }
            if let Some(dns_rule) = routing_rule_to_singbox_dns_rule(rule) {
                if let Some(tags) = dns_rule.get("rule_set").and_then(Value::as_array) {
                    for tag in tags.iter().filter_map(Value::as_str) {
                        collected_rule_sets.insert(tag.to_string());
                    }
                }
                rules.push(dns_rule);
            }
        }
    }

    json!({
        "servers": [bootstrap, remote, local],
        "rules": rules,
        "final": "remote",
        "independent_cache": true,
        "strategy": singbox_dns_strategy(routing),
        "rule_set": singbox_rule_set_entries(&collected_rule_sets.into_iter().collect::<Vec<_>>())
    })
}

fn parse_dns_server_new_format(address: &str) -> Value {
    if address.starts_with("https://") {
        if let Ok(url) = Url::parse(address) {
            return json!({
                "type": "https",
                "server": url.host_str().unwrap_or(address),
                "server_port": url.port().unwrap_or(443),
                "path": url.path()
            });
        }
    } else if address.starts_with("tls://") {
        let host = address.trim_start_matches("tls://");
        return json!({ "type": "tls", "server": host });
    } else if address.starts_with("quic://") {
        let host = address.trim_start_matches("quic://");
        return json!({ "type": "quic", "server": host });
    }
    json!({ "type": "udp", "server": address })
}

fn singbox_dns_strategy(routing: &RoutingSettings) -> String {
    let strategy = active_singbox_domain_strategy(routing);
    if strategy.is_empty() {
        "prefer_ipv4".into()
    } else {
        strategy
    }
}

fn active_singbox_domain_strategy(routing: &RoutingSettings) -> String {
    active_routing_item(routing)
        .and_then(|item| item.domain_strategy_4_singbox.clone())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            (!routing.domain_strategy_4_singbox.is_empty())
                .then(|| routing.domain_strategy_4_singbox.clone())
        })
        .unwrap_or_default()
}

fn active_xray_domain_strategy(routing: &RoutingSettings) -> String {
    active_routing_item(routing)
        .and_then(|item| item.domain_strategy.clone())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| routing.domain_strategy.clone())
}

fn normalize_outbound_tag(tag: Option<&str>) -> &'static str {
    match tag.unwrap_or("proxy") {
        "direct" => "direct",
        "block" => "block",
        _ => "proxy",
    }
}

fn routing_rule_to_singbox_dns_rule(rule: &RoutingRule) -> Option<Value> {
    let mut base = serde_json::Map::new();
    let server = match normalize_outbound_tag(rule.outbound_tag.as_deref()) {
        "direct" => "local",
        "block" => "local",
        _ => "remote",
    };
    base.insert("server".into(), json!(server));
    if !apply_singbox_rule_matchers(rule, &mut base) {
        return None;
    }
    Some(Value::Object(base))
}

fn routing_rule_to_singbox_route_rules(rule: &RoutingRule) -> (Vec<Value>, Vec<String>, bool) {
    let mut rules = Vec::new();
    let mut rule_sets = BTreeSet::new();
    let outbound = normalize_outbound_tag(rule.outbound_tag.as_deref());
    let contains_ip = rule.ip.iter().any(|value| !value.trim().is_empty());

    let mut base = serde_json::Map::new();
    if outbound == "block" {
        base.insert("action".into(), json!("reject"));
    } else {
        base.insert("outbound".into(), json!(outbound));
    }
    apply_singbox_base_matchers(rule, &mut base);

    let mut domain_rule = base.clone();
    let mut has_domain_match = false;
    for value in rule.domain.iter().filter(|value| !value.trim().is_empty()) {
        if append_singbox_domain_matcher(value, &mut domain_rule, &mut rule_sets) {
            has_domain_match = true;
        }
    }
    if has_domain_match || has_non_domain_matcher(&domain_rule) {
        rules.push(Value::Object(domain_rule));
    }

    let mut ip_rule = base.clone();
    let mut has_ip_match = false;
    for value in rule.ip.iter().filter(|value| !value.trim().is_empty()) {
        if append_singbox_ip_matcher(value, &mut ip_rule, &mut rule_sets) {
            has_ip_match = true;
        }
    }
    if has_ip_match {
        rules.push(Value::Object(ip_rule));
    }

    let mut process_rule = base.clone();
    if !rule.process.is_empty() {
        process_rule.insert("process_name".into(), json!(rule.process));
        rules.push(Value::Object(process_rule));
    }

    if rules.is_empty() && has_non_domain_matcher(&base) {
        rules.push(Value::Object(base));
    }

    (rules, rule_sets.into_iter().collect(), contains_ip)
}

fn apply_singbox_rule_matchers(rule: &RoutingRule, base: &mut serde_json::Map<String, Value>) -> bool {
    apply_singbox_base_matchers(rule, base);
    let mut matched = false;
    let mut rule_sets = BTreeSet::new();
    for value in rule.domain.iter().filter(|value| !value.trim().is_empty()) {
        if append_singbox_domain_matcher(value, base, &mut rule_sets) {
            matched = true;
        }
    }
    for value in rule.ip.iter().filter(|value| !value.trim().is_empty()) {
        if append_singbox_ip_matcher(value, base, &mut rule_sets) {
            matched = true;
        }
    }
    if !rule.process.is_empty() {
        base.insert("process_name".into(), json!(rule.process));
        matched = true;
    }
    matched
}

fn apply_singbox_base_matchers(rule: &RoutingRule, base: &mut serde_json::Map<String, Value>) {
    if let Some(port) = rule.port.as_deref().filter(|value| !value.is_empty()) {
        let (ports, ranges) = split_ports(port);
        if !ports.is_empty() {
            base.insert("port".into(), json!(ports));
        }
        if !ranges.is_empty() {
            base.insert("port_range".into(), json!(ranges));
        }
    }
    if let Some(network) = rule.network.as_deref().filter(|value| !value.is_empty()) {
        let values = split_csv_text(network);
        if !values.is_empty() {
            base.insert("network".into(), json!(values));
        }
    }
    if !rule.protocol.is_empty() {
        base.insert("protocol".into(), json!(rule.protocol));
    }
    if !rule.inbound_tag.is_empty() {
        base.insert("inbound".into(), json!(rule.inbound_tag));
    }
}

fn has_non_domain_matcher(rule: &serde_json::Map<String, Value>) -> bool {
    [
        "port",
        "port_range",
        "network",
        "protocol",
        "inbound",
        "process_name",
    ]
    .into_iter()
    .any(|key| rule.contains_key(key))
}

fn append_singbox_domain_matcher(
    raw: &str,
    rule: &mut serde_json::Map<String, Value>,
    rule_sets: &mut BTreeSet<String>,
) -> bool {
    if raw.starts_with('#') || raw.starts_with("ext:") || raw.starts_with("ext-domain:") {
        return false;
    }
    if let Some(value) = raw.strip_prefix("geosite:") {
        let tag = format!("geosite-{}", value);
        push_json_array_string(rule, "rule_set", tag.clone());
        rule_sets.insert(tag);
        return true;
    }
    if let Some(value) = raw.strip_prefix("regexp:") {
        push_json_array_string(rule, "domain_regex", value.replace("\\,", ","));
        return true;
    }
    if let Some(value) = raw.strip_prefix("domain:") {
        push_json_array_string(rule, "domain_suffix", value.to_string());
        return true;
    }
    if let Some(value) = raw.strip_prefix("full:") {
        push_json_array_string(rule, "domain", value.to_string());
        return true;
    }
    if let Some(value) = raw.strip_prefix("keyword:") {
        push_json_array_string(rule, "domain_keyword", value.to_string());
        return true;
    }
    if let Some(value) = raw.strip_prefix("dotless:") {
        push_json_array_string(rule, "domain_keyword", value.to_string());
        return true;
    }
    push_json_array_string(rule, "domain", raw.replace("\\,", ","));
    true
}

fn append_singbox_ip_matcher(
    raw: &str,
    rule: &mut serde_json::Map<String, Value>,
    rule_sets: &mut BTreeSet<String>,
) -> bool {
    if raw.starts_with("ext:") || raw.starts_with("ext-ip:") {
        return false;
    }
    if let Some(value) = raw.strip_prefix("geoip:") {
        let tag = format!("geoip-{}", value);
        push_json_array_string(rule, "rule_set", tag.clone());
        rule_sets.insert(tag);
        return true;
    }
    push_json_array_string(rule, "ip_cidr", raw.to_string());
    true
}

fn collect_custom_ruleset_tags(path: &str) -> Vec<String> {
    load_custom_singbox_rule_set(path)
        .ok()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.get("tag").and_then(Value::as_str).map(ToString::to_string))
        .collect()
}

fn load_custom_singbox_rule_set(path: &str) -> Result<Vec<Value>> {
    let resolved = PathBuf::from(path);
    if !resolved.exists() {
        return Ok(vec![]);
    }
    let raw = fs::read_to_string(&resolved)
        .with_context(|| format!("读取 sing-box 自定义 ruleset 失败: {}", resolved.display()))?;
    serde_json::from_str::<Vec<Value>>(&raw).context("自定义 ruleset JSON 解析失败")
}

fn push_json_array_string(target: &mut serde_json::Map<String, Value>, key: &str, value: String) {
    let entry = target
        .entry(key.to_string())
        .or_insert_with(|| Value::Array(vec![]));
    if let Some(items) = entry.as_array_mut() {
        items.push(json!(value));
    }
}

fn split_ports(raw: &str) -> (Vec<u16>, Vec<String>) {
    let mut ports = Vec::new();
    let mut ranges = Vec::new();
    for part in split_csv_text(raw) {
        if part.contains('-') {
            ranges.push(part.replace('-', ":"));
        } else if let Ok(port) = part.parse::<u16>() {
            ports.push(port);
        }
    }
    (ports, ranges)
}

fn split_csv_text(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

const SINGBOX_RULESET_URL: &str =
    "https://raw.githubusercontent.com/2dust/sing-box-rules/rule-set-{kind}/{tag}.srs";

fn singbox_rule_set_entries(tags: &[String]) -> Vec<Value> {
    tags.iter()
        .map(|tag| {
            let kind = if tag.starts_with("geoip") { "geoip" } else { "geosite" };
            let url = SINGBOX_RULESET_URL
                .replace("{kind}", kind)
                .replace("{tag}", tag);
            json!({
                "type": "remote",
                "format": "binary",
                "tag": tag,
                "url": url,
                "download_detour": "proxy"
            })
        })
        .collect()
}

fn singbox_route_rules(tun: &TunSettings, routing: &RoutingSettings) -> (Value, Vec<String>) {
    let mut rules: Vec<Value> = Vec::new();
    let mut rule_sets = BTreeSet::new();

    if tun.enabled {
        rules.push(json!({ "network": "udp", "port": [135, 137, 138, 139, 5353], "action": "reject" }));
        rules.push(json!({ "ip_cidr": ["224.0.0.0/3", "ff00::/8"], "action": "reject" }));
    }

    rules.push(json!({ "action": "sniff" }));
    rules.push(json!({ "protocol": ["dns"], "action": "hijack-dns" }));

    rules.push(json!({ "rule_set": ["geosite-cn"], "outbound": "direct" }));
    rules.push(json!({ "rule_set": ["geoip-cn"], "outbound": "direct" }));
    rule_sets.insert("geosite-cn".to_string());
    rule_sets.insert("geoip-cn".to_string());

    rules.push(json!({ "outbound": "direct", "clash_mode": "Direct" }));
    rules.push(json!({ "outbound": "proxy", "clash_mode": "Global" }));

    let resolve_strategy = active_singbox_domain_strategy(routing);
    let mut ip_rules = Vec::new();
    if routing.domain_strategy == "IPOnDemand" {
        rules.push(json!({
            "action": "resolve",
            "strategy": resolve_strategy
        }));
    }

    if let Some(active) = active_routing_item(routing) {
        for user_rule in &active.rule_set {
            if !user_rule.enabled || matches!(user_rule.rule_type, RoutingRuleType::Dns) {
                continue;
            }
            let (route_rules, route_rule_sets, contains_ip) =
                routing_rule_to_singbox_route_rules(user_rule);
            for tag in route_rule_sets {
                rule_sets.insert(tag);
            }
            if routing.domain_strategy == "IPIfNonMatch" && contains_ip {
                ip_rules.extend(route_rules);
            } else {
                rules.extend(route_rules);
            }
        }
        if let Some(custom_path) = active.custom_ruleset_path_4_singbox.as_deref() {
            for tag in collect_custom_ruleset_tags(custom_path) {
                rule_sets.insert(tag);
            }
        }
    }

    if routing.domain_strategy == "IPIfNonMatch" {
        rules.push(json!({
            "action": "resolve",
            "strategy": resolve_strategy
        }));
        rules.extend(ip_rules);
    }

    (Value::Array(rules), rule_sets.into_iter().collect())
}

fn xray_routing_rules(routing: &RoutingSettings) -> Vec<Value> {
    let mut rules = vec![json!({
        "type": "field",
        "port": "53",
        "outboundTag": "direct"
    })];

    if let Some(active) = active_routing_item(routing) {
        for rule in &active.rule_set {
            if !rule.enabled || matches!(rule.rule_type, RoutingRuleType::Dns) {
                continue;
            }
            rules.extend(routing_rule_to_xray_rules(rule));
        }
    }

    let final_rule = if active_xray_domain_strategy(routing) == "IPIfNonMatch" {
        json!({
            "type": "field",
            "ip": ["0.0.0.0/0", "::/0"],
            "outboundTag": if routing.mode == "direct" { "direct" } else { "proxy" }
        })
    } else {
        json!({
            "type": "field",
            "network": "tcp,udp",
            "outboundTag": if routing.mode == "direct" { "direct" } else { "proxy" }
        })
    };
    rules.push(final_rule);
    rules
}

fn routing_rule_to_xray_rules(rule: &RoutingRule) -> Vec<Value> {
    let outbound = normalize_outbound_tag(rule.outbound_tag.as_deref());
    let base = json!({
        "type": "field",
        "outboundTag": outbound,
        "port": rule.port.as_deref().filter(|value| !value.is_empty()),
        "network": rule.network.as_deref().filter(|value| !value.is_empty()),
        "protocol": (!rule.protocol.is_empty()).then_some(rule.protocol.clone()),
        "inboundTag": (!rule.inbound_tag.is_empty()).then_some(rule.inbound_tag.clone()),
    });

    let mut rules = Vec::new();

    if !rule.domain.is_empty() {
        let mut item = base.clone();
        item["domain"] = json!(rule
            .domain
            .iter()
            .filter(|value| !value.starts_with('#') && !value.starts_with("ext:") && !value.starts_with("ext-domain:"))
            .map(|value| value.replace("\\,", ","))
            .collect::<Vec<_>>());
        rules.push(item);
    }

    if !rule.ip.is_empty() {
        let mut item = base.clone();
        item["ip"] = json!(rule
            .ip
            .iter()
            .filter(|value| !value.starts_with("ext:") && !value.starts_with("ext-ip:"))
            .cloned()
            .collect::<Vec<_>>());
        rules.push(item);
    }

    if !rule.process.is_empty() {
        let mut item = base.clone();
        item["process"] = json!(rule.process);
        rules.push(item);
    }

    if rules.is_empty()
        && (rule.port.as_deref().is_some_and(|value| !value.is_empty())
            || rule.network.as_deref().is_some_and(|value| !value.is_empty())
            || !rule.protocol.is_empty()
            || !rule.inbound_tag.is_empty())
    {
        rules.push(base);
    }

    rules
}

fn generate_xray_config(
    profile: &Profile,
    mux: &crate::models::MuxSettings,
    proxy: &ProxySettings,
    tun: &TunSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
    tun_ports: Option<(u16, u16)>,
) -> Result<Value> {
    if tun.enabled && tun_ports.is_none() {
        return Err(anyhow!("Xray TUN 缺少 relay 配置"));
    }

    let mut outbound = match profile.protocol {
        ProfileProtocol::Vless => json!({
            "tag": "proxy",
            "protocol": "vless",
            "settings": {
                "vnext": [{
                    "address": profile.server,
                    "port": profile.port,
                    "users": [{
                        "id": profile.uuid.clone().unwrap_or_default(),
                        "encryption": "none",
                        "flow": profile.flow.clone().unwrap_or_default(),
                    }]
                }]
            },
            "streamSettings": xray_stream_settings(profile),
        }),
        ProfileProtocol::Vmess => json!({
            "tag": "proxy",
            "protocol": "vmess",
            "settings": {
                "vnext": [{
                    "address": profile.server,
                    "port": profile.port,
                    "users": [{
                        "id": profile.uuid.clone().unwrap_or_default(),
                        "alterId": 0,
                        "security": "auto",
                    }]
                }]
            },
            "streamSettings": xray_stream_settings(profile),
        }),
        ProfileProtocol::Trojan => json!({
            "tag": "proxy",
            "protocol": "trojan",
            "settings": {
                "servers": [{
                    "address": profile.server,
                    "port": profile.port,
                    "password": profile.password.clone().unwrap_or_default(),
                }]
            },
            "streamSettings": xray_stream_settings(profile),
        }),
        ProfileProtocol::Shadowsocks => json!({
            "tag": "proxy",
            "protocol": "shadowsocks",
            "settings": {
                "servers": [{
                    "address": profile.server,
                    "port": profile.port,
                    "method": profile.method.clone().unwrap_or_else(|| "aes-128-gcm".into()),
                    "password": profile.password.clone().unwrap_or_default(),
                }]
            }
        }),
        ProfileProtocol::Naive
        | ProfileProtocol::Hysteria2
        | ProfileProtocol::Tuic
        | ProfileProtocol::WireGuard
        | ProfileProtocol::Anytls => {
            return Err(anyhow!("当前协议暂不支持 Xray 出站，请切换到 sing-box"));
        }
    };

    if tun_ports.is_some() {
        outbound["streamSettings"]["sockopt"] = json!({
            "dialerProxy": "tun-protect-ss"
        });
    }

    if let Some(mux_object) = xray_mux_object(profile, mux) {
        outbound["mux"] = mux_object;
    }

    let mut inbounds = vec![
        json!({
            "tag": "socks-in",
            "listen": "127.0.0.1",
            "port": proxy.socks_port,
            "protocol": "socks",
            "settings": { "udp": true },
            "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
        }),
        json!({
            "tag": "http-in",
            "listen": "127.0.0.1",
            "port": proxy.http_port,
            "protocol": "http",
            "settings": {},
            "sniffing": { "enabled": true, "destOverride": ["http", "tls"] }
        }),
    ];

    let mut outbounds = vec![
        outbound,
        json!({ "tag": "direct", "protocol": "freedom", "settings": {} }),
        json!({ "tag": "block", "protocol": "blackhole", "settings": {} }),
    ];

    let mut rules = xray_routing_rules(routing);

    if let Some((tun_protect_port, proxy_relay_port)) = tun_ports {
        inbounds.push(json!({
            "tag": "proxy-relay-ss",
            "listen": "127.0.0.1",
            "port": proxy_relay_port,
            "protocol": "shadowsocks",
            "settings": {
                "network": "tcp,udp",
                "method": "none",
                "password": "none"
            }
        }));
        outbounds.push(json!({
            "tag": "tun-protect-ss",
            "protocol": "shadowsocks",
            "settings": {
                "servers": [{
                    "address": "127.0.0.1",
                    "port": tun_protect_port,
                    "method": "none",
                    "password": "none"
                }]
            }
        }));
        rules.insert(
            0,
            json!({
                "type": "field",
                "inboundTag": ["proxy-relay-ss"],
                "outboundTag": "proxy"
            }),
        );
    }

    Ok(json!({
        "log": {
            "loglevel": "info"
        },
        "dns": {
            "servers": [dns.remote_dns, dns.direct_dns]
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "routing": {
            "domainStrategy": active_xray_domain_strategy(routing),
            "rules": rules
        }
    }))
}

fn xray_mux_object(profile: &Profile, mux: &crate::models::MuxSettings) -> Option<Value> {
    if !mux_enabled_for_profile(profile, mux.enabled) {
        return None;
    }

    match profile.protocol {
        ProfileProtocol::Vmess => Some(json!({
            "enabled": true,
            "concurrency": mux.xray_concurrency.unwrap_or(8),
        })),
        ProfileProtocol::Vless => {
            if profile.flow.as_deref().is_some_and(|value| !value.is_empty()) {
                None
            } else {
                Some(json!({
                    "enabled": true,
                    "xudpConcurrency": mux.xray_xudp_concurrency.unwrap_or(16),
                    "xudpProxyUDP443": mux
                        .xray_xudp_proxy_udp_443
                        .clone()
                        .unwrap_or_else(|| "reject".into()),
                }))
            }
        }
        ProfileProtocol::Trojan | ProfileProtocol::Shadowsocks => Some(json!({
            "enabled": true,
            "xudpConcurrency": mux.xray_xudp_concurrency.unwrap_or(16),
            "xudpProxyUDP443": mux
                .xray_xudp_proxy_udp_443
                .clone()
                .unwrap_or_else(|| "reject".into()),
        })),
        _ => None,
    }
}

fn tls_object(profile: &Profile) -> Option<Value> {
    if !profile.tls && profile.security != "reality" {
        return None;
    }

    if profile.security == "reality" {
        return Some(json!({
            "enabled": true,
            "server_name": profile.sni.clone().unwrap_or_default(),
            "reality": {
                "enabled": true,
                "public_key": profile.reality_public_key.clone().unwrap_or_default(),
                "short_id": profile.reality_short_id.clone().unwrap_or_default(),
            },
            "utls": {
                "enabled": true,
                "fingerprint": profile.fingerprint.clone().unwrap_or_else(|| "chrome".into())
            }
        }));
    }

    let mut tls = json!({
        "enabled": true,
        "server_name": profile.sni.clone().unwrap_or_else(|| profile.server.clone()),
    });

    if !profile.alpn.is_empty() {
        tls["alpn"] = json!(profile.alpn);
    }

    if let Some(ref fp) = profile.fingerprint {
        if !fp.is_empty() {
            tls["utls"] = json!({
                "enabled": true,
                "fingerprint": fp
            });
        }
    }

    Some(tls)
}

fn transport_object(profile: &Profile) -> Option<Value> {
    match profile.network.as_str() {
        "ws" => Some(json!({
            "type": "ws",
            "path": profile.path.clone().unwrap_or_else(|| "/".into()),
            "headers": {
                "Host": profile.host.clone().unwrap_or_default()
            }
        })),
        "grpc" => Some(json!({
            "type": "grpc",
            "service_name": profile.service_name.clone().or_else(|| profile.path.clone()).unwrap_or_default(),
        })),
        "http" | "h2" => Some(json!({
            "type": "http",
            "path": profile.path.clone().unwrap_or_else(|| "/".into()),
            "host": split_csv(&profile.host),
        })),
        _ => None,
    }
}

fn xray_stream_settings(profile: &Profile) -> Value {
    let mut stream = json!({
        "network": normalize_network(&profile.network),
    });

    if profile.tls || profile.security == "reality" {
        let security = if profile.security == "reality" { "reality" } else { "tls" };
        stream["security"] = Value::String(security.into());
    }

    match normalize_network(&profile.network).as_str() {
        "ws" => {
            stream["wsSettings"] = json!({
                "path": profile.path.clone().unwrap_or_else(|| "/".into()),
                "headers": {
                    "Host": profile.host.clone().unwrap_or_default()
                }
            });
        }
        "grpc" => {
            stream["grpcSettings"] = json!({
                "serviceName": profile.service_name.clone().or_else(|| profile.path.clone()).unwrap_or_default(),
                "multiMode": false
            });
        }
        "h2" => {
            stream["httpSettings"] = json!({
                "host": split_csv(&profile.host),
                "path": profile.path.clone().unwrap_or_else(|| "/".into())
            });
        }
        _ => {}
    }

    if profile.security == "reality" {
        stream["realitySettings"] = json!({
            "serverName": profile.sni.clone().unwrap_or_else(|| profile.server.clone()),
            "fingerprint": profile.fingerprint.clone().unwrap_or_else(|| "chrome".into()),
            "publicKey": profile.reality_public_key.clone().unwrap_or_default(),
            "shortId": profile.reality_short_id.clone().unwrap_or_default(),
        });
    } else if profile.tls {
        stream["tlsSettings"] = json!({
            "serverName": profile.sni.clone().unwrap_or_else(|| profile.server.clone()),
            "fingerprint": profile.fingerprint.clone().unwrap_or_else(|| "chrome".into()),
            "alpn": profile.alpn,
        });
    }

    stream
}

fn parse_vless(raw: &str) -> Result<Profile> {
    let url = Url::parse(raw)?;
    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("VLESS")),
        protocol: ProfileProtocol::Vless,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(443),
        uuid: Some(url.username().to_string()),
        network: normalize_network(&query_value(&url, "type").unwrap_or_else(|| "tcp".into())),
        security: query_value(&url, "security").unwrap_or_else(|| "tls".into()),
        tls: query_value(&url, "security")
            .map(|value| value == "tls" || value == "reality")
            .unwrap_or(true),
        sni: query_value(&url, "sni"),
        host: query_value(&url, "host"),
        path: query_value(&url, "path"),
        service_name: query_value(&url, "serviceName"),
        flow: query_value(&url, "flow"),
        fingerprint: query_value(&url, "fp"),
        reality_public_key: query_value(&url, "pbk"),
        reality_short_id: query_value(&url, "sid"),
        alpn: query_value(&url, "alpn")
            .map(|value| value.split(',').map(|part| part.to_string()).collect())
            .unwrap_or_default(),
        ..Profile::default()
    })
}

fn parse_trojan(raw: &str) -> Result<Profile> {
    let url = Url::parse(raw)?;
    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("Trojan")),
        protocol: ProfileProtocol::Trojan,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(443),
        password: Some(url.username().to_string()),
        network: normalize_network(&query_value(&url, "type").unwrap_or_else(|| "tcp".into())),
        security: query_value(&url, "security").unwrap_or_else(|| "tls".into()),
        tls: query_value(&url, "security")
            .map(|value| value == "tls" || value == "reality")
            .unwrap_or(true),
        sni: query_value(&url, "sni"),
        host: query_value(&url, "host"),
        path: query_value(&url, "path"),
        service_name: query_value(&url, "serviceName"),
        fingerprint: query_value(&url, "fp"),
        reality_public_key: query_value(&url, "pbk"),
        reality_short_id: query_value(&url, "sid"),
        alpn: query_value(&url, "alpn")
            .map(|value| value.split(',').map(|part| part.to_string()).collect())
            .unwrap_or_default(),
        ..Profile::default()
    })
}

fn parse_shadowsocks(raw: &str) -> Result<Profile> {
    let without_scheme = raw.trim_start_matches("ss://");
    let (main_part, name) = without_scheme
        .split_once('#')
        .map(|(left, right)| (left, decode_fragment(right)))
        .unwrap_or((without_scheme, "Shadowsocks".into()));
    let (main_part, plugin_part) = main_part
        .split_once('?')
        .map(|(left, right)| (left, Some(right)))
        .unwrap_or((main_part, None));
    let (auth_part, host_part) = if main_part.contains('@') {
        main_part
            .split_once('@')
            .map(|(auth, host)| (auth.to_string(), host.to_string()))
            .context("无效的 Shadowsocks 链接")?
    } else {
        let decoded = decode_base64(main_part)?;
        decoded
            .split_once('@')
            .map(|(auth, host)| (auth.to_string(), host.to_string()))
            .context("无效的 Shadowsocks 编码")?
    };

    let decoded_auth = if auth_part.contains(':') {
        auth_part
    } else {
        decode_base64(&auth_part)?
    };

    let (method, password) = decoded_auth
        .split_once(':')
        .map(|(method, password)| (method.to_string(), password.to_string()))
        .context("无效的 Shadowsocks 用户信息")?;
    let (server, port) = host_part
        .split_once(':')
        .map(|(host, port)| (host.to_string(), port.parse::<u16>().unwrap_or(443)))
        .context("无效的 Shadowsocks 地址")?;

    let mut profile = Profile {
        id: crate::models::new_id("profile"),
        name,
        protocol: ProfileProtocol::Shadowsocks,
        server,
        port,
        password: Some(password),
        method: Some(method),
        tls: false,
        security: "none".into(),
        ..Profile::default()
    };

    if let Some(plugin) = plugin_part {
        let params = url::form_urlencoded::parse(plugin.as_bytes()).collect::<Vec<_>>();
        if let Some(plugin_value) = params
            .iter()
            .find_map(|(key, value)| (key == "plugin").then(|| value.to_string()))
        {
            profile.host = Some(plugin_value);
        }
    }

    Ok(profile)
}

fn parse_vmess(raw: &str) -> Result<Profile> {
    let encoded = raw.trim_start_matches("vmess://");
    let decoded = decode_base64(encoded)?;
    let payload: Value = serde_json::from_str(&decoded).context("无法解析 VMess JSON")?;

    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: payload.get("ps").and_then(Value::as_str).unwrap_or("VMess").to_string(),
        protocol: ProfileProtocol::Vmess,
        server: payload.get("add").and_then(Value::as_str).unwrap_or_default().to_string(),
        port: parse_port(payload.get("port")).unwrap_or(443),
        uuid: payload.get("id").and_then(Value::as_str).map(str::to_string),
        method: payload.get("scy").and_then(Value::as_str).map(str::to_string),
        network: normalize_network(payload.get("net").and_then(Value::as_str).unwrap_or("tcp")),
        security: payload.get("tls").and_then(Value::as_str).unwrap_or("none").to_string(),
        tls: payload
            .get("tls")
            .and_then(Value::as_str)
            .map(|value| value == "tls" || value == "reality")
            .unwrap_or(false),
        sni: payload.get("sni").and_then(Value::as_str).map(str::to_string),
        host: payload.get("host").and_then(Value::as_str).map(str::to_string),
        path: payload.get("path").and_then(Value::as_str).map(str::to_string),
        service_name: payload.get("serviceName").and_then(Value::as_str).map(str::to_string),
        fingerprint: payload.get("fp").and_then(Value::as_str).map(str::to_string),
        alpn: payload
            .get("alpn")
            .and_then(Value::as_str)
            .map(|value| value.split(',').map(|part| part.to_string()).collect())
            .unwrap_or_default(),
        ..Profile::default()
    })
}

fn parse_hysteria2(raw: &str) -> Result<Profile> {
    let normalized = raw.replacen("hy2://", "hysteria2://", 1);
    let url = Url::parse(&normalized)?;
    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("Hysteria2")),
        protocol: ProfileProtocol::Hysteria2,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(443),
        password: Some(if url.username().is_empty() {
            query_value(&url, "password").unwrap_or_default()
        } else {
            url.username().to_string()
        }),
        security: "tls".into(),
        tls: true,
        sni: query_value(&url, "sni"),
        fingerprint: query_value(&url, "fp"),
        alpn: query_value(&url, "alpn")
            .map(|value| value.split(',').map(|part| part.to_string()).collect())
            .unwrap_or_default(),
        host: query_value(&url, "obfs"),
        path: query_value(&url, "obfs-password"),
        ..Profile::default()
    })
}

fn parse_tuic(raw: &str) -> Result<Profile> {
    let url = Url::parse(raw)?;
    let (uuid, password) = url
        .username()
        .split_once(':')
        .map(|(uuid, password)| (uuid.to_string(), password.to_string()))
        .unwrap_or((url.username().to_string(), query_value(&url, "password").unwrap_or_default()));

    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("TUIC")),
        protocol: ProfileProtocol::Tuic,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(443),
        uuid: Some(uuid),
        password: Some(password),
        method: query_value(&url, "congestion_control"),
        security: "tls".into(),
        tls: true,
        sni: query_value(&url, "sni"),
        alpn: query_value(&url, "alpn")
            .map(|value| value.split(',').map(|part| part.to_string()).collect())
            .unwrap_or_default(),
        ..Profile::default()
    })
}

fn parse_naive(raw: &str) -> Result<Profile> {
    let normalized = raw
        .replacen("naive+https://", "naive://", 1)
        .replacen("naive+quic://", "naive://", 1);
    let url = Url::parse(&normalized)?;
    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("Naive")),
        protocol: ProfileProtocol::Naive,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(443),
        uuid: Some(url.username().to_string()),
        password: Some(url.password().unwrap_or_default().to_string()),
        security: "tls".into(),
        tls: true,
        sni: query_value(&url, "sni").or_else(|| query_value(&url, "host")),
        ..Profile::default()
    })
}

fn parse_anytls(raw: &str) -> Result<Profile> {
    let url = Url::parse(raw)?;
    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("AnyTLS")),
        protocol: ProfileProtocol::Anytls,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(443),
        password: Some(url.username().to_string()),
        security: query_value(&url, "security").unwrap_or_else(|| "tls".into()),
        tls: true,
        sni: query_value(&url, "sni"),
        alpn: query_value(&url, "alpn")
            .map(|value| value.split(',').map(|part| part.to_string()).collect())
            .unwrap_or_default(),
        ..Profile::default()
    })
}

fn parse_wireguard(raw: &str) -> Result<Profile> {
    let url = Url::parse(raw)?;
    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: decode_fragment(url.fragment().unwrap_or("WireGuard")),
        protocol: ProfileProtocol::WireGuard,
        server: url.host_str().unwrap_or_default().to_string(),
        port: url.port().unwrap_or(51820),
        password: query_value(&url, "secretKey"),
        reality_public_key: query_value(&url, "publicKey"),
        host: query_value(&url, "address"),
        path: query_value(&url, "reserved"),
        tls: false,
        security: "none".into(),
        ..Profile::default()
    })
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find_map(|(query_key, value)| (query_key == key).then(|| value.to_string()))
}

fn normalize_network(network: &str) -> String {
    match network {
        "http" => "h2".into(),
        other => other.to_string(),
    }
}

fn split_csv(value: &Option<String>) -> Vec<String> {
    value
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn expand_subscription_body(raw: &str) -> Vec<String> {
    let direct_lines = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if direct_lines.iter().any(|line| looks_like_share_link(line)) {
        return direct_lines;
    }

    if let Ok(decoded) = decode_base64(raw.trim()) {
        let decoded_lines = decoded
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        if decoded_lines.iter().any(|line| looks_like_share_link(line)) {
            return decoded_lines;
        }
    }

    direct_lines
}

fn looks_like_share_link(line: &str) -> bool {
    [
        "vless://",
        "vmess://",
        "trojan://",
        "ss://",
        "hysteria2://",
        "hy2://",
        "tuic://",
        "naive://",
        "naive+https://",
        "naive+quic://",
        "anytls://",
        "wireguard://",
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn parse_port(value: Option<&Value>) -> Option<u16> {
    match value {
        Some(Value::String(value)) => value.parse::<u16>().ok(),
        Some(Value::Number(value)) => value.as_u64().and_then(|number| u16::try_from(number).ok()),
        _ => None,
    }
}

fn decode_fragment(input: &str) -> String {
    urlencoding::decode(input)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| input.to_string())
}

fn decode_base64(input: &str) -> Result<String> {
    let normalized = input.replace('-', "+").replace('_', "/");
    let padding = (4 - normalized.len() % 4) % 4;
    let padded = format!("{normalized}{}", "=".repeat(padding));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(padded)
        .context("Base64 解码失败")?;
    Ok(String::from_utf8(bytes)?)
}

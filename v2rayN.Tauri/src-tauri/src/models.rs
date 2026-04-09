use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoreType {
    Xray,
    SingBox,
    Mihomo,
}

impl Default for CoreType {
    fn default() -> Self {
        Self::SingBox
    }
}

impl CoreType {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::SingBox => "sing_box",
            Self::Mihomo => "mihomo",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileProtocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Tuic,
    WireGuard,
    Naive,
    Anytls,
}

impl Default for ProfileProtocol {
    fn default() -> Self {
        Self::Vless
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProfileConfigType {
    #[default]
    Native,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalConfigFormat {
    SingBox,
    Xray,
    Clash,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MuxOverride {
    #[default]
    FollowGlobal,
    ForceEnable,
    ForceDisable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub core_type: CoreType,
    pub protocol: ProfileProtocol,
    pub server: String,
    pub port: u16,
    pub uuid: Option<String>,
    pub password: Option<String>,
    pub method: Option<String>,
    pub network: String,
    pub security: String,
    pub tls: bool,
    pub sni: Option<String>,
    pub host: Option<String>,
    pub path: Option<String>,
    pub service_name: Option<String>,
    pub flow: Option<String>,
    pub fingerprint: Option<String>,
    pub reality_public_key: Option<String>,
    pub reality_short_id: Option<String>,
    pub alpn: Vec<String>,
    pub udp: bool,
    pub mux_override: MuxOverride,
    pub source_subscription_id: Option<String>,
    pub config_type: ProfileConfigType,
    pub external_config_format: Option<ExternalConfigFormat>,
    pub external_config_path: Option<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: new_id("profile"),
            name: "新节点".into(),
            core_type: CoreType::SingBox,
            protocol: ProfileProtocol::Vless,
            server: String::new(),
            port: 443,
            uuid: None,
            password: None,
            method: None,
            network: "tcp".into(),
            security: "tls".into(),
            tls: true,
            sni: None,
            host: None,
            path: None,
            service_name: None,
            flow: None,
            fingerprint: Some("chrome".into()),
            reality_public_key: None,
            reality_short_id: None,
            alpn: vec![],
            udp: true,
            mux_override: MuxOverride::FollowGlobal,
            source_subscription_id: None,
            config_type: ProfileConfigType::Native,
            external_config_format: None,
            external_config_path: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub more_urls: Vec<String>,
    pub user_agent: String,
    pub filter: Option<String>,
    pub auto_update_interval_secs: Option<u64>,
    pub convert_core_target: Option<CoreType>,
    pub use_proxy_on_refresh: bool,
    pub last_checked_at: Option<String>,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

impl Default for Subscription {
    fn default() -> Self {
        Self {
            id: new_id("sub"),
            name: "新订阅".into(),
            url: String::new(),
            enabled: true,
            more_urls: vec![],
            user_agent: "v2rayN-tauri".into(),
            filter: None,
            auto_update_interval_secs: None,
            convert_core_target: None,
            use_proxy_on_refresh: true,
            last_checked_at: None,
            last_synced_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProxySettings {
    pub http_port: u16,
    pub socks_port: u16,
    pub mixed_port: u16,
    pub bypass_domains: Vec<String>,
    pub use_system_proxy: bool,
}

impl Default for ProxySettings {
    fn default() -> Self {
        Self {
            http_port: 10809,
            socks_port: 10808,
            mixed_port: 10810,
            bypass_domains: vec!["localhost".into(), "127.0.0.1".into()],
            use_system_proxy: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TunSettings {
    pub enabled: bool,
    pub interface_name: String,
    pub mtu: u16,
    pub auto_route: bool,
    pub strict_route: bool,
    pub stack: String,
}

impl Default for TunSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            interface_name: String::new(),
            mtu: 9000,
            auto_route: true,
            strict_route: false,
            stack: "system".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DnsSettings {
    pub remote_dns: String,
    pub direct_dns: String,
}

impl Default for DnsSettings {
    fn default() -> Self {
        Self {
            remote_dns: "1.1.1.1".into(),
            direct_dns: "223.5.5.5".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RoutingSettings {
    pub mode: String,
    pub domain_strategy: String,
    pub domain_strategy_4_singbox: String,
    pub routing_index_id: Option<String>,
    pub template_source_url: Option<String>,
    pub items: Vec<RoutingItem>,
}

impl Default for RoutingSettings {
    fn default() -> Self {
        Self {
            mode: "rule".into(),
            domain_strategy: "AsIs".into(),
            domain_strategy_4_singbox: String::new(),
            routing_index_id: None,
            template_source_url: None,
            items: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingRuleType {
    #[default]
    All,
    Routing,
    Dns,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RoutingRule {
    pub id: String,
    pub rule_type: RoutingRuleType,
    pub enabled: bool,
    pub remarks: Option<String>,
    pub type_name: Option<String>,
    pub port: Option<String>,
    pub network: Option<String>,
    pub inbound_tag: Vec<String>,
    pub outbound_tag: Option<String>,
    pub ip: Vec<String>,
    pub domain: Vec<String>,
    pub protocol: Vec<String>,
    pub process: Vec<String>,
    pub app_display_name: Option<String>,
    pub app_icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RoutingItem {
    pub id: String,
    pub remarks: String,
    pub url: String,
    pub rule_set: Vec<RoutingRule>,
    pub rule_num: usize,
    pub enabled: bool,
    pub locked: bool,
    pub custom_icon: Option<String>,
    pub custom_ruleset_path_4_singbox: Option<String>,
    pub domain_strategy: Option<String>,
    pub domain_strategy_4_singbox: Option<String>,
    pub sort: usize,
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RoutingTemplate {
    pub version: String,
    pub routing_items: Vec<RoutingItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MuxSettings {
    pub enabled: bool,
    pub xray_concurrency: Option<i32>,
    pub xray_xudp_concurrency: Option<i32>,
    pub xray_xudp_proxy_udp_443: Option<String>,
    pub sing_box_protocol: String,
    pub sing_box_max_connections: u16,
    pub sing_box_padding: Option<bool>,
}

impl Default for MuxSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            xray_concurrency: Some(8),
            xray_xudp_concurrency: Some(16),
            xray_xudp_proxy_udp_443: Some("reject".into()),
            sing_box_protocol: "h2mux".into(),
            sing_box_max_connections: 8,
            sing_box_padding: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClashSettings {
    pub external_controller_port: u16,
    pub enable_ipv6: bool,
    pub allow_lan: bool,
    pub bind_address: String,
    pub rule_mode: String,
    pub secret: Option<String>,
    pub enable_mixin_content: bool,
    pub mixin_content: String,
    pub proxies_sorting: u8,
    pub proxies_auto_refresh: bool,
    pub proxies_auto_delay_test_interval: u16,
    pub proxies_auto_delay_test_url: String,
    pub providers_auto_refresh: bool,
    pub providers_refresh_interval: u16,
    pub connections_auto_refresh: bool,
    pub connections_refresh_interval: u16,
}

impl Default for ClashSettings {
    fn default() -> Self {
        Self {
            external_controller_port: 10813,
            enable_ipv6: false,
            allow_lan: false,
            bind_address: "127.0.0.1".into(),
            rule_mode: "rule".into(),
            secret: None,
            enable_mixin_content: false,
            mixin_content: String::new(),
            proxies_sorting: 0,
            proxies_auto_refresh: false,
            proxies_auto_delay_test_interval: 10,
            proxies_auto_delay_test_url: "https://www.gstatic.com/generate_204".into(),
            providers_auto_refresh: false,
            providers_refresh_interval: 10,
            connections_auto_refresh: false,
            connections_refresh_interval: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub selected_profile_id: Option<String>,
    pub profiles: Vec<Profile>,
    pub subscriptions: Vec<Subscription>,
    pub proxy: ProxySettings,
    pub tun: TunSettings,
    pub dns: DnsSettings,
    pub routing: RoutingSettings,
    pub mux: MuxSettings,
    pub clash: ClashSettings,
}

impl Default for AppConfig {
    fn default() -> Self {
        let profile = Profile::default();
        Self {
            selected_profile_id: Some(profile.id.clone()),
            profiles: vec![profile],
            subscriptions: vec![],
            proxy: ProxySettings::default(),
            tun: TunSettings::default(),
            dns: DnsSettings::default(),
            routing: RoutingSettings::default(),
            mux: MuxSettings::default(),
            clash: ClashSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppPaths {
    pub root: String,
    pub bin: String,
    pub bin_configs: String,
    pub gui_logs: String,
    pub state_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RunningStatus {
    pub running: bool,
    pub core_type: Option<CoreType>,
    pub profile_id: Option<String>,
    pub executable_path: Option<String>,
    pub config_path: Option<String>,
    pub pid: Option<u32>,
    pub elevated: bool,
    pub helper_core_type: Option<CoreType>,
    pub helper_config_path: Option<String>,
    pub helper_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreAssetStatus {
    pub core_type: CoreType,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub download_url: Option<String>,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreLogEvent {
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundTaskEvent {
    pub task: String,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppStatus {
    pub paths: AppPaths,
    pub config: AppConfig,
    pub runtime: RunningStatus,
    pub core_assets: Vec<CoreAssetStatus>,
    pub proxy_probe: Option<ProxyProbe>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProbe {
    pub outbound_ip: String,
    pub country: Option<String>,
    pub city: Option<String>,
    pub isp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClashProxyGroup {
    pub name: String,
    pub proxy_type: String,
    pub now: Option<String>,
    pub all: Vec<String>,
    pub last_delay_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClashConnection {
    pub id: String,
    pub network: Option<String>,
    pub r#type: Option<String>,
    pub rule: Option<String>,
    pub chains: Vec<String>,
    pub upload: Option<u64>,
    pub download: Option<u64>,
    pub host: Option<String>,
    pub destination: Option<String>,
    pub start: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClashProxyProvider {
    pub name: String,
    pub provider_type: String,
    pub vehicle_type: Option<String>,
    pub updated_at: Option<String>,
    pub proxies: Vec<String>,
}

pub fn new_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("{prefix}-{millis}")
}

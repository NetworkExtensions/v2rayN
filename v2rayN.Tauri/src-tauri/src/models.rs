use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoreType {
    Xray,
    SingBox,
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
}

impl Default for ProfileProtocol {
    fn default() -> Self {
        Self::Vless
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
    pub id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub last_synced_at: Option<String>,
}

impl Default for Subscription {
    fn default() -> Self {
        Self {
            id: new_id("sub"),
            name: "新订阅".into(),
            url: String::new(),
            enabled: true,
            last_synced_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
            interface_name: "utun233".into(),
            mtu: 9000,
            auto_route: true,
            strict_route: false,
            stack: "system".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct RoutingSettings {
    pub mode: String,
}

impl Default for RoutingSettings {
    fn default() -> Self {
        Self {
            mode: "rule".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub selected_profile_id: Option<String>,
    pub profiles: Vec<Profile>,
    pub subscriptions: Vec<Subscription>,
    pub proxy: ProxySettings,
    pub tun: TunSettings,
    pub dns: DnsSettings,
    pub routing: RoutingSettings,
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
pub struct AppStatus {
    pub paths: AppPaths,
    pub config: AppConfig,
    pub runtime: RunningStatus,
    pub core_assets: Vec<CoreAssetStatus>,
}

pub fn new_id(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();

    format!("{prefix}-{millis}")
}

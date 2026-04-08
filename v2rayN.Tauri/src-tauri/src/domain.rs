use crate::models::{
    AppConfig, CoreType, DnsSettings, Profile, ProfileProtocol, ProxySettings, RoutingSettings,
    Subscription, TunSettings,
};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

#[derive(Debug, Clone)]
pub struct HelperConfig {
    pub core_type: CoreType,
    pub file_name: String,
    pub config: Value,
}

#[derive(Debug, Clone)]
pub struct RuntimeBundle {
    pub main_core_type: CoreType,
    pub main_file_name: String,
    pub main_config: Value,
    pub helper: Option<HelperConfig>,
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
            profiles.push(profile);
        }
    }

    Ok(profiles)
}

pub fn merge_imported_profiles(config: &mut AppConfig, imported: Vec<Profile>) -> usize {
    let before = config.profiles.len();
    for profile in imported {
        if config
            .profiles
            .iter()
            .any(|existing| existing.server == profile.server && existing.port == profile.port && existing.name == profile.name)
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
    subscription.last_synced_at = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".into()),
    );
}

pub fn generate_runtime_bundle(config: &AppConfig) -> Result<RuntimeBundle> {
    let profile = ensure_profile(config)?;

    if config.tun.enabled && matches!(profile.core_type, CoreType::Xray) {
        let tun_protect_port = portpicker::pick_unused_port().unwrap_or(30901);
        let proxy_relay_port = portpicker::pick_unused_port().unwrap_or(30902);
        let main_config = generate_xray_config(
            profile,
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
            main_file_name: "config.json".into(),
            main_config,
            helper: Some(HelperConfig {
                core_type: CoreType::SingBox,
                file_name: "config-helper.json".into(),
                config: helper_config,
            }),
        });
    }

    let main_config = match profile.core_type {
        CoreType::SingBox => generate_sing_box_config(profile, &config.proxy, &config.tun, &config.dns, &config.routing)?,
        CoreType::Xray => generate_xray_config(profile, &config.proxy, &config.tun, &config.dns, &config.routing, None)?,
    };

    Ok(RuntimeBundle {
        main_core_type: profile.core_type.clone(),
        main_file_name: "config.json".into(),
        main_config,
        helper: None,
    })
}

pub fn generate_preview(config: &AppConfig) -> Result<Value> {
    let bundle = generate_runtime_bundle(config)?;
    let mut preview = json!({
        "main_core_type": bundle.main_core_type,
        "main_file_name": bundle.main_file_name,
        "main_config": bundle.main_config,
    });

    if let Some(helper) = bundle.helper {
        preview["helper"] = json!({
            "core_type": helper.core_type,
            "file_name": helper.file_name,
            "config": helper.config,
        });
    }

    Ok(preview)
}

fn generate_sing_box_config(
    profile: &Profile,
    proxy: &ProxySettings,
    tun: &TunSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
) -> Result<Value> {
    let outbound = build_singbox_outbound(profile);

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
        "rules": singbox_route_rules(tun),
        "default_domain_resolver": { "server": "bootstrap" }
    });
    route["rule_set"] = Value::Array(singbox_rule_set_entries(ALL_RULESET_TAGS));

    Ok(json!({
        "log": {
            "level": "warn",
            "timestamp": true,
        },
        "dns": singbox_dns_block(dns),
        "inbounds": inbounds,
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" },
            { "type": "block", "tag": "block" }
        ],
        "route": route
    }))
}

fn build_singbox_outbound(profile: &Profile) -> Value {
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

    ob
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

    json!({
        "log": {
            "level": "warn",
            "timestamp": true
        },
        "dns": singbox_dns_block(dns),
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
            "rule_set": singbox_rule_set_entries(ALL_RULESET_TAGS)
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

fn singbox_dns_block(dns: &DnsSettings) -> Value {
    let mut bootstrap = parse_dns_server_new_format(BOOTSTRAP_DNS_ADDRESS);
    bootstrap["tag"] = json!("bootstrap");

    let mut remote = parse_dns_server_new_format(&dns.remote_dns);
    remote["tag"] = json!("remote");
    remote["detour"] = json!("proxy");
    remote["domain_resolver"] = json!("bootstrap");

    let mut local = parse_dns_server_new_format(&dns.direct_dns);
    local["tag"] = json!("local");
    local["domain_resolver"] = json!("bootstrap");

    json!({
        "servers": [bootstrap, remote, local],
        "rules": [
            {
                "rule_set": ["geosite-google"],
                "server": "remote",
                "strategy": "prefer_ipv4"
            },
            {
                "rule_set": ["geosite-cn"],
                "server": "local",
                "strategy": "prefer_ipv4"
            }
        ],
        "final": "remote",
        "independent_cache": true,
        "strategy": "prefer_ipv4"
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

const ALL_RULESET_TAGS: &[&str] = &["geosite-google", "geosite-cn", "geoip-cn"];

const SINGBOX_RULESET_URL: &str =
    "https://raw.githubusercontent.com/2dust/sing-box-rules/rule-set-{kind}/{tag}.srs";

fn singbox_rule_set_entries(tags: &[&str]) -> Vec<Value> {
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

fn singbox_route_rules(tun: &TunSettings) -> Value {
    let mut rules: Vec<Value> = Vec::new();

    if tun.enabled {
        rules.push(json!({ "network": "udp", "port": [135, 137, 138, 139, 5353], "action": "reject" }));
        rules.push(json!({ "ip_cidr": ["224.0.0.0/3", "ff00::/8"], "action": "reject" }));
    }

    rules.push(json!({ "action": "sniff" }));
    rules.push(json!({ "protocol": ["dns"], "action": "hijack-dns" }));

    rules.push(json!({ "rule_set": ["geosite-cn"], "outbound": "direct" }));
    rules.push(json!({ "rule_set": ["geoip-cn"], "outbound": "direct" }));

    Value::Array(rules)
}

fn generate_xray_config(
    profile: &Profile,
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

    let mut rules = vec![
        json!({
            "type": "field",
            "port": "53",
            "outboundTag": "direct"
        }),
        json!({
            "type": "field",
            "network": "tcp,udp",
            "outboundTag": if routing.mode == "direct" { "direct" } else { "proxy" }
        }),
    ];

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
            "domainStrategy": "IPIfNonMatch",
            "rules": rules
        }
    }))
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

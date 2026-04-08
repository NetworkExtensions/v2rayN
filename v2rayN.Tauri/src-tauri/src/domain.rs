use crate::models::{
    AppConfig, CoreType, DnsSettings, Profile, ProfileProtocol, ProxySettings, RoutingSettings,
    Subscription, TunSettings,
};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashSet;
use url::Url;

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

    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let mut profile = if line.starts_with("vless://") {
            parse_vless(line)?
        } else if line.starts_with("trojan://") {
            parse_trojan(line)?
        } else if line.starts_with("ss://") {
            parse_shadowsocks(line)?
        } else if line.starts_with("vmess://") {
            parse_vmess(line)?
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

pub fn generate_core_config(config: &AppConfig) -> Result<Value> {
    let profile = ensure_profile(config)?;

    match profile.core_type {
        CoreType::SingBox => generate_sing_box_config(profile, &config.proxy, &config.tun, &config.dns, &config.routing),
        CoreType::Xray => generate_xray_config(profile, &config.proxy, &config.tun, &config.dns, &config.routing),
    }
}

fn generate_sing_box_config(
    profile: &Profile,
    proxy: &ProxySettings,
    tun: &TunSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
) -> Result<Value> {
    let outbound = match profile.protocol {
        ProfileProtocol::Vless => json!({
            "type": "vless",
            "tag": "proxy",
            "server": profile.server,
            "server_port": profile.port,
            "uuid": profile.uuid.clone().unwrap_or_default(),
            "flow": profile.flow.clone().unwrap_or_default(),
            "tls": tls_object(profile),
            "transport": transport_object(profile),
        }),
        ProfileProtocol::Vmess => json!({
            "type": "vmess",
            "tag": "proxy",
            "server": profile.server,
            "server_port": profile.port,
            "uuid": profile.uuid.clone().unwrap_or_default(),
            "security": "auto",
            "tls": tls_object(profile),
            "transport": transport_object(profile),
        }),
        ProfileProtocol::Trojan => json!({
            "type": "trojan",
            "tag": "proxy",
            "server": profile.server,
            "server_port": profile.port,
            "password": profile.password.clone().unwrap_or_default(),
            "tls": tls_object(profile),
            "transport": transport_object(profile),
        }),
        ProfileProtocol::Shadowsocks => json!({
            "type": "shadowsocks",
            "tag": "proxy",
            "server": profile.server,
            "server_port": profile.port,
            "method": profile.method.clone().unwrap_or_else(|| "aes-128-gcm".into()),
            "password": profile.password.clone().unwrap_or_default(),
        }),
    };

    let mut inbounds = vec![
        json!({
            "type": "socks",
            "tag": "socks-in",
            "listen": "127.0.0.1",
            "listen_port": proxy.socks_port,
            "sniff": true,
            "sniff_override_destination": true,
        }),
        json!({
            "type": "http",
            "tag": "http-in",
            "listen": "127.0.0.1",
            "listen_port": proxy.http_port,
            "sniff": true,
        }),
        json!({
            "type": "mixed",
            "tag": "mixed-in",
            "listen": "127.0.0.1",
            "listen_port": proxy.mixed_port,
            "sniff": true,
        }),
    ];

    if tun.enabled {
        inbounds.push(json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": tun.interface_name,
            "mtu": tun.mtu,
            "auto_route": tun.auto_route,
            "strict_route": tun.strict_route,
            "stack": tun.stack,
        }));
    }

    Ok(json!({
        "log": {
            "level": "info",
            "timestamp": true,
        },
        "dns": {
            "servers": [
                { "tag": "remote", "address": dns.remote_dns, "detour": "proxy" },
                { "tag": "direct", "address": dns.direct_dns, "detour": "direct" }
            ],
            "final": "remote",
        },
        "inbounds": inbounds,
        "outbounds": [
            outbound,
            { "type": "direct", "tag": "direct" },
            { "type": "block", "tag": "block" }
        ],
        "route": {
            "auto_detect_interface": true,
            "final": if routing.mode == "direct" { "direct" } else { "proxy" },
            "rules": [
                {
                    "protocol": "dns",
                    "outbound": "direct"
                }
            ]
        }
    }))
}

fn generate_xray_config(
    profile: &Profile,
    proxy: &ProxySettings,
    tun: &TunSettings,
    dns: &DnsSettings,
    routing: &RoutingSettings,
) -> Result<Value> {
    if tun.enabled {
        return Err(anyhow!("当前实现暂未在 Xray 路径上启用 TUN，请切换到 sing-box"));
    }

    let outbound = match profile.protocol {
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
    };

    Ok(json!({
        "log": {
            "loglevel": "info"
        },
        "dns": {
            "servers": [dns.remote_dns, dns.direct_dns]
        },
        "inbounds": [
            {
                "tag": "socks-in",
                "listen": "127.0.0.1",
                "port": proxy.socks_port,
                "protocol": "socks",
                "settings": { "udp": true },
                "sniffing": { "enabled": true, "destOverride": ["http", "tls", "quic"] }
            },
            {
                "tag": "http-in",
                "listen": "127.0.0.1",
                "port": proxy.http_port,
                "protocol": "http",
                "settings": {},
                "sniffing": { "enabled": true, "destOverride": ["http", "tls"] }
            }
        ],
        "outbounds": [
            outbound,
            { "tag": "direct", "protocol": "freedom", "settings": {} },
            { "tag": "block", "protocol": "blackhole", "settings": {} }
        ],
        "routing": {
            "domainStrategy": "IPIfNonMatch",
            "rules": [
                {
                    "type": "field",
                    "port": "53",
                    "outboundTag": "direct"
                },
                {
                    "type": "field",
                    "network": "tcp,udp",
                    "outboundTag": if routing.mode == "direct" { "direct" } else { "proxy" }
                }
            ]
        }
    }))
}

fn tls_object(profile: &Profile) -> Value {
    if !profile.tls && profile.security != "reality" {
        return json!({"enabled": false});
    }

    if profile.security == "reality" {
        return json!({
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
        });
    }

    json!({
        "enabled": profile.tls,
        "server_name": profile.sni.clone().unwrap_or_else(|| profile.server.clone()),
        "alpn": profile.alpn,
        "utls": {
            "enabled": profile.fingerprint.is_some(),
            "fingerprint": profile.fingerprint.clone().unwrap_or_else(|| "chrome".into())
        }
    })
}

fn transport_object(profile: &Profile) -> Value {
    match profile.network.as_str() {
        "ws" => json!({
            "type": "ws",
            "path": profile.path.clone().unwrap_or_else(|| "/".into()),
            "headers": {
                "Host": profile.host.clone().unwrap_or_default()
            }
        }),
        "grpc" => json!({
            "type": "grpc",
            "service_name": profile.service_name.clone().unwrap_or_default(),
        }),
        _ => json!({ "type": "tcp" }),
    }
}

fn xray_stream_settings(profile: &Profile) -> Value {
    let mut stream = json!({
        "network": profile.network,
    });

    if profile.tls || profile.security == "reality" {
        let security = if profile.security == "reality" { "reality" } else { "tls" };
        stream["security"] = Value::String(security.into());
    }

    if profile.network == "ws" {
        stream["wsSettings"] = json!({
            "path": profile.path.clone().unwrap_or_else(|| "/".into()),
            "headers": {
                "Host": profile.host.clone().unwrap_or_default()
            }
        });
    }

    if profile.network == "grpc" {
        stream["grpcSettings"] = json!({
            "serviceName": profile.service_name.clone().unwrap_or_default(),
            "multiMode": false
        });
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
        network: query_value(&url, "type").unwrap_or_else(|| "tcp".into()),
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
        network: query_value(&url, "type").unwrap_or_else(|| "tcp".into()),
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

    Ok(Profile {
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
    })
}

fn parse_vmess(raw: &str) -> Result<Profile> {
    let encoded = raw.trim_start_matches("vmess://");
    let decoded = decode_base64(encoded)?;
    let payload: Value = serde_json::from_str(&decoded).context("无法解析 VMess JSON")?;

    Ok(Profile {
        id: crate::models::new_id("profile"),
        name: payload
            .get("ps")
            .and_then(Value::as_str)
            .unwrap_or("VMess")
            .to_string(),
        protocol: ProfileProtocol::Vmess,
        server: payload
            .get("add")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        port: payload
            .get("port")
            .and_then(Value::as_str)
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(443),
        uuid: payload.get("id").and_then(Value::as_str).map(str::to_string),
        network: payload.get("net").and_then(Value::as_str).unwrap_or("tcp").to_string(),
        security: payload.get("tls").and_then(Value::as_str).unwrap_or("none").to_string(),
        tls: payload.get("tls").and_then(Value::as_str).map(|value| value == "tls").unwrap_or(false),
        sni: payload.get("sni").and_then(Value::as_str).map(str::to_string),
        host: payload.get("host").and_then(Value::as_str).map(str::to_string),
        path: payload.get("path").and_then(Value::as_str).map(str::to_string),
        service_name: payload.get("path").and_then(Value::as_str).map(str::to_string),
        ..Profile::default()
    })
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find_map(|(query_key, value)| (query_key == key).then(|| value.to_string()))
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

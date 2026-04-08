use crate::models::ProxyProbe;
use anyhow::{Context, Result};
use reqwest::blocking::{Client, ClientBuilder};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct IpSbGeoip {
    ip: Option<String>,
    country: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    organization: Option<String>,
}

pub fn probe_proxy(socks_port: u16) -> Result<ProxyProbe> {
    let proxy_url = format!("socks5h://127.0.0.1:{socks_port}");
    let client = ClientBuilder::new()
        .proxy(reqwest::Proxy::all(proxy_url)?)
        .timeout(Duration::from_secs(10))
        .user_agent("v2rayN-tauri")
        .build()
        .context("创建代理探测客户端失败")?;

    fetch_probe(client)
}

pub fn probe_direct() -> Result<ProxyProbe> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("v2rayN-tauri")
        .build()
        .context("创建直连探测客户端失败")?;

    fetch_probe(client)
}

fn fetch_probe(client: Client) -> Result<ProxyProbe> {
    let response = client
        .get("https://api.ip.sb/geoip")
        .send()?
        .error_for_status()?
        .json::<IpSbGeoip>()?;

    Ok(ProxyProbe {
        outbound_ip: response.ip.unwrap_or_else(|| "unknown".into()),
        country: response.country,
        city: response.city,
        isp: response.isp.or(response.organization),
    })
}

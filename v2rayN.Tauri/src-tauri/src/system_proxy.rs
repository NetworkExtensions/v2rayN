use anyhow::{anyhow, Context, Result};
use log;
use std::process::Command;

/// Set system proxy for all network services using a single mixed port,
/// matching the original v2rayN `proxy_set_osx_sh` behavior.
pub fn set_macos_proxy(host: &str, port: u16, bypass_domains: &[String]) -> Result<()> {
    let services = network_services()?;
    let port_str = port.to_string();
    for service in &services {
        run_networksetup(&["-setwebproxy", service, host, &port_str])?;
        run_networksetup(&["-setsecurewebproxy", service, host, &port_str])?;
        run_networksetup(&["-setsocksfirewallproxy", service, host, &port_str])?;

        let mut args = vec!["-setproxybypassdomains", service.as_str()];
        let refs: Vec<&str> = bypass_domains.iter().map(|s| s.as_str()).collect();
        args.extend(refs);
        run_networksetup(&args)?;
        log::info!("已为 '{service}' 设置代理 {host}:{port}");
    }
    Ok(())
}

pub fn clear_macos_proxy() -> Result<()> {
    let services = network_services()?;
    for service in &services {
        run_networksetup(&["-setwebproxystate", service, "off"])?;
        run_networksetup(&["-setsecurewebproxystate", service, "off"])?;
        run_networksetup(&["-setsocksfirewallproxystate", service, "off"])?;
        log::info!("已为 '{service}' 清除代理");
    }
    Ok(())
}

fn network_services() -> Result<Vec<String>> {
    let output = Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
        .context("读取 macOS 网络服务失败")?;
    if !output.status.success() {
        return Err(anyhow!("networksetup -listallnetworkservices 执行失败"));
    }

    let services: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.contains('*'))
        .map(str::to_string)
        .collect();

    log::debug!("检测到网络服务: {:?}", services);
    Ok(services)
}

fn run_networksetup(args: &[&str]) -> Result<()> {
    let output = Command::new("networksetup")
        .args(args)
        .output()
        .with_context(|| format!("执行 networksetup {:?} 失败", args))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "networksetup {} 失败 (exit {}): {}",
            args.join(" "),
            output.status.code().unwrap_or(-1),
            stderr.trim()
        ))
    }
}

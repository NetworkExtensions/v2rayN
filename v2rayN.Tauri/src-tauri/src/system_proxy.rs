use anyhow::{anyhow, Context, Result};
use std::process::Command;

pub fn set_macos_proxy(host: &str, http_port: u16, socks_port: u16, bypass_domains: &[String]) -> Result<()> {
    let services = network_services()?;
    for service in services {
        run_networksetup(["-setwebproxy", &service, host, &http_port.to_string()])?;
        run_networksetup(["-setsecurewebproxy", &service, host, &http_port.to_string()])?;
        run_networksetup(["-setsocksfirewallproxy", &service, host, &socks_port.to_string()])?;

        let mut args = vec!["-setproxybypassdomains".to_string(), service.clone()];
        args.extend(bypass_domains.iter().cloned());
        run_networksetup_owned(args)?;
    }
    Ok(())
}

pub fn clear_macos_proxy() -> Result<()> {
    let services = network_services()?;
    for service in services {
        run_networksetup(["-setwebproxystate", &service, "off"])?;
        run_networksetup(["-setsecurewebproxystate", &service, "off"])?;
        run_networksetup(["-setsocksfirewallproxystate", &service, "off"])?;
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

    let services = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.starts_with('*'))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    Ok(services)
}

fn run_networksetup(args: impl IntoIterator<Item = impl AsRef<str>>) -> Result<()> {
    let status = Command::new("networksetup")
        .args(args.into_iter().map(|arg| arg.as_ref().to_string()))
        .status()
        .context("执行 networksetup 失败")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("networksetup 参数执行失败"))
    }
}

fn run_networksetup_owned(args: Vec<String>) -> Result<()> {
    run_networksetup(args)
}

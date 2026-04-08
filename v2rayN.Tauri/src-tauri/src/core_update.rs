use crate::models::{CoreAssetStatus, CoreType};
use anyhow::{anyhow, Context, Result};
use flate2::read::GzDecoder;
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use std::{
    fs::{self, File},
    io::{self, Cursor},
    path::{Path, PathBuf},
    process::Command,
};
use tar::Archive;
use tauri::AppHandle;
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct CorePaths {
    pub bin_root: PathBuf,
}

impl CorePaths {
    pub fn executable_dir(&self, core_type: &CoreType) -> PathBuf {
        self.bin_root.join(core_type.key())
    }
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

pub fn list_core_statuses(app: &AppHandle, core_paths: &CorePaths) -> Result<Vec<CoreAssetStatus>> {
    let _ = app;
    [CoreType::Xray, CoreType::SingBox, CoreType::Mihomo]
        .iter()
        .map(|core_type| core_status(core_paths, core_type.clone()))
        .collect()
}

pub fn download_core(app: &AppHandle, core_paths: &CorePaths, core_type: CoreType) -> Result<CoreAssetStatus> {
    let _ = app;
    let release = fetch_release(&core_type)?;
    let asset = select_asset(&core_type, &release.assets)?;
    let directory = core_paths.executable_dir(&core_type);
    fs::create_dir_all(&directory)?;
    install_asset(&asset.browser_download_url, &directory, &core_type)?;

    core_status(core_paths, core_type).map(|mut status| {
        status.latest_version = Some(release.tag_name);
        status.download_url = Some(asset.browser_download_url);
        status
    })
}

fn core_status(core_paths: &CorePaths, core_type: CoreType) -> Result<CoreAssetStatus> {
    let executable_path = resolve_executable(core_paths, &core_type);
    let installed_version = executable_path
        .as_ref()
        .and_then(|path| get_installed_version(path, &core_type).ok());
    let latest_release = fetch_release(&core_type).ok();
    let download_url = latest_release
        .as_ref()
        .and_then(|release| select_asset(&core_type, &release.assets).ok())
        .map(|asset| asset.browser_download_url);

    Ok(CoreAssetStatus {
        core_type,
        installed_version,
        latest_version: latest_release.map(|release| release.tag_name),
        download_url,
        executable_path: executable_path.map(|path| path.to_string_lossy().to_string()),
    })
}

fn fetch_release(core_type: &CoreType) -> Result<GithubRelease> {
    let client = github_client()?;
    let repo = match core_type {
        CoreType::Xray => "XTLS/Xray-core",
        CoreType::SingBox => "SagerNet/sing-box",
        CoreType::Mihomo => "MetaCubeX/mihomo",
    };
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let response = client.get(url).send()?.error_for_status()?;
    Ok(response.json()?)
}

fn github_client() -> Result<Client> {
    Client::builder()
        .user_agent("v2rayN-tauri")
        .build()
        .context("创建 GitHub 客户端失败")
}

fn select_asset(core_type: &CoreType, assets: &[GithubAsset]) -> Result<GithubAsset> {
    assets
        .iter()
        .find(|asset| match core_type {
            CoreType::Xray => asset.name == "Xray-macos-arm64-v8a.zip",
            CoreType::SingBox => asset.name.contains("-darwin-arm64.tar.gz"),
            CoreType::Mihomo => asset.name.contains("mihomo-darwin-arm64") && asset.name.ends_with(".gz"),
        })
        .cloned()
        .context("未找到当前平台可用的核心安装包")
}

fn install_asset(url: &str, target_dir: &Path, core_type: &CoreType) -> Result<()> {
    let client = github_client()?;
    let bytes = client.get(url).send()?.error_for_status()?.bytes()?;

    if url.ends_with(".zip") {
        let mut archive = ZipArchive::new(Cursor::new(bytes))?;
        for index in 0..archive.len() {
            let mut file = archive.by_index(index)?;
            if file.is_dir() {
                continue;
            }
            let output_path = target_dir.join(Path::new(file.name()).file_name().unwrap_or_default());
            let mut output = File::create(&output_path)?;
            io::copy(&mut file, &mut output)?;
            set_executable(&output_path)?;
        }
        return Ok(());
    }

    if url.ends_with(".tar.gz") {
        let reader = Cursor::new(bytes);
        let gz = GzDecoder::new(reader);
        let mut archive = Archive::new(gz);
        archive.unpack(target_dir)?;
        flatten_single_directory(target_dir)?;
        let executable = resolve_executable_path(target_dir, core_type);
        if let Some(path) = executable {
            set_executable(&path)?;
        }
        return Ok(());
    }

    if url.ends_with(".gz") {
        let reader = Cursor::new(bytes);
        let mut gz = GzDecoder::new(reader);
        let asset_name = url
            .rsplit('/')
            .next()
            .context("无法解析安装包文件名")?;
        let output_name = asset_name.strip_suffix(".gz").unwrap_or(asset_name);
        let output_path = target_dir.join(output_name);
        let mut output = File::create(&output_path)?;
        io::copy(&mut gz, &mut output)?;
        set_executable(&output_path)?;
        return Ok(());
    }

    Err(anyhow!("暂不支持该安装包格式"))
}

fn flatten_single_directory(target_dir: &Path) -> Result<()> {
    let entries = fs::read_dir(target_dir)?
        .flatten()
        .map(|entry| entry.path())
        .collect::<Vec<_>>();

    if entries.len() != 1 || !entries[0].is_dir() {
        return Ok(());
    }

    let inner = entries[0].clone();
    for entry in fs::read_dir(&inner)?.flatten() {
        let target = target_dir.join(entry.file_name());
        fs::rename(entry.path(), target)?;
    }
    fs::remove_dir_all(inner)?;
    Ok(())
}

pub fn resolve_executable(core_paths: &CorePaths, core_type: &CoreType) -> Option<PathBuf> {
    resolve_executable_path(&core_paths.executable_dir(core_type), core_type)
        .or_else(|| resolve_executable_path(&core_paths.bin_root, core_type))
}

fn resolve_executable_path(directory: &Path, core_type: &CoreType) -> Option<PathBuf> {
    let names = match core_type {
        CoreType::Xray => vec!["xray"],
        CoreType::SingBox => vec!["sing-box", "sing-box-client"],
        CoreType::Mihomo => vec!["mihomo", "clash", "mihomo-darwin-arm64", "mihomo-darwin-amd64-v1"],
    };

    let exact = names
        .into_iter()
        .map(|name| directory.join(name))
        .find(|path| path.exists());
    if exact.is_some() {
        return exact;
    }

    if matches!(core_type, CoreType::Mihomo) {
        return fs::read_dir(directory)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name == "mihomo" || name == "clash" || name.starts_with("mihomo-"))
                    .unwrap_or(false)
            });
    }

    None
}

fn get_installed_version(path: &Path, core_type: &CoreType) -> Result<String> {
    let output = match core_type {
        CoreType::Xray => Command::new(path).arg("-version").output()?,
        CoreType::SingBox => Command::new(path).arg("version").output()?,
        CoreType::Mihomo => Command::new(path).arg("-v").output()?,
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout
        .split_whitespace()
        .find(|part| Version::parse(part.trim_start_matches('v')).is_ok())
        .map(str::to_string)
        .context("无法解析核心版本")?;
    Ok(version)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let metadata = fs::metadata(path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

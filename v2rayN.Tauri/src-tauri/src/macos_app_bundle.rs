use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize)]
pub struct AppBundleInfo {
    pub display_name: String,
    pub bundle_identifier: String,
    pub bundle_path: String,
    pub process_names: Vec<String>,
    pub icon_base64: Option<String>,
}

pub fn resolve_app_bundle(path: &str) -> Result<AppBundleInfo> {
    let bundle_path = PathBuf::from(path);
    anyhow::ensure!(
        bundle_path.extension().and_then(|e| e.to_str()) == Some("app"),
        "路径不是 .app 包：{path}"
    );
    anyhow::ensure!(bundle_path.exists(), "路径不存在：{path}");

    let contents = bundle_path.join("Contents");
    let plist_path = contents.join("Info.plist");
    anyhow::ensure!(plist_path.exists(), "找不到 Info.plist：{}", plist_path.display());

    let display_name = read_plist_display_name(&plist_path, &bundle_path)?;
    let bundle_identifier = read_plist_key(&plist_path, "CFBundleIdentifier")
        .unwrap_or_default();

    let mut process_names = BTreeSet::new();
    if let Ok(exe) = read_plist_key(&plist_path, "CFBundleExecutable") {
        if !exe.is_empty() {
            process_names.insert(exe);
        }
    }

    collect_helper_executables(&contents, &mut process_names);

    let icon_base64 = extract_icon_base64(&plist_path, &contents);

    Ok(AppBundleInfo {
        display_name,
        bundle_identifier,
        bundle_path: path.to_string(),
        process_names: process_names.into_iter().collect(),
        icon_base64,
    })
}

fn read_plist_display_name(plist_path: &Path, bundle_path: &Path) -> Result<String> {
    if let Ok(name) = read_plist_key(plist_path, "CFBundleDisplayName") {
        if !name.is_empty() {
            return Ok(name);
        }
    }
    if let Ok(name) = read_plist_key(plist_path, "CFBundleName") {
        if !name.is_empty() {
            return Ok(name);
        }
    }
    let stem = bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();
    Ok(stem)
}

fn read_plist_key(plist_path: &Path, key: &str) -> Result<String> {
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}"), &plist_path.to_string_lossy()])
        .output()
        .with_context(|| format!("执行 PlistBuddy 失败: {key}"))?;

    if !output.status.success() {
        anyhow::bail!("PlistBuddy 读取 {key} 失败");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn collect_helper_executables(contents: &Path, names: &mut BTreeSet<String>) {
    let frameworks = contents.join("Frameworks");
    if !frameworks.exists() {
        return;
    }
    if let Ok(entries) = std::fs::read_dir(&frameworks) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                let helper_plist = path.join("Contents/Info.plist");
                if let Ok(exe) = read_plist_key(&helper_plist, "CFBundleExecutable") {
                    if !exe.is_empty() {
                        names.insert(exe);
                    }
                }
            }
        }
    }

    let helpers = contents.join("Helpers");
    if helpers.exists() {
        if let Ok(entries) = std::fs::read_dir(&helpers) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("app") {
                    let helper_plist = path.join("Contents/Info.plist");
                    if let Ok(exe) = read_plist_key(&helper_plist, "CFBundleExecutable") {
                        if !exe.is_empty() {
                            names.insert(exe);
                        }
                    }
                }
            }
        }
    }
}

fn extract_icon_base64(plist_path: &Path, contents: &Path) -> Option<String> {
    let icon_file = read_plist_key(plist_path, "CFBundleIconFile").ok()?;
    if icon_file.is_empty() {
        return None;
    }

    let icon_name = if icon_file.ends_with(".icns") {
        icon_file
    } else {
        format!("{icon_file}.icns")
    };

    let icns_path = contents.join("Resources").join(&icon_name);
    if !icns_path.exists() {
        return None;
    }

    let png_bytes = convert_icns_to_png(&icns_path)?;
    Some(format!("data:image/png;base64,{}", BASE64.encode(&png_bytes)))
}

pub fn list_applications() -> Result<Vec<AppBundleInfo>> {
    let apps_dir = PathBuf::from("/Applications");
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                if let Ok(info) = resolve_app_bundle(&path.to_string_lossy()) {
                    results.push(info);
                }
            }
        }
    }
    results.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    Ok(results)
}

fn convert_icns_to_png(icns_path: &Path) -> Option<Vec<u8>> {
    let output = Command::new("sips")
        .args([
            "-s", "format", "png",
            "-Z", "64",
            &icns_path.to_string_lossy(),
            "--out", "/dev/stdout",
        ])
        .output()
        .ok()?;

    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
}

use crate::models::{AppConfig, AppPaths};
use anyhow::{Context, Result};
use std::{fs, path::{Path, PathBuf}};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone)]
pub struct ConfigStore {
    paths: AppPaths,
}

impl ConfigStore {
    pub fn bootstrap(app: &AppHandle) -> Result<Self> {
        let base = app
            .path()
            .app_local_data_dir()
            .context("无法解析应用数据目录")?;

        let paths = AppPaths {
            root: base.to_string_lossy().to_string(),
            bin: base.join("bin").to_string_lossy().to_string(),
            bin_configs: base.join("binConfigs").to_string_lossy().to_string(),
            gui_logs: base.join("guiLogs").to_string_lossy().to_string(),
            state_file: base.join("app-state.json").to_string_lossy().to_string(),
        };

        for directory in [&paths.root, &paths.bin, &paths.bin_configs, &paths.gui_logs] {
            fs::create_dir_all(directory)
                .with_context(|| format!("创建目录失败: {directory}"))?;
        }

        let store = Self { paths };
        migrate_legacy_binaries(&store.paths)?;
        if !PathBuf::from(&store.paths.state_file).exists() {
            store.save(&AppConfig::default())?;
        }

        Ok(store)
    }

    pub fn paths(&self) -> AppPaths {
        self.paths.clone()
    }

    pub fn save(&self, config: &AppConfig) -> Result<()> {
        let serialized = serde_json::to_string_pretty(config)?;
        fs::write(&self.paths.state_file, serialized)
            .with_context(|| format!("写入状态文件失败: {}", self.paths.state_file))?;
        Ok(())
    }

    pub fn load(&self) -> Result<AppConfig> {
        let raw = fs::read_to_string(&self.paths.state_file)
            .with_context(|| format!("读取状态文件失败: {}", self.paths.state_file))?;
        let mut config = serde_json::from_str::<AppConfig>(&raw).unwrap_or_default();
        normalize_config(&mut config);
        Ok(config)
    }
}

fn migrate_legacy_binaries(paths: &AppPaths) -> Result<()> {
    let current_bin = PathBuf::from(&paths.bin);
    copy_root_bins_into_core_dirs(&current_bin)?;

    let Some(root_parent) = Path::new(&paths.root).parent() else {
        return Ok(());
    };

    for legacy_dir_name in ["com.tauri.dev", "com.dywang.v2rayn.tauri"] {
        let legacy_root = root_parent.join(legacy_dir_name);
        let legacy_bin = legacy_root.join("bin");
        if !legacy_bin.exists() || legacy_bin == current_bin {
            continue;
        }
        copy_binary_if_needed(&legacy_bin.join("xray"), &current_bin.join("xray").join("xray"))?;
        copy_binary_if_needed(&legacy_bin.join("sing-box"), &current_bin.join("sing_box").join("sing-box"))?;
        copy_binary_if_needed(&legacy_bin.join("sing-box-client"), &current_bin.join("sing_box").join("sing-box-client"))?;
    }

    Ok(())
}

fn copy_root_bins_into_core_dirs(bin_root: &Path) -> Result<()> {
    copy_binary_if_needed(&bin_root.join("xray"), &bin_root.join("xray").join("xray"))?;
    copy_binary_if_needed(&bin_root.join("sing-box"), &bin_root.join("sing_box").join("sing-box"))?;
    copy_binary_if_needed(
        &bin_root.join("sing-box-client"),
        &bin_root.join("sing_box").join("sing-box-client"),
    )?;
    Ok(())
}

fn copy_binary_if_needed(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() || destination.exists() {
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建核心目录失败: {}", parent.display()))?;
    }

    fs::copy(source, destination).with_context(|| {
        format!(
            "复制旧核心失败: {} -> {}",
            source.display(),
            destination.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(destination)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(destination, permissions)?;
    }

    Ok(())
}

fn normalize_config(config: &mut AppConfig) {
    if config.profiles.is_empty() {
        let profile = crate::models::Profile::default();
        config.selected_profile_id = Some(profile.id.clone());
        config.profiles.push(profile);
    }

    if config.selected_profile_id.is_none() {
        config.selected_profile_id = config.profiles.first().map(|profile| profile.id.clone());
    }
}

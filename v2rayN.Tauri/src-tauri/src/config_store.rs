use crate::models::{AppConfig, AppPaths};
use anyhow::{Context, Result};
use std::{fs, path::PathBuf};
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

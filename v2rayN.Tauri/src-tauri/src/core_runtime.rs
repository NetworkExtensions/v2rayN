use crate::{
    config_store::ConfigStore,
    core_update::{resolve_executable, CorePaths},
    domain::generate_core_config,
    models::{CoreLogEvent, CoreType, RunningStatus},
};
use anyhow::{anyhow, Context, Result};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
};
use tauri::{AppHandle, Emitter};

#[derive(Debug)]
pub struct RuntimeManager {
    process: Mutex<Option<ManagedProcess>>,
}

#[derive(Debug)]
struct ManagedProcess {
    child: Child,
    status: RunningStatus,
}

impl RuntimeManager {
    pub fn new() -> Self {
        Self {
            process: Mutex::new(None),
        }
    }

    pub fn status(&self) -> RunningStatus {
        self.process
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|managed| managed.status.clone()))
            .unwrap_or_default()
    }

    pub fn stop(&self) -> Result<RunningStatus> {
        let mut guard = self.process.lock().map_err(|_| anyhow!("运行时状态不可用"))?;
        if let Some(process) = guard.as_mut() {
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
        *guard = None;
        Ok(RunningStatus::default())
    }

    pub fn start(
        &self,
        app: &AppHandle,
        store: &ConfigStore,
        core_paths: &CorePaths,
    ) -> Result<RunningStatus> {
        self.stop()?;

        let config = store.load()?;
        let selected_profile = crate::domain::ensure_profile(&config)?;
        let executable = resolve_executable(core_paths, &selected_profile.core_type)
            .context("未找到已安装的核心，请先下载核心")?;
        let generated = generate_core_config(&config)?;
        let config_path = PathBuf::from(store.paths().bin_configs).join("config.json");
        fs::write(&config_path, serde_json::to_vec_pretty(&generated)?)?;

        let mut command = Command::new(&executable);
        match selected_profile.core_type {
            CoreType::Xray => {
                command.arg("run").arg("-c").arg(&config_path);
                command.env("XRAY_LOCATION_ASSET", &store.paths().bin);
            }
            CoreType::SingBox => {
                command
                    .arg("run")
                    .arg("-c")
                    .arg(&config_path)
                    .arg("--disable-color");
            }
        }

        command
            .current_dir(&store.paths().bin_configs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().context("启动核心失败")?;
        bind_log_stream(app, child.stdout.take(), "stdout");
        bind_log_stream(app, child.stderr.take(), "stderr");

        let status = RunningStatus {
            running: true,
            core_type: Some(selected_profile.core_type.clone()),
            profile_id: Some(selected_profile.id.clone()),
            executable_path: Some(executable.to_string_lossy().to_string()),
            config_path: Some(config_path.to_string_lossy().to_string()),
        };

        let mut guard = self.process.lock().map_err(|_| anyhow!("运行时状态不可用"))?;
        *guard = Some(ManagedProcess {
            child,
            status: status.clone(),
        });

        Ok(status)
    }
}

fn bind_log_stream(app: &AppHandle, stream: Option<impl std::io::Read + Send + 'static>, source: &'static str) {
    let Some(stream) = stream else {
        return;
    };

    let app = app.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(|line| line.ok()) {
            let _ = app.emit(
                "core-log",
                CoreLogEvent {
                    level: if source == "stderr" { "error".into() } else { "info".into() },
                    source: source.into(),
                    message: line,
                },
            );
        }
    });
}

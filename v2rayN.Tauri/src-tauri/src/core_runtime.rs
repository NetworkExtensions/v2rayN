use crate::{
    config_store::ConfigStore,
    core_update::{resolve_executable, CorePaths},
    domain::generate_runtime_bundle,
    events::EventSender,
    models::{CoreLogEvent, CoreType, RunningStatus},
    tun,
};
use anyhow::{anyhow, Context, Result};
use std::{
    fs::{self, File},
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    thread,
    time::Duration,
};

const HEALTH_CHECK_DELAY: Duration = Duration::from_millis(1500);

#[derive(Debug)]
pub struct RuntimeManager {
    process: Mutex<Option<ManagedProcess>>,
}

#[derive(Debug)]
struct ManagedProcess {
    children: Vec<Child>,
    elevated_pids: Vec<u32>,
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
            for child in &mut process.children {
                let _ = child.kill();
                let _ = child.wait();
            }
            for pid in &process.elevated_pids {
                let _ = tun::stop_elevated_process(*pid);
            }
        }
        *guard = None;
        Ok(RunningStatus::default())
    }

    pub fn start(
        &self,
        events: &EventSender,
        store: &ConfigStore,
        core_paths: &CorePaths,
    ) -> Result<RunningStatus> {
        self.stop()?;

        let config = store.load()?;
        let selected_profile = crate::domain::ensure_profile(&config)?;
        let bundle = generate_runtime_bundle(&config)?;
        let executable = resolve_executable(core_paths, &bundle.main_core_type)
            .context("未找到已安装的核心，请先下载核心")?;
        let config_path = PathBuf::from(store.paths().bin_configs.clone()).join(&bundle.main_artifact.file_name);
        fs::write(&config_path, bundle.main_artifact.content.as_bytes())?;

        let mut children = Vec::new();
        let mut elevated_pids = Vec::new();

        let main_process = if config.tun.enabled && matches!(bundle.main_core_type, CoreType::SingBox) {
            let pid = start_elevated_core(
                events,
                &store.paths().bin_configs,
                &store.paths().gui_logs,
                &executable,
                &bundle.main_core_type,
                &config_path,
                runtime_envs(&bundle.main_core_type, store),
                "core-main",
            )?;
            elevated_pids.push(pid);
            (Some(pid), true)
        } else {
            let child = start_direct_core(
                events,
                &store.paths().bin_configs,
                &executable,
                &bundle.main_core_type,
                &config_path,
                runtime_envs(&bundle.main_core_type, store),
                "main",
            )?;
            let pid = child.id();
            children.push(child);
            (Some(pid), false)
        };

        let mut helper_core_type = None;
        let mut helper_config_path = None;
        let mut helper_pid = None;

        if let Some(helper) = bundle.helper {
            let helper_executable = resolve_executable(core_paths, &helper.core_type)
                .context("未找到 TUN 辅助核心，请先下载对应核心")?;
            let helper_path = PathBuf::from(store.paths().bin_configs.clone()).join(&helper.artifact.file_name);
            fs::write(&helper_path, helper.artifact.content.as_bytes())?;

            let pid = start_elevated_core(
                events,
                &store.paths().bin_configs,
                &store.paths().gui_logs,
                &helper_executable,
                &helper.core_type,
                &helper_path,
                runtime_envs(&helper.core_type, store),
                "core-helper",
            )?;
            elevated_pids.push(pid);
            helper_core_type = Some(helper.core_type.clone());
            helper_config_path = Some(helper_path.to_string_lossy().to_string());
            helper_pid = Some(pid);
        }

        thread::sleep(HEALTH_CHECK_DELAY);
        health_check_children(&mut children)?;
        health_check_elevated_pids(&elevated_pids)?;

        let status = RunningStatus {
            running: true,
            core_type: Some(bundle.main_core_type.clone()),
            profile_id: Some(selected_profile.id.clone()),
            executable_path: Some(executable.to_string_lossy().to_string()),
            config_path: Some(config_path.to_string_lossy().to_string()),
            pid: main_process.0,
            elevated: main_process.1,
            helper_core_type,
            helper_config_path,
            helper_pid,
        };

        let mut guard = self.process.lock().map_err(|_| anyhow!("运行时状态不可用"))?;
        *guard = Some(ManagedProcess {
            children,
            elevated_pids,
            status: status.clone(),
        });

        Ok(status)
    }
}

fn runtime_envs(core_type: &CoreType, store: &ConfigStore) -> Vec<(String, String)> {
    match core_type {
        CoreType::Xray => vec![("XRAY_LOCATION_ASSET".into(), store.paths().bin)],
        CoreType::SingBox | CoreType::Mihomo => vec![],
    }
}

fn start_direct_core(
    events: &EventSender,
    working_directory: &str,
    executable: &Path,
    core_type: &CoreType,
    config_path: &Path,
    envs: Vec<(String, String)>,
    log_source: &'static str,
) -> Result<Child> {
    let mut command = Command::new(executable);
    apply_core_command(&mut command, core_type, config_path);
    for (key, value) in envs {
        command.env(key, value);
    }

    command
        .current_dir(working_directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().context("启动核心失败")?;
    bind_log_stream(events.clone(), child.stdout.take(), format!("{log_source}-stdout"));
    bind_log_stream(events.clone(), child.stderr.take(), format!("{log_source}-stderr"));
    Ok(child)
}

fn start_elevated_core(
    events: &EventSender,
    working_directory: &str,
    log_directory: &str,
    executable: &Path,
    core_type: &CoreType,
    config_path: &Path,
    envs: Vec<(String, String)>,
    log_name: &str,
) -> Result<u32> {
    let args = core_args(core_type, config_path);
    let log_path = tun::log_path(Path::new(log_directory), log_name);
    let pid = tun::start_elevated_process(
        Path::new(working_directory),
        executable,
        &args,
        &envs,
        &log_path,
    )?;
    bind_log_file(events.clone(), log_path, log_name.to_string());
    Ok(pid)
}

fn core_args(core_type: &CoreType, config_path: &Path) -> Vec<String> {
    match core_type {
        CoreType::Xray => vec![
            "run".into(),
            "-c".into(),
            config_path.to_string_lossy().to_string(),
        ],
        CoreType::SingBox => vec![
            "run".into(),
            "-c".into(),
            config_path.to_string_lossy().to_string(),
            "--disable-color".into(),
        ],
        CoreType::Mihomo => vec!["-f".into(), config_path.to_string_lossy().to_string()],
    }
}

fn apply_core_command(command: &mut Command, core_type: &CoreType, config_path: &Path) {
    command.args(core_args(core_type, config_path));
}

fn bind_log_stream(events: EventSender, stream: Option<impl std::io::Read + Send + 'static>, source: String) {
    let Some(stream) = stream else {
        return;
    };

    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().map_while(|line| line.ok()) {
            events.emit_core_log(CoreLogEvent {
                level: if source.ends_with("stderr") { "error".into() } else { "info".into() },
                source: source.clone(),
                message: line,
            });
        }
    });
}

fn health_check_children(children: &mut [Child]) -> Result<()> {
    for child in children.iter_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(-1);
                return Err(anyhow!(
                    "核心进程启动后立即退出 (exit code {code})，请检查日志"
                ));
            }
            Ok(None) => {}
            Err(e) => {
                return Err(anyhow!("检查核心进程状态失败: {e}"));
            }
        }
    }
    Ok(())
}

fn health_check_elevated_pids(pids: &[u32]) -> Result<()> {
    for pid in pids {
        let status = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {}
            _ => {
                return Err(anyhow!(
                    "提权核心进程 (PID {pid}) 启动后立即退出，请检查日志"
                ));
            }
        }
    }
    Ok(())
}

fn bind_log_file(events: EventSender, path: PathBuf, source: String) {
    thread::spawn(move || {
        let mut cursor = 0u64;
        let mut remainder = String::new();
        loop {
            if let Ok(mut file) = File::open(&path) {
                if let Ok(metadata) = file.metadata() {
                    if metadata.len() < cursor {
                        cursor = 0;
                        remainder.clear();
                    }
                }

                if file.seek(SeekFrom::Start(cursor)).is_ok() {
                    let mut chunk = String::new();
                    if file.read_to_string(&mut chunk).is_ok() {
                        cursor = cursor.saturating_add(chunk.len() as u64);
                        if !chunk.is_empty() {
                            remainder.push_str(&chunk);
                            let ends_with_newline = remainder.ends_with('\n') || remainder.ends_with('\r');
                            let mut lines = remainder.lines().map(str::to_string).collect::<Vec<_>>();
                            if ends_with_newline {
                                remainder.clear();
                            } else {
                                remainder = lines.pop().unwrap_or_default();
                            }

                            for line in lines {
                                events.emit_core_log(CoreLogEvent {
                                    level: "info".into(),
                                    source: source.clone(),
                                    message: line,
                                });
                            }
                        }
                    }
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
    });
}

use anyhow::{anyhow, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn start_elevated_process(
    working_directory: &Path,
    executable: &Path,
    args: &[String],
    envs: &[(String, String)],
    log_path: &Path,
) -> Result<u32> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let command = build_shell_command(working_directory, executable, args, envs, log_path);
    let script = format!("do shell script {} with administrator privileges", apple_script_quote(&command));
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .context("执行提权命令失败")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("提权启动失败: {}", stderr.trim()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .parse::<u32>()
        .context("无法解析提权进程 PID")
}

pub fn stop_elevated_process(pid: u32) -> Result<()> {
    let command = format!("kill -TERM {pid} >/dev/null 2>&1 || true");
    let script = format!("do shell script {} with administrator privileges", apple_script_quote(&command));
    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .context("执行提权停止失败")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("无法停止提权进程"))
    }
}

fn build_shell_command(
    working_directory: &Path,
    executable: &Path,
    args: &[String],
    envs: &[(String, String)],
    log_path: &Path,
) -> String {
    let env_assignment = envs
        .iter()
        .map(|(key, value)| format!("{key}={}", shell_quote(value)))
        .collect::<Vec<_>>()
        .join(" ");

    let args = args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ");

    let executable = shell_quote(&executable.to_string_lossy());
    let working_directory = shell_quote(&working_directory.to_string_lossy());
    let log_path = shell_quote(&log_path.to_string_lossy());

    let env_prefix = if env_assignment.is_empty() {
        String::new()
    } else {
        format!("{env_assignment} ")
    };

    format!(
        "cd {working_directory} && {env_prefix}{executable} {args} > {log_path} 2>&1 & echo $!"
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

fn apple_script_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn log_path(root: &Path, name: &str) -> PathBuf {
    root.join(format!("{name}.log"))
}

use std::process::Command as StdCommand;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SSHConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SSHExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// SSH 连接测试
#[tauri::command]
pub async fn ssh_test_connection(config: SSHConfig) -> Result<String, String> {
    let port = config.port.to_string();
    let host_port = format!("{}@{}", config.username, config.host);

    let mut args = vec![
        "-o", "StrictHostKeyChecking=no",
        "-o", "ConnectTimeout=10",
        "-p", &port,
        &host_port,
        "echo CONNECTED",
    ];

    if let Some(ref key) = config.key_path {
        args.insert(0, "-i");
        args.insert(1, key);
    }

    let output = StdCommand::new("ssh")
        .args(&args)
        .output()
        .map_err(|e| format!("SSH command failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.contains("CONNECTED") {
        Ok("SSH connection successful".to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("SSH connection failed: {}", stderr))
    }
}

/// SSH 远程执行命令
#[tauri::command]
pub async fn ssh_exec(config: SSHConfig, command: String) -> Result<SSHExecResult, String> {
    let port = config.port.to_string();
    let host_port = format!("{}@{}", config.username, config.host);

    let mut args = vec![
        "-o", "StrictHostKeyChecking=no",
        "-o", "ConnectTimeout=30",
        "-p", &port,
        &host_port,
        &command,
    ];

    if let Some(ref key) = config.key_path {
        args.insert(0, "-i");
        args.insert(1, key);
    }

    let output = StdCommand::new("ssh")
        .args(&args)
        .output()
        .map_err(|e| format!("SSH exec failed: {}", e))?;

    Ok(SSHExecResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// SSH 读取远程文件
#[tauri::command]
pub async fn ssh_read_file(config: SSHConfig, remote_path: String) -> Result<String, String> {
    let port = config.port.to_string();
    let host_port = format!("{}@{}", config.username, config.host);

    let cmd = format!("cat {}", remote_path);
    let mut args = vec![
        "-o", "StrictHostKeyChecking=no",
        "-p", &port,
        &host_port,
        &cmd,
    ];

    if let Some(ref key) = config.key_path {
        args.insert(0, "-i");
        args.insert(1, key);
    }

    let output = StdCommand::new("ssh")
        .args(&args)
        .output()
        .map_err(|e| format!("SSH read failed: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// SSH 列出远程目录
#[tauri::command]
pub async fn ssh_list_dir(config: SSHConfig, remote_path: String) -> Result<Vec<String>, String> {
    let port = config.port.to_string();
    let host_port = format!("{}@{}", config.username, config.host);

    let cmd = format!("ls -la {}", remote_path);
    let mut args = vec![
        "-o", "StrictHostKeyChecking=no",
        "-p", &port,
        &host_port,
        &cmd,
    ];

    if let Some(ref key) = config.key_path {
        args.insert(0, "-i");
        args.insert(1, key);
    }

    let output = StdCommand::new("ssh")
        .args(&args)
        .output()
        .map_err(|e| format!("SSH ls failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(stdout.lines().map(|l| l.to_string()).collect())
}

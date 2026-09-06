use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

fn hide_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW_FLAG);
    }
}

/// 在系统终端中打开项目目录
#[tauri::command]
pub fn open_terminal(path: String) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "wt", "-d", &path])
            .spawn()
            .map_err(|e| format!("Failed to open Windows Terminal: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-a", "Terminal", &path])
            .spawn()
            .map_err(|e| format!("Failed to open Terminal: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("gnome-terminal")
            .args(["--working-directory", &path])
            .spawn()
            .or_else(|_| Command::new("xterm").args(["-e", &format!("cd {}", path)]).spawn())
            .map_err(|e| format!("Failed to open terminal: {}", e))?;
    }

    Ok(format!("Terminal opened at {}", path))
}

/// 在项目目录中运行命令
#[tauri::command]
pub async fn run_command(path: String, command: String) -> Result<String, String> {
    let output = if cfg!(target_os = "windows") {
        let mut cmd = Command::new("cmd");
        hide_window(&mut cmd);
        cmd.args(["/C", &command])
            .current_dir(&path)
            .output()
    } else {
        Command::new("sh")
            .args(["-c", &command])
            .current_dir(&path)
            .output()
    }
    .map_err(|e| format!("Command failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(format!("Exit {}: {} {}", output.status.code().unwrap_or(-1), stdout, stderr))
    }
}

/// 检测系统安装的运行时
#[tauri::command]
pub fn detect_runtimes() -> Vec<serde_json::Value> {
    let checks: Vec<(&str, &str, &str)> = vec![
        ("python", "python --version", "python3 --version"),
        ("node", "node --version", ""),
        ("npm", "npm --version", ""),
        ("java", "java -version 2>&1", ""),
        ("go", "go version", ""),
        ("rust", "rustc --version", ""),
        ("gcc", "gcc --version", ""),
        ("git", "git --version", ""),
        ("docker", "docker --version", ""),
    ];

    checks
        .into_iter()
        .filter_map(|(name, cmd1, cmd2)| {
            let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
            hide_window(&mut cmd);
            let output = cmd
                .args(if cfg!(windows) {
                    vec!["/C", cmd1]
                } else {
                    vec!["-c", cmd1]
                })
                .output()
                .ok();

            if let Some(out) = output {
                let version = String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                if !version.is_empty() {
                    return Some(serde_json::json!({
                        "name": name,
                        "version": version.trim(),
                        "available": true,
                    }));
                }
            }

            // 尝试备用命令
            if !cmd2.is_empty() {
                let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "sh" });
                hide_window(&mut cmd);
                let output2 = cmd
                    .args(if cfg!(windows) {
                        vec!["/C", cmd2]
                    } else {
                        vec!["-c", cmd2]
                    })
                    .output()
                    .ok();
                if let Some(out) = output2 {
                    let version = String::from_utf8_lossy(&out.stdout)
                        .lines().next().unwrap_or("unknown").to_string();
                    if !version.is_empty() {
                        return Some(serde_json::json!({
                            "name": name, "version": version.trim(), "available": true,
                        }));
                    }
                }
            }

            Some(serde_json::json!({
                "name": name, "version": null, "available": false,
            }))
        })
        .collect()
}

use std::path::PathBuf;
use std::process::Command;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as AsyncCommand;

/// ─── deepseek-tui CLI 桥接 ───
/// 将自主执行的 Agent 任务委托给 deepseek-tui CLI
/// CLI 负责 Tool 调用（文件/shell/git/web）、MCP、会话管理

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CLIConfig {
    pub api_key: String,
    pub workspace: PathBuf,
    pub persona_prompt: String, // 要注入到 AGENTS.md 的 Persona system prompt
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CLIResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// 检查 deepseek CLI 是否可用
pub fn is_cli_available() -> bool {
    Command::new("deepseek")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 获取 CLI 版本
pub fn get_cli_version() -> Option<String> {
    Command::new("deepseek")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if v.is_empty() { None } else { Some(v) }
        })
}

/// 将 Persona prompt 写入 workspace 下的 AGENTS.md
pub fn inject_persona_prompt(workspace: &PathBuf, persona_prompt: &str) -> Result<(), String> {
    let agents_path = workspace.join("AGENTS.md");
    std::fs::write(&agents_path, persona_prompt)
        .map_err(|e| format!("Failed to write AGENTS.md: {}", e))
}

/// 通过 deepseek CLI 执行 Agent 任务（后台模式）
pub async fn execute_agent_task(
    config: &CLIConfig,
    task: &str,
) -> Result<CLIResult, String> {
    // 1. 注入 Persona prompt 到 AGENTS.md
    inject_persona_prompt(&config.workspace, &config.persona_prompt)?;

    // 2. 执行 deepseek CLI
    let mut cmd = AsyncCommand::new("deepseek");
    cmd.args([
        "--workspace", &config.workspace.display().to_string(),
        "--model", "auto",
        task,
    ]);
    cmd.env("DEEPSEEK_API_KEY", &config.api_key);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn deepseek CLI: {}. Install via: npm install -g @deepseek/cli", e))?;

    let stdout = child.stdout.take().ok_or("No stdout")?;
    let _stderr = child.stderr.take().ok_or("No stderr")?;

    let mut reader = BufReader::new(stdout).lines();
    let mut output = String::new();

    while let Some(line) = reader.next_line().await
        .map_err(|e| format!("Read error: {}", e))?
    {
        output.push_str(&line);
        output.push('\n');
    }

    let status = child.wait().await.map_err(|e| format!("Wait error: {}", e))?;

    if !status.success() {
        return Ok(CLIResult {
            success: false,
            output,
            error: Some(format!("CLI exited with code: {:?}", status.code())),
        });
    }

    Ok(CLIResult {
        success: true,
        output,
        error: None,
    })
}

/// Tauri 命令：检查 deepseek CLI 是否安装
#[tauri::command]
pub fn check_deepseek_cli() -> serde_json::Value {
    let available = is_cli_available();
    let version = get_cli_version();
    serde_json::json!({
        "available": available,
        "version": version,
        "install_hint": if !available { Some("npm install -g @deepseek/cli".to_string()) } else { None::<String> },
    })
}

/// Tauri 命令：通过 CLI 执行 Agent 任务
#[tauri::command]
pub async fn run_cli_agent_task(
    workspace: String,
    task: String,
    persona_prompt: String,
    api_key: String,
) -> Result<serde_json::Value, String> {
    if !is_cli_available() {
        return Err("deepseek CLI not installed. Run: npm install -g @deepseek/cli".into());
    }

    let config = CLIConfig {
        api_key,
        workspace: PathBuf::from(&workspace),
        persona_prompt,
    };

    let result = execute_agent_task(&config, &task).await?;

    Ok(serde_json::json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
    }))
}

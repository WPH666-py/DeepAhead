use tauri::State;

use crate::ai::{AgentDefinition, SafetyHooks, ContextCompressor, CompressedMessage};
use crate::ai::{build_system_prompt, DeepSeekClient, Message};

/// 列出所有内置 Agent
#[tauri::command]
pub fn list_agents() -> Vec<AgentDefinition> {
    AgentDefinition::all()
}

/// 使用指定 Agent 发送消息
#[tauri::command]
pub async fn send_agent_message(
    agent_name: String,
    mode: String,
    message: String,
    history: Vec<Message>,
    ds_client: State<'_, DeepSeekClient>,
) -> Result<serde_json::Value, String> {
    // 1. 获取 Agent 定义
    let agent = AgentDefinition::find(&agent_name)
        .ok_or_else(|| format!("Agent '{}' not found. Available: code-explorer, code-architect, code-reviewer", agent_name))?;

    // 2. 原生模式基础提示（无 Persona 注入；编排由原装工作流引擎负责）
    let base_prompt = build_system_prompt(&mode, &[]);

    // 3. 组装 System Prompt（Agent 定义 + 原生模式提示）
    let system_prompt = format!(
        "{}\n\n---\n\n## Active Agent: {}\n{}\n\n## Agent Allowed Tools\n{}\n\nFollow the agent's specific workflow.",
        base_prompt,
        agent.name,
        agent.system_prompt,
        agent.allowed_tools.join(", "),
    );

    // 4. 压缩对话历史
    let compressor = ContextCompressor::with_defaults();
    let compressed_messages: Vec<CompressedMessage> = history.iter().map(|m| CompressedMessage {
        role: m.role.clone(),
        content: m.content.clone(),
        estimated_tokens: ContextCompressor::estimate_tokens(&m.content),
    }).collect();
    let mut final_history: Vec<Message> = if compressor.needs_compression(&compressed_messages) {
        let compressed = compressor.compress(&compressed_messages);
        compressed.iter().map(|cm| Message {
            role: cm.role.clone(),
            content: cm.content.clone(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            r#type: cm.role.clone(),
        }).collect()
    } else {
        history
    };
    // 兼容前端旧消息：缺失 type 时默认用 role
    for m in &mut final_history {
        if m.r#type.is_empty() { m.r#type = m.role.clone(); }
    }

    // 5. 调用 DeepSeek API
    let resp = ds_client.chat(&system_prompt, &final_history).await?;

    let ai_message = resp.choices.first()
        .map(|c| c.message.clone())
        .unwrap_or_else(|| Message {
            role: "assistant".into(),
            content: "[No response]".into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            r#type: "assistant".into(),
        });

    Ok(serde_json::json!({
        "message": ai_message,
        "usage": {
            "prompt_tokens": resp.usage.prompt_tokens,
            "completion_tokens": resp.usage.completion_tokens,
            "total_tokens": resp.usage.total_tokens,
        },
        "agent": agent_name,
        "mode": mode,
    }))
}

/// 对代码内容运行安全钩子检查
#[tauri::command]
pub fn run_safety_check(content: String) -> Result<Vec<serde_json::Value>, String> {
    let hooks = SafetyHooks::new();
    let results = hooks.evaluate_all(&content);
    let json: Vec<_> = results.iter().map(|r| {
        serde_json::json!({
            "rule_id": r.rule_id,
            "message": r.message,
            "action": match r.action {
                crate::ai::HookAction::Confirm => "confirm",
                crate::ai::HookAction::Warn => "warn",
                crate::ai::HookAction::Block => "block",
                crate::ai::HookAction::Log => "log",
            },
            "triggered": r.triggered,
        })
    }).collect();
    Ok(json)
}

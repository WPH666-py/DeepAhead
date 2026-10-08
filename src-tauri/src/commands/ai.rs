use tauri::{State, Emitter};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ai::{
    AgentEvent, AgentLoopInput, AgentLoopOutput, ApprovalGate, ApprovalMode,
    build_system_prompt, native_system_prompt,
    ContextCompressor, CompressorConfig, CompressedMessage, ContextFile,
    DeepSeekClient, Message, modes,
    UndoStore, apply_undo,
    DEFAULT_CONTEXT_LIMIT,
};

/// 生成一次 Agent 运行的唯一 ID（时间戳 + 进程内自增 + 长度，避免依赖第三方随机库）
static RUN_COUNTER: AtomicU64 = AtomicU64::new(0);
fn new_run_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("run_{}_{}_{}", nanos, std::process::id(), c)
}

/// ─── AI IPC 命令 ───

/// 列出所有可用 AI 模式
#[tauri::command]
pub fn list_ai_modes() -> Vec<serde_json::Value> {
    modes::list_modes()
        .into_iter()
        .map(|(id, desc)| {
            let (engine, upstream, license, mechanism) = modes::engine_info(id);
            serde_json::json!({
                "id": id,
                "name": match id {
                    "dsh" => "DSH",
                    "dsk" => "DSK",
                    "dsa" => "DSA",
                    "dsf" => "DSF",
                    _ => id,
                },
                "desc": desc,
                "engine": engine,
                "upstream": upstream,
                "license": license,
                "mechanism": mechanism,
            })
        })
        .collect()
}

/// 切换 AI 模式 → 返回模式元数据 + 原生 System Prompt 预览
/// （无 Persona 文件加载：编排完全由 Rust 原装工作流引擎驱动）
#[tauri::command]
pub fn switch_ai_mode(mode: String) -> Result<serde_json::Value, String> {
    let m = modes::meta(&mode)
        .ok_or_else(|| format!("未知模式：{}（支持 dsh / dsk / dsa / dsf）", mode))?;

    let native = native_system_prompt(&mode);
    let preview: String = native.chars().take(500).collect();

    Ok(serde_json::json!({
        "mode": mode,
        "name": m.name,
        "provider": m.provider,
        "emulated_model": m.emulated_model,
        "coding_style": m.coding_style,
        "review_rigor": m.review_rigor,
        "architecture_first": m.architecture_first,
        "best_for": m.best_for,
        "desc": m.desc,
        "system_prompt_preview": preview,
        "engine": m.engine,
        "upstream": m.upstream,
        "license": m.license,
        "mechanism": m.mechanism,
    }))
}

/// 配置 DeepSeek API Key
#[tauri::command]
pub async fn configure_deepseek(
    api_key: String,
    base_url: Option<String>,
    model: Option<String>,
    ds_client: State<'_, DeepSeekClient>,
) -> Result<String, String> {
    ds_client.set_config(api_key, base_url, model).await;
    Ok("DeepSeek API configured successfully".to_string())
}

/// 发送 AI 消息（使用当前模式的原生提示 + DeepSeek API，不调工具）
#[tauri::command]
pub async fn send_ai_message(
    mode: String,
    message: String,
    history: Vec<Message>,
    context_paths: Vec<String>,
    ds_client: State<'_, DeepSeekClient>,
) -> Result<serde_json::Value, String> {
    let context_files: Vec<ContextFile> = context_paths
        .iter()
        .map(|path| {
            let parsed = crate::ai::file_parser::parse_file(path);
            ContextFile {
                path: path.clone(),
                content: Some(parsed.content),
            }
        })
        .collect();

    let system_prompt = build_system_prompt(&mode, &context_files);

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

    let resp = ds_client.chat(&system_prompt, &final_history).await?;

    let raw_message = resp
        .choices
        .first()
        .map(|c| c.message.clone())
        .unwrap_or_else(|| Message {
            role: "assistant".into(),
            content: "[No response from model]".into(),
            tool_calls: None,
            tool_call_id: None,
            name: None,
            reasoning_content: None,
            r#type: "assistant".into(),
        });

    // 剥离 tool_calls 等内部字段，返回前端需要的 role + content + type（保留 reasoning_content 供 thinking 模式回传）
    let safe_message = serde_json::json!({
        "role": raw_message.role,
        "content": raw_message.content,
        "reasoning_content": raw_message.reasoning_content,
        "type": raw_message.r#type,
    });

    Ok(serde_json::json!({
        "message": safe_message,
        "usage": {
            "prompt_tokens": resp.usage.prompt_tokens,
            "completion_tokens": resp.usage.completion_tokens,
            "total_tokens": resp.usage.total_tokens,
        },
        "mode": mode,
    }))
}

/// 流式发送 AI 消息 — 通过 Tauri events 实时推送 token 到前端
#[tauri::command]
pub async fn send_ai_message_stream(
    app: tauri::AppHandle,
    mode: String,
    message: String,
    history: Vec<Message>,
    context_paths: Vec<String>,
    ds_client: State<'_, DeepSeekClient>,
) -> Result<serde_json::Value, String> {
    let context_files: Vec<ContextFile> = context_paths
        .iter()
        .map(|path| {
            let parsed = crate::ai::file_parser::parse_file(path);
            ContextFile {
                path: path.clone(),
                content: Some(parsed.content),
            }
        })
        .collect();

    let system_prompt = build_system_prompt(&mode, &context_files);

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

    let app_handle = app.clone();
    let full_content = ds_client.chat_stream(&system_prompt, &final_history, move |token| {
        let _ = app_handle.emit("ai-stream-token", token);
    }).await?;

    let _ = app.emit("ai-stream-done", serde_json::json!({
        "content": full_content,
        "mode": mode,
    }).to_string());

    Ok(serde_json::json!({
        "content": full_content,
        "mode": mode,
    }))
}

/// ════════════════════════════════════════════════════════
/// 新增：Agent Loop 版本（带 9 个工具的真实工作流）
/// ════════════════════════════════════════════════════════

/// 同步版：执行完整 agent loop 一次性返回结果
#[tauri::command]
pub async fn send_ai_message_with_tools(
    app: tauri::AppHandle,
    mode: String,
    message: String,
    history: Vec<Message>,
    context_paths: Vec<String>,
    working_dir: Option<String>,
    approval_mode: Option<String>,
    context_limit: Option<usize>,
    auto_compress: Option<bool>,
    ds_client: State<'_, DeepSeekClient>,
    undo_store: State<'_, UndoStore>,
    approval_gate: State<'_, Arc<ApprovalGate>>,
) -> Result<serde_json::Value, String> {
    let wd = PathBuf::from(working_dir.unwrap_or_else(|| ".".to_string()));

    // 上下文文件（仅注入原生系统提示；原装工作流编排由引擎 extra_preamble 注入）
    let context_files: Vec<ContextFile> = context_paths
        .iter()
        .map(|path| {
            let parsed = crate::ai::file_parser::parse_file(path);
            ContextFile {
                path: path.clone(),
                content: Some(parsed.content),
            }
        })
        .collect();
    let system_prompt = build_system_prompt(&mode, &context_files);

    // DeepSeekClient 本身可 Clone（内部 Arc 共享配置），这里 clone 一份独立的 owned 实例
    // 给 agent_loop 使用，避免 State 生命周期问题
    let ds_for_loop = ds_client.inner().clone();
    let deepseek_arc = Arc::new(ds_for_loop);

    // 审批门：绑定 AppHandle（step 模式用它向前端发出审批事件）
    let gate_arc: Arc<ApprovalGate> = approval_gate.inner().clone();
    gate_arc.set_app(app.clone());

    let run_id = new_run_id();
    // 上下文窗口 + 压缩模式（自动 = 超 85% 自动压缩用户上下文；手动 = 仅提示）
    let effective_limit = context_limit.unwrap_or(DEFAULT_CONTEXT_LIMIT).max(1000);
    let effective_auto_compress = auto_compress.unwrap_or(false);

    let input = AgentLoopInput {
        mode: mode.clone(),
        user_message: message,
        history,
        context_paths,
        working_dir: wd,
        deepseek: deepseek_arc,
        system_prompt,
        run_id: run_id.clone(),
        undo_store: Arc::new(undo_store.inner().clone()),
        max_iterations_override: None,
        extra_preamble: None,
        approval_mode: ApprovalMode::parse(&approval_mode.unwrap_or_else(|| "step".to_string())),
        approval_gate: gate_arc,
        context_limit: effective_limit,
        auto_compress: effective_auto_compress,
    };

    // 事件转发到 Tauri：每个 agent 事件触发 ai-agent-event
    // 按模式分发到原装工作流引擎（dsh=原生循环；dsk/dsa/dsf=厂商引擎）
    let app_for_events = app.clone();
    let output: AgentLoopOutput = crate::ai::workflow::run(input, move |event: AgentEvent| {
        // 转为 serde_json::Value 再 emit，避免复杂枚举序列化问题
        if let Ok(ev_value) = serde_json::to_value(&event) {
            let _ = app_for_events.emit("ai-agent-event", ev_value);
        }
    }).await?;

    // 不返回 events 数组（已通过 Tauri 事件实时推送），只返回摘要
    Ok(serde_json::json!({
        "content": output.final_content,
        "total_iterations": output.total_iterations,
        "total_tool_calls": output.total_tool_calls,
        "mode": mode,
        "event_count": output.events.len(),
        "run_id": output.run_id,
        "context_tokens": output.context_tokens,
        "context_limit": output.context_limit,
        "context_ratio": output.context_ratio,
        "compressed": output.compressed,
    }))
}

/// ════════════════════════════════════════════════════════
/// 上下文占用比例 + 压缩（自动 / 手动）
/// ════════════════════════════════════════════════════════

/// 实时估算一段对话的上下文占用。
/// frontend 传入当前 System Prompt、对话消息与上下文文件路径，后端返回 tokens / limit / ratio。
#[tauri::command]
pub fn estimate_context_usage(
    system_prompt: Option<String>,
    messages: Vec<Message>,
    context_paths: Option<Vec<String>>,
    context_limit: Option<usize>,
) -> serde_json::Value {
    let limit = context_limit.unwrap_or(DEFAULT_CONTEXT_LIMIT).max(1000);

    let mut tokens = ContextCompressor::estimate_tokens(system_prompt.as_deref().unwrap_or(""));
    if let Some(paths) = &context_paths {
        for p in paths {
            let parsed = crate::ai::file_parser::parse_file(p);
            tokens += ContextCompressor::estimate_tokens(&parsed.content);
        }
    }
    tokens += messages
        .iter()
        .map(|m| ContextCompressor::estimate_tokens(&m.content))
        .sum::<usize>();

    let ratio = (tokens as f64 / limit as f64).min(1.0);
    serde_json::json!({
        "tokens": tokens,
        "limit": limit,
        "ratio": ratio,
    })
}

/// 手动压缩上下文：返回压缩前后的 Token 数与压缩后的消息序列。
/// 调用方可用 `compressed_messages` 直接替换当前对话历史（对话不被清空，只压缩较早轮次）。
#[tauri::command]
pub fn compress_context(
    messages: Vec<Message>,
    max_tokens: Option<usize>,
    preserve_recent_turns: Option<usize>,
) -> serde_json::Value {
    let mut config = CompressorConfig::with_limit(max_tokens.unwrap_or(DEFAULT_CONTEXT_LIMIT), true);
    if let Some(n) = preserve_recent_turns {
        config.preserve_recent_turns = n.max(1);
    }
    let compressor = ContextCompressor::new(config);

    let input: Vec<CompressedMessage> = messages
        .iter()
        .map(|m| CompressedMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            estimated_tokens: ContextCompressor::estimate_tokens(&m.content),
        })
        .collect();

    let before_tokens = compressor.total_tokens(&input);
    let compressed = compressor.compress(&input);
    let after_tokens = compressor.total_tokens(&compressed);

    let summary = compressed
        .iter()
        .find(|m| m.content.contains("Conversation Summary"))
        .map(|m| m.content.clone())
        .unwrap_or_default();

    let compressed_messages: Vec<serde_json::Value> = compressed
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
                "type": m.role,
            })
        })
        .collect();

    serde_json::json!({
        "before_tokens": before_tokens,
        "after_tokens": after_tokens,
        "removed_messages": input.len().saturating_sub(compressed.len()),
        "summary": summary,
        "compressed_messages": compressed_messages,
    })
}

/// 当前上下文压缩配置（默认窗口 / 阈值比例 / 保留轮数）
#[tauri::command]
pub fn get_context_config(context_limit: Option<usize>) -> serde_json::Value {
    let config = CompressorConfig::with_limit(context_limit.unwrap_or(DEFAULT_CONTEXT_LIMIT), false);
    serde_json::json!({
        "max_tokens": config.max_tokens,
        "warn_ratio": config.warn_ratio,
        "compression_threshold": config.warn_ratio,
        "preserve_recent_turns": config.preserve_recent_turns,
    })
}

/// 查询某次 Agent 运行记录的"可撤销文件变更"数量（撤回对话框用）
#[tauri::command]
pub fn get_run_undo_count(run_id: String, undo_store: State<'_, UndoStore>) -> usize {
    undo_store.count(&run_id)
}

/// 撤销某次 Agent 运行的文件变更（write/edit 修改的文件恢复原样，新建文件删除）
/// 返回：撤销动作描述列表
#[tauri::command]
pub fn undo_run_changes(run_id: String, undo_store: State<'_, UndoStore>) -> Vec<String> {
    let entries = undo_store.take(&run_id);
    apply_undo(&entries)
}

/// 检查 DeepSeek 连接健康状态
#[tauri::command]
pub async fn check_deepseek_health(
    ds_client: State<'_, DeepSeekClient>,
) -> Result<String, String> {
    ds_client.health_check().await
}

/// 解析上下文文件——前端可在文件选择器中预览解析结果
#[tauri::command]
pub fn parse_context_file(path: String) -> Result<serde_json::Value, String> {
    let parsed = crate::ai::file_parser::parse_file(&path);
    Ok(serde_json::json!({
        "path": parsed.path,
        "content": parsed.content,
        "format": parsed.format,
        "size_bytes": parsed.size_bytes,
        "is_binary": parsed.is_binary,
        "truncated": parsed.truncated,
        "success": parsed.success,
        "error": parsed.error,
    }))
}

/// ════════════════════════════════════════════════════════
/// 多模态视觉：DeepSeek-OCR + ModLens
/// ════════════════════════════════════════════════════════

/// 配置视觉识别引擎（provider / api_key / base_url / model）
#[tauri::command]
pub fn configure_vision(
    provider: String,
    api_key: String,
    base_url: Option<String>,
    model: Option<String>,
) -> Result<String, String> {
    crate::ai::vision::set_config(provider, api_key, base_url, model);
    Ok("Vision config saved".to_string())
}

/// 当前视觉配置快照
#[tauri::command]
pub fn get_vision_config() -> serde_json::Value {
    let cfg = crate::ai::vision::get_config();
    serde_json::json!({
        "provider": cfg.provider,
        "api_key": if cfg.api_key.is_empty() { "" } else { "****" },
        "base_url": cfg.base_url,
        "model": cfg.model,
        "configured": crate::ai::vision::is_configured(),
    })
}

/// 识别一张图片（ModLens / DeepSeek-OCR），把结果转译为文本供模型使用
#[tauri::command]
pub async fn analyze_image(
    image_path: String,
    prompt: Option<String>,
) -> Result<serde_json::Value, String> {
    let result = crate::ai::vision::analyze_image(&image_path, prompt.as_deref()).await?;
    Ok(serde_json::json!({
        "text": result.text,
        "provider": result.provider,
        "image_path": result.image_path,
    }))
}

/// 保存粘贴的图片（base64，可带 data: 前缀）为临时文件，返回路径
#[tauri::command]
pub fn save_temp_image(data: String, ext: String) -> Result<String, String> {
    crate::ai::vision::save_temp_image(&data, &ext)
}

/// ════════════════════════════════════════════════════════
/// 执行许可（需分步确认 / 全流程开放）—— 对标 Harness 审批
/// ════════════════════════════════════════════════════════

/// 前端对某个工具调用的审批应答（approval_id 由 tool_approval_required 事件下发）
#[tauri::command]
pub async fn respond_tool_approval(
    approval_id: String,
    approved: bool,
    approval_gate: State<'_, Arc<ApprovalGate>>,
) -> Result<(), String> {
    approval_gate.inner().clone().resolve(approval_id, approved).await
}

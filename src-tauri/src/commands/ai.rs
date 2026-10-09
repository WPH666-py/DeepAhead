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
    // 续跑上下文：用户在裁决卡片上选择 ❌ 放行后，前端自动回「继续」时携带。
    // 形如 {"run_id":"...","tool":"write","op_type":"write","path":"D:\\a.txt"}。
    resume_context: Option<serde_json::Value>,
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
    // 续跑：把"用户已放行、请立即执行"作为额外前言注入（模型据此直接动手，不再请示）
    let extra_preamble = resume_context.as_ref().and_then(|rc| {
        let tool = rc.get("tool").and_then(|v| v.as_str()).unwrap_or("");
        if tool.is_empty() {
            return None;
        }
        let path = rc.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let args = rc.get("args").and_then(|v| v.as_str()).unwrap_or("");
        Some(format!(
            "## 续跑（用户在裁决卡片上选择了 ❌ 放行）\n\
             用户刚刚在裁决卡片上放行了下面这个操作，规则引擎已登记一次性放行：\n\
             - 工具：`{}`\n\
             - 参数：`{}`\n\
             - 路径/类型：{} / {}\n\
             **请立即重新发起这同一个调用**（工具名与参数保持一致，便于消费一次性放行），\
             不需要再向用户确认；放行成功后继续完成原任务。若该操作已经不再必要，直接说明原因并给出结论。",
            tool,
            if args.chars().count() > 400 { args.chars().take(400).collect::<String>() } else { args.to_string() },
            if path.is_empty() { "（不适用）" } else { path },
            rc.get("op_type").and_then(|v| v.as_str()).unwrap_or("any"),
        ))
    });
    if let Some(extra) = &extra_preamble {
        crate::ai::runtime_log::info(
            "agent",
            &format!("续跑注入：{}", extra.replace('\n', " ").chars().take(200).collect::<String>()),
        );
    }

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

    let parsed_mode = ApprovalMode::parse(&approval_mode.unwrap_or_else(|| "step".to_string()));
    crate::ai::runtime_log::info(
        "agent",
        &format!(
            "收到用户消息 run={} 模式={} 执行许可={} 工具={} 工作目录={}",
            run_id,
            mode.to_uppercase(),
            parsed_mode.describe(),
            if message.starts_with("[System]") { "对话" } else { "Agent" },
            wd.display()
        ),
    );

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
        extra_preamble,
        approval_mode: parsed_mode,
        approval_gate: gate_arc,
        context_limit: effective_limit,
        auto_compress: effective_auto_compress,
        memory: crate::ai::memory::get_config(),
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
/// 长期记忆协议（移植自 baaai123/dsh-memory-protocol）
/// ════════════════════════════════════════════════════════

/// 读取长期记忆配置
#[tauri::command]
pub fn get_memory_config() -> serde_json::Value {
    serde_json::to_value(crate::ai::memory::get_config()).unwrap_or(serde_json::json!({}))
}

/// 更新长期记忆配置
///
/// ⚠️ 同样受 Tauri v2 camelCase 约定约束：
/// `enforce_weave` ← `enforceWeave`、`inject_weave` ← `injectWeave`、
/// `auto_ingest` ← `autoIngest`、`fail_open` ← `failOpen`。
#[tauri::command]
pub fn set_memory_config(
    enabled: Option<bool>,
    enforce_weave: Option<bool>,
    inject_weave: Option<bool>,
    auto_ingest: Option<bool>,
    allowlist: Option<Vec<String>>,
    fail_open: Option<bool>,
) -> serde_json::Value {
    let mut cfg = crate::ai::memory::get_config();
    if let Some(v) = enabled { cfg.enabled = v; }
    if let Some(v) = enforce_weave { cfg.enforce_weave = v; }
    if let Some(v) = inject_weave { cfg.inject_weave = v; }
    if let Some(v) = auto_ingest { cfg.auto_ingest = v; }
    if let Some(v) = allowlist { cfg.allowlist = v; }
    if let Some(v) = fail_open { cfg.fail_open = v; }
    serde_json::to_value(crate::ai::memory::set_config(cfg)).unwrap_or(serde_json::json!({}))
}

/// 手动查阅记忆（weave）
#[tauri::command]
pub fn memory_weave(query: String, top_k: Option<usize>) -> Result<String, String> {
    crate::ai::memory::weave(&query, "manual", top_k)
}

/// 写入一条记忆
#[tauri::command]
pub fn memory_ingest(
    content: String,
    role: Option<String>,
    kind: Option<String>,
) -> Result<serde_json::Value, String> {
    let rec = crate::ai::memory::ingest(
        &content,
        role.as_deref().unwrap_or("user"),
        "manual",
        kind.as_deref(),
    )?;
    serde_json::to_value(rec).map_err(|e| e.to_string())
}

/// 检索记忆
#[tauri::command]
pub fn memory_search(query: String, limit: Option<usize>) -> Result<serde_json::Value, String> {
    let hits = crate::ai::memory::search(&query, limit)?;
    let arr: Vec<serde_json::Value> = hits
        .into_iter()
        .map(|(r, s)| {
            serde_json::json!({
                "id": r.id, "kind": r.kind, "text": r.text,
                "role": r.role, "created_at": r.created_at, "score": s,
            })
        })
        .collect();
    Ok(serde_json::json!({ "hits": arr }))
}

/// 记忆库状态
#[tauri::command]
pub fn memory_status() -> serde_json::Value {
    crate::ai::memory::status()
}

/// 最近的记忆（界面展示）
#[tauri::command]
pub fn memory_recent(limit: Option<usize>) -> serde_json::Value {
    serde_json::json!({ "records": crate::ai::memory::recent(limit) })
}

/// 清空全部记忆
#[tauri::command]
pub fn memory_clear() -> Result<usize, String> {
    crate::ai::memory::clear()
}

/// ════════════════════════════════════════════════════════
/// 插件体检（移植自 dsh-plugin-vet）
/// ════════════════════════════════════════════════════════

/// 静态扫描一个包目录/文件，返回两段式评分卡的第一段
#[tauri::command]
pub fn vet_scan(target: String) -> Result<serde_json::Value, String> {
    let report = crate::ai::plugin_vet::scan_path(&target)?;
    let name = std::path::Path::new(&target)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| target.clone());
    let scorecard = crate::ai::plugin_vet::render_scorecard(&report, &name);
    Ok(serde_json::json!({
        "report": report,
        "scorecard": scorecard,
    }))
}

/// 写健康档案（第二段：人工/模型审计结论）
#[tauri::command]
pub fn vet_write_health_record(
    target: String,
    name: Option<String>,
    version: Option<String>,
    risk: Option<String>,
    recommendation: Option<String>,
    notes: Option<String>,
) -> Result<String, String> {
    let report = crate::ai::plugin_vet::scan_path(&target)?;
    let n = name.unwrap_or_else(|| {
        std::path::Path::new(&target)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| target.clone())
    });
    crate::ai::plugin_vet::write_health_record(
        &n,
        version.as_deref().unwrap_or("1.0.0"),
        &report,
        risk.as_deref().unwrap_or("clean"),
        recommendation.as_deref().unwrap_or("review"),
        notes.as_deref().unwrap_or(""),
    )
}

/// 列出已有健康档案
#[tauri::command]
pub fn vet_list_records() -> serde_json::Value {
    serde_json::json!({ "records": crate::ai::plugin_vet::list_health_records() })
}

/// ════════════════════════════════════════════════════════
/// 规则引擎（移植自 dsh-rule-engine）
/// ════════════════════════════════════════════════════════

/// 从 $DSH_HOME/AGENTS.md 载入规则并给出概览
#[tauri::command]
pub fn rules_load() -> serde_json::Value {
    let rules = crate::ai::rules_engine::load_rules_from_home();
    let mut summary = crate::ai::rules_engine::rules_summary(&rules);
    summary["rules_path"] = serde_json::json!(
        crate::ai::rules_engine::dsh_home().join("AGENTS.md").to_string_lossy().to_string()
    );
    summary
}

/// 解析一段规则文本（不落盘，供界面预览）
#[tauri::command]
pub fn rules_parse(text: String) -> serde_json::Value {
    let rules = crate::ai::rules_engine::parse_rules(&text);
    crate::ai::rules_engine::rules_summary(&rules)
}

/// 读取审计账本
#[tauri::command]
pub fn rules_audit(limit: Option<usize>) -> serde_json::Value {
    serde_json::json!({
        "records": crate::ai::rules_engine::read_audit(limit.unwrap_or(50)),
        "path": crate::ai::rules_engine::audit_path().to_string_lossy().to_string(),
    })
}

/// 试算一次工具调用是否会被硬门拦下（界面自检用）
#[tauri::command]
pub fn rules_test_guard(
    tool: String,
    arguments: Option<serde_json::Value>,
    user_text: Option<String>,
) -> serde_json::Value {
    let cfg = crate::ai::rules_engine::RuleEngineConfig::default();
    let mut st = crate::ai::rules_engine::RuleEngineState::default();
    let text = user_text.unwrap_or_default();
    st.real_user_seen = !text.is_empty();
    st.has_execute_clause = crate::ai::rules_engine::has_execute_clause(&text);
    st.user_text = text;
    let args = arguments.unwrap_or(serde_json::json!({}));
    let now = chrono::Utc::now().timestamp();
    let d = crate::ai::rules_engine::guard_decision(&cfg, &mut st, &tool, &args, now);
    serde_json::to_value(d).unwrap_or(serde_json::json!({}))
}

/// 读取规则引擎配置（开关 / 未知工具策略 / 裁决卡片）
#[tauri::command]
pub fn rules_get_config() -> serde_json::Value {
    serde_json::to_value(crate::ai::rules_engine::get_config())
        .unwrap_or(serde_json::json!({}))
}

/// 应用界面开关（即时影响 agent loop 的硬门与裁决卡片）
///
/// ⚠️ Tauri v2 参数命名约定：`#[tauri::command]` 默认 `rename_all = "camelCase"`，
/// Rust 侧的 snake_case 形参在 JS 侧必须用 camelCase 传
/// （`turn_card_enabled` ← `turnCardEnabled`）。传错会被**静默忽略**，
/// 表现为"开关点了没反应"。
#[tauri::command]
pub fn rules_set_toggles(
    enabled: Option<bool>,
    turn_card_enabled: Option<bool>,
    task_contract_enabled: Option<bool>,
) -> serde_json::Value {
    let cfg = crate::ai::rules_engine::set_ui_toggles(enabled, turn_card_enabled, task_contract_enabled);
    serde_json::to_value(cfg).unwrap_or(serde_json::json!({}))
}

/// 执行一条 /guard 命令（面板内命令行）
#[tauri::command]
pub fn rules_guard_command(input: String) -> serde_json::Value {
    let mut cfg = crate::ai::rules_engine::get_config();
    let mut st = crate::ai::rules_engine::RuleEngineState::default();
    st.rules = crate::ai::rules_engine::load_rules_from_home();
    let r = crate::ai::rules_engine::run_guard_command(&mut cfg, &mut st, &input);
    // unlock / bypass 等运行期窗口写回全局，下一次 Agent 运行即刻生效
    crate::ai::rules_engine::apply_runtime_windows(cfg.unlock_until, cfg.bypass_until);
    crate::ai::runtime_log::info("rules", &format!("/guard {} → {}", input.trim(), r.text.trim()));
    serde_json::to_value(r).unwrap_or(serde_json::json!({"ok": false, "text": "命令执行失败"}))
}

/// **执行许可档位 → 规则引擎开关联动**（规则引擎的开关不交给用户自选）
///
/// - 需逐步确认 / 仅确认风险操作：规则引擎所有开关全开（硬门 + 裁决卡片 + 任务契约）；
/// - 全流程开放：规则引擎所有开关**全关**，所有操作**永久放行**。
#[tauri::command]
pub fn rules_link_mode(mode: String) -> serde_json::Value {
    let m = crate::ai::approval::ApprovalMode::parse(&mode);
    let cfg = crate::ai::rules_engine::link_ui_mode(&m);
    serde_json::json!({
        "mode": m.as_str(),
        "label": m.label(),
        "rule_engine_on": m.rule_engine_enabled(),
        "turn_card_on": m.turn_card_enabled(),
        "gate_every_call": m.gate_every_call(),
        "config": serde_json::to_value(cfg).unwrap_or(serde_json::json!({})),
    })
}

/// ─── 回合裁决卡片 → 放行 / 拦截（待决规则裁决队列）───

/// 用户在卡片上做出选择：
/// - `approved = true`（❌ 放行）：登记一次性放行并置「待继续」，前端自动回「继续」；
/// - `approved = false`（✅ 拦截）：不放行，本轮不再重试该操作。
#[tauri::command]
pub fn rules_resolve_pending(
    run_id: String,
    block_index: usize,
    approved: bool,
) -> Result<serde_json::Value, String> {
    let op = crate::ai::pending_guard::resolve(&run_id, block_index, approved)?;
    let mut v = serde_json::to_value(&op).map_err(|e| e.to_string())?;
    v["has_continue"] = serde_json::json!(crate::ai::pending_guard::has_continue(&run_id));
    Ok(v)
}

/// 读取「待继续」状态（Agent 跑完后前端调用一次：需要则自动回「继续」再跑一轮）
#[tauri::command]
pub fn rules_take_continue(run_id: String) -> serde_json::Value {
    let cont = crate::ai::pending_guard::take_continue(&run_id);
    let op = if cont {
        crate::ai::pending_guard::take_last_allowed(&run_id)
    } else {
        None
    };
    serde_json::json!({
        "has_continue": cont,
        "op": serde_json::to_value(op).unwrap_or(serde_json::Value::Null),
    })
}

/// 待决裁决队列状态（只读，供界面显示）
#[tauri::command]
pub fn rules_pending_state(run_id: String) -> serde_json::Value {
    crate::ai::pending_guard::snapshot(&run_id)
}

/// 试算一次工具调用是否属于「风险操作」（仅确认风险操作档位的判定口径）
#[tauri::command]
pub fn rules_is_risk(tool: String, arguments: Option<serde_json::Value>) -> serde_json::Value {
    let args = arguments.unwrap_or(serde_json::json!({}));
    serde_json::json!({ "risk": crate::ai::rules_engine::is_risk_operation(&tool, &args) })
}

/// ════════════════════════════════════════════════════════
/// 运行时日志（实时落盘到用户本机安装目录，用户随时可查）
/// ════════════════════════════════════════════════════════

/// 前端日志面板的每条记录都实时落盘（level: info / warn / error）
#[tauri::command]
pub fn runtime_log_write(level: String, scope: String, message: String) -> bool {
    let lv = match level.as_str() {
        "warn" | "warning" => "warn",
        "error" | "fatal" => "error",
        _ => "info",
    };
    crate::ai::runtime_log::write(lv, &scope, &message);
    true
}

/// 日志文件位置与体积（界面显示"日志已实时保存到 …"）
#[tauri::command]
pub fn runtime_log_status() -> serde_json::Value {
    crate::ai::runtime_log::status()
}

/// 磁盘日志尾部（界面内直接查看，不必离开应用）
#[tauri::command]
pub fn runtime_log_tail(lines: Option<usize>) -> serde_json::Value {
    let n = lines.unwrap_or(200).clamp(10, 5000);
    serde_json::json!({
        "text": crate::ai::runtime_log::tail(n),
        "path": crate::ai::runtime_log::log_path().to_string_lossy().to_string(),
    })
}

/// 在系统文件管理器中打开日志目录（安装目录下的 DeepAhead\logs）
#[tauri::command]
pub fn runtime_log_open_dir() -> Result<String, String> {
    let dir = crate::ai::runtime_log::log_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建日志目录失败: {}", e))?;
    let path = dir.to_string_lossy().to_string();
    #[cfg(target_os = "windows")]
    let spawned = std::process::Command::new("explorer").arg(&path).spawn();
    #[cfg(target_os = "macos")]
    let spawned = std::process::Command::new("open").arg(&path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let spawned = std::process::Command::new("xdg-open").arg(&path).spawn();
    match spawned {
        Ok(_) => {
            crate::ai::runtime_log::info("app", &format!("已在文件管理器中打开日志目录：{}", path));
            Ok(path)
        }
        Err(e) => Err(format!("打开日志目录失败：{}（目录：{}）", e, path)),
    }
}

/// 回合裁决卡片列表
#[tauri::command]
pub fn rules_turn_cards(session_id: Option<String>, limit: Option<usize>) -> serde_json::Value {
    let cards = crate::ai::rules_engine::list_turn_cards(session_id.as_deref(), limit);
    serde_json::json!({
        "cards": cards,
        "path": crate::ai::rules_engine::turn_cards_path().to_string_lossy().to_string(),
    })
}

/// 判例登记（一次性）：✅拦对了 / ❌拦错了
#[tauri::command]
pub fn rules_rate_turn_card(
    key: String,
    block_index: usize,
    verdict: String,
    expected_verdict: Option<String>,
) -> Result<serde_json::Value, String> {
    let card = crate::ai::rules_engine::rate_turn_card(
        &key,
        block_index,
        &verdict,
        expected_verdict.as_deref(),
    )?;
    serde_json::to_value(card).map_err(|e| e.to_string())
}

/// 把卡片挂到某条助手消息上
#[tauri::command]
pub fn rules_attach_turn_card(key: String, message_id: String) -> Result<serde_json::Value, String> {
    let card = crate::ai::rules_engine::attach_turn_card(&key, &message_id)?;
    serde_json::to_value(card).map_err(|e| e.to_string())
}

/// 已登记的指纹放行（7 天判例）
#[tauri::command]
pub fn rules_labels() -> serde_json::Value {
    serde_json::json!({ "labels": crate::ai::rules_engine::load_labels() })
}

/// 计算一条命令的指纹（界面自检 / 手动撤销用）
#[tauri::command]
pub fn rules_fingerprint(command: String) -> serde_json::Value {
    serde_json::json!({ "fingerprint": crate::ai::rules_engine::fingerprint_of(&command) })
}

/// ════════════════════════════════════════════════════════
/// 费用统计（移植自 dsh-cost-meter）
/// ════════════════════════════════════════════════════════

#[tauri::command]
pub fn cost_snapshot() -> serde_json::Value {
    crate::ai::cost_meter::snapshot()
}

/// 更新费用配置（币种/展示/预算/保留期）
#[tauri::command]
pub fn cost_set_config(
    currency: Option<String>,
    symbol: Option<String>,
    decimals: Option<u32>,
    exchange_rate: Option<f64>,
    history_days: Option<u32>,
    budget_enabled: Option<bool>,
    budget_amount: Option<f64>,
    budget_period: Option<String>,
) -> serde_json::Value {
    let cfg = crate::ai::cost_meter::update_config(|c| {
        if let Some(v) = &currency {
            c.currency = if v.eq_ignore_ascii_case("cny") {
                crate::ai::cost_meter::Currency::Cny
            } else {
                crate::ai::cost_meter::Currency::Usd
            };
        }
        if let Some(v) = symbol { c.symbol = v; }
        if let Some(v) = decimals { c.decimals = v; }
        if let Some(v) = exchange_rate { if v > 0.0 { c.exchange_rate = v; } }
        if let Some(v) = history_days { c.history_days = v.clamp(7, 3650); }
        if let Some(v) = budget_enabled { c.budget_enabled = v; }
        if let Some(v) = budget_amount { c.budget_amount = v; }
        if let Some(v) = budget_period { c.budget_period = v; }
    });
    serde_json::to_value(cfg).unwrap_or(serde_json::json!({}))
}

/// 清空费用历史
#[tauri::command]
pub fn cost_clear() -> serde_json::Value {
    serde_json::json!({ "cleared_days": crate::ai::cost_meter::clear_history() })
}

/// ════════════════════════════════════════════════════════
/// 上下文压缩（billion-context）：窗口与判定自检
/// ════════════════════════════════════════════════════════

/// 返回当前压缩引擎的阈值与默认配置（供界面展示与调参）
#[tauri::command]
pub fn context_engine_config(context_limit: Option<usize>) -> serde_json::Value {
    let cfg = crate::ai::billion_context::BillionConfig::with_limit(
        context_limit.unwrap_or(crate::ai::context::DEFAULT_CONTEXT_LIMIT),
    );
    serde_json::json!({
        "model_context_limit": cfg.model_context_limit,
        "max_context_limit_pct": cfg.max_context_limit_pct,
        "min_context_limit_pct": cfg.min_context_limit_pct,
        "emergency_threshold_pct": cfg.emergency_threshold_pct,
        "nudge_growth_tokens": cfg.nudge_growth_tokens(),
        "growth_floor": cfg.growth_floor_effective(),
        "min_pressure_benefit": cfg.min_pressure_benefit(),
        "tier_threshold_1": cfg.tier_threshold(1),
        "tier_threshold_2": cfg.tier_threshold(2),
        "tiers_enabled": cfg.tiers_enabled,
        "min_compress_range": cfg.min_compress_range,
        "min_summary_length": cfg.min_summary_length,
        "max_summary_length": cfg.max_summary_length,
        "preserve_recent_messages": cfg.preserve_recent_messages,
        "preserve_recent_tokens": cfg.preserve_recent_tokens,
        "engine": "billion-context (acp-kernel port)",
    })
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

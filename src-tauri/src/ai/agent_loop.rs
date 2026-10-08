use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::ai::approval::{ApprovalGate, ApprovalMode};
use crate::ai::context::{CompressedMessage, CompressorConfig, ContextCompressor};
use crate::ai::deepseek::{DeepSeekClient, Message};
use crate::ai::tools::{ToolRegistry, ToolCall, ToolResult, ToolSchema, ToolFunction, SubagentExecutor, detect_runtimes};
use crate::ai::undo::UndoStore;

/// ─── Agent Loop 配置 ───

#[derive(Debug, Clone)]
pub struct LoopConfig {
    /// 最大工具调用迭代次数
    pub max_iterations: usize,
    /// 是否在每步后注入"先读后改"提醒（Claude 模式）
    pub inject_read_before_edit_reminder: bool,
    /// 是否在每 N 步注入"目标完成度"提醒（GPT 模式）
    pub inject_progress_reminder_every: Option<usize>,
    /// 是否在开始前要求 Grep/Glob 概览（Gemini 模式）
    pub require_initial_scan: bool,
    /// 是否强制分解为子任务（Qwen 模式）
    pub require_task_decomposition: bool,
    /// 是否每步要求"先输出推理"（Kimi 模式）
    pub require_thinking_prefix: bool,
}

impl LoopConfig {
    /// 根据模式决定 Loop 行为
    pub fn for_mode(mode: &str) -> Self {
        match mode {
            "dsh" => Self {
                // 0 = 不限步数：循环直到模型给出结论（不再调用工具）或出错
                max_iterations: 0,
                inject_read_before_edit_reminder: true,
                inject_progress_reminder_every: None,
                require_initial_scan: true,
                require_task_decomposition: false,
                require_thinking_prefix: false,
            },
            "dsk" => Self {
                max_iterations: 0,
                inject_read_before_edit_reminder: false,
                inject_progress_reminder_every: Some(5),
                require_initial_scan: false,
                require_task_decomposition: false,
                require_thinking_prefix: false,
            },
            "dsa" => Self {
                max_iterations: 0,
                inject_read_before_edit_reminder: true,
                inject_progress_reminder_every: None,
                require_initial_scan: false,
                require_task_decomposition: true,
                require_thinking_prefix: false,
            },
            "dsf" => Self {
                max_iterations: 0,
                inject_read_before_edit_reminder: true,
                inject_progress_reminder_every: None,
                require_initial_scan: false,
                require_task_decomposition: true,
                require_thinking_prefix: false,
            },
            _ => Self {
                max_iterations: 0,
                inject_read_before_edit_reminder: false,
                inject_progress_reminder_every: None,
                require_initial_scan: false,
                require_task_decomposition: false,
                require_thinking_prefix: false,
            },
        }
    }
}

/// ─── Agent 事件（用于向前端流式推送）───

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEventKind {
    /// Agent 开始
    Started { mode: String, max_iterations: usize },
    /// 助手文本增量
    AssistantText { content: String },
    /// 工具调用请求（arguments 为 JSON 字符串，避免 Value 序列化问题）
    ToolCallRequested { id: String, name: String, arguments: String },
    /// 工具执行完成
    ToolCallExecuted { id: String, name: String, success: bool, output: String },
    /// 工具调用待审批（需分步确认模式）：前端应弹出审批卡片并调用 respond_tool_approval
    ToolApprovalRequired { approval_id: String, id: String, name: String, arguments: String },
    /// 审批结果（前端据此把工具卡片置为执行中/已拒绝）
    ToolApprovalResolved { id: String, approved: bool, output: String },
    /// 迭代计数
    Iteration { current: usize, max: usize },
    /// 循环结束（reasoning_content 供前端保存，thinking 模式回传必需）
    Done { content: String, total_iterations: usize, total_tool_calls: usize, reasoning_content: Option<String> },
    /// 错误
    Error { message: String },
    /// 文件系统变化（write/edit/delete 等成功后触发，前端应刷新文件树）
    FileChanged { reason: String },
    /// 上下文自动压缩发生（tool 调用链上提示）
    ContextCompressed { before_tokens: usize, after_tokens: usize },
    /// 实时上下文占用（每轮推送；tokens = 当前对话占用 Token 估算值）
    ContextUsage { tokens: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEvent {
    pub kind: AgentEventKind,
    pub ts: u64,
}

impl AgentEvent {
    pub fn new(kind: AgentEventKind) -> Self {
        Self {
            kind,
            ts: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
        }
    }
}

/// ─── Agent Loop 主入口 ───

pub struct AgentLoopInput {
    pub mode: String,
    pub user_message: String,
    pub history: Vec<Message>,
    pub context_paths: Vec<String>,
    pub working_dir: PathBuf,
    pub deepseek: std::sync::Arc<DeepSeekClient>,
    /// 命令层组装好的原生 System Prompt（模式基础提示 + 上下文文件内容）
    pub system_prompt: String,
    /// 本次运行的唯一 ID（撤回对话时按 run 回滚文件）
    pub run_id: String,
    /// 撤销日志存储
    pub undo_store: Arc<UndoStore>,
    /// 可选：覆盖步数上限（子智能体用；None = 使用模式默认 = 0 不限）
    pub max_iterations_override: Option<usize>,
    /// 可选：附加到 System Prompt 末尾的原装工作流编排内容
    /// （厂商引擎的 plan / skill / 检查清单等，由 workflow 模块注入）
    pub extra_preamble: Option<String>,
    /// 执行许可模式（step = 需分步确认；open = 全流程开放）
    pub approval_mode: ApprovalMode,
    /// 审批门（跨阶段/子智能体共享；open 模式为无操作放行）
    pub approval_gate: Arc<ApprovalGate>,
    /// 上下文窗口（Token）：用于计算占用比例与压缩触发线
    pub context_limit: usize,
    /// 上下文压缩模式：true = 自动（≥85% 自动压缩用户上下文，不清空对话）；false = 手动（仅提示）
    pub auto_compress: bool,
}

pub struct AgentLoopOutput {
    pub final_content: String,
    pub total_iterations: usize,
    pub total_tool_calls: usize,
    pub events: Vec<AgentEvent>,
    /// 本次运行唯一 ID
    pub run_id: String,
    /// 估算的上下文 Token 数（发送给模型前）
    pub context_tokens: usize,
    /// 上下文窗口（Token）
    pub context_limit: usize,
    /// 最终上下文占用比例（0.0 ~ 1.0）
    pub context_ratio: f64,
    /// 是否发生了上下文自动压缩
    pub compressed: bool,
}

/// ─── 运行 Agent Loop ───

pub async fn run_agent_loop<F>(
    input: AgentLoopInput,
    mut on_event: F,
) -> Result<AgentLoopOutput, String>
where
    F: FnMut(AgentEvent) + Send,
{
    let mut config = LoopConfig::for_mode(&input.mode);
    if let Some(m) = input.max_iterations_override {
        config.max_iterations = m;
    }
    let tools = ToolRegistry::new_with_undo(input.working_dir.clone(), input.run_id.clone(), input.undo_store.clone())
        .with_subagent_executor(make_subagent_executor(&input));
    let tool_schemas: Vec<ToolSchema> = ToolRegistry::schemas();

    // 记录执行前已存在的临时脚本，避免误删用户文件
    let existing_temp_files = snapshot_temp_py_files(&input.working_dir);

    let mut events: Vec<AgentEvent> = Vec::new();
    let emit = |ev: AgentEvent, sink: &mut Vec<AgentEvent>, cb: &mut dyn FnMut(AgentEvent)| {
        sink.push(ev.clone());
        cb(ev);
    };

    emit(
        AgentEvent::new(AgentEventKind::Started {
            mode: input.mode.clone(),
            max_iterations: config.max_iterations,
        }),
        &mut events,
        &mut on_event,
    );

    // 1. 使用命令层组装好的原生 System Prompt（模式基础提示 + 上下文文件内容；
    //    工作流编排内容由引擎经 extra_preamble 注入）
    let mut system_prompt = input.system_prompt.clone();

    // 注入模式特有的引擎循环规则
    system_prompt.push_str(&mode_loop_directives(&input.mode, &config));

    // 注入厂商原装工作流编排内容（workflow 引擎的 plan / skill / 检查清单等）
    if let Some(extra) = &input.extra_preamble {
        if !extra.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(extra);
        }
    }

    // 2. 初始化消息历史
    //    压缩策略（自动 / 手动）：
    //    - 自动：占用 ≥85% 时压缩"用户上下文"（历史对话），保留最近若干轮，不清空对话
    //    - 手动：只统计占用并上报，不自动压缩（前端提示用户手动压缩或清空对话）
    //    工具结果与当前用户消息配对必须原样保留，因此压缩点在每轮请求之前。
    let mut compressed = false;
    let mut history: Vec<Message> = input.history.clone();
    let mut compressor = ContextCompressor::new(CompressorConfig::with_limit(
        input.context_limit,
        input.auto_compress,
    ));

    // 上下文占用估算：System Prompt + 本次用户消息 + 历史对话 + 工具结果
    let base_tokens = ContextCompressor::estimate_tokens(&system_prompt);
    let mut last_reported_tokens = usize::MAX;

    // 先做一次起始压缩（历史本身已超阈值时）
    {
        let cm = messages_to_compressed(&history);
        if compressor.needs_compression(&cm) {
            let before_tokens = base_tokens + compressor.total_tokens(&cm);
            let comp = compressor.compress(&cm);
            let after_tokens = base_tokens + compressor.total_tokens(&comp);
            history = comp
                .into_iter()
                .map(|c| Message {
                    role: c.role.clone(),
                    content: c.content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                    r#type: c.role.clone(),
                })
                .collect();
            compressed = true;
            let ev = AgentEvent::new(AgentEventKind::ContextCompressed { before_tokens, after_tokens });
            events.push(ev.clone());
            on_event(ev);
        }
    }

    // 上下文占用在每轮循环前重新计算（见下方 ContextUsage 事件）

    let mut messages: Vec<Message> = Vec::new();
    messages.push(Message {
        role: "user".into(),
        content: input.user_message.clone(),
        tool_calls: None,
        tool_call_id: None,
        name: None,
        reasoning_content: None,
        r#type: "user".into(),
    });
    // 历史消息也加进去（如果 history 非空）
    for h in &history {
        let mut m = h.clone();
        // 兼容老格式：补全新字段
        if m.tool_calls.is_none() { m.tool_calls = None; }
        if m.tool_call_id.is_none() { m.tool_call_id = None; }
        if m.name.is_none() { m.name = None; }
        if m.r#type.is_empty() { m.r#type = m.role.clone(); }
        messages.push(m);
    }

    let mut final_content = String::new();
    let mut total_tool_calls = 0;
    let mut last_reasoning: Option<String> = None;

    // 3. 主循环（max_iterations = 0 表示不限步数：直到模型给出结论或出错才结束）
    let mut iter: usize = 0;
    loop {
        if config.max_iterations > 0 && iter >= config.max_iterations {
            break;
        }
        emit(
            AgentEvent::new(AgentEventKind::Iteration {
                current: iter + 1,
                max: config.max_iterations,
            }),
            &mut events,
            &mut on_event,
        );

        // ─── 上下文占用实时上报 + 阈值处理（自动压缩 / 手动提示）───
        // 后端只上报占用 Token；压缩模式与阈值提示由前端按 85% 规则决定并展示。
        {
            let usage = current_usage(base_tokens, &messages);
            if usage != last_reported_tokens {
                last_reported_tokens = usage;
                let ev = AgentEvent::new(AgentEventKind::ContextUsage { tokens: usage });
                events.push(ev.clone());
                on_event(ev);
            }
        }
        // 自动压缩模式：占用超阈值时压缩"用户上下文"（保留最近若干轮，不清空对话）
        if input.auto_compress {
            let cm = messages_to_compressed(&messages);
            if compressor.needs_compression(&cm) {
                let before_tokens = base_tokens + compressor.total_tokens(&cm);
                let comp = compressor.compress(&cm);
                let after_tokens = base_tokens + compressor.total_tokens(&comp);
                messages = comp
                    .into_iter()
                    .map(|c| Message {
                        role: c.role.clone(),
                        content: c.content.clone(),
                        tool_calls: None,
                        tool_call_id: None,
                        name: None,
                        reasoning_content: None,
                        r#type: c.role.clone(),
                    })
                    .collect();
                compressed = true;
                let ev = AgentEvent::new(AgentEventKind::ContextCompressed { before_tokens, after_tokens });
                events.push(ev.clone());
                on_event(ev);
                let ev = AgentEvent::new(AgentEventKind::ContextUsage { tokens: after_tokens });
                events.push(ev.clone());
                on_event(ev);
                last_reported_tokens = after_tokens;
            }
        }

        // Kimi 模式：每步注入"先思考"提醒
        if config.require_thinking_prefix && iter > 0 {
            messages.push(Message {
                role: "user".into(),
                content: "[Reminder] Before your next action, briefly explain your reasoning (1-2 sentences).".into(),
                tool_calls: None,
                tool_call_id: None,
                name: None,
                reasoning_content: None,
                r#type: "user".into(),
            });
        }

        // GPT 模式：每 N 步注入目标检查
        if let Some(every) = config.inject_progress_reminder_every {
            if iter > 0 && iter % every == 0 {
                messages.push(Message {
                    role: "user".into(),
                    content: format!(
                        "[Progress Check] You've completed {} iterations. Review:\n\
                         - What's the original goal?\n\
                         - What have you completed?\n\
                         - What's the next concrete step?\n\
                         If you've completed the goal, output the final answer and STOP.",
                        iter
                    ),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                    reasoning_content: None,
                    r#type: "user".into(),
                });
            }
        }

        // 调用 DeepSeek
        let resp = input.deepseek
            .chat_with_tools(&system_prompt, &messages, Some(&tool_schemas))
            .await;

        let response = match resp {
            Ok(r) => r,
            Err(e) => {
                cleanup_temp_py_files(&input.working_dir, &existing_temp_files);
                let ev = AgentEvent::new(AgentEventKind::Error { message: e.clone() });
                events.push(ev.clone());
                on_event(ev);
                return Err(e);
            }
        };

        let choice = match response.choices.first() {
            Some(c) => c,
            None => {
                cleanup_temp_py_files(&input.working_dir, &existing_temp_files);
                let ev = AgentEvent::new(AgentEventKind::Error {
                    message: "No choices in response".into(),
                });
                events.push(ev.clone());
                on_event(ev);
                return Err("No choices in response".into());
            }
        };

        let assistant_msg = &choice.message;
        final_content = assistant_msg.content.clone();
        last_reasoning = assistant_msg.reasoning_content.clone();

        // DSML 双源检测：DeepSeek 推理模型会把工具调用写成 content 或 reasoning_content 里的 DSML 文本
        let dsml_source = format!(
            "{}{}",
            assistant_msg.content,
            assistant_msg.reasoning_content.clone().unwrap_or_default()
        );
        let dsml_calls = parse_dsml_tool_calls(&dsml_source);
        let has_dsml = !dsml_calls.is_empty();

        // 推送助手文本（DSML 属于工具调用文本，不直接展示）
        if !assistant_msg.content.is_empty() && !has_dsml {
            let ev = AgentEvent::new(AgentEventKind::AssistantText {
                content: assistant_msg.content.clone(),
            });
            events.push(ev.clone());
            on_event(ev);
        }

        // 工具调用：结构化优先；为空时用 DSML 解析结果
        let mut tool_calls = assistant_msg.tool_calls.clone().unwrap_or_default();

        // 把助手消息加入历史，确保 type 字段非空；
        // DSML 调用改写为规范 tool_calls 形态，保证后续请求的 tool→tool_calls 链完整
        let mut captured_assistant = assistant_msg.clone();
        if captured_assistant.r#type.is_empty() { captured_assistant.r#type = captured_assistant.role.clone(); }
        if tool_calls.is_empty() && has_dsml {
            tool_calls = dsml_calls;
            captured_assistant.content.clear();
            captured_assistant.tool_calls = Some(tool_calls.clone());
        }
        messages.push(captured_assistant);
        if tool_calls.is_empty() {
            // 没有工具调用 = 任务完成
            cleanup_temp_py_files(&input.working_dir, &existing_temp_files);
            let ev = AgentEvent::new(AgentEventKind::Done {
                content: final_content.clone(),
                total_iterations: iter + 1,
                total_tool_calls,
                reasoning_content: last_reasoning.clone(),
            });
            events.push(ev.clone());
            on_event(ev);
            return Ok(AgentLoopOutput {
                final_content,
                total_iterations: iter + 1,
                total_tool_calls,
                events,
                run_id: input.run_id.clone(),
                context_tokens: current_usage(base_tokens, &messages),
                context_limit: compressor.config().max_tokens,
                context_ratio: compressor.usage_ratio(&messages_to_compressed(&messages)),
                compressed,
            });
        }

        // 执行工具调用
        for call in &tool_calls {
            total_tool_calls += 1;

            // 解析 arguments（可能是字符串）
            let call_with_parsed_args = normalize_tool_call(call);

            // 通知前端（arguments 转为 JSON 字符串，避免 Value 序列化问题）
            let ev = AgentEvent::new(AgentEventKind::ToolCallRequested {
                id: call_with_parsed_args.id.clone(),
                name: call_with_parsed_args.function.name.clone(),
                arguments: call_with_parsed_args.function.arguments.to_string(),
            });
            events.push(ev.clone());
            on_event(ev);

            // Claude 模式：Edit/Write 前必须 Read
            if config.inject_read_before_edit_reminder {
                let name = &call_with_parsed_args.function.name;
                if (name == "edit" || name == "write")
                    && !has_recent_read(&messages, &call_with_parsed_args.function.arguments, name == "write")
                {
                    let warning = format!(
                        "[System] You must call `read` on the target file BEFORE `{}`. \
                         Edit/Write without Read is a HARD VIOLATION of the read-before-edit protocol. \
                         Please read the file first.",
                        name
                    );
                    let warning_str = warning.clone();
                    messages.push(tool_result_message(
                        &call_with_parsed_args.id,
                        &call_with_parsed_args.function.name,
                        &ToolResult { success: false, output: warning, data: None },
                    ));
                    let ev = AgentEvent::new(AgentEventKind::ToolCallExecuted {
                        id: call_with_parsed_args.id.clone(),
                        name: call_with_parsed_args.function.name.clone(),
                        success: false,
                        output: warning_str,
                    });
                    events.push(ev.clone());
                    on_event(ev);
                    continue;
                }
            }

            // ─── 执行许可门（需分步确认 / 全流程开放，对标 Harness 审批）───
            if input.approval_mode == ApprovalMode::StepConfirm {
                let approval_id = format!("ap_{}_{}", input.run_id, total_tool_calls);
                let args_str = call_with_parsed_args.function.arguments.to_string();
                let allowed = input
                    .approval_gate
                    .request(
                        &approval_id,
                        &call_with_parsed_args.id,
                        &call_with_parsed_args.function.name,
                        &args_str,
                        input.approval_mode,
                    )
                    .await?;
                // 通知前端状态流转（执行中 / 已拒绝）
                let resolved = if allowed {
                    "✅ 用户已批准，开始执行".to_string()
                } else {
                    "⛔ 用户拒绝执行该工具调用".to_string()
                };
                let ev = AgentEvent::new(AgentEventKind::ToolApprovalResolved {
                    id: call_with_parsed_args.id.clone(),
                    approved: allowed,
                    output: resolved.clone(),
                });
                events.push(ev.clone());
                on_event(ev);

                if !allowed {
                    // 拒绝：以工具失败结果回灌，让模型换方案或与用户确认
                    let msg = "⛔ 用户拒绝执行该工具调用（需分步确认模式）。\
                               请改用不需要该工具的方案继续，或先向用户确认后再调用。"
                        .to_string();
                    messages.push(tool_result_message(
                        &call_with_parsed_args.id,
                        &call_with_parsed_args.function.name,
                        &ToolResult { success: false, output: msg.clone(), data: None },
                    ));
                    let ev = AgentEvent::new(AgentEventKind::ToolCallExecuted {
                        id: call_with_parsed_args.id.clone(),
                        name: call_with_parsed_args.function.name.clone(),
                        success: false,
                        output: msg,
                    });
                    events.push(ev.clone());
                    on_event(ev);
                    continue;
                }
            }

            // 真正执行
            let result = tools.execute(&call_with_parsed_args).await;

            let ev = AgentEvent::new(AgentEventKind::ToolCallExecuted {
                id: call_with_parsed_args.id.clone(),
                name: call_with_parsed_args.function.name.clone(),
                success: result.success,
                output: truncate_for_display(&result.output, 2000),
            });
            events.push(ev.clone());
            on_event(ev);

            // 写入类工具成功后通知前端刷新文件树
            // （write/edit 必触发；bash/delete 在 result.success 时也触发，
            //  因为脚本可能生成文件，delete 必然改了文件树）
            if result.success {
                let name = call_with_parsed_args.function.name.as_str();
                if matches!(name, "write" | "edit" | "delete" | "bash") {
                    let ev = AgentEvent::new(AgentEventKind::FileChanged {
                        reason: name.to_string(),
                    });
                    events.push(ev.clone());
                    on_event(ev);
                }
            }

            // 把工具结果加入消息
            messages.push(tool_result_message(
                &call_with_parsed_args.id,
                &call_with_parsed_args.function.name,
                &result,
            ));
        }
        iter += 1;
    }

    // 达到最大迭代（设置过步数上限时才可能走到这里）
    cleanup_temp_py_files(&input.working_dir, &existing_temp_files);
    let ev = AgentEvent::new(AgentEventKind::Done {
        content: final_content.clone(),
        total_iterations: config.max_iterations,
        total_tool_calls,
        reasoning_content: last_reasoning.clone(),
    });
    events.push(ev.clone());
    on_event(ev);

    Ok(AgentLoopOutput {
        final_content,
        total_iterations: config.max_iterations,
        total_tool_calls,
        events,
        run_id: input.run_id.clone(),
        context_tokens: current_usage(base_tokens, &messages),
        context_limit: compressor.config().max_tokens,
        context_ratio: compressor.usage_ratio(&messages_to_compressed(&messages)),
        compressed,
    })
}

// ════════════════════════════════════════════════════════
// 辅助函数
// ════════════════════════════════════════════════════════

/// 通用消息 → 可压缩消息（带 Token 估算）
fn messages_to_compressed(ms: &[Message]) -> Vec<CompressedMessage> {
    ms.iter()
        .map(|m| CompressedMessage {
            role: m.role.clone(),
            content: m.content.clone(),
            estimated_tokens: ContextCompressor::estimate_tokens(&m.content),
        })
        .collect()
}

/// 当前上下文占用 = 基础（System Prompt）+ 全部对话消息
fn current_usage(base: usize, ms: &[Message]) -> usize {
    base + ms
        .iter()
        .map(|m| ContextCompressor::estimate_tokens(&m.content))
        .sum::<usize>()
}

fn mode_loop_directives(mode: &str, cfg: &LoopConfig) -> String {
    let mut s = String::new();
    s.push_str("\n\n## Agent Loop Directives (per engine)\n");

    // 通用规则（所有引擎适用）
    s.push_str("### Universal rules (apply to all engines)\n");
    s.push_str("- **Whole-file writes**: A single `write` call may carry the ENTIRE file content (up to ~60000 characters). Prefer one `write` per file instead of chunking.\n");
    s.push_str("- **Batch tools (efficiency)**: For multi-file changes, use ONE `batch_write`/`batch_edit` call instead of many separate write/edit calls.\n");
    s.push_str("- **Sub-agents (parallelism)**: Delegate independent subtasks (e.g. separate modules/files) to `subagents` — 1-4 sub-agents run in parallel with full tool access and return their own conclusions. This cuts total wall-clock and main-loop steps.\n");
    s.push_str("- **Chunk long bash commands**: If a `bash` command string is long, split it across multiple bash calls.\n");
    s.push_str("- **Tool result truncation**: If a tool returns a long output, you can use `read` with `offset`/`limit` or `grep` to inspect specific parts instead of dumping the whole thing again.\n");
    s.push_str("\n");

    // 注入可用运行时信息
    s.push_str("### Runtime environment\n");
    s.push_str(&detect_runtimes());
    s.push_str("- Use `check_runtime` tool to verify a specific runtime before executing code in that language.\n");
    s.push_str("- If a runtime is missing from the list above, DO NOT attempt to `bash` commands that require it (e.g. `node`, `java`, `gcc`, `go`, `cargo`, `dotnet`, `php`).\n");
    s.push_str("- Instead, ask the user to install the missing runtime, or fall back to an available language.\n");
    s.push_str("- Python is ALWAYS available (bundled with the app at `python/python.exe` relative to working dir). Use `python/python.exe` or just `python` in bash commands.\n");
    s.push_str("- Use Python (bundled) for data analysis, PDF/Excel processing, and quick scripts.\n");
    s.push_str("- For installing extra Python packages: use `python -m pip install <pkg>` in bash. Pre-installed: pymupdf (fitz).\n");
    s.push_str("\n");

    match mode {
        "dsh" => {
            s.push_str("You are running the **DSH** mode (DeepSeek Harness native agent loop):\n");
            s.push_str("- Work autonomously through the agent loop; favor tool calls over guesswork.\n");
            s.push_str("- For multi-step tasks, FIRST call `todo_write` to plan.\n");
            s.push_str("- For any build/test/run, use `bash`.\n");
            s.push_str("- For unfamiliar codebases, FIRST call `glob` + `grep` to build a mental map.\n");
            s.push_str("- Process large code in chunks: `read` with offset/limit, then synthesize.\n");
            s.push_str("- After completion, use `grep` to verify no leftover debug code.\n");
            s.push_str("- If a tool call fails twice, switch approach and explain why.\n");
        }
        "dsk" => {
            s.push_str("You are running the **DSK** mode (Kimi K3 engine — kimi-code):\n");
            s.push_str("- Plan → Generate → Review → Refine.\n");
            s.push_str("- Before generating code, briefly state your plan in the response.\n");
            s.push_str("- Every 5 iterations you'll be asked to check progress against the goal.\n");
            s.push_str("- Prefer minimal, runnable iterations. Verify after each step.\n");
        }
        "dsa" => {
            s.push_str("You are running the **DSA** mode (GPT-6 Astra engine — astra-advisor):\n");
            s.push_str("- You are the 总指挥 (architect & acceptance owner). Break the task into concrete bounded deliverables FIRST (`todo_write`), then delegate independent parts to parallel subagents.\n");
            s.push_str("- Every delegation must announce: bounded responsibility + selection reason. Do not duplicate parent work in a subagent.\n");
            s.push_str("- Fail closed: if a tool/model/effort is unavailable, report the limitation instead of silently substituting.\n");
            s.push_str("- After substantial work, inspect the full diff and re-run the requested checks before accepting.\n");
            s.push_str("- A read-only reviewer returns ship/fix-first/rethink only after your own verification.\n");
        }
        "dsf" => {
            s.push_str("You are running the **DSF** mode (Claude Fable 5.1 engine — fable-orchestrator + fablewright):\n");
            s.push_str("- Post the call sheet before the first task tool: route (solo/delegate/audit/full/ensemble) / cast / reader / independence / risk. `solo` is the default — delegation must pass a test.\n");
            s.push_str("- You own intent, architecture, interfaces, decomposition, verification and acceptance. Delegated work substitutes for your work, never duplicates it.\n");
            s.push_str("- Evidence outranks assertion: inspect the real diff and re-run the checks yourself before any of it counts.\n");
            s.push_str("- A fresh reader returns one verdict (ship/fix-first/rethink) and never fixes its own findings.\n");
            s.push_str("- Fail closed: never substitute a missing pin or silently downgrade a route.\n");
        }
        _ => {}
    }

    if cfg.max_iterations > 0 {
        s.push_str(&format!("\nMax iterations for this run: {}.\n", cfg.max_iterations));
    }
    s
}

/// 规范化路径：统一反斜杠为正斜杠，并 lowercase
fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").to_lowercase()
}

/// 检查最近是否对目标文件调用过 read（Claude 模式强制）
fn has_recent_read(messages: &[Message], call_args: &Value, is_write: bool) -> bool {
    // Write 到新文件不需要先 Read
    if is_write {
        if let Some(path) = call_args.get("file_path").and_then(|v| v.as_str()) {
            if !std::path::Path::new(path).exists() {
                return true;
            }
        }
    }
    let target_path = normalize_path(
        call_args.get("file_path").and_then(|v| v.as_str()).unwrap_or("")
    );
    if target_path.is_empty() { return false; }
    // 在最近 20 条消息中查找 read 调用且 file_path 相同（路径规范化后比较）
    for m in messages.iter().rev().take(20) {
        if m.role != "tool" { continue; }
        // tool 消息的 content 包含 "File: <path>"
        if let Some(idx) = m.content.find("File: ") {
            let after = &m.content[idx + 6..];
            let path_str = after.lines().next().unwrap_or("").trim();
            if normalize_path(path_str) == target_path {
                return true;
            }
        }
    }
    false
}

fn normalize_tool_call(call: &ToolCall) -> ToolCall {
    // DeepSeek 返回的 arguments 是字符串，需要解析
    if call.function.arguments.is_string() {
        let s = call.function.arguments.as_str().unwrap_or("{}");
        match serde_json::from_str::<Value>(s) {
            Ok(v) => ToolCall {
                id: call.id.clone(),
                kind: call.kind.clone(),
                function: crate::ai::tools::ToolFunction {
                    name: call.function.name.clone(),
                    arguments: v,
                },
            },
            Err(_) => call.clone(),
        }
    } else {
        call.clone()
    }
}

fn tool_result_message(id: &str, name: &str, result: &ToolResult) -> Message {
    let content = if result.success {
        result.output.clone()
    } else {
        format!("[ERROR] {}", result.output)
    };
    Message {
        role: "tool".into(),
        content,
        tool_calls: None,
        tool_call_id: Some(id.to_string()),
        name: Some(name.to_string()),
        reasoning_content: None,
        r#type: "tool".into(),
    }
}

fn truncate_for_display(s: &str, max: usize) -> String {
    if s.len() <= max { return s.to_string(); }
    let cut = (max as f64 * 0.8) as usize;
    let safe_cut = s.char_indices()
        .find(|(i, _)| *i >= cut)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    format!("{}... [truncated, {} total chars]", &s[..safe_cut], s.len())
}

/// 扫描工作区中已存在的下划线开头 Python 临时脚本
fn snapshot_temp_py_files(dir: &Path) -> HashSet<PathBuf> {
    let mut set = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('_') && name.ends_with(".py") {
                        set.insert(path);
                    }
                }
            }
        }
    }
    set
}

/// 删除本次 Agent 运行期间新增的下划线开头 Python 临时脚本
fn cleanup_temp_py_files(dir: &Path, existing: &HashSet<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && !existing.contains(&path) {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with('_') && name.ends_with(".py") {
                        let _ = std::fs::remove_file(&path);
                    }
                }
            }
        }
    }
}

// ════════════════════════════════════════════════════════
// DSML 文本工具调用解析（DeepSeek 推理模型把调用写成
// <｜DSML｜tool_calls> 形式的纯文本，位于 content 或 reasoning_content）
// ════════════════════════════════════════════════════════

/// 真实标签为 `<｜DSML｜tag>`（｜ = U+FF5C），归一化去掉「｜DSML｜」后即为标准 XML 形状
fn parse_dsml_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls: Vec<ToolCall> = Vec::new();
    if text.is_empty() {
        return calls;
    }
    let normalized = text.replace("\u{FF5C}DSML\u{FF5C}", "");
    let mut rest = normalized.as_str();
    while let Some(open) = rest.find("<tool_calls>") {
        let after_open = &rest[open + "<tool_calls>".len()..];
        let Some(close) = after_open.find("</tool_calls>") else {
            break;
        };
        let block = &after_open[..close];
        rest = &after_open[close + "</tool_calls>".len()..];

        let mut b = block;
        while let Some(inv_pos) = b.find("<invoke") {
            let inv_after = &b[inv_pos..];
            let Some(inv_close) = inv_after.find("</invoke>") else {
                break;
            };
            let inv = &inv_after[..inv_close + "</invoke>".len()];
            b = &inv_after[inv_close + "</invoke>".len()..];

            let Some(name) = dsml_attribute(inv, "name") else {
                continue;
            };
            let mut args = serde_json::Map::new();
            let mut p = inv;
            while let Some(param_pos) = p.find("<parameter") {
                let param_after = &p[param_pos..];
                let Some(param_close) = param_after.find("</parameter>") else {
                    break;
                };
                let param = &param_after[..param_close + "</parameter>".len()];
                p = &param_after[param_close + "</parameter>".len()..];
                let Some(pname) = dsml_attribute(param, "name") else {
                    continue;
                };
                let is_str = param.contains("string=\"true\"");
                let value = dsml_param_body(param);
                let v = if is_str {
                    Value::String(value)
                } else {
                    serde_json::from_str::<Value>(&value).unwrap_or(Value::String(value))
                };
                args.insert(pname, v);
            }
            calls.push(ToolCall {
                id: format!("dsml_{}", calls.len()),
                kind: "function".into(),
                function: ToolFunction { name, arguments: Value::Object(args) },
            });
        }
    }
    calls
}

/// 从标签头提取 `attr="..."` 的值
fn dsml_attribute(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{}=\"", attr);
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 提取 `<parameter ...>BODY</parameter>` 的 BODY
fn dsml_param_body(param: &str) -> String {
    match param.find('>') {
        Some(gt) => param[gt + 1..].trim_end_matches("</parameter>").to_string(),
        None => String::new(),
    }
}

// ════════════════════════════════════════════════════════
// 子智能体执行器：把 subagents 工具的指令交给嵌套 Agent Loop
// （独立上下文 + 全套工具，最多 40 轮；事件静默，结论回传主循环）
// ════════════════════════════════════════════════════════

fn make_subagent_executor(input: &AgentLoopInput) -> SubagentExecutor {
    let deepseek = input.deepseek.clone();
    let system_prompt = input.system_prompt.clone();
    let mode = input.mode.clone();
    let working_dir = input.working_dir.clone();
    let undo_store = input.undo_store.clone();
    let run_id = input.run_id.clone();
    let approval_mode = input.approval_mode;
    let approval_gate = input.approval_gate.clone();
    let context_limit = input.context_limit;
    let auto_compress = input.auto_compress;
    Arc::new(move |instruction: String| -> futures_util::future::BoxFuture<'static, Result<String, String>> {
        let deepseek = deepseek.clone();
        let system_prompt = system_prompt.clone();
        let mode = mode.clone();
        let working_dir = working_dir.clone();
        let undo_store = undo_store.clone();
        let run_id = run_id.clone();
        let approval_gate = approval_gate.clone();
        Box::pin(async move {
            let sub_input = AgentLoopInput {
                mode,
                user_message: instruction,
                history: Vec::new(),
                context_paths: Vec::new(),
                working_dir,
                deepseek,
                system_prompt,
                // 子智能体的文件变更记入主 run 的撤销日志（撤回对话时一并回滚）
                run_id,
                undo_store,
                // 子智能体带步数上限，避免嵌套任务失控
                max_iterations_override: Some(40),
                extra_preamble: None,
                // 子智能体同样受执行许可门约束（step 模式下每个工具调用都需要批准）
                approval_mode,
                approval_gate,
                // 子智能体沿用主循环的上下文窗口与压缩模式
                context_limit,
                auto_compress,
            };
            let output = run_agent_loop(sub_input, |_| {}).await;
            match output {
                Ok(o) => Ok(o.final_content),
                Err(e) => Err(e),
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dsml_parse_content_and_reasoning() {
        // 真实 DSML 格式：<｜DSML｜tool_calls>（｜ = U+FF5C），带字符串/非字符串参数
        let text = "<\u{FF5C}DSML\u{FF5C}tool_calls>\n\
                    <\u{FF5C}DSML\u{FF5C}invoke name=\"read\">\n\
                    <\u{FF5C}DSML\u{FF5C}parameter name=\"file_path\" string=\"true\">d:\\a\\b.js</\u{FF5C}DSML\u{FF5C}parameter>\n\
                    <\u{FF5C}DSML\u{FF5C}parameter name=\"offset\" string=\"false\">340</\u{FF5C}DSML\u{FF5C}parameter>\n\
                    <\u{FF5C}DSML\u{FF5C}parameter name=\"limit\" string=\"false\">135</\u{FF5C}DSML\u{FF5C}parameter>\n\
                    </\u{FF5C}DSML\u{FF5C}invoke>\n\
                    </\u{FF5C}DSML\u{FF5C}tool_calls>";
        let calls = parse_dsml_tool_calls(text);
        assert_eq!(calls.len(), 1, "应解析出 1 个调用");
        assert_eq!(calls[0].function.name, "read");
        assert_eq!(calls[0].function.arguments["file_path"], json!("d:\\a\\b.js"));
        assert_eq!(calls[0].function.arguments["offset"], json!(340));
        assert_eq!(calls[0].function.arguments["limit"], json!(135));
    }

    #[test]
    fn dsml_parse_empty_and_garbage() {
        assert!(parse_dsml_tool_calls("").is_empty());
        assert!(parse_dsml_tool_calls("plain text no markup").is_empty());
        // 非法 JSON 参数值 → 回退为字符串
        let text = "<\u{FF5C}DSML\u{FF5C}tool_calls><\u{FF5C}DSML\u{FF5C}invoke name=\"write\">\
                    <\u{FF5C}DSML\u{FF5C}parameter name=\"content\" string=\"false\">{not json</\u{FF5C}DSML\u{FF5C}parameter>\
                    </\u{FF5C}DSML\u{FF5C}invoke></\u{FF5C}DSML\u{FF5C}tool_calls>";
        let calls = parse_dsml_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function.arguments["content"], json!("{not json"));
    }
}

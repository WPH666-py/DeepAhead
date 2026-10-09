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
    /// 工具仍在执行（心跳：每 ~20s 一次，elapsed_secs = 已耗时秒数）
    ///
    /// 存在的意义：工具（尤其是 read_image / bash）可能跑几分钟，
    /// 期间若没有任何事件，界面只能一直显示「思考中…」，用户无法判断是死是活。
    ToolProgress { id: String, name: String, elapsed_secs: u64 },
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
    /// 回合末裁决卡片（本轮有工具被硬门拦下时推送；前端据此渲染 ✅/❌ 卡片）
    TurnCard { card: Value },
    /// 文本审计命中（助手文本无法阻断，只审计 + 注入纠正）
    TextAudit { hits: Value, injection: String },
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
    /// 长期记忆协议配置（移植自 dsh-memory-protocol）
    pub memory: crate::ai::memory::MemoryConfig,
    /// 单个工具调用的最长执行时间（秒）；0 = 不限制。
    /// 兜底用：任何工具都不该让一整轮 Agent 永久卡死。
    pub tool_timeout_secs: u64,
}

/// 单个工具调用的默认上限（秒）。10 分钟足够跑完 OCR / 模型下载 / 长脚本，
/// 又能在"某个工具真的挂住"时把控制权交还给用户。
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 600;

/// 工具执行期间的心跳间隔（秒）
const TOOL_HEARTBEAT_SECS: u64 = 20;

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

    crate::ai::runtime_log::info(
        "agent",
        &format!(
            "Agent 循环开始 run={} mode={} 工作目录={} 执行许可={}",
            input.run_id,
            input.mode.to_uppercase(),
            input.working_dir.display(),
            input.approval_mode.describe()
        ),
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

    // ─── 长期记忆协议：轮首 weave（对齐上游 agent/pre-step）───
    // 每轮先查阅长期记忆，把检索结果注入系统提示；
    // 「成功但为空」同样视为已 weave（满足门），「失败」不满足门。
    let mut memory_weaved_this_turn = false;
    let mut memory_available = true;
    if input.memory.enabled && input.memory.inject_weave {
        match crate::ai::memory::weave(&input.user_message, &input.run_id, None) {
            Ok(ctx) => {
                memory_weaved_this_turn = true;
                if !ctx.trim().is_empty() {
                    system_prompt.push_str("\n\n## 长期记忆（本轮已自动查阅）\n");
                    system_prompt.push_str(crate::ai::memory::MEMORY_INTRO);
                    system_prompt.push('\n');
                    system_prompt.push_str(&ctx);
                }
            }
            Err(e) => {
                // 失败开放：记忆后端不可用时不阻塞主流程，只降级
                memory_available = false;
                let note = format!(
                    "\n\n## 长期记忆\n（记忆后端不可用，本轮已按「失败开放」降级继续：{}）",
                    e
                );
                system_prompt.push_str(&note);
            }
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
    // ─── 费用统计累计 + 规则引擎运行时状态 ───
    let mut cost_input: usize = 0;
    let mut cost_output: usize = 0;
    let mut cost_cache_hit: usize = 0;
    let mut cost_cache_miss: usize = 0;
    let mut cost_reasoning: usize = 0;
    let mut rule_state = crate::ai::rules_engine::RuleEngineState::default();
    rule_state.real_user_seen = true;
    rule_state.user_text = input.user_message.clone();
    rule_state.has_execute_clause = crate::ai::rules_engine::has_execute_clause(&input.user_message);
    let rule_cfg = crate::ai::rules_engine::get_config();
    // 执行许可档位是规则引擎的总闸：
    //   - 需逐步确认 / 仅确认风险操作 → 规则引擎全开（硬门 + 裁决卡片）
    //   - 全流程开放 → 规则引擎所有开关全关，所有操作**永久放行**
    let guard_enabled = rule_cfg.enabled && input.approval_mode.rule_engine_enabled();
    let turn_card_on = rule_cfg.turn_card_enabled && input.approval_mode.turn_card_enabled();
    // 把界面设置的 unlock / bypass 窗口同步给本次运行（运行期可被 /guard 改动）
    let mut rule_cfg = rule_cfg;
    rule_cfg.unlock_until = rule_cfg.unlock_until.max(0);
    // 回合末裁决卡片的被拦记录（对齐 dsh-rule-engine-client 的卡片契约）
    let mut turn_blocks: Vec<crate::ai::rules_engine::TurnCardBlock> = Vec::new();
    // 文本审计状态（本轮只读调用数 / 是否见过验证动作）
    let mut audit_state = crate::ai::rules_engine::TextAuditState::default();
    // 任务契约（由全局配置装载；armed 时生效）
    let mut contract = crate::ai::rules_engine::TaskContract {
        armed: rule_cfg.task_contract_enabled,
        level: if rule_cfg.task_contract_enabled { "guard".into() } else { "off".into() },
        ..Default::default()
    };

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

        // ─── 费用统计：累计本次迭代的 token 用量 ───
        {
            let u = &response.usage;
            let hit = u.cache_hit_tokens as usize;
            let miss = if u.cache_miss_tokens > 0 {
                u.cache_miss_tokens as usize
            } else {
                (u.prompt_tokens as usize).saturating_sub(hit)
            };
            cost_cache_hit += hit;
            cost_cache_miss += miss;
            cost_output += u.completion_tokens as usize;
            cost_reasoning += u.reasoning_tokens as usize;
            cost_input += u.prompt_tokens as usize;
        }

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
            crate::ai::runtime_log::info(
                "agent",
                &format!(
                    "Agent 循环结束 run={} 步数={} 工具调用={} 被拦={}",
                    input.run_id,
                    iter + 1,
                    total_tool_calls,
                    turn_blocks.len()
                ),
            );
            let ev = AgentEvent::new(AgentEventKind::Done {
                content: final_content.clone(),
                total_iterations: iter + 1,
                total_tool_calls,
                reasoning_content: last_reasoning.clone(),
            });
            events.push(ev.clone());
            on_event(ev);
            memory_auto_ingest(&input.memory, &input.run_id, &input.user_message, &final_content);
            emit_turn_card(turn_card_on, &input.run_id, &input.user_message, &turn_blocks, &mut on_event, &mut events);
            emit_text_audit(&rule_cfg, &input.user_message, &final_content, &mut audit_state, &mut on_event, &mut events);
            cost_record(
                &input.mode, &input.run_id,
                cost_input, cost_output, cost_cache_hit, cost_cache_miss, cost_reasoning,
            );
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

            // ─── 长期记忆硬门（对齐上游 tools/pre-execute）───
            // 未 weave 就调用非记忆工具 → 拒绝。记忆工具自身永远放行（否则死锁）。
            {
                let name = call_with_parsed_args.function.name.as_str();
                if name == "memory_weave" {
                    memory_weaved_this_turn = true;
                }
                let exempt = crate::ai::memory::is_memory_tool(name)
                    || input.memory.allowlist.iter().any(|a| a == name);
                let should_gate = input.memory.enabled
                    && input.memory.enforce_weave
                    && !exempt
                    && !memory_weaved_this_turn;
                // 失败开放：后端不可用时放行（避免把应用锁死）
                let deny = if should_gate && !memory_available && input.memory.fail_open {
                    false
                } else {
                    should_gate
                };
                if deny {
                    let reason = crate::ai::memory::MEMORY_DENY_REASON.to_string();
                    messages.push(tool_result_message(
                        &call_with_parsed_args.id,
                        name,
                        &ToolResult { success: false, output: reason.clone(), data: None },
                    ));
                    let ev = AgentEvent::new(AgentEventKind::ToolCallExecuted {
                        id: call_with_parsed_args.id.clone(),
                        name: name.to_string(),
                        success: false,
                        output: reason,
                    });
                    events.push(ev.clone());
                    on_event(ev);
                    continue;
                }
            }

            // ─── 规则引擎硬门（移植自 dsh-rule-engine）───
            // 与上游一致：拒绝发生在工具执行之前，理由写明补救动作，并记入审计账本。
            // 用户在裁决卡片上选择 ❌ 放行后，同一次调用会命中一次性放行，直接通过。
            if guard_enabled {
                let now = chrono::Utc::now().timestamp();
                let (op_type, op_path) = crate::ai::rules_engine::operation_of(
                    &call_with_parsed_args.function.name,
                    &call_with_parsed_args.function.arguments,
                );
                let pre_approved = crate::ai::pending_guard::consume_allow(
                    &call_with_parsed_args.function.name,
                    &op_type,
                    &op_path,
                );
                let decision = if pre_approved {
                    crate::ai::rules_engine::GuardDecision {
                        allow: true,
                        rule_id: "__card-allow".into(),
                        kind: "card-allow".into(),
                        reason: "用户在回合裁决卡片上放行（❌），一次性放行已消费".into(),
                        err_id: String::new(),
                    }
                } else {
                    crate::ai::rules_engine::guard_decision(
                        &rule_cfg,
                        &mut rule_state,
                        &call_with_parsed_args.function.name,
                        &call_with_parsed_args.function.arguments,
                        now,
                    )
                };
                crate::ai::rules_engine::audit(
                    if decision.allow { "allow" } else { "deny" },
                    &decision.rule_id,
                    &call_with_parsed_args.function.name,
                    &decision.reason,
                    &decision.err_id,
                );
                if !decision.allow {
                    // 登记待决记录：前端出卡片，用户 ❌ 放行 / ✅ 拦截
                    let block_index = turn_blocks.len();
                    crate::ai::pending_guard::record(
                        &input.run_id,
                        block_index,
                        &call_with_parsed_args.function.name,
                        &op_type,
                        &op_path,
                        &call_with_parsed_args.function.arguments.to_string(),
                        &decision.reason,
                        &decision.err_id,
                    );
                    turn_blocks.push(crate::ai::rules_engine::make_card_block(
                        block_index,
                        &call_with_parsed_args.function.name,
                        &call_with_parsed_args.function.arguments,
                        &decision,
                        &format!("工具 {} 被规则 {} 拦下", call_with_parsed_args.function.name, decision.rule_id),
                    ));
                    crate::ai::runtime_log::info(
                        "rules",
                        &format!(
                            "硬门拦下 run={} #{} tool={} 规则={}：{}",
                            input.run_id,
                            block_index,
                            call_with_parsed_args.function.name,
                            decision.rule_id,
                            decision.reason
                        ),
                    );
                    let reason = decision.reason.clone();
                    messages.push(tool_result_message(
                        &call_with_parsed_args.id,
                        &call_with_parsed_args.function.name,
                        &ToolResult { success: false, output: reason.clone(), data: None },
                    ));
                    let ev = AgentEvent::new(AgentEventKind::ToolCallExecuted {
                        id: call_with_parsed_args.id.clone(),
                        name: call_with_parsed_args.function.name.clone(),
                        success: false,
                        output: reason,
                    });
                    events.push(ev.clone());
                    on_event(ev);
                    continue;
                }
            }

            // ─── 任务契约（移植自 dsh-rule-engine contract.js）───
            if contract.armed {
                let decision = crate::ai::rules_engine::decide_contract_action(
                    &mut contract,
                    &call_with_parsed_args.function.name,
                    &call_with_parsed_args.function.arguments,
                );
                if !decision.allow {
                    let reason = format!(
                        "[guardian:contract] {}（{}）",
                        decision.message, decision.reason_code
                    );
                    audit_contract(&call_with_parsed_args.function.name, &reason);
                    turn_blocks.push(crate::ai::rules_engine::make_card_block(
                        turn_blocks.len(),
                        &call_with_parsed_args.function.name,
                        &call_with_parsed_args.function.arguments,
                        &crate::ai::rules_engine::GuardDecision {
                            allow: false,
                            rule_id: "__contract".into(),
                            kind: "task-contract-deny".into(),
                            reason: reason.clone(),
                            err_id: String::new(),
                        },
                        "任务契约拒绝",
                    ));
                    messages.push(tool_result_message(
                        &call_with_parsed_args.id,
                        &call_with_parsed_args.function.name,
                        &ToolResult { success: false, output: reason.clone(), data: None },
                    ));
                    let ev = AgentEvent::new(AgentEventKind::ToolCallExecuted {
                        id: call_with_parsed_args.id.clone(),
                        name: call_with_parsed_args.function.name.clone(),
                        success: false,
                        output: reason,
                    });
                    events.push(ev.clone());
                    on_event(ev);
                    continue;
                }
            }

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

            // ─── 执行许可门（三档：需逐步确认 / 仅确认风险操作 / 全流程开放）───
            // 需逐步确认：每一个工具调用都要看一眼；
            // 仅确认风险操作：只有风险操作（变更类 / 未知类 / 外发类）才要确认；
            // 全流程开放：直接放行（规则引擎同步全关，所有操作永久放行）。
            let needs_confirm = match input.approval_mode {
                ApprovalMode::FullOpen => false,
                ApprovalMode::StepConfirm => true,
                ApprovalMode::RiskOnly => crate::ai::rules_engine::is_risk_operation(
                    &call_with_parsed_args.function.name,
                    &call_with_parsed_args.function.arguments,
                ),
            };
            if needs_confirm {
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
                // 通知前端状态流转（执行中 / 已拦截）
                let resolved = if allowed {
                    "❌ 放行该操作，自动回复「继续」，Agent 继续跑".to_string()
                } else {
                    "✅ 拦截该操作（Agent 换方案或先向用户确认）".to_string()
                };
                let ev = AgentEvent::new(AgentEventKind::ToolApprovalResolved {
                    id: call_with_parsed_args.id.clone(),
                    approved: allowed,
                    output: resolved.clone(),
                });
                events.push(ev.clone());
                on_event(ev);

                if !allowed {
                    // 拦截：以工具失败结果回灌，让模型换方案或与用户确认
                    let msg = format!(
                        "✅ 用户拦截了该工具调用（{}）。\
                         请不要用同样的参数重试；改用不需要该操作的方案继续，\
                         或先向用户说明你打算做什么并征得同意。",
                        input.approval_mode.label()
                    );
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

            // ─── 真正执行（带心跳 + 总超时）───
            // 心跳：每 20s 推一次 ToolProgress，界面能看到"某个工具正在跑、跑了多久"，
            //       而不是永远停在「思考中…」。
            // 总超时：任何工具都不该把整轮 Agent 永久卡死（read_image 无超时挂死就是前车之鉴）。
            let tool_name = call_with_parsed_args.function.name.clone();
            let tool_id = call_with_parsed_args.id.clone();
            let started = std::time::Instant::now();
            let exec_fut = tools.execute(&call_with_parsed_args);
            tokio::pin!(exec_fut);

            let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(TOOL_HEARTBEAT_SECS));
            heartbeat.tick().await; // 第一个 tick 立即返回，跳过
            let timeout_secs = input.tool_timeout_secs.max(1);

            let result = loop {
                tokio::select! {
                    r = &mut exec_fut => break Some(r),
                    _ = heartbeat.tick() => {
                        let ev = AgentEvent::new(AgentEventKind::ToolProgress {
                            id: tool_id.clone(),
                            name: tool_name.clone(),
                            elapsed_secs: started.elapsed().as_secs(),
                        });
                        events.push(ev.clone());
                        on_event(ev);
                    }
                    _ = tokio::time::sleep(std::time::Duration::from_secs(timeout_secs)) => {
                        break None;
                    }
                }
            };

            let result = match result {
                Some(r) => r,
                None => {
                    let msg = format!(
                        "⏱ 工具 {} 超过 {}s 未返回，已被中止（不会继续占用本轮）。\
                         请换一种方式完成任务：例如先把长任务拆小、检查路径/参数是否正确，\
                         或先向用户说明卡在哪里。",
                        tool_name, timeout_secs
                    );
                    crate::ai::runtime_log::warn(
                        "tools",
                        &format!("工具超时中止：{}（{}s）", tool_name, timeout_secs),
                    );
                    ToolResult { success: false, output: msg, data: None }
                }
            };

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

    memory_auto_ingest(&input.memory, &input.run_id, &input.user_message, &final_content);
    emit_turn_card(turn_card_on, &input.run_id, &input.user_message, &turn_blocks, &mut on_event, &mut events);
    emit_text_audit(&rule_cfg, &input.user_message, &final_content, &mut audit_state, &mut on_event, &mut events);
    crate::ai::runtime_log::info(
        "agent",
        &format!(
            "Agent 循环达到最大迭代 run={} 步数={} 工具调用={} 被拦={}",
            input.run_id,
            config.max_iterations,
            total_tool_calls,
            turn_blocks.len()
        ),
    );
    // 正常收尾：清理本轮的待决裁决记录（一次性放行仍保留，供续跑消费）
    if !turn_blocks.is_empty() {
        crate::ai::runtime_log::info(
            "rules",
            &format!(
                "本轮共 {} 条被拦记录；待用户在裁决卡片上选择 ❌ 放行 / ✅ 拦截（放行将自动回复「继续」）",
                turn_blocks.len()
            ),
        );
    }
    cost_record(
        &input.mode, &input.run_id,
        cost_input, cost_output, cost_cache_hit, cost_cache_miss, cost_reasoning,
    );
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

/// 契约拒绝的审计（契约原因码单独记账，便于 /guard log 排查）
fn audit_contract(tool: &str, reason: &str) {
    crate::ai::rules_engine::audit("task-contract-deny", "__contract", tool, reason, "");
}

/// 文本审计 + 纠正注入（移植自 dsh-rule-engine text-detect / semantic）。
///
/// 边界与上游一致：**助手输出不可阻断**，这里只审计并把纠正文本注入会话；
/// 交付门为「每 (会话,规则) 一次、每小时 ≤3 次」。
fn emit_text_audit(
    cfg: &crate::ai::rules_engine::RuleEngineConfig,
    user_message: &str,
    final_content: &str,
    st: &mut crate::ai::rules_engine::TextAuditState,
    on_event: &mut dyn FnMut(AgentEvent),
    events: &mut Vec<AgentEvent>,
) {
    if !cfg.enabled || !cfg.correct_inject || final_content.trim().is_empty() {
        return;
    }
    let hits = crate::ai::rules_engine::text_audit(final_content, user_message, st);
    if hits.is_empty() {
        return;
    }
    let now = chrono::Utc::now().timestamp();
    let (deliver, fresh) = crate::ai::rules_engine::should_deliver_injection(st, &hits, now);
    for h in &hits {
        crate::ai::rules_engine::audit(
            "correct",
            &h.rule_id,
            "-",
            &format!("文本审计命中：{}（{}）", h.title, h.evidence),
            "",
        );
    }
    if !deliver {
        return;
    }
    let injection = crate::ai::rules_engine::render_injection(&fresh);
    if let Ok(v) = serde_json::to_value(&fresh) {
        let ev = AgentEvent::new(AgentEventKind::TextAudit { hits: v, injection });
        events.push(ev.clone());
        on_event(ev);
    }
}

/// 回合末生成裁决卡片（对齐 dsh-rule-engine-client：一条被拦记录都没有时不产生卡片）。
/// 是否产生由**执行许可档位**决定（`全流程开放` 档位不产生卡片）。
fn emit_turn_card(
    turn_card_on: bool,
    session_id: &str,
    user_message: &str,
    blocks: &[crate::ai::rules_engine::TurnCardBlock],
    on_event: &mut dyn FnMut(AgentEvent),
    events: &mut Vec<AgentEvent>,
) {
    if !turn_card_on || blocks.is_empty() {
        return;
    }
    match crate::ai::rules_engine::record_turn_card(session_id, 1, user_message, blocks.to_vec()) {
        Ok(card) => {
            crate::ai::runtime_log::info(
                "rules",
                &format!(
                    "回合裁决卡片已生成 key={} 共 {} 条被拦记录（用户在卡片上选择 ❌ 放行 / ✅ 拦截）",
                    card.key,
                    card.blocks.len()
                ),
            );
            if let Ok(v) = serde_json::to_value(&card) {
                let ev = AgentEvent::new(AgentEventKind::TurnCard { card: v });
                events.push(ev.clone());
                on_event(ev);
            }
        }
        Err(e) => {
            crate::ai::runtime_log::warn("rules", &format!("裁决卡片落盘失败（不影响主流程）：{}", e));
        }
    }
}

/// 费用记账（移植自 dsh-cost-meter）：一次 Agent 运行累计的 token 用量记一笔。
/// 记账失败必须被吞掉——计量不能拖垮主流程。
#[allow(clippy::too_many_arguments)]
fn cost_record(
    mode: &str,
    session_id: &str,
    input_tokens: usize,
    output_tokens: usize,
    cache_hit: usize,
    cache_miss: usize,
    reasoning: usize,
) {
    if input_tokens == 0 && output_tokens == 0 && cache_hit == 0 && cache_miss == 0 {
        return;
    }
    let model = crate::ai::cost_meter::canonical_model("deepseek-v4-flash");
    let _ = mode;
    // 净输入 = 未命中缓存部分（命中部分单独按 cacheHit 计价，二者互斥）
    let usage = crate::ai::cost_meter::CallUsage {
        model,
        provider: "deepseek".into(),
        session_id: session_id.to_string(),
        session_title: format!("{} 模式会话", mode.to_uppercase()),
        input: cache_miss,
        output: output_tokens,
        cache_read: cache_hit,
        cache_write: 0,
        reasoning,
        is_plan: false,
        at: chrono::Utc::now().timestamp(),
    };
    let _ = crate::ai::cost_meter::record_call(&usage);
}

/// 轮末自动归档（对齐上游 agent/turn-stopping）：
/// 把本轮用户诉求与最终回复合并写入长期记忆，失败不影响主流程。
fn memory_auto_ingest(
    cfg: &crate::ai::memory::MemoryConfig,
    session_id: &str,
    user_message: &str,
    final_content: &str,
) {
    if !cfg.enabled || !cfg.auto_ingest {
        return;
    }
    let u = user_message.trim();
    let a = final_content.trim();
    if u.is_empty() && a.is_empty() {
        return;
    }
    let text = if a.is_empty() {
        u.to_string()
    } else {
        format!("用户诉求：{}\n\n本轮结论：{}", u, a)
    };
    // 失败必须被吞掉：归档失败不能阻塞轮次收尾
    let _ = crate::ai::memory::ingest(&text, "user", session_id, Some("turn"));
}

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
    let memory_cfg = input.memory.clone();
    let tool_timeout_secs = input.tool_timeout_secs;
    Arc::new(move |instruction: String| -> futures_util::future::BoxFuture<'static, Result<String, String>> {
        let deepseek = deepseek.clone();
        let system_prompt = system_prompt.clone();
        let mode = mode.clone();
        let working_dir = working_dir.clone();
        let undo_store = undo_store.clone();
        let run_id = run_id.clone();
        let approval_gate = approval_gate.clone();
        let memory_cfg = memory_cfg.clone();
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
                // 子智能体共享同一套长期记忆配置
                memory: memory_cfg.clone(),
                // 子智能体沿用主循环的工具超时
                tool_timeout_secs,
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

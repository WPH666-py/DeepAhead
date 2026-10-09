// ═══════════════════════════════════════════════════════════════════
// 工作流引擎（Workflow Engines）
//
// DeepAhead 的四种模式，每一种都由「厂商原装工作流引擎」驱动，并与
// DeepSeek 运行时（deepseek.rs 客户端 + agent_loop.rs 核心循环 +
// tools.rs 工具注册表）强强结合 —— 不是只靠 System Prompt 模拟风格。
//
// | 模式 | 引擎                           | 上游原装仓库                                       | 许可证     |
// |------|--------------------------------|----------------------------------------------------|------------|
// | DSH  | DeepSeek Harness 原生 Agent   | deepseek-ai（DeepSeek Harness）                    | MIT        |
// | DSK  | Kimi K3 / Kimi Code CLI       | github.com/MoonshotAI/kimi-code                    | MIT        |
// | DSA  | GPT-6 Astra 动态委派          | github.com/DannyMac180/astra-advisor               | MIT        |
// | DSF  | Claude Fable 5.1 剧本制       | codejunkie99/fable-orchestrator + DivyamTalwar/fablewright | MIT |
//
// 上游原版源码按原文完整随仓保存在 DeepAhead/vendor/（含 LICENSE），
// 本模块与其对应关系的说明见 DeepAhead/docs/WORKFLOW-ENGINES.md。
// ═══════════════════════════════════════════════════════════════════

pub mod astra;
pub mod fable;
pub mod kimi;

use crate::ai::agent_loop::{AgentEvent, AgentEventKind, AgentLoopInput, AgentLoopOutput};
use crate::ai::deepseek::{DeepSeekClient, Message};
use std::sync::Arc;

/// 按模式分发到对应的原装工作流引擎
pub async fn run<F>(input: AgentLoopInput, on_event: F) -> Result<AgentLoopOutput, String>
where
    F: FnMut(AgentEvent) + Send,
{
    match input.mode.as_str() {
        // DSH = DeepSeek Harness 原生 Agent 循环（DeepSeek 运行时基准）
        "dsh" => crate::ai::agent_loop::run_agent_loop(input, on_event).await,
        // DSK = Kimi K3 原装工作流（计划 → 工具执行 → 塔式审查修复）
        "dsk" => kimi::run(input, on_event).await,
        // DSA = GPT-6 Astra 原装工作流（总指挥拆解 → 动态委派 → 复验 → 只读审查）
        "dsa" => astra::run(input, on_event).await,
        // DSF = Claude Fable 5.1 原装工作流（CALL SHEET → 委派 → 亲验 → 只读裁决）
        "dsf" => fable::run(input, on_event).await,
        other => Err(format!("未知工作流模式：{}（支持 dsh / dsk / dsa / dsf）", other)),
    }
}

// ═══════════════════════════════════════════════════════════════════
// 引擎共享助手
// ═══════════════════════════════════════════════════════════════════

/// 记录事件到本地 sink 并转发给回调
pub fn emit<F>(events: &mut Vec<AgentEvent>, event: AgentEvent, cb: &mut F)
where
    F: FnMut(AgentEvent),
{
    events.push(event.clone());
    cb(event);
}

/// 一次纯文本模型调用（无工具）—— 引擎的规划/设计阶段使用
pub async fn text_call(
    deepseek: &Arc<DeepSeekClient>,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let messages = vec![Message {
        role: "user".into(),
        content: user.to_string(),
        tool_calls: None,
        tool_call_id: None,
        name: None,
        reasoning_content: None,
        r#type: "user".into(),
    }];
    let resp = deepseek.chat(system, &messages).await?;
    let content = resp
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_default();
    if content.trim().is_empty() {
        return Err("规划阶段模型返回空内容".into());
    }
    Ok(content)
}

/// 阶段循环结果（回传阶段结论，工具调用数等统计）
#[derive(Debug, Clone, Default)]
pub struct PhaseResult {
    pub content: String,
    pub iterations: usize,
    pub tool_calls: usize,
}

/// 运行一个「阶段」：本质是嵌套的核心 Agent Loop（DeepSeek 运行时），
/// 但生命周期事件按阶段规则处理：
/// - `echo_lifecycle = true`（主阶段）：Started / Done 照常转发；
/// - `echo_lifecycle = false`（辅助阶段）：Started 静默、Done 仅回传不转发，
///   避免前端出现多次 started/done 动画；tool/text/file 事件全量转发。
/// `max_iterations = 0` 表示沿用「无步数上限」语义（直到模型给出结论）。
/// `user_override` 可替换本阶段的用户消息（辅助阶段用自己的指令）。
pub async fn run_phase<F>(
    mut input: AgentLoopInput,
    max_iterations: usize,
    preamble: Option<String>,
    user_override: Option<String>,
    echo_lifecycle: bool,
    on_event: &mut F,
) -> Result<PhaseResult, String>
where
    F: FnMut(AgentEvent) + Send,
{
    if max_iterations > 0 {
        input.max_iterations_override = Some(max_iterations);
    } else {
        input.max_iterations_override = None;
    }
    input.extra_preamble = preamble;
    if let Some(u) = user_override {
        input.user_message = u;
    }
    let mut captured: Option<String> = None;
    let mut total_tool_calls = 0usize;
    let mut iterations = 0usize;

    let mut filtered = |event: AgentEvent| -> Option<AgentEvent> {
        match &event.kind {
            AgentEventKind::Started { .. } => {
                if echo_lifecycle {
                    Some(event.clone())
                } else {
                    None
                }
            }
            AgentEventKind::Done {
                content,
                total_iterations,
                total_tool_calls: t,
                ..
            } => {
                captured = Some(content.clone());
                iterations = *total_iterations;
                total_tool_calls = *t;
                if echo_lifecycle {
                    Some(event.clone())
                } else {
                    None
                }
            }
            _ => Some(event.clone()),
        }
    };

    let output = crate::ai::agent_loop::run_agent_loop(input, |ev| {
        if let Some(fwd) = filtered(ev) {
            on_event(fwd);
        }
    })
    .await?;

    let content = captured.unwrap_or(output.final_content.clone());
    Ok(PhaseResult {
        content,
        iterations: if iterations > 0 { iterations } else { output.total_iterations },
        tool_calls: if total_tool_calls > 0 { total_tool_calls } else { output.total_tool_calls },
    })
}

/// 汇总多次阶段结果为一个最终输出（事件流已实时转发过）
pub fn assemble_output<'a>(
    input: &AgentLoopInput,
    phases: &[PhaseResult],
    events: Vec<AgentEvent>,
    context_tokens: usize,
) -> AgentLoopOutput {
    let mut final_content = String::new();
    let mut total_iterations = 0usize;
    let mut total_tool_calls = 0usize;
    for p in phases {
        total_iterations += p.iterations;
        total_tool_calls += p.tool_calls;
        if !p.content.trim().is_empty() {
            final_content = p.content.clone();
        }
    }
    AgentLoopOutput {
        final_content,
        total_iterations,
        total_tool_calls,
        events,
        run_id: input.run_id.clone(),
        context_tokens,
        context_limit: input.context_limit,
        context_ratio: if input.context_limit > 0 {
            (context_tokens as f64 / input.context_limit as f64).min(1.0)
        } else {
            0.0
        },
        compressed: false,
    }
}

/// 生成一个用于阶段嵌套执行的浅拷贝（保留 deepseek/undo/工作目录，
/// 重置消息历史——阶段有独立的上下文，正是上游"子智能体/技能阶段"的语义）
pub fn clone_for_phase(input: &AgentLoopInput) -> AgentLoopInput {
    AgentLoopInput {
        mode: input.mode.clone(),
        user_message: input.user_message.clone(),
        history: Vec::new(),
        context_paths: input.context_paths.clone(),
        working_dir: input.working_dir.clone(),
        deepseek: input.deepseek.clone(),
        system_prompt: input.system_prompt.clone(),
        run_id: input.run_id.clone(),
        undo_store: input.undo_store.clone(),
        max_iterations_override: None,
        extra_preamble: None,
        approval_mode: input.approval_mode,
        approval_gate: input.approval_gate.clone(),
        // 阶段沿用主运行的上下文窗口与压缩模式
        context_limit: input.context_limit,
        auto_compress: input.auto_compress,
        // 长期记忆配置沿用主运行
        memory: input.memory.clone(),
        // 工具超时沿用主运行（阶段不该比主循环更放任）
        tool_timeout_secs: input.tool_timeout_secs,
        // 取消信号与心跳标签一并继承：阶段子循环同样要能被停掉
        cancel: input.cancel.clone(),
        heartbeat_label: input.heartbeat_label.clone(),
    }
}

/// 估算上下文 token（与 context.rs 一致的粗糙估算）
pub fn est_tokens(text: &str) -> usize {
    crate::ai::context::ContextCompressor::estimate_tokens(text)
}

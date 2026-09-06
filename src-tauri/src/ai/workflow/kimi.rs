// ═══════════════════════════════════════════════════════════════════
// DSK 工作流引擎 — Kimi Code CLI 原装工作流（Next-Gen Agent）
//
// 上游：github.com/MoonshotAI/kimi-code（MIT）
//   - packages/agent-core-v2：下一代 Agent 核心 —— Agent 循环、任务(task)
//     规划、塔式(tower)子智能体编排、工具执行器；循环默认不设步数上限，
//     直到模型给出结论（agent-loop 事件流：text → tool_use → tool_result …）。
//   - 使用形态（kimi-code 文档）：先给出任务规划，再由执行 Agent 循环推进，
//     子智能体以"塔"的形式被派遣并回传结论，主 Agent 最终给出完整答案。
//
// DeepAhead 集成方式：规划(plan) → 执行(执行 Agent 循环，无限步) →
// 塔式审查(子智能体塔审查/修复) —— 全部运行在 DeepSeek 运行时之上，
// 复用 agent_loop.rs 的工具执行/DSML 解析/上下文压缩/撤销日志。
// 原版源码随仓保存于 vendor/kimi-code/（见 docs/WORKFLOW-ENGINES.md）。
// ═══════════════════════════════════════════════════════════════════

use crate::ai::agent_loop::{AgentEvent, AgentEventKind, AgentLoopInput, AgentLoopOutput};
use crate::ai::workflow::{
    assemble_output, clone_for_phase, emit, est_tokens, run_phase, text_call,
};

/// Kimi 规划器 system（对齐 kimi-code task 规划语义：目标/步骤/验收标准）
const KIMI_PLANNER_SYSTEM: &str = r#"You are the planner of the Kimi Code CLI next-gen agent (MoonshotAI/kimi-code).
Given a user task, produce a concise ONE-SHOT task plan in this structure — nothing else:
```
【目标】one line
【步骤】numbered list, each step verifiable with tools
【验收标准】checklist to prove completion
```"#;

/// 执行阶段的循环规则（对齐 kimi-code agent 循环：无步数上限，直到给出结论）
const KIMI_EXEC_PREAMBLE: &str = r#"
## Kimi Code 原装执行循环（Next-Gen Agent）
- 按任务规划逐步执行，每步先『读/看』再『改/写』。
- 剩余未开始的步骤在执行过程中可修正，但完成后要在总结中说明偏差。
- 无步数上限：持续调用工具直到任务全部完成，随后直接给结论。
"#;

/// 塔式子智能体审查指令（kimi-code tower 语义：独立上下文克隆、回传结论）
const KIMI_TOWER_REVIEW: &str = r#"【塔式子智能体 · 审查塔】
你是一个独立的审查子智能体（塔），拥有完整工具集。请对照【任务规划】与【验收标准】
审查已完成的工作：
1. 逐项核对验收标准，用工具重新读取实际文件内容（不要凭摘要判断）；
2. 发现的问题直接修复（写文件工具可用）；
3. 最后输出审查结论：已完成项 / 修复项 / 仍未满足项。
修复后如仍有未满足项，明确列出原因和下一步建议。"#;

pub async fn run<F>(input: AgentLoopInput, mut on_event: F) -> Result<AgentLoopOutput, String>
where
    F: FnMut(AgentEvent) + Send,
{
    let mut events: Vec<AgentEvent> = Vec::new();
    let mut phases: Vec<crate::ai::workflow::PhaseResult> = Vec::new();

    // 规划（one-shot 任务规划）
    let plan = text_call(&input.deepseek, KIMI_PLANNER_SYSTEM, &input.user_message).await?;
    let plan_header = format!("📋 【DSK · Kimi 任务规划】\n{}\n", plan);
    emit(
        &mut events,
        AgentEvent::new(AgentEventKind::AssistantText {
            content: plan_header.clone(),
        }),
        &mut on_event,
    );

    // 执行（核心 Agent 循环，0 = 无步数上限）
    let exec = run_phase(
        clone_for_phase(&input),
        0,
        Some(format!("{}\n【任务规划】\n{}", KIMI_EXEC_PREAMBLE, plan)),
        None,
        true,
        &mut on_event,
    )
    .await?;
    phases.push(exec.clone());

    // 塔式子智能体审查（有界，避免失控）
    let mut tower_input = clone_for_phase(&input);
    tower_input.user_message = format!("{}\n\n【任务规划】\n{}", KIMI_TOWER_REVIEW, plan);
    let tower = run_phase(
        tower_input,
        25,
        Some("## 审查塔运行说明\n- 你是主 Agent 派遣的独立塔（子智能体），文件变更计入同一撤销日志。\n- 只输出审查结论与修复摘要。".into()),
        None,
        false,
        &mut on_event,
    )
    .await?;
    phases.push(tower);

    // Done 事件（聚合统计）
    let mut output = assemble_output(
        &input,
        &phases,
        events,
        est_tokens(&input.user_message) + est_tokens(&plan),
    );
    let done = AgentEvent::new(AgentEventKind::Done {
        content: output.final_content.clone(),
        total_iterations: output.total_iterations,
        total_tool_calls: output.total_tool_calls,
        reasoning_content: None,
    });
    output.events.push(done.clone());
    on_event(done);
    Ok(output)
}

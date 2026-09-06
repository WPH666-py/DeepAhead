// ═══════════════════════════════════════════════════════════════════
// DSA 工作流引擎 — GPT-6 Astra 原装工作流（Astra Advisor orchestration）
//
// 上游：github.com/DannyMac180/astra-advisor（MIT，随仓 vendor/astra-advisor/）
//   - README：GPT-6 Astra 拆解任务 → 能力预检 → 动态委派（spawn_agent，
//     显式 model + reasoning_effort + fork_turns: none）→ 父会话继续
//     有用工作 → 完整 diff 检查 + 重跑检查 → 只读审查员（ship /
//     fix-first / rethink）→ 接受。无预定义角色、无固定子智能体数上限。
//   - plugins/astra-advisor/skills/orchestration/SKILL.md：
//     ASTRA ROUTE 机器可审计声明；parent/effort 按“可观测”记录；
//     每个委派必须有界、独立，且派发前公布任务名/精确边界/选择理由；
//     fail closed（缺失/冲突/不可用 → 该委派关闭，不静默替换）；
//     只读审查员回传 VERDICT/REASON/FINDINGS/RESIDUAL RISK；
//     任务完成输出 API 等效成本票据。
//
// DeepAhead 集成（只烧 DeepSeek Token）：
//   ASTRA ROUTE 声明 → 总指挥拆解（one-shot plan：目标/有界交付物/验收标准）
//   → 实施主阶段（总指挥继续有用工作；独立交付物通过 1-4 并行子智能体
//   委派，派发前声明 <Agent> — DeepSeek V4: 有界责任 + 选择理由）
//   → 总指挥复验（真实 diff + 重跑检查）
//   → 只读审查员（独立上下文，回传 VERDICT/REASON/FINDINGS/RESIDUAL RISK）
//   → 接受门：ship 通过；fix-first 修正+复验+新审查（限 1 轮）；
//     rethink 重新拆解+实施（限 1 轮）。
// 原版源码随仓保存于 vendor/astra-advisor/（见 docs/WORKFLOW-ENGINES.md）。
// ═══════════════════════════════════════════════════════════════════

use crate::ai::agent_loop::{AgentEvent, AgentEventKind, AgentLoopInput, AgentLoopOutput};
use crate::ai::workflow::{assemble_output, clone_for_phase, emit, est_tokens, run_phase, text_call};

/// ASTRA ROUTE 声明（对齐上游 orchestration SKILL 的机器可审计声明）
const ASTRA_ROUTE_TEMPLATE: &str = r#"ASTRA ROUTE
parent: deepseek-v4 (observed) / deepseek std (observed)
delegation: dynamic subagents (deepseek-v4, single runtime — all pins observed)
risk: <在实施阶段中按具体任务填充>
"#;

/// 总指挥拆解器（ASTRA 意图：目标 → 有界交付物 → 验收标准）
const ASTRA_PLAN_SYSTEM: &str = r#"You are the planner of the Astra Advisor orchestration (GPT-6 Astra, DannyMac180/astra-advisor).
Given the user task, produce a concise ONE-SHOT architecture & decomposition — nothing else:
```
【目标】one line
【有界交付物】numbered list; each deliverable must be concrete, bounded and independently verifiable; mark which are truly independent (可并行委派)
【验收标准】checklist to prove completion
```"#;

/// 实施主阶段的编排纪律（ASTRA：总指挥继续工作 + 动态委派 + fail closed）
const ASTRA_IMPLEMENT_PREAMBLE: &str = r#"
## GPT-6 Astra 原装实施纪律（DannyMac180/astra-advisor）
- 你是总指挥（architect & acceptance owner）：意图、架构、拆解、委派决定、父验证与接受都归你。
- 按【有界交付物】执行；每个独立交付物优先通过 subagents 并行委派（1-4 个子智能体），
  你本人继续有用的父工作，而不是等待或重复子智能体。
- 每次委派前声明：`<Agent> — DeepSeek V4: <有界责任>` + 一条选择理由；子智能体带回独立结论。
- 委派不重复：不把父会话该做的实现/验证丢给子智能体。
- fail closed：某个工具/能力不可用时报告限制并只做安全的父工作，绝不静默替换。
- 无步数上限：直到所有交付物完成并给出结论。
"#;

/// 总指挥复验（ASTRA：完整 diff + 重跑检查）
const ASTRA_VERIFY: &str = r#"【总指挥复验 · Parent Verification】
按时完成的重述：你是总指挥，任何子智能体的报告都是「声明」，不是证据。
1. 检查完整 diff：用 bash 运行 git status / git diff（项目非 Git 时用 grep/read 通读关键改动）；
2. 重跑请求的检查（构建/测试/运行，至少运行一次能证明结论的命令）；
3. 列出：改动清单、重跑命令与结果、与验收标准的差距（没有差距写「无」）。
不要修改文件。"#;

/// 只读审查员（ASTRA：fresh read-only reviewer，回传 VERDICT）
const ASTRA_REVIEWER: &str = r#"【只读审查员 · Fresh Read-only Reviewer】
你是一个独立审查员（拥有完整读取工具，但本阶段禁止任何写/改/删除文件的操作）。
请根据实际改动集与证据，回传唯一裁决：
```
ASTRA REVIEW
VERDICT: ship | fix-first | rethink
REASON: <evidence-based reason>
FINDINGS: <precise findings or none>
RESIDUAL RISK: <remaining risk or none>
```
- 只输出以上结构；你永远不修复自己发现的问题。
- 若改动导致验收标准全部满足且未发现新风险 → ship。
- 存在可修复缺陷 → fix-first（列出精确发现）。
- 架构/拆解方向错误 → rethink。"#;

/// fix-first 修正指令（父修正 + 复验）
const ASTRA_FIX: &str = r#"【总指挥修正 · Parent Correction】
审查员返回 fix-first。请逐条修复其 FINDINGS（写工具可用），
随后重跑复验命令（构建/测试/运行），并再次给出：修复清单 + 重跑结果。"#;

/// 判断审查裁决
fn verdict_of(content: &str) -> &'static str {
    let lower = content.to_lowercase();
    if lower.contains("verdict:") {
        let head = lower.find("verdict:").unwrap();
        let tail = &lower[head..];
        if tail.starts_with("verdict: rethink") || tail.contains("verdict: rethink") {
            return "rethink";
        }
        if tail.contains("verdict: fix-first") || tail.contains("verdict: fix_first") {
            return "fix-first";
        }
        if tail.contains("verdict: ship") {
            return "ship";
        }
    }
    if lower.contains("rethink") && !lower.contains("no rethink") {
        return "rethink";
    }
    if lower.contains("fix-first") || lower.contains("fix first") {
        return "fix-first";
    }
    "ship"
}

pub async fn run<F>(input: AgentLoopInput, mut on_event: F) -> Result<AgentLoopOutput, String>
where
    F: FnMut(AgentEvent) + Send,
{
    let mut events: Vec<AgentEvent> = Vec::new();
    let mut phases: Vec<crate::ai::workflow::PhaseResult> = Vec::new();

    // 1. ASTRA ROUTE 声明（第一个任务工具调用之前，机器可审计）
    emit(
        &mut events,
        AgentEvent::new(AgentEventKind::AssistantText {
            content: format!(
                "🛰️ 【DSA · ASTRA ROUTE】\n{}\n",
                ASTRA_ROUTE_TEMPLATE
            ),
        }),
        &mut on_event,
    );

    // 2. 总指挥拆解
    let plan = text_call(&input.deepseek, ASTRA_PLAN_SYSTEM, &input.user_message).await?;
    emit(
        &mut events,
        AgentEvent::new(AgentEventKind::AssistantText {
            content: format!("🧭 【DSA · 总指挥拆解】\n{}\n", plan),
        }),
        &mut on_event,
    );

    // 3. 实施主阶段（总指挥继续有用工作 + 动态委派）
    let implement = run_phase(
        clone_for_phase(&input),
        0,
        Some(format!("{}\n【总指挥拆解】\n{}", ASTRA_IMPLEMENT_PREAMBLE, plan)),
        None,
        true,
        &mut on_event,
    )
    .await?;
    phases.push(implement.clone());

    // 4. 总指挥复验（完整 diff + 重跑检查）
    let verify = run_phase(
        clone_for_phase(&input),
        15,
        Some("## 总指挥复验运行说明\n- 只读阶段：不修改文件。\n- 子智能体报告只视为声明，必须亲验。".into()),
        Some(ASTRA_VERIFY.to_string()),
        false,
        &mut on_event,
    )
    .await?;
    phases.push(verify.clone());

    // 5. 只读审查员（fresh reviewer）
    let mut review = run_phase(
        clone_for_phase(&input),
        20,
        Some("## 审查员运行说明\n- 只读：禁止 write/edit/delete/bash(修改类) 等一切会改文件的操作。\n- 你永远不修复发现的问题，只输出裁决。".into()),
        Some(ASTRA_REVIEWER.to_string()),
        false,
        &mut on_event,
    )
    .await?;
    phases.push(review.clone());

    // 6. 接受门：ship / fix-first / rethink（各有界，最多一轮修正）
    let verdict = verdict_of(&review.content);
    if verdict == "fix-first" {
        let fix = run_phase(
            clone_for_phase(&input),
            15,
            None,
            Some(ASTRA_FIX.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        phases.push(fix.clone());
        let recheck = run_phase(
            clone_for_phase(&input),
            12,
            None,
            Some(ASTRA_VERIFY.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        phases.push(recheck.clone());
        let fresh_review = run_phase(
            clone_for_phase(&input),
            20,
            Some("## 审查员运行说明\n- 只读：禁止任何会修改文件的操作；不修复发现的问题，只输出裁决。\n- 这是修复后的新审查，前一次裁决已无效。".into()),
            Some(ASTRA_REVIEWER.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        review = fresh_review.clone();
        phases.push(fresh_review);
        emit(
            &mut events,
            AgentEvent::new(AgentEventKind::AssistantText {
                content: format!("⚖️ 【DSA · 修复后新审查】\n{}\n", review.content),
            }),
            &mut on_event,
        );
    } else if verdict == "rethink" {
        // 重新拆解 + 重新实施（有界一轮）
        let plan2 = text_call(
            &input.deepseek,
            ASTRA_PLAN_SYSTEM,
            &format!("{}\n\n（上一次实施被审查员判定为 rethink，请从架构层面重新拆解）", input.user_message),
        )
        .await?;
        emit(
            &mut events,
            AgentEvent::new(AgentEventKind::AssistantText {
                content: format!("🧭 【DSA · 重新拆解】\n{}\n", plan2),
            }),
            &mut on_event,
        );
        let implement2 = run_phase(
            clone_for_phase(&input),
            0,
            Some(format!("{}\n【重新拆解】\n{}", ASTRA_IMPLEMENT_PREAMBLE, plan2)),
            None,
            true,
            &mut on_event,
        )
        .await?;
        phases.push(implement2.clone());
        let verify2 = run_phase(
            clone_for_phase(&input),
            15,
            None,
            Some(ASTRA_VERIFY.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        phases.push(verify2.clone());
        let fresh_review = run_phase(
            clone_for_phase(&input),
            20,
            Some("## 审查员运行说明\n- 只读：禁止任何会修改文件的操作；不修复发现的问题，只输出裁决。\n- 这是重新实施后的新审查。".into()),
            Some(ASTRA_REVIEWER.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        review = fresh_review.clone();
        phases.push(fresh_review);
    }

    emit(
        &mut events,
        AgentEvent::new(AgentEventKind::AssistantText {
            content: format!("🏁 【DSA · 审查裁决】\n{}\n", review.content),
        }),
        &mut on_event,
    );

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

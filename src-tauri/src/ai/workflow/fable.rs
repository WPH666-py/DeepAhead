// ═══════════════════════════════════════════════════════════════════
// DSF 工作流引擎 — Claude Fable 5.1 原装工作流
// （Fable Orchestrator + Fablewright 双上游，均 MIT，随仓 vendor/）
//
// 上游 1：github.com/codejunkie99/fable-orchestrator（MIT）
//   - skill/fable/SKILL.md：Fable 5.1 只做规划与裁定（orchestration
//     decisions only）；实施 worker 限定 GPT-5.6 Luna 或 DeepSeek V4
//     Flash；先只读侦察工作区 → 构建编排包（目标/验收标准/上下文/
//     受保护文件/worker 菜单/并发上限）→ 要求有界任务图（角色/模型/
//     责任人/依赖/预期输出/验证/停止条件）→ 校验图 → 并行派发独立
//     就绪节点 → 汇总证据、检查改动、比例验证；上限三次 Fable 调用。
// 上游 2：github.com/DivyamTalwar/fablewright（MIT）
//   - skills/fablewright/SKILL.md：Fable 5.1 是 playwright（wright）：
//     在第一个任务工具前张贴 CALL SHEET（route: solo|delegate|audit|
//     full|ensemble; cast; reader; independence: cross-family; risk）；
//     三定律——①作者与验收分离（reader 永远不修自己的发现）
//     ②证据高于断言（wright 亲检真实 diff 并重跑检查）
//     ③fail closed（缺失/冲突/不可用 → 该路径关闭，永不静默替换）；
//     五段式委派规格（OBJECTIVE / FILES AND OWNERSHIP / INTERFACES /
//     CONSTRAINTS / VERIFICATION）；reader 家族必须异于作者；
//     crew 同族才可 ensemble（否则退化为顺序 full）；
//     verdict: ship / fix-first / rethink。
//
// DeepAhead 集成（只烧 DeepSeek Token）：
//   ① Fable 规划（wright，text_call）：CALL SHEET + 五段式规格 +
//      委派测试（solo 优先；spec 可写 ≠ 可解）
//   ② 实施主阶段：wright 拥有架构/接口/拆解；实施节点全部为
//      DeepSeek V4（机械/大批量工作 → 并行子智能体 flash 车道；
//      判断型工作 → 主循环 terra 车道）；每个委派都是五段式规格；
//      委派替代 wright 的工作，绝不重复；先只读侦察再编包
//   ③ 验证（wright）：亲检真实 diff + 重跑检查（证据高于断言）
//   ④ 只读 reader：单运行时下无跨家族读者 → 诚实记录
//      independence: same-family 并作为残余风险（Fablewright 契约
//      要求明说，而不是粉饰成 cross-family）；回传唯一裁决
//   ⑤ 接受门：ship → 完成；fix-first → 车道修正+复验+新 reader（1 轮）；
//      rethink → 修订规格+重新实施（1 轮）；修改变更即令旧裁决失效。
// 原版源码随仓保存于 vendor/fable-orchestrator/ 与 vendor/fablewright/。
// ═══════════════════════════════════════════════════════════════════

use crate::ai::agent_loop::{AgentEvent, AgentEventKind, AgentLoopInput, AgentLoopOutput};
use crate::ai::workflow::{assemble_output, clone_for_phase, emit, est_tokens, run_phase, text_call};

/// Fable 规划器（wright：CALL SHEET + 五段式规格 + 委派测试）
const FABLE_PLAN_SYSTEM: &str = r#"You are the wright of Fablewright: Claude Fable 5.1 writes the specification and accepts the result.
Work through the decision ladder (stop at the first match): solo → audit → delegate → full → ensemble.
`solo` is the default and needs a reason to leave. Delegate only when the work is bounded,
fully specifiable, and worth a fresh context. Build the machine-auditable block FIRST, then the
five-part specification. Output format — nothing else:
```
FABLEWRIGHT CALL SHEET
route: solo | delegate | audit | full | ensemble
cast: none | <lane-id>[, <lane-id>...]
reader: none | <lane-id>
independence: cross-family | same-family | not-applicable
risk: <concise, task-specific rationale>

【目标】OBJECTIVE: one line
【文件与所有权】FILES AND OWNERSHIP: exact owned files per lane; concurrent edits must be preserved
【接口】INTERFACES: what stays outside the lanes
【约束】CONSTRAINTS: protected files, scope limits, stop conditions
【验证】VERIFICATION: commands/checks that prove completion
```
An single-runtime note applies: cast and reader both run DeepSeek V4, so record
`independence: same-family` and carry it as residual risk - never round it up to cross-family.
For genuinely parallel work, `full` over `ensemble`: with one runtime every lane is one family,
and integration must be written as a sequential step owned by the wright."#;

/// 实施主阶段编排纪律（wright 拥有意图/架构/接口/拆解/验收）
const FABLE_IMPLEMENT_PREAMBLE: &str = r#"
## Claude Fable 5.1 原装实施纪律（Fablewright / Fable Orchestrator）
- 你是 wright：意图、架构、接口与拆解、调用单、验证、接受全部归你，且不能离手。
- 委派测试：如果为了检查委派结果你仍要做一遍，那就是 solo —— 直接做；委派是替代，不是重复。
- 每个委派必须携带五段式规格：OBJECTIVE / FILES AND OWNERSHIP / INTERFACES / CONSTRAINTS / VERIFICATION；
  每个 worker 只做自己有界的部分，必须保留并发编辑，绝不扩大范围。
- 先只读侦察（glob/grep/read）再决策；机械/大批量/高吞吐工作派给并行子智能体（flash 车道），
  判断型/高风险工作由你亲自做主循环（terra 车道）。
- 证据高于断言：worker 报告是声明；你亲检真实 diff 并重跑检查后才算数。
- fail closed：模型/角色/effort 缺失或不可用 → 报告阻断，绝不静默替换、绝不悄悄降级路线。
- 无步数上限：直到验收标准满足或明确报告阻断。
"#;

/// wright 验证阶段（亲检真实 diff + 重跑检查）
const FABLE_VERIFY: &str = r#"【Wright 验证 · Evidence over Assertion】
你是 wright。任何 worker 报告都只是声明；按五段式规格【验证】逐项亲验：
1. 检查真实 diff：bash 运行 git diff / git status（非 Git 项目用 grep/read 通读改动）；
2. 重跑每一项检查命令并记录真实输出；
3. 列出：改动清单、验证命令与结果、与验收标准的差距（无差距写「无」）。
不要修改文件。"#;

/// 只读 reader（fresh read-only，唯一裁决）
const FABLE_READER: &str = r#"【Fresh Reader · 只读裁决员】
你是一个全新上下文的只读审查员（禁止任何写/改/删除文件的操作，永不修复自己发现的问题）。
给定 wright 的验证证据与改动集，回传唯一裁决：
```
VERDICT: ship | fix-first | rethink
REASON: <evidence-based reason>
FINDINGS: <precise findings or none>
RESIDUAL RISK: <remaining risk — 记录 independence: same-family 同族残余风险>
```
- 全部满足且无新风险 → ship；存在可修复缺陷 → fix-first；结构/方向错误 → rethink。"#;

/// fix-first 修正（车道修正 + wright 复验）
const FABLE_FIX: &str = r#"【修正 · Fix-first】
reader 返回 fix-first。逐条修复其 FINDINGS（写工具可用），随后重跑【验证】检查命令，
给出：修复清单 + 重跑结果。"#;

fn verdict_of(content: &str) -> &'static str {
    let lower = content.to_lowercase();
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

    // 1. Fable 规划（wright：CALL SHEET + 五段式规格）
    let plan = text_call(&input.deepseek, FABLE_PLAN_SYSTEM, &input.user_message).await?;
    emit(
        &mut events,
        AgentEvent::new(AgentEventKind::AssistantText {
            content: format!("🎭 【DSF · Fable 5.1 规划】\n{}\n", plan),
        }),
        &mut on_event,
    );

    // 2. 实施主阶段（wright + 委派；DeepsSeek V4 单一运行时车道）
    let implement = run_phase(
        clone_for_phase(&input),
        0,
        Some(format!("{}\n【Fable 规划】\n{}", FABLE_IMPLEMENT_PREAMBLE, plan)),
        None,
        true,
        &mut on_event,
    )
    .await?;
    phases.push(implement.clone());

    // 3. wright 验证（亲检 diff + 重跑检查）
    let verify = run_phase(
        clone_for_phase(&input),
        15,
        Some("## wright 验证运行说明\n- 只读阶段：不修改文件。".into()),
        Some(FABLE_VERIFY.to_string()),
        false,
        &mut on_event,
    )
    .await?;
    phases.push(verify.clone());

    // 4. 只读 reader（新鲜上下文，唯一裁决）
    let mut reader = run_phase(
        clone_for_phase(&input),
        20,
        Some("## reader 运行说明\n- 只读：禁止 write/edit/delete 及一切会修改文件的操作。\n- 你永远不修复发现的问题。\n- 单运行时下无跨家族读者：记录 independence: same-family 为残余风险。".into()),
        Some(FABLE_READER.to_string()),
        false,
        &mut on_event,
    )
    .await?;
    phases.push(reader.clone());

    // 5. 接受门：ship / fix-first / rethink（各有界一轮）
    let verdict = verdict_of(&reader.content);
    if verdict == "fix-first" {
        let fix = run_phase(
            clone_for_phase(&input),
            15,
            None,
            Some(FABLE_FIX.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        phases.push(fix.clone());
        let recheck = run_phase(
            clone_for_phase(&input),
            12,
            None,
            Some(FABLE_VERIFY.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        phases.push(recheck.clone());
        let fresh_reader = run_phase(
            clone_for_phase(&input),
            20,
            Some("## reader 运行说明\n- 只读：禁止任何会修改文件的操作；不修复发现的问题。\n- 这是修正后的新 reader，前一次裁决已失效。".into()),
            Some(FABLE_READER.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        reader = fresh_reader.clone();
        phases.push(fresh_reader);
        emit(
            &mut events,
            AgentEvent::new(AgentEventKind::AssistantText {
                content: format!("⚖️ 【DSF · 修正后新裁决】\n{}\n", reader.content),
            }),
            &mut on_event,
        );
    } else if verdict == "rethink" {
        let plan2 = text_call(
            &input.deepseek,
            FABLE_PLAN_SYSTEM,
            &format!("{}\n\n（上一次实施被 reader 判定为 rethink，请修订架构/拆解后重新给出完整规格）", input.user_message),
        )
        .await?;
        emit(
            &mut events,
            AgentEvent::new(AgentEventKind::AssistantText {
                content: format!("🎭 【DSF · 修订规格】\n{}\n", plan2),
            }),
            &mut on_event,
        );
        let implement2 = run_phase(
            clone_for_phase(&input),
            0,
            Some(format!("{}\n【修订规格】\n{}", FABLE_IMPLEMENT_PREAMBLE, plan2)),
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
            Some(FABLE_VERIFY.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        phases.push(verify2.clone());
        let fresh_reader = run_phase(
            clone_for_phase(&input),
            20,
            Some("## reader 运行说明\n- 只读：禁止任何会修改文件的操作；不修复发现的问题。\n- 这是修订实施后的新裁决。".into()),
            Some(FABLE_READER.to_string()),
            false,
            &mut on_event,
        )
        .await?;
        reader = fresh_reader.clone();
        phases.push(fresh_reader);
    }

    emit(
        &mut events,
        AgentEvent::new(AgentEventKind::AssistantText {
            content: format!("🏁 【DSF · 最终裁决】\n{}\n", reader.content),
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

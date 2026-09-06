# PROVENANCE — codejunkie99/fable-orchestrator + DivyamTalwar/fablewright（Claude Fable 5.1）

Fable 5.1 的「剧本制」工作流由两个互补的上游仓库共同构成，均 MIT：

## A. codejunkie99/fable-orchestrator（Fable 只规划与裁定）

- **Upstream:** https://github.com/codejunkie99/fable-orchestrator (MIT)
- **Branch:** `master`
- **Fetch date:** 2026-09-06
- **Fetch method:** 直连原始文件保存（Invoke-WebRequest），内容逐字保存。

| Local path (under `vendor\fable-orchestrator\`) | Upstream URL |
|---|---|
| `README.md` | `https://raw.githubusercontent.com/codejunkie99/fable-orchestrator/master/README.md` |
| `LICENSE` | `https://raw.githubusercontent.com/codejunkie99/fable-orchestrator/master/LICENSE` |
| `skill\fable\SKILL.md` | `https://raw.githubusercontent.com/codejunkie99/fable-orchestrator/master/skill/fable/SKILL.md` |

### 工作流算法（fable-orchestrator）

1. 只读侦察工作区，构建紧凑编排包（目标/验收标准/上下文/约束/受保护文件/
   已收集证据/可调用 worker 菜单/并发上限/用户偏好）；
2. Fable 5.1 返回有界任务图（角色、模型/agent 类型、所有者/责任、依赖、
   预期输出、验证、停止条件），实施节点限定 GPT-5.6 Luna 或 DeepSeek V4 Flash；
3. Codex 校验图 → 并行派发独立就绪节点（报告 `Agent — Model: 有界责任` 后立即开始）；
4. 汇总证据、检查改动文件、比例验证；复杂任务再送回 Fable 做下一张图或最终裁定
   （上限三次 Fable 调用）；
5. 验收标准与验证通过才结束，报告选中模型、实质变更与具体证明。
   Fable 的编排输出按 `Fable 5.1 speaks:` 原文展示，绝不伪装为 worker 输出。

## B. DivyamTalwar/fablewright（作者与验收分离 + 证据 + fail closed）

- **Upstream:** https://github.com/DivyamTalwar/fablewright (MIT)
- **Branch:** `main`（受保护；PR 合并制）
- **Fetch date:** 2026-09-06
- **Fetch method:** 直连原始文件保存（Invoke-WebRequest），内容逐字保存。

| Local path (under `vendor\fablewright\`) | Upstream URL |
|---|---|
| `README.md` | `https://raw.githubusercontent.com/DivyamTalwar/fablewright/main/README.md` |
| `LICENSE` | `https://raw.githubusercontent.com/DivyamTalwar/fablewright/main/LICENSE` |
| `skills\fablewright\SKILL.md` | `https://raw.githubusercontent.com/DivyamTalwar/fablewright/main/skills/fablewright/SKILL.md` |

### 工作流算法（fablewright）

- **三定律**：① 作者与验收分离（reader 绝不修复自己发现的问题、wright 不为
  自己委派的 lane 盖章）；② 证据高于断言（worker 报告是声明，wright 亲检
  真实 diff 并重跑检查）；③ fail closed（缺失/冲突/不可用/不可观测 → 该路径
  关闭，永不静默替换、永不悄悄降级路线）。
- **CALL SHEET**：第一个任务工具前张贴唯一机器可审计路由块
  （route: solo|delegate|audit|full|ensemble / cast / reader / independence / risk），
  阶梯取第一个匹配；`solo` 是默认且需要理由离开；后续只能"升级"且带新证据。
- **五段式委派规格**：OBJECTIVE / FILES AND OWNERSHIP / INTERFACES /
  CONSTRAINTS / VERIFICATION；委派替代 wright 的工作而非重复；
  **什么都不离开 wright**（需求、架构、接口、拆解、调用单、规格、亲验、
  升级判断、接受）。
- **独立裁决**：reader 家族必须异于作者家族（跨家族可按家族表判定）；
  同族只可记录 `same-family` 并携带残余风险；`full`/`ensemble` 的 reader
  只在 wright 自己验证之后启动；`ship` → 报告；`fix-first` → 修正+复验+
  **新的** reader；`rethink` → 修订架构，不报告完成。
- **单一运行时降级**：单运行时（DeepSeek V4）下不存在跨家族 reader，
  如实记录 `independence: same-family` 为残余风险；`ensemble` 在同族约束下
  退化为顺序 `full` 并在 risk 行记录原因。

DeepAhead 的 Rust 移植引擎：`src-tauri/src/ai/workflow/fable.rs`（DSF 模式）。

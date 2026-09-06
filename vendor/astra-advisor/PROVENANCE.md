# PROVENANCE — DannyMac180/astra-advisor（GPT-6 Astra）

- **Upstream:** https://github.com/DannyMac180/astra-advisor (MIT)
- **License:** MIT（见 `LICENSE`；© 2026 Danny Mac）
- **Branch:** `master`（README 中 `codex plugin marketplace add DannyMac180/astra-advisor --ref main`）
- **Fetch date:** 2026-09-06
- **Fetch method:** 直连原始文件保存（Invoke-WebRequest），内容逐字保存。

## Saved files

| Local path (under `vendor\astra-advisor\`) | Upstream URL |
|---|---|
| `README.md` | `https://raw.githubusercontent.com/DannyMac180/astra-advisor/master/README.md` |
| `LICENSE` | `https://raw.githubusercontent.com/DannyMac180/astra-advisor/master/LICENSE` |
| `skills\orchestration\SKILL.md` | `https://raw.githubusercontent.com/DannyMac180/astra-advisor/master/plugins/astra-advisor/skills/orchestration/SKILL.md` |

## Technical summary — Astra orchestration algorithm (as evidenced)

GPT-6 Astra 是「架构师 + 验收所有者」：在有界交付物（bounded deliverable）上
动态决策是否并行委派。

- **路线声明**：能力预检后、第一个实现/委派工具调用前，输出机器可审计的
  `ASTRA ROUTE`（parent / delegation / risk），模型与 effort 按"可观测"记录，
  不可观测就说 unobservable，绝不假装运行时固定了某个 pin。
- **动态委派**：通过通用 `collaboration.spawn_agent` 工具（显式 `model` +
  `reasoning_effort` + `fork_turns: none`），从 `gpt-5.6-sol / terra / luna`
  按任务风险、上下文与独立工作量动态选择——无预定义角色表、无固定数量上限；
  每个子智能体拿到具体、有界、独立的交付物；父会话继续有用工作，不重复委派。
- **可见性**：每个委派（含审查）派发前公布任务名/精确边界/请求的模型与 effort/
  选择理由；返回时给出实际状态与观测到的设置（不一致时两者都展示）。
- **fail closed**：选中模型/effort/工具缺失、冲突或不可观测 → 该委派关闭并报告，
  绝不静默替换。
- **验收门**：实质性实现必须由 Astra 亲检完整 diff 并重跑检查，然后派遣新鲜
  只读审查员（同样动态选择模型/effort），要求回传
  `VERDICT: ship | fix-first | rethink` + REASON/FINDINGS/RESIDUAL RISK；
  只接受 `ship`；`fix-first` → 父修正 + 复验 + 新审查；`rethink` → 修订计划。
- **成本票据**：任务结束输出 API 等效成本票据（区分整任务/仅委派/部分覆盖，
  缺失用量如实说明）。

DeepAhead 的 Rust 移植引擎：`src-tauri/src/ai/workflow/astra.rs`（DSA 模式）。

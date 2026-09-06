# PROVENANCE — MoonshotAI/kimi-code (agent-core-v2)

- **Upstream:** https://github.com/MoonshotAI/kimi-code (MIT)
- **License:** MIT (see `LICENSE`; © 2026 Moonshot AI)
- **Commit SHA:** `c52d583143ed99f3e8deb087a97c909439b424c9` (main, 2026-09-02T13:33:58Z; "docs: remove agent-core-v2, kap-server, and transcript package docs (#3481)")
- **Fetch date:** 2026-09-02
- **Fetch method:** `web_fetch` only; contents saved verbatim.

## Saved files

| Local path (under `vendor\kimi-code\`) | Upstream URL |
|---|---|
| `LICENSE` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/LICENSE` |
| `AGENTS.md` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/AGENTS.md` |
| `agent-core-v2\CHANGELOG.md` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/packages/agent-core-v2/CHANGELOG.md` |
| `agent-core-v2\src\index.ts` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/packages/agent-core-v2/src/index.ts` |
| `agent-core-v2\src\agent\task\errors.ts` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/packages/agent-core-v2/src/agent/task/errors.ts` |
| `agent-core-v2\src\agent\task\types.ts` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/packages/agent-core-v2/src/agent/task/types.ts` |
| `agent-core-v2\src\agent\state\agentState.ts` | `https://raw.githubusercontent.com/MoonshotAI/kimi-code/main/packages/agent-core-v2/src/agent/state/agentState.ts` |

Notes: `task/types.ts` and `task/errors.ts` were the two smallest source files in `src/agent/task/` per the contents listing (318 B and 1522 B); `state/agentState.ts` (756 B) was the smallest in `src/agent/state/`. All fetches returned HTTP 200; no truncation observed. The `commits/main` response also reported `files[].patch` data (removed docs) but that is upstream content, not evidence of any issue.

## Technical summary — Kimi agent workflow algorithm (as evidenced)

agent-core-v2 is the DI × Scope agent engine behind kap-server. Services register at four `LifecycleScope` tiers (App / Workspace / Session / Agent, `app/scopes.ts`); per-agent state and the turn loop live at Agent scope, while App holds registries/factories and Workspace holds live session handlers. `index.ts` exposes the hot-path units:

- **Agent loop** (`agent/loop`): `loop`, `loopService`, `loopContinuation`, `handoffStep`, gated by a `[loop_control]` config section. Per-turn step caps come from `KIMI_LOOP_MAX_STEPS_PER_TURN`; per-step retries come from `KIMI_LOOP_MAX_RETRIES_PER_STEP` (`stepRetry` / `stepRetryService`). The turn engine (`llmRequester`, `contextMemory`, `contextProjector`, `fullCompaction`) wraps provider calls (`kosong`) and replays wire records for context.
- **Tool cycle**: the LLM streams (`turn.started` / `turn_ended`, `usage.record`), then tool calls are executed by `toolExecutor` (+ `toolHooks`) and results appended for the next step. Tools: os bash/glob/grep/read/write, edit, fetch-url, web-search, select-tools, todo-list, plan (enter/exit plan mode), goal (create/get/update/set-budget), cron, subagent `agent` tool, task-list/output/stop/wait, ask-user-question, and the swarm/tower multi-agent tool sets. `toolDedupe` collapses identical calls within one step; `toolResultTruncation` bounds result size; `toolSelect` exposes dynamic tools.
- **Phases / state**: the agent's live phase (idle, running, streaming, tool call, retrying, awaiting approval, interrupted, ended) is a single model field driven by turn events; live-only runtime state is `persist: false` while `IStateRegistry`-backed `replayableKeys()` (state/agentState.ts) survives replays.
- **Approval**: `permissionGate`, `toolApproval`, `permissionMode`/`permissionPolicy`/`permissionRules`, `sessionToolPolicyGate`; interaction ids (`approval_*`, `question_*`, `user_tool_*`) are minted engine-side; AskUserQuestion answers are keyed by question text.
- **Background tasks** (`agent/task`): `AgentTask` interface — `start(sink)`, `onDetach`, `forceStop`, `toInfo`. Statuses: running/completed/failed/timed_out/killed/lost (terminal set); `AgentTaskSink` carries AbortSignal + `appendOutput` + `settle`; `task.limit_exceeded` is retryable. Global limit `KIMI_CODE_BACKGROUND_MAX_RUNNING_TASKS`.
- **Sub-agent / tower**: session-scoped `subagent`/`spawn` + `agentTool`; custom agent Markdown frontmatter can restrict delegated sub-agent types; `runAgentTurn` (`AGENT_RUN_PROMPT_ORIGIN`) carries the orchestrator prompt onto subagent turns. `features/tower` (init/plan/spawn/merge/teardown/send/inbox/finding/review/mission/status) and `features/swarm` (`agent-swarm`) add multi-agent orchestration; `features/goal` provides persistent goal/budget/deadline tracking.

import { invoke } from "@tauri-apps/api/core";

// ─── Types ───
export interface Message { id?: string; role: string; content: string; type?: string; reasoning_content?: string | null; }
export interface AIResponse {
  message: Message;
  usage: { prompt_tokens: number; completion_tokens: number; total_tokens: number };
  mode: string;
}
export interface AgentResponse { message: Message; usage: AIResponse["usage"]; agent: string; mode: string; }
export interface AgentLoopResult {
  content: string;
  total_iterations: number;
  total_tool_calls: number;
  mode: string;
  event_count: number;
  run_id: string;
  context_tokens: number;
  compressed: boolean;
}export interface ModeInfo { id: string; name: string; desc: string; provider: string; emulated_model: string; coding_style: string; review_rigor: string; architecture_first: boolean; best_for: string[]; system_prompt_preview: string; engine?: string; upstream?: string; license?: string; mechanism?: string; }
export interface AgentDef { name: string; description: string; system_prompt: string; allowed_tools: string[]; }
export interface FileEntry { name: string; path: string; is_dir: boolean; size: number; children?: FileEntry[]; }
export interface DirListResult { entries: FileEntry[]; path: string; }

export interface GitStatus { branch: string; changes: string[]; staged: string[]; untracked: string[]; ahead: number; behind: number; clean: boolean; }
export interface GitLogEntry { hash: string; author: string; date: string; message: string; }
export interface GitDiffResult { files: string[]; diff: string; }
/** 提交图节点（对应上游 dsh-git-graph 的 GraphCommit） */
export interface GitGraphCommit {
  oid: string;
  short: string;
  parents: string[];
  author: string;
  /** 作者时间（Unix 秒，%at） */
  author_time: number;
  subject: string;
  /** 解析后的引用（已去掉 HEAD -> / tag: 前缀） */
  refs: string[];
}
/** 提交图视图（含分页信息） */
export interface GitGraphView {
  branch: string;
  commits: GitGraphCommit[];
  has_more: boolean;
}
/** 泳道字形（对齐上游 LaneGlyph） */
export type LaneGlyph = "node" | "pass" | "merge" | "gap";
/** 一行的泳道布局（对齐上游 GraphRowLanes） */
export interface GitGraphLaneRow {
  columns: LaneGlyph[];
  nodeColumn: number;
  merge: boolean;
}

export interface SSHConfig { host: string; port: number; username: string; password?: string; key_path?: string; }
export interface SSHExecResult { stdout: string; stderr: string; exit_code: number; }

export interface Session { id: string; name: string; mode: string; agent: string; messages: Message[]; created_at: string; updated_at: string; total_tokens: number; }
export interface SessionMeta { id: string; name: string; mode: string; agent: string; message_count: number; updated_at: string; }

export interface SafetyResult { rule_id: string; message: string; action: "confirm"|"warn"|"block"|"log"; triggered: boolean; }

// ─── 多模态视觉（DeepSeek-OCR / ModLens） ───
export interface VisionConfigInfo { provider: string; api_key: string; base_url: string; model: string; configured: boolean; }
export interface VisionResult { text: string; provider: string; image_path: string; }

// ─── 上下文占用 + 压缩 ───
export interface ContextUsage { tokens: number; limit: number; ratio: number; }
/** 长期记忆配置（dsh-memory-protocol 移植） */
export interface MemoryConfigInfo {
  enabled: boolean;
  enforce_weave: boolean;
  inject_weave: boolean;
  auto_ingest: boolean;
  allowlist: string[];
  fail_open: boolean;
}

// ─── 回合末裁决卡片（dsh-rule-engine-client 移植）───
export interface TurnCardBlock {
  i: number;
  tool: string;
  args: string;
  rule_id: string;
  title: string;
  reason: string;
  err_id: string;
  /** "" | "correct" | "incorrect" —— 非空即锁定（判例一次性） */
  label: string;
  labeled_at: number;
}
export interface TurnCard {
  key: string;
  session_id: string;
  message_id: string;
  turn: number;
  user_text: string;
  blocks: TurnCardBlock[];
  verdict: string;
  at: number;
}
export interface CompressResult {
  before_tokens: number;
  after_tokens: number;
  removed_messages: number;
  summary: string;
  compressed_messages: Message[];
}

export const tauriAPI = {
  // ─── 项目 ───
  createProject: (name: string, path: string) => invoke<string>("create_project", { name, path }),
  openProject: (path: string) => invoke<string>("open_project", { path }),

  // ─── 文件 ───
  listDirectory: (path: string, depth?: number) => invoke<DirListResult>("list_directory", { path, depth: depth??2 }),
  readFile: (path: string) => invoke<string>("smart_read_file", { path }),
  writeFile: (path: string, content: string) => invoke<string>("write_file_content", { path, content }),
  previewExcel: (path: string, sheet?: string) => invoke<string>("preview_excel_as_markdown", { path, sheet: sheet || null }),
  previewCsv: (path: string) => invoke<string>("preview_csv_as_markdown", { path }),

  // ─── AI ───
  listAIModes: () => invoke<{id:string;name:string;desc:string}[]>("list_ai_modes"),
  switchAIMode: (mode: string) => invoke<ModeInfo>("switch_ai_mode", { mode }),
  configureDeepSeek: (apiKey: string, baseUrl?: string, model?: string) => invoke<string>("configure_deepseek", { apiKey, baseUrl: baseUrl||null, model: model||null }),
  checkDeepSeekHealth: () => invoke<string>("check_deepseek_health"),

  // ─── 多模态视觉（DeepSeek-OCR / ModLens） ───
  configureVision: (provider: string, apiKey: string, baseUrl?: string, model?: string) => invoke<string>("configure_vision", { provider, apiKey, baseUrl: baseUrl||null, model: model||null }),
  getVisionConfig: () => invoke<VisionConfigInfo>("get_vision_config"),
  analyzeImage: (imagePath: string, prompt?: string) => invoke<VisionResult>("analyze_image", { imagePath, prompt: prompt||null }),
  saveTempImage: (data: string, ext: string) => invoke<string>("save_temp_image", { data, ext }),
  sendAIMessage: (mode: string, message: string, history: Message[], contextPaths: string[]) => invoke<AIResponse>("send_ai_message", { mode, message, history, contextPaths }),
  sendAIMessageStream: (mode: string, message: string, history: Message[], contextPaths: string[]) => invoke<{content:string;mode:string}>("send_ai_message_stream", { mode, message, history, contextPaths }),

  // ─── Agent Loop with Tools（Claude Code / Cursor 风格）───
  sendAIMessageWithTools: (
    mode: string,
    message: string,
    history: Message[],
    contextPaths: string[],
    workingDir?: string,
    approvalMode: string = "step",
    contextLimit?: number,
    autoCompress: boolean = false,
  ) =>
    invoke<AgentLoopResult>(
      "send_ai_message_with_tools",
      {
        mode, message, history, contextPaths,
        workingDir: workingDir || null,
        approvalMode,
        contextLimit: contextLimit ?? null,
        autoCompress,
      }
    ),
  // 应答一次工具调用审批（需分步确认模式；approvalId 来自 tool_approval_required 事件）
  respondToolApproval: (approvalId: string, approved: boolean) => invoke<void>("respond_tool_approval", { approvalId, approved }),
  // 查询某次 Agent 运行可撤销的文件变更数量（撤回对话）
  getRunUndoCount: (runId: string) => invoke<number>("get_run_undo_count", { runId }),
  // 撤销某次 Agent 运行的文件变更
  undoRunChanges: (runId: string) => invoke<string[]>("undo_run_changes", { runId }),
  // 订阅 agent 事件
  onAgentEvent: async (handler: (event: any) => void) => {
    const { listen } = await import("@tauri-apps/api/event");
    return listen("ai-agent-event", (e: any) => handler(e.payload));
  },

  // ─── Agent ───
  listAgents: () => invoke<AgentDef[]>("list_agents"),
  sendAgentMessage: (agentName: string, mode: string, message: string, history: Message[]) => invoke<AgentResponse>("send_agent_message", { agentName, mode, message, history }),
  runSafetyCheck: (content: string) => invoke<SafetyResult[]>("run_safety_check", { content }),

  // ─── 文件解析（多模态上下文预览）───
  parseContextFile: (path: string) => invoke<{
    path: string; content: string; format: string;
    size_bytes: number; is_binary: boolean; truncated: boolean;
    success: boolean; error: string | null;
  }>("parse_context_file", { path }),

  // ─── Git ───
  gitStatus: (path: string) => invoke<GitStatus>("git_status", { path }),
  gitDiff: (path: string, staged?: boolean) => invoke<GitDiffResult>("git_diff", { path, staged }),
  gitLog: (path: string, count?: number) => invoke<GitLogEntry[]>("git_log", { path, count: count??20 }),
  gitBranches: (path: string) => invoke<string[]>("git_branches", { path }),
  gitClone: (url: string, target: string, proxy?: string) => invoke<string>("git_clone", { url, target, proxy: proxy||null }),
  gitPush: (path: string, username: string, token: string, repo: string, branch: string, message: string) => invoke<string>("git_push", { path, username, token, repo, branch, message }),
  /** 提交图数据（「历史提交记录」的 git-graph 视图） */
  gitLogGraph: (path: string, count?: number, all?: boolean) =>
    invoke<GitGraphView>("git_log_graph", { path, count: count ?? 200, all: all ?? true }),
  /** 单个提交的改动详情 */
  gitCommitDetail: (path: string, hash: string) =>
    invoke<{ hash: string; stat: string; files: string[]; patch: string }>("git_commit_detail", { path, hash }),

  // ─── SSH ───
  sshTest: (config: SSHConfig) => invoke<string>("ssh_test_connection", { config }),
  sshExec: (config: SSHConfig, command: string) => invoke<SSHExecResult>("ssh_exec", { config, command }),
  sshReadFile: (config: SSHConfig, remotePath: string) => invoke<string>("ssh_read_file", { config, remotePath }),
  sshListDir: (config: SSHConfig, remotePath: string) => invoke<string[]>("ssh_list_dir", { config, remotePath }),

  // ─── 终端 ───
  openTerminal: (path: string) => invoke<string>("open_terminal", { path }),
  runCommand: (path: string, command: string) => invoke<string>("run_command", { path, command }),
  runFile: (path: string, runtime?: string) => invoke<string>("run_file", { path, runtime: runtime || null }),
  detectRuntimes: () => invoke<{name:string;version:string|null;available:boolean;path:string|null}[]>("detect_runtimes_enhanced"),

  // ─── 上下文占用比例 + 压缩 ───
  estimateContextUsage: (systemPrompt: string, messages: Message[], contextPaths: string[] = []) =>
    invoke<ContextUsage>("estimate_context_usage", { systemPrompt, messages, contextPaths }),
  compressContext: (messages: { role: string; content: string }[], maxTokens: number, preserveRecentTurns = 4) =>
    invoke<CompressResult>("compress_context", { messages, maxTokens, preserveRecentTurns }),
  getContextConfig: () => invoke<{ max_tokens: number; compression_threshold: number; preserve_recent_turns: number }>("get_context_config"),
  /** billion-context 引擎阈值（移植版） */
  contextEngineConfig: (contextLimit?: number) => invoke<Record<string, any>>("context_engine_config", { contextLimit: contextLimit ?? null }),

  // ─── 长期记忆协议（dsh-memory-protocol 移植）───
  getMemoryConfig: () => invoke<MemoryConfigInfo>("get_memory_config"),
  setMemoryConfig: (patch: Partial<MemoryConfigInfo>) => invoke<MemoryConfigInfo>("set_memory_config", patch as any),
  memoryWeave: (query: string, topK?: number) => invoke<string>("memory_weave", { query, topK: topK ?? null }),
  memoryIngest: (content: string, role?: string, kind?: string) =>
    invoke<any>("memory_ingest", { content, role: role ?? null, kind: kind ?? null }),
  memorySearch: (query: string, limit?: number) =>
    invoke<{ hits: { id: string; kind: string; text: string; role: string; created_at: number; score: number }[] }>(
      "memory_search", { query, limit: limit ?? null }),
  memoryStatus: () => invoke<{ records: number; dir: string; file: string; available: boolean }>("memory_status"),
  memoryRecent: (limit?: number) => invoke<{ records: any[] }>("memory_recent", { limit: limit ?? null }),
  memoryClear: () => invoke<number>("memory_clear"),

  // ─── 插件体检（dsh-plugin-vet 移植）───
  vetScan: (target: string) => invoke<{ report: any; scorecard: string }>("vet_scan", { target }),
  vetWriteHealthRecord: (target: string, name?: string, version?: string, risk?: string, recommendation?: string, notes?: string) =>
    invoke<string>("vet_write_health_record", { target, name: name ?? null, version: version ?? null, risk: risk ?? null, recommendation: recommendation ?? null, notes: notes ?? null }),
  vetListRecords: () => invoke<{ records: string[] }>("vet_list_records"),

  // ─── 规则引擎（dsh-rule-engine 移植）───
  rulesLoad: () => invoke<any>("rules_load"),
  rulesParse: (text: string) => invoke<any>("rules_parse", { text }),
  rulesAudit: (limit?: number) => invoke<{ records: any[]; path: string }>("rules_audit", { limit: limit ?? null }),
  rulesTestGuard: (tool: string, args?: any, userText?: string) =>
    invoke<any>("rules_test_guard", { tool, arguments: args ?? null, userText: userText ?? null }),
  rulesGetConfig: () => invoke<any>("rules_get_config"),
  rulesSetToggles: (patch: { enabled?: boolean; turn_card_enabled?: boolean; task_contract_enabled?: boolean }) =>
    invoke<any>("rules_set_toggles", patch),
  rulesGuardCommand: (input: string) => invoke<{ ok: boolean; text: string }>("rules_guard_command", { input }),
  rulesTurnCards: (sessionId?: string, limit?: number) =>
    invoke<{ cards: TurnCard[]; path: string }>("rules_turn_cards", { sessionId: sessionId ?? null, limit: limit ?? null }),
  rulesRateTurnCard: (key: string, blockIndex: number, verdict: string, expectedVerdict?: string) =>
    invoke<TurnCard>("rules_rate_turn_card", { key, blockIndex, verdict, expectedVerdict: expectedVerdict ?? null }),
  rulesAttachTurnCard: (key: string, messageId: string) =>
    invoke<TurnCard>("rules_attach_turn_card", { key, messageId }),
  rulesLabels: () => invoke<{ labels: { fingerprint: string; label: string; at: number; expires_at: number }[] }>("rules_labels"),
  rulesFingerprint: (command: string) => invoke<{ fingerprint: string | null }>("rules_fingerprint", { command }),

  // ─── 费用统计（dsh-cost-meter 移植）───
  costSnapshot: () => invoke<Record<string, any>>("cost_snapshot"),
  costSetConfig: (patch: Record<string, any>) => invoke<Record<string, any>>("cost_set_config", patch),
  costClear: () => invoke<{ cleared_days: number }>("cost_clear"),

  // ─── 会话 ───
  saveSession: (id: string, name: string, mode: string, agent: string, messages: Message[], totalTokens: number) => invoke<string>("save_session", { id, name, mode, agent, messages, totalTokens }),
  loadSession: (id: string) => invoke<Session>("load_session", { id }),
  listSessions: () => invoke<SessionMeta[]>("list_sessions"),
  deleteSession: (id: string) => invoke<string>("delete_session", { id }),

  // ─── CLI 桥接 ───
  checkDeepSeekCli: () => invoke<{available:boolean;version:string|null;install_hint:string|null}>("check_deepseek_cli"),
  runCliAgentTask: (workspace: string, task: string, personaPrompt: string, apiKey: string) => invoke<{success:boolean;output:string;error:string|null}>("run_cli_agent_task", { workspace, task, personaPrompt, apiKey }),
};

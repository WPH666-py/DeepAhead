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
}
export interface ModeInfo { id: string; name: string; desc: string; provider: string; emulated_model: string; coding_style: string; review_rigor: string; architecture_first: boolean; best_for: string[]; system_prompt_preview: string; engine?: string; upstream?: string; license?: string; mechanism?: string; }
export interface AgentDef { name: string; description: string; system_prompt: string; allowed_tools: string[]; }
export interface FileEntry { name: string; path: string; is_dir: boolean; size: number; children?: FileEntry[]; }
export interface DirListResult { entries: FileEntry[]; path: string; }

export interface GitStatus { branch: string; changes: string[]; staged: string[]; untracked: string[]; ahead: number; behind: number; clean: boolean; }
export interface GitLogEntry { hash: string; author: string; date: string; message: string; }
export interface GitDiffResult { files: string[]; diff: string; }

export interface SSHConfig { host: string; port: number; username: string; password?: string; key_path?: string; }
export interface SSHExecResult { stdout: string; stderr: string; exit_code: number; }

export interface Session { id: string; name: string; mode: string; agent: string; messages: Message[]; created_at: string; updated_at: string; total_tokens: number; }
export interface SessionMeta { id: string; name: string; mode: string; agent: string; message_count: number; updated_at: string; }

export interface SafetyResult { rule_id: string; message: string; action: "confirm"|"warn"|"block"|"log"; triggered: boolean; }

// ─── 多模态视觉（DeepSeek-OCR / ModLens） ───
export interface VisionConfigInfo { provider: string; api_key: string; base_url: string; model: string; configured: boolean; }
export interface VisionResult { text: string; provider: string; image_path: string; }

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
  sendAIMessageWithTools: (mode: string, message: string, history: Message[], contextPaths: string[], workingDir?: string, approvalMode: string = "step") =>
    invoke<AgentLoopResult>(
      "send_ai_message_with_tools",
      { mode, message, history, contextPaths, workingDir: workingDir || null, approvalMode }
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

  // ─── 会话 ───
  saveSession: (id: string, name: string, mode: string, agent: string, messages: Message[], totalTokens: number) => invoke<string>("save_session", { id, name, mode, agent, messages, totalTokens }),
  loadSession: (id: string) => invoke<Session>("load_session", { id }),
  listSessions: () => invoke<SessionMeta[]>("list_sessions"),
  deleteSession: (id: string) => invoke<string>("delete_session", { id }),

  // ─── CLI 桥接 ───
  checkDeepSeekCli: () => invoke<{available:boolean;version:string|null;install_hint:string|null}>("check_deepseek_cli"),
  runCliAgentTask: (workspace: string, task: string, personaPrompt: string, apiKey: string) => invoke<{success:boolean;output:string;error:string|null}>("run_cli_agent_task", { workspace, task, personaPrompt, apiKey }),
};

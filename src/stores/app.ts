import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { tauriAPI, type ModeInfo, type Message, type AgentDef, type FileEntry, type TurnCard, type PendingOp } from "../services/tauri-api";
import type { EditorTheme } from "../utils/codemirror";
import { applySkin, type SkinVariant } from "../utils/skins";

/** 生成消息 ID（WebView 支持 crypto.randomUUID 时优先） */
function newMsgId(): string {
  try {
    if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  } catch (_) {}
  return `m_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
}

// ─── 执行许可三档（规则引擎开关由档位唯一决定，不交给用户自选）───
export type ApprovalModeId = "step" | "risk" | "open";
export interface ApprovalModeMeta {
  id: ApprovalModeId;
  label: string;
  desc: string;
  /** 该档位下规则引擎是否全开 */
  ruleEngineOn: boolean;
  /** 该档位下是否每一个工具调用都要用户确认 */
  gateEveryCall: boolean;
}
export const APPROVAL_MODES: ApprovalModeMeta[] = [
  {
    id: "step",
    label: "需逐步确认",
    desc: "每一步工具调用都先查给你看：规则引擎所有开关全开。",
    ruleEngineOn: true,
    gateEveryCall: true,
  },
  {
    id: "risk",
    label: "仅确认风险操作",
    desc: "规则引擎所有开关全开，只有风险操作（写盘 / 命令 / 删除 / 推送）才查给你看。",
    ruleEngineOn: true,
    gateEveryCall: false,
  },
  {
    id: "open",
    label: "全流程开放",
    desc: "规则引擎所有开关全关，所有操作永久放行，Agent 一路自主跑完。",
    ruleEngineOn: false,
    gateEveryCall: false,
  },
];
export function normalizeApprovalMode(v: string | null | undefined): ApprovalModeId {
  return v === "risk" || v === "open" || v === "step" ? v : "step";
}
export function approvalModeLabel(id: string): string {
  return APPROVAL_MODES.find(m => m.id === id)?.label || id;
}

// ─── 日志（"日志"面板数据源）───
export type LogKind = "mode" | "question" | "answer" | "tool" | "system" | "context";
export interface LogEntry {
  id: string;
  ts: number;
  kind: LogKind;
  title: string;
  detail?: string;
}

/** 日志级别：失败 / 危险 → warn，其余 → info（磁盘日志按级别标注） */
function logLevelOf(kind: LogKind, title: string): "info" | "warn" {
  if (kind === "system" && /(失败|错误|拦截|拒绝|超时|异常|error|failed)/i.test(title)) return "warn";
  return "info";
}

// ─── 上下文占用 ───
/** 上下文占用达到该比例时提示/自动压缩 */
export const CONTEXT_WARN_RATIO = 0.85;
export const DEFAULT_CONTEXT_LIMIT = 128000;

// ─── 输入区图片 ───
/** 每次提问最多可附加的图片数 */
export const MAX_PASTE_IMAGES = 6;
export interface ImageAttachment { path: string; preview: string; name: string; }

/** 日志中的参数美化（JSON 字符串 → 缩进 JSON），只读取叶子字段，避免序列化运行时对象 */
function formatLogArgs(args: unknown): string {
  if (args == null) return "(无参数)";
  if (typeof args === "string") {
    try { return JSON.stringify(JSON.parse(args), null, 2); } catch { return args; }
  }
  try { return JSON.stringify(args, null, 2); } catch { return String(args); }
}

export const useAppStore = defineStore("app", () => {
  const currentProject = ref<string | null>(null);
  const currentMode = ref<string>("dsh");
  const currentAgent = ref<string>(""); // "" = 无 Agent
  const apiKey = ref<string>("");
  const baseUrl = ref<string>("https://api.deepseek.com");
  const model = ref<string>("deepseek-chat");
  const editorTheme = ref<EditorTheme>((localStorage.getItem("editorTheme") as EditorTheme) || "classic");

  // 模式信息（元数据 + 原生 System Prompt 预览；无 Persona 注入层）
  const modeInfo = ref<ModeInfo | null>(null);
  const modeInfoLoading = ref(false);

  // Agent 列表
  const agents = ref<AgentDef[]>([]);

  // AI 对话
  const messages = ref<Message[]>([]);
  const isLoading = ref(false);
  const totalTokens = ref(0);
  const streamingContent = ref("");  // 流式响应当前累积内容
  // 上下文统计（最近一次 Agent 运行发送给模型的估算 Token 数）
  const lastContextTokens = ref(0);
  // 每条用户消息触发的 Agent 运行 ID（撤回对话时按 run 回滚文件变更）
  const runIdsByUserMsg = new Map<string, string>();

  // ─── 上下文占用比例 + 压缩（自动 / 手动）───
  // contextLimit：模型上下文窗口（Token）；contextTokens：当前对话占用（实时估算）
  const contextLimit = ref<number>(Number(localStorage.getItem("deep-ide-context-limit")) || DEFAULT_CONTEXT_LIMIT);
  const contextTokens = ref(0);
  // 压缩模式："auto" = 超过 85% 自动压缩用户上下文（不清空对话）；"manual" = 仅提示用户手动压缩
  const compressionMode = ref<"auto" | "manual">(
    (localStorage.getItem("deep-ide-compression-mode") as "auto" | "manual") || "manual"
  );
  /** 当前占用比例 0~1 */
  const contextRatio = computed(() =>
    contextLimit.value > 0 ? Math.min(contextTokens.value / contextLimit.value, 1) : 0
  );
  /** 是否已达到提示阈值（85%） */
  const contextWarning = computed(() => contextRatio.value >= CONTEXT_WARN_RATIO);
  const contextPercent = computed(() => Math.round(contextRatio.value * 1000) / 10);

  // ─── 日志：模式切换 / 每轮提问与回复 / 工具调用 / 操作过程 ───
  // 每条记录都**实时落盘**到用户本机安装目录（%LOCALAPPDATA%\DeepAhead\logs），
  // 用户不打开面板也能随时查看、复现、报障。
  const sessionLogs = ref<LogEntry[]>([]);
  function appendLog(kind: LogKind, title: string, detail?: string) {
    sessionLogs.value.push({ id: newMsgId(), ts: Date.now(), kind, title, detail });
    // 防止无限增长（保留最近 2000 条）
    if (sessionLogs.value.length > 2000) sessionLogs.value.splice(0, sessionLogs.value.length - 2000);
    // 实时落盘（失败静默：日志是旁路，不能影响主流程）
    void writeLogToDisk(logLevelOf(kind, title), kind, title, detail);
  }
  /** 把一条日志追加到磁盘日志文件（Tauri 环境外静默跳过，便于纯浏览器预览） */
  async function writeLogToDisk(level: "info" | "warn", kind: string, title: string, detail?: string) {
    try {
      const msg = detail ? `${title}\n${detail}` : title;
      await tauriAPI.runtimeLogWrite(level, `frontend/${kind}`, msg);
    } catch (_) { /* 非 Tauri 环境或写盘失败：忽略 */ }
  }
  function clearLogs() { sessionLogs.value = []; }

  // ─── 待发送的粘贴图片（每次提问最多 MAX_PASTE_IMAGES 张）───
  // preview 用于缩略图展示，path 用于发送时交给视觉引擎识别
  const pastedImages = ref<ImageAttachment[]>([]);

  // ─── 回合末裁决卡片（dsh-rule-engine-client 移植）───
  // 按 card.key 索引；message_id 回填后可挂在对应助手消息下渲染
  const turnCards = ref<Record<string, TurnCard>>({});
  /** 取某条消息上的裁决卡片（无则 undefined） */
  function turnCardForMessage(messageId: string): TurnCard | undefined {
    return Object.values(turnCards.value).find(c => c.message_id === messageId);
  }
  /**
   * 回合裁决卡片：用户只回答一次。
   *   - **❌ 放行**（verdict="incorrect"）：登记一次性放行 → Agent 继续跑
   *     （循环还在等着就地续跑；循环已结束则自动回复「继续」再跑一轮）。
   *   - **✅ 拦截**（verdict="correct"）：拦住该操作，Agent 换方案或先向你确认。
   */
  async function rateTurnCard(key: string, blockIndex: number, verdict: "correct" | "incorrect") {
    const card = turnCards.value[key];
    const blk = card?.blocks.find(b => b.i === blockIndex);
    const runId = card?.session_id || "";
    const approve = verdict === "incorrect";
    try {
      const saved = await tauriAPI.rulesRateTurnCard(key, blockIndex, verdict);
      turnCards.value = { ...turnCards.value, [saved.key]: saved };
      // 语义映射到后端待决队列：❌ = 放行，✅ = 拦截
      // 后端返回的 PendingOp 带有被拦调用的**完整参数**，续跑据此原样重放该调用。
      let resolvedOp: PendingOp | null = null;
      try {
        resolvedOp = await tauriAPI.rulesResolvePending(runId, blockIndex, approve);
      } catch (e) {
        // 记录找不到（例如重启后补裁、或本轮没有硬门记录）不算失败，只记日志
        appendLog("system", "放行登记未命中待决队列（不影响判定）", String(e));
      }
      if (approve) {
        resumeCtx = resolvedOp || {
          run_id: runId,
          i: blockIndex,
          tool: blk?.tool || "",
          op_type: "",
          path: "",
          args_full: "",
          args: blk?.args || "",
          reason: blk?.reason || "",
          err_id: blk?.err_id || "",
          resolved: true,
          allowed: true,
          at: Date.now(),
        };
        appendLog(
          "system",
          `❌ 放行该操作（${blk?.tool || "工具"}）`,
          "规则引擎已登记一次性放行；Agent 正在继续，循环已结束时会自动回复「继续」。"
        );
      } else {
        appendLog(
          "system",
          `✅ 拦截该操作（${blk?.tool || "工具"}）`,
          blk ? `规则 ${blk.rule_id}｜${blk.reason}` : undefined
        );
      }
      // 循环已经结束 → 立刻把「继续」发出去
      if (!isLoading.value) void flushResume();
    } catch (e: any) {
      addSystemMessage(`裁决登记失败: ${e}`);
    }
  }
  /** 载入已落盘的裁决卡片（重启后仍可跨回合补裁） */
  async function loadTurnCards(sessionId?: string) {
    try {
      const r = await tauriAPI.rulesTurnCards(sessionId, 200);
      const map: Record<string, TurnCard> = {};
      for (const c of r.cards || []) map[c.key] = c;
      turnCards.value = { ...turnCards.value, ...map };
    } catch (_) { /* 读取失败静默 */ }
  }

  // Agent Loop 工具调用追踪
  const toolCalls = ref<Array<{
    id: string;
    name: string;
    arguments: any;
    success?: boolean;
    output?: string;
    status: "pending" | "awaiting" | "running" | "done" | "error";
    /** 后端心跳上报的已执行秒数（tool_progress 事件） */
    elapsedSecs?: number;
  }>>([]);
  const agentIterations = ref(0);
  const agentMaxIterations = ref(0);
  const useTools = ref<boolean>(true); // 是否启用工具调用（Claude Code 模式）

  // ─── 执行许可三档（对标 Harness 审批；规则引擎开关由档位唯一决定，不交给用户自选）───
  //  "step" = 需逐步确认      → 规则引擎全开 + 每一步都查看（每个工具调用都弹卡片）
  //  "risk" = 仅确认风险操作  → 规则引擎全开，只对风险操作弹卡片
  //  "open" = 全流程开放      → 规则引擎所有开关全关，所有操作永久放行
  const approvalMode = ref<ApprovalModeId>(
    normalizeApprovalMode(localStorage.getItem("deepahead-approval-mode"))
  );
  /**
   * 切换执行许可档位：本地持久化 + **后端联动规则引擎开关**。
   * 用户只选档位，规则引擎的开关由档位唯一决定（全流程开放 = 全关 + 永久放行）。
   */
  function setApprovalMode(mode: string) {
    const m = normalizeApprovalMode(mode);
    approvalMode.value = m;
    localStorage.setItem("deepahead-approval-mode", m);
    const meta = APPROVAL_MODES.find(x => x.id === m);
    appendLog("mode", `执行许可 → ${meta?.label || m}`, meta?.desc);
    tauriAPI.rulesLinkMode(m)
      .then(r => {
        appendLog(
          "system",
          `规则引擎已由「${r.label || meta?.label}」档位接管`,
          `引擎 ${r.rule_engine_on ? "全开" : "全关（永久放行）"}｜裁决卡片 ${r.turn_card_on ? "开" : "关"}｜逐调用审批 ${r.gate_every_call ? "开" : "关"}`
        );
      })
      .catch(e => console.warn("[DeepAhead] 规则引擎联动失败:", e));
  }
  // 等待用户批准的当前工具调用（需逐步确认 / 仅确认风险操作档位）
  const pendingApproval = ref<{ approvalId: string; toolId: string; name: string; arguments: any } | null>(null);
  /**
   * 前端应答：❌ 放行（approved=true）/ ✅ 拦截（approved=false）。
   * 后端随后把结果回灌给模型，Agent 继续跑。
   */
  async function respondApproval(approved: boolean) {
    const p = pendingApproval.value;
    if (!p) return;
    pendingApproval.value = null;
    try {
      await tauriAPI.respondToolApproval(p.approvalId, approved);
      appendLog(
        "tool",
        approved ? `❌ 已放行 ${p.name}（自动回复「继续」）` : `✅ 已拦截 ${p.name}`,
        formatLogArgs(p.arguments)
      );
    } catch (e: any) {
      addSystemMessage(`审批应答失败: ${e}`);
      pendingApproval.value = null;
    }
  }

  // ─── 裁决卡片 → 放行 → 自动继续（续跑队列）───
  /** 待续跑的操作（用户在卡片上选择 ❌ 放行后登记） */
  let resumeCtx: PendingOp | null = null;
  /**
   * 续跑执行器：由 EditorPage 注册。
   * 之所以用回调而不是直接调 sendMessageWithTools：续跑必须复用发送链路
   * （文件树刷新 / 上下文路径 / 日志 / 看门狗），不能另起一套。
   */
  let resumeRunner: ((op: PendingOp | null) => Promise<void>) | null = null;
  function setResumeRunner(fn: (op: PendingOp | null) => Promise<void>) {
    resumeRunner = fn;
  }
  /** 是否有待续跑的操作 */
  function hasPendingResume(): boolean {
    return resumeCtx !== null;
  }
  /**
   * Agent 跑完后调用：把「待续跑」变成真正的一轮「继续」。
   * 返回 true 表示已接管续跑（调用方不要再重复触发）。
   */
  async function flushResume(): Promise<boolean> {
    if (isLoading.value) return false;
    const op = resumeCtx;
    if (!op) return false;
    resumeCtx = null;
    appendLog(
      "system",
      "❌ 已放行，自动回复「继续」",
      `规则引擎写入一次性放行：${op.tool}${op.path ? " → " + op.path : ""}`
    );
    if (!resumeRunner) {
      appendLog("system", "续跑执行器未注册：请手动回复「继续」");
      return false;
    }
    try {
      await resumeRunner(op);
    } catch (e: any) {
      appendLog("system", "续跑失败", String(e));
    }
    return true;
  }

  // 文件树
  const fileTree = ref<FileEntry[]>([]);
  const fileTreePath = ref<string>("");
  const selectedFile = ref<string>("");

  const displayMessages = computed(() => messages.value);

  // ─── 项目 ───
  function setProject(path: string) { currentProject.value = path; }
  async function openProject(path: string) {
    await tauriAPI.openProject(path);
    currentProject.value = path;
  }
  function closeProject() { currentProject.value = null; }

  // ─── 文件 ───
  async function loadFileTree(path: string) {
    try {
      const result = await tauriAPI.listDirectory(path, 3);
      fileTree.value = result.entries;
      fileTreePath.value = result.path;
    } catch (e: any) {
      console.error("Failed to load file tree:", e);
    }
  }

  // ─── AI 模式 ───
  async function switchMode(mode: string) {
    currentMode.value = mode;
    modeInfoLoading.value = true;
    try {
      modeInfo.value = await tauriAPI.switchAIMode(mode);
      const name = modeInfo.value?.name || mode.toUpperCase();
      appendLog(
        "mode",
        `切换模式 → ${name}`,
        [modeInfo.value?.engine, modeInfo.value?.upstream, modeInfo.value?.mechanism]
          .filter(Boolean)
          .join(" · ") || undefined
      );
    } catch (e: any) {
      addSystemMessage(`模式切换失败: ${e}`);
      appendLog("mode", `模式切换失败 → ${mode}`, String(e));
    } finally {
      modeInfoLoading.value = false;
    }
  }

  async function loadAgents() {
    try { agents.value = await tauriAPI.listAgents(); }
    catch (e: any) { console.error("Failed to load agents:", e); }
  }

  async function configureApiKey(key: string) {
    apiKey.value = key;
    try {
      await tauriAPI.configureDeepSeek(key, baseUrl.value, model.value);
      addSystemMessage("DeepSeek API 连接成功");
    } catch (e: any) {
      addSystemMessage(`API 配置失败: ${e}`);
    }
  }

  // ─── 发送消息（流式）───
  async function sendMessageStream(content: string, contextPaths: string[] = []) {
    if (!content.trim()) return;
    if (!apiKey.value) {
      addSystemMessage("请先配置 DeepSeek API Key");
      return;
    }

    const userMsg: Message = { id: newMsgId(), role: "user", content, type: "user" };
    messages.value.push(userMsg);
    isLoading.value = true;
    streamingContent.value = "";
    const history = messages.value.filter(m => m.role !== "system");

    // 添加占位消息，用于流式更新
    messages.value.push({ id: newMsgId(), role: "assistant", content: "", type: "assistant" });
    const msgIndex = messages.value.length - 1;

    // 兜底：20 分钟未返回则强制恢复界面（正常长流式请求不会受影响）
    let streamSettled = false;
    const streamWatchdog = setTimeout(() => {
      if (!streamSettled) {
        console.warn("[DeepAhead] Stream IPC did not settle; forcing UI recovery.");
        messages.value[msgIndex].content = messages.value[msgIndex].content || "⚠️ 会话在后台中断，界面已自动恢复。";
        isLoading.value = false;
      }
    }, 20 * 60 * 1000);

    try {
      await tauriAPI.sendAIMessageStream(currentMode.value, content, history, contextPaths);
      streamSettled = true;
      clearTimeout(streamWatchdog);
      // 流式 completion 后，content 从 event 中积累
      messages.value[msgIndex].content = streamingContent.value;
      appendLog("answer", `AI 回复（${currentMode.value.toUpperCase()} 模式）`, streamingContent.value);
    } catch (e: any) {
      streamSettled = true;
      clearTimeout(streamWatchdog);
      messages.value[msgIndex].content = `错误: ${e}`;
      appendLog("system", "AI 回复失败", String(e));
    } finally {
      isLoading.value = false;
    }
  }

  // ─── 发送消息（带 9 个工具的 Agent Loop）───
  /**
   * @param requestOverride 直接发送给模型的完整提示词（用于图片识别结果等预先组装的内容）；
   *                        不传则根据 content 自动判定是否需要"写文档"后缀
   */
  async function sendMessageWithTools(
    content: string,
    contextPaths: string[] = [],
    workingDir?: string,
    requestOverride?: string,
    resumeOp?: PendingOp | null
  ) {
    if (!content.trim()) return;
    if (!apiKey.value) {
      addSystemMessage("请先配置 DeepSeek API Key");
      return;
    }

    appendLog("question", content, contextPaths.length ? `上下文文件：\n${contextPaths.join("\n")}` : undefined);
    const userMsg: Message = { id: newMsgId(), role: "user", content, type: "user" };
    messages.value.push(userMsg);
    isLoading.value = true;
    streamingContent.value = "";
    toolCalls.value = [];
    agentIterations.value = 0;
    agentMaxIterations.value = 0;
    const history = messages.value.filter(m => m.role !== "system");

    // 添加占位消息
    messages.value.push({ id: newMsgId(), role: "assistant", content: "🛠 工具调用中...\n", type: "assistant" });
    const msgIndex = messages.value.length - 1;
    let accumulatedText = "";

    function updateAssistantContent() {
      const maxI = agentMaxIterations.value || "∞";
      messages.value[msgIndex].content = `🛠 [${agentIterations.value}/${maxI} 步 | 已调 ${toolCalls.value.length} 个工具]\n\n${accumulatedText}`;
    }

    // 非代码生成请求：要求 AI 把结果写成 Markdown 文件
    const codeGenPatterns = [
      /代码/, /code/, /python|py\b/, /javascript|js\b/, /typescript|ts\b/,
      /\bjava\b/, /c\+\+|cpp/, /rust|go\b|php|ruby|swift|kotlin/,
      /写.*程序/, /写.*脚本/, /生成.*代码/, /实现.*功能/, /编写/,
      /函数|class|接口|\bapi\b/, /\bprogram|\bscript/
    ];
    const isCodeRequest = codeGenPatterns.some(p => p.test(content.toLowerCase()));
    // 续跑轮：明确告诉模型"用户已放行，立刻用同样的工具与参数重试"，并带上续跑上下文
    const resumeContext = resumeOp?.tool
      ? {
          run_id: resumeOp.run_id,
          tool: resumeOp.tool,
          op_type: resumeOp.op_type,
          path: resumeOp.path,
          args: resumeOp.args_full,
          block_index: resumeOp.i,
        }
      : null;
    const resumeDirective = resumeOp?.tool
      ? `[System] 用户在回合裁决卡片上选择了 ❌ 放行，规则引擎已为下面这个调用登记**一次性放行**：\n` +
        `- 工具：\`${resumeOp.tool}\`\n` +
        (resumeOp.path ? `- 路径：\`${resumeOp.path}\`\n` : "") +
        (resumeOp.op_type ? `- 操作类型：${resumeOp.op_type}\n` : "") +
        `请**立即重新发起同一个调用**（工具名与参数保持一致，以便消费这次一次性放行），不要再向用户请示；` +
        `放行成功后继续完成原任务。若该操作已不再必要，直接说明原因并给出最终结论。`
      : "";
    const requestContent = requestOverride
      ?? (isCodeRequest || resumeOp?.tool
        ? (resumeDirective ? `${content}\n\n${resumeDirective}` : content)
        : `${content}\n\n[System] 本次请求不涉及代码生成。请把回答整理成 Markdown 文档并保存到工作区，文件名要反映主题。最终回复中只给出文件路径和简要说明，不要输出大段正文。`);

    // 防"思考中"卡死：事件后若 IPC 在超时内未返回，强制恢复界面
    let invokeSettled = false;
    let watchdogTimer: ReturnType<typeof setTimeout> | null = null;
    function armWatchdog(ms: number) {
      if (watchdogTimer) clearTimeout(watchdogTimer);
      watchdogTimer = setTimeout(() => {
        if (!invokeSettled) {
          console.warn("[DeepAhead] Agent loop IPC did not settle; forcing UI recovery.");
          messages.value[msgIndex].content =
            messages.value[msgIndex].content ||
            "⚠️ 会话在后台中断（网络/服务异常），界面已自动恢复。您可以重新发送该问题。";
          isLoading.value = false;
          invokeSettled = true;
          try { unlisten(); } catch (_) {}
        }
      }, ms);
    }
    function clearWatchdog() {
      if (watchdogTimer) { clearTimeout(watchdogTimer); watchdogTimer = null; }
    }
    let unlisten: () => void = () => {};

    try {
      // 订阅事件，实时更新 toolCalls 状态
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen("ai-agent-event", async (event: any) => {
        const ev = event.payload;
        if (!ev || !ev.kind || !ev.kind.type) return;
        const k = ev.kind;
        try {
          if (k.type === "started") {
            agentMaxIterations.value = k.max_iterations;
            // 兜底：整个运行最长 10 分钟无结果则强制恢复界面
            armWatchdog(600000);
          } else if (k.type === "iteration") {
            agentIterations.value = k.current;
            updateAssistantContent();
          } else if (k.type === "tool_call_requested") {
            toolCalls.value.push({
              id: k.id, name: k.name, arguments: k.arguments,
              status: "running"
            });
            appendLog("tool", `调用工具 ${k.name}`, formatLogArgs(k.arguments));
          } else if (k.type === "tool_approval_required") {
            // 需分步确认：弹出审批卡片，工具卡片进入等待态
            const t = toolCalls.value.find(t => t.id === k.id);
            if (t) {
              t.status = "awaiting";
              t.arguments = k.arguments;
            } else {
              toolCalls.value.push({
                id: k.id, name: k.name, arguments: k.arguments,
                status: "awaiting"
              });
            }
            pendingApproval.value = { approvalId: k.approval_id, toolId: k.id, name: k.name, arguments: k.arguments };
            appendLog("tool", `等待批准：${k.name}`, formatLogArgs(k.arguments));
          } else if (k.type === "tool_approval_resolved") {
            // 审批结果：批准 → 执行中；拒绝 → 记录拒绝
            const t = toolCalls.value.find(t => t.id === k.id);
            if (t) {
              t.status = k.approved ? "running" : "error";
              if (!k.approved) { t.success = false; t.output = k.output || "用户拒绝执行"; }
            }
            if (pendingApproval.value && pendingApproval.value.toolId === k.id) {
              pendingApproval.value = null;
            }
            appendLog("tool", k.approved ? "✅ 已批准执行" : "⛔ 用户拒绝执行");
          } else if (k.type === "tool_progress") {
            // 工具执行心跳：后端每 ~20s 报一次，避免长时间"思考中…"看起来像卡死
            const t = toolCalls.value.find(t => t.id === k.id);
            const secs = Number(k.elapsed_secs || 0);
            if (t) {
              t.status = "running";
              t.elapsedSecs = secs;
            }
            appendLog(
              "tool",
              `⏳ ${k.name} 仍在执行（${secs}s）`,
              "长任务属正常；若长时间无进展可点日志里的工具条目查看参数。"
            );
          } else if (k.type === "tool_call_executed") {
            const tc = toolCalls.value.find(t => t.id === k.id);
            if (tc) {
              tc.success = k.success;
              tc.output = k.output;
              tc.status = k.success ? "done" : "error";
            }
            appendLog(
              "tool",
              `${k.success ? "✓" : "✗"} ${k.name} 执行${k.success ? "完成" : "失败"}`,
              k.output
            );
          } else if (k.type === "assistant_text") {
            accumulatedText += k.content;
            updateAssistantContent();
          } else if (k.type === "context_usage") {
            // 实时上下文占用（后端每轮推送）
            applyContextUsage(k.tokens || 0);
          } else if (k.type === "turn_card") {
            // 回合末裁决卡片：挂到本轮助手消息上（对齐 dsh-rule-engine-client）
            const card = k.card;
            if (card && card.key) {
              const lastAssistant = [...messages.value].reverse().find(m => m.role === "assistant");
              card.message_id = lastAssistant?.id || "";
              turnCards.value = { ...turnCards.value, [card.key]: card };
              // 把 messageId 落盘：重启后仍能把卡片挂回原消息（可跨回合补裁）
              if (card.message_id) {
                try {
                  const saved = await tauriAPI.rulesAttachTurnCard(card.key, card.message_id);
                  turnCards.value = { ...turnCards.value, [saved.key]: saved };
                } catch (_) { /* 落盘失败不影响展示 */ }
              }
              appendLog(
                "system",
                `⚖️ 回合裁决卡片：${card.blocks?.length || 0} 条被拦记录`,
                "可在对话中逐条判定「拦对了 / 拦错了」（❌ 会使同类命令学习放行）"
              );
            }
          } else if (k.type === "text_audit") {
            // 文本审计：助手输出不可阻断，只把纠正文本注入会话
            const injection = String(k.injection || "");
            if (injection) {
              addSystemMessage(injection);
              appendLog(
                "system",
                "文本审计命中（已注入纠正）",
                Array.isArray(k.hits)
                  ? k.hits.map((h: any) => `规则 ${h.rule_id}：${h.title}`).join("\n")
                  : undefined
              );
            }
          } else if (k.type === "context_compressed") {
            const before = (k.before_tokens || 0) / 1000;
            const after = (k.after_tokens || 0) / 1000;
            applyContextUsage(k.after_tokens || 0);
            appendLog(
              "context",
              `上下文自动压缩：${before.toFixed(1)}k → ${after.toFixed(1)}k Tokens`,
              "历史过长，已保留最近对话（对话未被清空）"
            );
            addSystemMessage(`📦 上下文自动压缩：${before.toFixed(1)}k → ${after.toFixed(1)}k Tokens（历史过长，已保留最近对话）`);
          } else if (k.type === "done") {
            messages.value[msgIndex].content = accumulatedText || k.content;
            // thinking 模式要求 reasoning_content 随历史回传 → 存入消息
            if (k.reasoning_content) messages.value[msgIndex].reasoning_content = k.reasoning_content;
            appendLog("answer", `AI 回复（${currentMode.value.toUpperCase()} 模式）`, accumulatedText || k.content);
            armWatchdog(20000);
          } else if (k.type === "error") {
            messages.value[msgIndex].content = `❌ 错误: ${k.message}`;
            armWatchdog(20000);
          } else if (k.type === "file_changed") {
            // 工具改了文件，刷新文件树
            if (currentProject.value) {
              loadFileTree(currentProject.value);
            }
          }
        } catch (err: any) {
          console.error("[DeepAhead] agent event handler error:", err, ev);
        }
      });

      const wd = workingDir || currentProject.value || undefined;
      const result = await tauriAPI.sendAIMessageWithTools(
        currentMode.value,
        requestContent,
        history,
        contextPaths,
        wd,
        approvalMode.value,
        contextLimit.value,
        compressionMode.value === "auto"
      );
      invokeSettled = true;
      // 运行结束时清理遗留的待审批卡片（若审批门已超时关闭）
      pendingApproval.value = null;
      clearWatchdog();
      messages.value[msgIndex].content = messages.value[msgIndex].content || result.content;
      runIdsByUserMsg.set(userMsg.id || "", result.run_id);
      lastContextTokens.value = result.context_tokens || 0;
      applyContextUsage(result.context_tokens || 0);
      appendLog(
        "system",
        `✅ Agent Loop 完成：${result.total_iterations} 步 / ${result.total_tool_calls} 个工具调用`,
        `上下文占用 ${(contextRatio.value * 100).toFixed(1)}%（${((result.context_tokens || 0) / 1000).toFixed(1)}k / ${(contextLimit.value / 1000).toFixed(0)}k Tokens）`
      );
      addSystemMessage(`✅ Agent Loop 完成: ${result.total_iterations} 步, ${result.total_tool_calls} 个工具调用`);

      unlisten();
      // 每轮结束后：检查上下文占用（自动压缩 / 手动提示）
      await maybeAutoCompressContext();
    } catch (e: any) {
      invokeSettled = true;
      clearWatchdog();
      messages.value[msgIndex].content = `❌ 错误: ${e}`;
      lastContextTokens.value = 0;
      appendLog("system", `❌ Agent Loop 失败`, String(e));
    } finally {
      isLoading.value = false;
      // 本轮有被拦记录 + 用户已选择 ❌ 放行 → 自动回复「继续」，Agent 接着跑
      if (resumeCtx) {
        try { unlisten(); } catch (_) {}
        void flushResume();
      }
    }
  }

  function appendStreamToken(token: string) {
    streamingContent.value += token;
    // 更新最后一条 assistant 消息
    const msgs = messages.value;
    for (let i = msgs.length - 1; i >= 0; i--) {
      if (msgs[i].role === "assistant") {
        msgs[i].content = streamingContent.value;
        break;
      }
    }
  }

  // ─── 发送消息（普通 / Agent）───
  async function sendMessage(content: string, contextPaths: string[] = []) {
    if (!content.trim()) return;
    if (!apiKey.value) {
      addSystemMessage("请先配置 DeepSeek API Key");
      return;
    }

    messages.value.push({ id: newMsgId(), role: "user", content, type: "user" });
    isLoading.value = true;

    try {
      let resp;
      const history = messages.value.filter(m => m.role !== "system");

      if (currentAgent.value) {
        resp = await tauriAPI.sendAgentMessage(currentAgent.value, currentMode.value, content, history);
      } else {
        resp = await tauriAPI.sendAIMessage(currentMode.value, content, history, contextPaths);
      }

      resp.message.id = newMsgId();
      messages.value.push(resp.message);
      totalTokens.value += resp.usage.total_tokens;
    } catch (e: any) {
      messages.value.push({ id: newMsgId(), role: "assistant", content: `错误: ${e}` });
    } finally {
      isLoading.value = false;
    }
  }

  // ─── 撤回对话 ───
  /// 某条消息触发的 Agent 运行 ID（没有则返回空串）
  function runIdForMsg(msgId: string): string {
    return runIdsByUserMsg.get(msgId) || "";
  }
  /// 移除从 index 开始的所有消息
  function removeMessagesFrom(index: number) {
    if (index < 0 || index >= messages.value.length) return;
    messages.value.splice(index);
    toolCalls.value = [];
    agentIterations.value = 0;
    agentMaxIterations.value = 0;
  }
  /// 清除从 index 开始的消息对应的 run 追踪（避免内存泄漏）
  function clearRunIdsFrom(index: number) {
    const ids = messages.value.slice(index).map(m => m.id || "").filter(Boolean);
    for (const id of ids) runIdsByUserMsg.delete(id);
  }

  function addSystemMessage(content: string) {
    messages.value.push({ id: newMsgId(), role: "system", content, type: "system" });
  }

  // ─── 上下文占用比例：设置 / 更新 / 压缩 ───
  function setContextLimit(limit: number) {
    contextLimit.value = Math.max(1000, Math.floor(limit) || DEFAULT_CONTEXT_LIMIT);
    localStorage.setItem("deep-ide-context-limit", String(contextLimit.value));
  }
  function setCompressionMode(mode: "auto" | "manual") {
    compressionMode.value = mode;
    localStorage.setItem("deep-ide-compression-mode", mode);
    appendLog("context", `上下文压缩模式：${mode === "auto" ? "自动压缩" : "手动压缩"}`);
  }
  /** 后端返回的上下文用量 → 同步到界面 */
  function applyContextUsage(tokens: number) {
    if (typeof tokens === "number" && tokens >= 0) contextTokens.value = tokens;
  }
  /**
   * 本地重算上下文占用（无需等待 Agent 运行）。
   * 使用与后端一致的估算口径：消息内容字符数 / 2.5。
   */
  function recomputeContextUsage() {
    const chars = messages.value.reduce((sum, m) => sum + (m.content ? m.content.length : 0), 0);
    applyContextUsage(Math.ceil(chars / 2.5));
  }
  /** 手动压缩当前对话上下文（保留最近若干轮，不新建对话、不清空对话） */
  async function compressContextManually(): Promise<{ before: number; after: number } | null> {
    const history = messages.value
      .filter(m => m.role !== "system")
      .map(m => ({ role: m.role, content: m.content }));
    if (history.length === 0) {
      addSystemMessage("当前没有可压缩的上下文。");
      return null;
    }
    // 少于 9 条时压缩保留下限（最近 4 轮 = 8 条）已覆盖全部内容，压缩无意义
    if (history.length <= 8) {
      appendLog("context", "上下文轮次过少，无需压缩", `当前仅 ${history.length} 条消息，压缩不会释放占用`);
      return null;
    }
    try {
      const r = await tauriAPI.compressContext(history, contextLimit.value, 4);
      const before = r.before_tokens || 0;
      const after = r.after_tokens || 0;
      // 用压缩后的消息序列替换对话历史（真实降低后续每轮发送的上下文）
      if (Array.isArray(r.compressed_messages) && r.compressed_messages.length > 0) {
        messages.value = r.compressed_messages.map((m) => ({
          id: newMsgId(),
          role: m.role,
          content: m.content,
          type: m.type || m.role,
        }));
      }
      contextTokens.value = after;
      appendLog(
        "context",
        `压缩上下文：${(before / 1000).toFixed(1)}k → ${(after / 1000).toFixed(1)}k Tokens`,
        `释放 ${(((before - after) / Math.max(before, 1)) * 100).toFixed(1)}% 上下文占用（对话未被清空，仅压缩较早轮次）`
      );
      addSystemMessage(
        `📦 已压缩上下文：${(before / 1000).toFixed(1)}k → ${(after / 1000).toFixed(1)}k Tokens（对话未被清空）`
      );
      return { before, after };
    } catch (e: any) {
      addSystemMessage(`压缩上下文失败: ${e}`);
      return null;
    }
  }
  /**
   * 每轮结束后检查上下文占用：
   * - 自动模式：≥85% 自动压缩用户上下文（不清空对话）
   * - 手动模式：≥85% 仅提示用户压缩上下文或清空当前对话
   */
  async function maybeAutoCompressContext() {
    if (!contextWarning.value) return;
    if (compressionMode.value === "auto") {
      appendLog("context", `上下文占用 ${contextPercent.value}% ≥ 85%，触发自动压缩`, "自动压缩用户上下文，不清空对话");
      await compressContextManually();
    } else {
      appendLog(
        "context",
        `⚠️ 上下文占用 ${contextPercent.value}% 已超过 85%`,
        "手动压缩模式：建议压缩上下文或清空当前对话"
      );
      addSystemMessage(
        `⚠️ 当前对话上下文已占用 ${contextPercent.value}%（≥85%）。建议压缩上下文或清空当前对话（可在 AI 配置中设置自动压缩）。`
      );
    }
  }
  /** 清空会话：清空右侧 AI 对话内容与上下文统计，但保留"日志"面板内容 */
  function clearSession() {
    messages.value = [];
    totalTokens.value = 0;
    streamingContent.value = "";
    toolCalls.value = [];
    agentIterations.value = 0;
    agentMaxIterations.value = 0;
    lastContextTokens.value = 0;
    contextTokens.value = 0;
    pendingApproval.value = null;
    pastedImages.value = [];
    runIdsByUserMsg.clear();
  }

  // ─── 多模态视觉配置 ───
  async function configureVision(provider: string, key: string, baseUrl: string, model: string) {
    await tauriAPI.configureVision(provider, key, baseUrl, model);
    addSystemMessage(`视觉引擎已配置为 ${provider}/${model}`);
  }

  // ─── 粘贴图片：存临时文件 → 识别 → 与问题一起发送 ───
  /** 兼容旧代码：单图访问器（返回第一张） */
  const pastedImage = computed(() => pastedImages.value[0] ?? null);

  /** 追加一张粘贴图片；超出上限返回 false */
  async function addPastedImage(data: string, ext: string): Promise<boolean> {
    if (pastedImages.value.length >= MAX_PASTE_IMAGES) {
      appendLog("system", `粘贴图片被忽略`, `每次提问最多 ${MAX_PASTE_IMAGES} 张图片`);
      addSystemMessage(`每次提问最多 ${MAX_PASTE_IMAGES} 张图片，请先发送或移除已添加的图片。`);
      return false;
    }
    try {
      const path = await tauriAPI.saveTempImage(data, ext);
      pastedImages.value.push({
        path,
        preview: data,
        name: `图片 ${pastedImages.value.length + 1}`,
      });
      return true;
    } catch (e: any) {
      addSystemMessage(`粘贴图片失败: ${e}`);
      return false;
    }
  }
  /** 兼容旧调用 */
  async function setPastedImageFromBase64(data: string, ext: string) {
    await addPastedImage(data, ext);
  }
  function removePastedImage(index: number) {
    pastedImages.value.splice(index, 1);
  }
  function clearPastedImage() { pastedImages.value = []; }

  /// 发送一条带图片的消息：先识别每张图片，再把识别结果 + 用户问题一起发给模型
  async function sendWithImages(question: string, imagePaths: string[]) {
    if (!apiKey.value) { addSystemMessage("请先配置 DeepSeek API Key"); return; }
    isLoading.value = true;
    try {
      addSystemMessage(`正在识别 ${imagePaths.length} 张图片...`);
      appendLog("system", `视觉引擎识图：${imagePaths.length} 张`, imagePaths.join("\n"));
      const parts: string[] = [];
      for (let i = 0; i < imagePaths.length; i++) {
        const res = await tauriAPI.analyzeImage(imagePaths[i]);
        parts.push(`【图片 ${i + 1}】（识别引擎：${res.provider}）\n${res.text}`);
      }
      const fullPrompt = `用户上传了 ${imagePaths.length} 张图片，以下是图片识别结果：\n\n${parts.join("\n\n")}\n\n---\n用户问题：${question || "请描述并分析这些图片。"}`;
      if (useTools.value) {
        await sendMessageWithTools(question || `请分析这 ${imagePaths.length} 张图片`, [], undefined, fullPrompt);
      } else {
        await sendMessageStream(fullPrompt, []);
      }
    } catch (e: any) {
      addSystemMessage(`识图失败: ${e}`);
      isLoading.value = false;
    }
  }

  /// 兼容旧调用：单图发送
  async function sendWithImage(question: string, imagePath: string) {
    await sendWithImages(question, [imagePath]);
  }

  function clearMessages() {
    messages.value = [];
    totalTokens.value = 0;
  }

  function setEditorTheme(theme: EditorTheme) {
    editorTheme.value = theme;
    localStorage.setItem("editorTheme", theme);
  }

  // ─── 界面皮肤（覆盖文件树/编辑区/AI区域，与编辑器 CodeMirror 主题相互独立） ───
  const skinId = ref<string | null>(localStorage.getItem("DeepAhead-skin-id") || null);
  const skinVariant = ref<SkinVariant>((localStorage.getItem("DeepAhead-skin-variant") as SkinVariant) || "light");
  function setSkin(id: string | null, variant: SkinVariant = "light") {
    skinId.value = id;
    skinVariant.value = variant;
    if (id) {
      localStorage.setItem("DeepAhead-skin-id", id);
      localStorage.setItem("DeepAhead-skin-variant", variant);
    } else {
      localStorage.removeItem("DeepAhead-skin-id");
      localStorage.removeItem("DeepAhead-skin-variant");
    }
    applySkin(id, variant);
  }

  // ─── 安全检查 ───
  async function checkSafety(content: string) {
    try {
      const results = await tauriAPI.runSafetyCheck(content);
      for (const r of results) {
        if (r.triggered) {
          addSystemMessage(`${r.action === 'block' ? '🚫' : r.action === 'warn' ? '⚠️' : '🔍'} ${r.message}`);
        }
      }
    } catch (e: any) {
      console.error("Safety check failed:", e);
    }
  }

  return {
    currentProject, currentMode, currentAgent,
    apiKey, baseUrl, model,
    modeInfo, modeInfoLoading, agents,
    messages, isLoading, totalTokens, displayMessages, streamingContent, lastContextTokens,
    fileTree, fileTreePath, selectedFile,
    editorTheme,
    setProject, openProject, closeProject,
    loadFileTree,
    switchMode, loadAgents, configureApiKey,
    sendMessage, sendMessageStream, sendMessageWithTools, appendStreamToken, addSystemMessage, clearMessages,
    runIdForMsg, removeMessagesFrom, clearRunIdsFrom,
    toolCalls, agentIterations, agentMaxIterations, useTools,
    approvalMode, setApprovalMode, pendingApproval, respondApproval,
    /** 续跑链路：EditorPage 注册执行器，Agent 跑完后自动把「继续」发出去 */
    setResumeRunner, flushResume, hasPendingResume,
    setEditorTheme,
    skinId, skinVariant, setSkin,
    checkSafety,
    configureVision,
    pastedImage, pastedImages, setPastedImageFromBase64, addPastedImage, removePastedImage,
    clearPastedImage, sendWithImage, sendWithImages,
    // 日志（"日志"面板：模式切换 / 提问与回复 / 工具调用 / 操作过程）
    sessionLogs, appendLog, clearLogs,
    // 上下文占用比例 + 压缩（自动 / 手动）
    contextLimit, contextTokens, contextRatio, contextPercent, contextWarning,
    compressionMode, setContextLimit, setCompressionMode, applyContextUsage, recomputeContextUsage,
    compressContextManually, maybeAutoCompressContext,
    // 清空会话（保留日志）
    clearSession,
    // 回合末裁决卡片（dsh-rule-engine-client 移植）
    turnCards, turnCardForMessage, rateTurnCard, loadTurnCards,
  };
});

// 让 Vite 开发模式下 store 修改可热更新（避免新增 state/action 后旧实例残留）
if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useAppStore, import.meta.hot));
}

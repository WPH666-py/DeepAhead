import { defineStore, acceptHMRUpdate } from "pinia";
import { ref, computed } from "vue";
import { tauriAPI, type ModeInfo, type Message, type AgentDef, type FileEntry } from "../services/tauri-api";
import type { EditorTheme } from "../utils/codemirror";
import { applySkin, type SkinVariant } from "../utils/skins";

/** 生成消息 ID（WebView 支持 crypto.randomUUID 时优先） */
function newMsgId(): string {
  try {
    if (typeof crypto !== "undefined" && crypto.randomUUID) return crypto.randomUUID();
  } catch (_) {}
  return `m_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;
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

  // Agent Loop 工具调用追踪
  const toolCalls = ref<Array<{
    id: string;
    name: string;
    arguments: any;
    success?: boolean;
    output?: string;
    status: "pending" | "awaiting" | "running" | "done" | "error";
  }>>([]);
  const agentIterations = ref(0);
  const agentMaxIterations = ref(0);
  const useTools = ref<boolean>(true); // 是否启用工具调用（Claude Code 模式）

  // ─── 执行许可模式（对标 Harness 审批；开启工具后新增）───
  // "step" = 需分步确认；"open" = 全流程开放
  const approvalMode = ref<string>((localStorage.getItem("deepahead-approval-mode") as string) || "step");
  function setApprovalMode(mode: string) {
    approvalMode.value = mode;
    localStorage.setItem("deepahead-approval-mode", mode);
  }
  // 等待用户批准的当前工具调用（需分步确认模式）
  const pendingApproval = ref<{ approvalId: string; toolId: string; name: string; arguments: any } | null>(null);
  // 前端应答：允许 / 拒绝该工具调用
  async function respondApproval(approved: boolean) {
    const p = pendingApproval.value;
    if (!p) return;
    pendingApproval.value = null;
    try {
      await tauriAPI.respondToolApproval(p.approvalId, approved);
    } catch (e: any) {
      addSystemMessage(`审批应答失败: ${e}`);
      pendingApproval.value = null;
    }
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
    } catch (e: any) {
      addSystemMessage(`模式切换失败: ${e}`);
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
    } catch (e: any) {
      streamSettled = true;
      clearTimeout(streamWatchdog);
      messages.value[msgIndex].content = `错误: ${e}`;
    } finally {
      isLoading.value = false;
    }
  }

  // ─── 发送消息（带 9 个工具的 Agent Loop）───
  async function sendMessageWithTools(content: string, contextPaths: string[] = [], workingDir?: string) {
    if (!content.trim()) return;
    if (!apiKey.value) {
      addSystemMessage("请先配置 DeepSeek API Key");
      return;
    }

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
    const requestContent = isCodeRequest
      ? content
      : `${content}\n\n[System] 本次请求不涉及代码生成。请把回答整理成 Markdown 文档并保存到工作区，文件名要反映主题。最终回复中只给出文件路径和简要说明，不要输出大段正文。`;

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
      unlisten = await listen("ai-agent-event", (event: any) => {
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
          } else if (k.type === "tool_call_executed") {
            const tc = toolCalls.value.find(t => t.id === k.id);
            if (tc) {
              tc.success = k.success;
              tc.output = k.output;
              tc.status = k.success ? "done" : "error";
            }
          } else if (k.type === "assistant_text") {
            accumulatedText += k.content;
            updateAssistantContent();
          } else if (k.type === "context_compressed") {
            const before = (k.before_tokens || 0) / 1000;
            const after = (k.after_tokens || 0) / 1000;
            addSystemMessage(`📦 上下文自动压缩：${before.toFixed(1)}k → ${after.toFixed(1)}k Tokens（历史过长，已保留最近对话）`);
          } else if (k.type === "done") {
            messages.value[msgIndex].content = accumulatedText || k.content;
            // thinking 模式要求 reasoning_content 随历史回传 → 存入消息
            if (k.reasoning_content) messages.value[msgIndex].reasoning_content = k.reasoning_content;
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
      const result = await tauriAPI.sendAIMessageWithTools(currentMode.value, requestContent, history, contextPaths, wd, approvalMode.value);
      invokeSettled = true;
      // 运行结束时清理遗留的待审批卡片（若审批门已超时关闭）
      pendingApproval.value = null;
      clearWatchdog();
      messages.value[msgIndex].content = messages.value[msgIndex].content || result.content;
      runIdsByUserMsg.set(userMsg.id || "", result.run_id);
      lastContextTokens.value = result.context_tokens || 0;
      addSystemMessage(`✅ Agent Loop 完成: ${result.total_iterations} 步, ${result.total_tool_calls} 个工具调用`);

      unlisten();
    } catch (e: any) {
      invokeSettled = true;
      clearWatchdog();
      messages.value[msgIndex].content = `❌ 错误: ${e}`;
      lastContextTokens.value = 0;
    } finally {
      isLoading.value = false;
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

  // ─── 多模态视觉配置 ───
  async function configureVision(provider: string, key: string, baseUrl: string, model: string) {
    await tauriAPI.configureVision(provider, key, baseUrl, model);
    addSystemMessage(`视觉引擎已配置为 ${provider}/${model}`);
  }

  // ─── 粘贴图片：存临时文件 → 识别 → 与问题一起发送 ───
  // preview 用于前端缩略图预览，path 用于发送时识别
  const pastedImage = ref<{ path: string; preview: string } | null>(null);
  async function setPastedImageFromBase64(data: string, ext: string) {
    try {
      const path = await tauriAPI.saveTempImage(data, ext);
      pastedImage.value = { path, preview: data };
    } catch (e: any) {
      addSystemMessage(`粘贴图片失败: ${e}`);
    }
  }
  function clearPastedImage() { pastedImage.value = null; }

  /// 发送一条带图片的消息：先识别图片，再把识别结果 + 用户问题一起发给模型
  async function sendWithImage(question: string, imagePath: string) {
    if (!apiKey.value) { addSystemMessage("请先配置 DeepSeek API Key"); return; }
    isLoading.value = true;
    try {
      addSystemMessage("正在识别图片...");
      const res = await tauriAPI.analyzeImage(imagePath);
      const fullPrompt = `用户上传了一张图片（识别引擎：${res.provider}），以下是图片识别结果：\n\n${res.text}\n\n---\n用户问题：${question || "请描述并分析这张图片。"}`;
      if (useTools.value) {
        await sendMessageWithTools(fullPrompt, []);
      } else {
        await sendMessageStream(fullPrompt, []);
      }
    } catch (e: any) {
      addSystemMessage(`识图失败: ${e}`);
      isLoading.value = false;
    }
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
    setEditorTheme,
    skinId, skinVariant, setSkin,
    checkSafety,
    configureVision,
    pastedImage, setPastedImageFromBase64, clearPastedImage, sendWithImage,
  };
});

// 让 Vite 开发模式下 store 修改可热更新（避免新增 state/action 后旧实例残留）
if (import.meta.hot) {
  import.meta.hot.accept(acceptHMRUpdate(useAppStore, import.meta.hot));
}

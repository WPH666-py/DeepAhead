# DeepAhead 四种模式 → 厂商原装工作流引擎

> 版本 0.2.0 · 2026-09-06
>
> DeepAhead 的 DSH / DSK / DSA / DSF 四种模式，**无 Persona 注入层**（`personas/`
> 与 Persona 加载器已整体移除，不做风格模拟）：
> DSK / DSA / DSF 由 **Rust 移植的厂商原装工作流引擎**驱动，引擎与 DeepSeek 运行时
> （`deepseek.rs` 客户端 + `agent_loop.rs` 核心循环 + `tools.rs` 工具注册表）**强强结合**，
> 只消耗 DeepSeek Token。开启「工具（Agent）」后还提供执行许可（需分步确认 /
> 全流程开放），对标 DeepSeek Harness 审批门。

## 一、引擎总览

| 模式 | 引擎 | 上游原装仓库（Open Source） | 许可证 | 引擎源码位置 |
|------|------|---------------------------|--------|--------------|
| **DSH** | DeepSeek Harness 原生 Agent | DeepSeek Harness / deepseek-ai | MIT | `src/ai/agent_loop.rs`（原生循环） |
| **DSK** | Kimi K3 / Kimi Code CLI | [`MoonshotAI/kimi-code`](https://github.com/MoonshotAI/kimi-code) | MIT | `src/ai/workflow/kimi.rs` |
| **DSA** | GPT-6 Astra 动态委派 | [`DannyMac180/astra-advisor`](https://github.com/DannyMac180/astra-advisor) | MIT | `src/ai/workflow/astra.rs` |
| **DSF** | Claude Fable 5.1 剧本制 | [`codejunkie99/fable-orchestrator`](https://github.com/codejunkie99/fable-orchestrator) + [`DivyamTalwar/fablewright`](https://github.com/DivyamTalwar/fablewright) | MIT | `src/ai/workflow/fable.rs` |

四种模式共用同一个 DeepSeek V4 运行时与同一套 18 工具（read/write/edit/batch_*/bash/
grep/glob/subagents/todo/web_search/read_image/read_pdf/read_excel/check_runtime…）、
DSML 工具调用解析、上下文自动压缩、撤回撤销日志与执行许可门。

## 二、原版源码（vendor/）

按上游仓库原文（含 LICENSE、PROVENANCE 锁定）随仓保存，保证"原装工作流源代码"真实可查：

```
vendor/
├── kimi-code/                     # MoonshotAI/kimi-code @ c52d583（MIT）
│   ├── LICENSE / AGENTS.md
│   ├── agent-core-v2/CHANGELOG.md                # 下一代 Agent 核心功能清单
│   ├── agent-core-v2/src/index.ts                # Agent 引擎入口（35KB）
│   ├── agent-core-v2/src/agent/task/{types,errors}.ts   # 任务规划/后台任务契约
│   ├── agent-core-v2/src/agent/state/agentState.ts      # Agent 阶段状态机
│   └── PROVENANCE.md
├── astra-advisor/                 # DannyMac180/astra-advisor（MIT，master）
│   ├── LICENSE / README.md
│   ├── skills/orchestration/SKILL.md              # ASTRA ROUTE / spawn 委派 / 只读审查裁决
│   └── PROVENANCE.md
├── fable-orchestrator/            # codejunkie99/fable-orchestrator（MIT，master）
│   ├── LICENSE / README.md
│   ├── skill/fable/SKILL.md                       # Fable 只规划与裁定；实施节点限定
│   └── PROVENANCE.md
└── fablewright/                   # DivyamTalwar/fablewright（MIT，main）
    ├── LICENSE / README.md
    ├── skills/fablewright/SKILL.md                # CALL SHEET 路由 / 三定律 / 五段式规格
    └── PROVENANCE.md
```

所有迁移均为"忠实移植 + 来源标注"：Rust 引擎实现核心编排算法并在注释中注明对应
上游文件；vendor/ 目录保留原文供对照与审阅，许可证文件一并保留。

## 三、各引擎工作流

### DSH —— DeepSeek Harness 原生 Agent（基准）

原生 `agent_loop.rs`：无步数上限（0 = 循环到模型给出结论）、todo 规划、全局扫描、
读前必改约束、批量写/编辑、1-4 并行子智能体、DSML 双源解析、上下文自动压缩。

### DSK —— Kimi K3 原装工作流（kimi.rs）

对齐 `kimi-code/packages/agent-core-v2` 的 Next-Gen Agent 语义：

1. **任务规划（plan）**：one-shot 规划器输出【目标 / 步骤 / 验收标准】；
2. **执行（execute）**：核心 Agent 循环，无步数上限（`KIMI_LOOP_MAX_STEPS_PER_TURN`
   语义：默认无限步、工具执行器循环：模型生成 → 工具调用 → 结果回灌 → 直到结论），
   随规划注入 `toolDedupe` / `toolResultTruncation` 同源纪律（批量工具、结果截断）；
3. **塔式审查（tower review）**：派遣有界审查子智能体（`features/tower` 的
   `spawn/review/merge`）对照验收标准逐项核验并修复，回传结论。

### DSA —— GPT-6 Astra 原装工作流（astra.rs）

对齐 `astra-advisor`（README + `skills/orchestration/SKILL.md`）：

1. **ASTRA ROUTE 声明**：第一个任务工具调用前输出机器可审计声明
   （parent: deepseek-v4 / observed；delegation: 动态子智能体；risk）；
2. **总指挥拆解**：one-shot 输出【目标 / 有界交付物（标记真正独立的）/ 验收标准】；
3. **实施主阶段**：总指挥继续有用父工作；每个独立交付物通过 1-4 并行子智能体委派
   （派发前声明 `<Agent> — DeepSeek V4: <有界责任>` + 选择理由；无固定数量上限），
   缺失/冲突/不可用 → 该委派 fail closed 并报告限制，绝不静默替换；
4. **总指挥复验**：检查完整 diff + 重跑检查（子智能体报告只是声明）；
5. **只读审查员**：新鲜上下文、只读，回传
   `VERDICT: ship | fix-first | rethink` + REASON/FINDINGS/RESIDUAL RISK；
6. **接受门**：只接受 ship；fix-first → 父修正 + 复验 + **新**审查（限 1 轮）；
   rethink → 重新拆解 + 重新实施 + 新审查（限 1 轮）。

### DSF —— Claude Fable 5.1 原装工作流（fable.rs）

对齐 `fable-orchestrator/skill/fable/SKILL.md` 与 `fablewright/skills/fablewright/SKILL.md`：

1. **Fable 规划（wright）**：输出完整 `FABLEWRIGHT CALL SHEET`
   （route: solo|delegate|audit|full|ensemble；cast；reader；independence；risk）
   + 五段式规格（OBJECTIVE / FILES AND OWNERSHIP / INTERFACES / CONSTRAINTS /
   VERIFICATION）；`solo` 默认；委派前先只读侦察工作区；
2. **实施主阶段**：wright 拥有意图/架构/接口/拆解；每个委派携带五段式规格；
   委派替代 wright 的工作而非重复；机械/大批量工作 → 并行子智能体（flash 车道），
   判断型/高风险 → 主循环（terra 车道）；fail closed；
3. **wright 亲验**：检查真实 diff + 重跑检查（证据高于断言）；
4. **只读 reader**：新鲜上下文、唯一裁决（ship / fix-first / rethink）；
   单运行时（DeepSeek V4）下无跨家族读者 → 如实记录
   `independence: same-family` 并携带残余风险，绝不粉饰为 cross-family；
   `ensemble` 同族约束下退化为顺序 `full` 并写入 risk 行；
5. **接受门**：ship → 完成；fix-first → 车道修正 + 亲验 + **新** reader（限 1 轮）；
   rethink → 修订规格 + 重新实施 + 新 reader（限 1 轮）；任何修正令旧裁决失效。

## 四、执行许可（需分步确认 / 全流程开放）

对标 DeepSeek Harness 审批门（`src/ai/approval.rs`）：

- **需分步确认（step，默认）**：每个工具调用执行前，后端 `ApprovalGate` 通过
  `ai-agent-event`（kind=`tool_approval_required`）向 AI 面板推送审批卡片；
  前端调用 `respond_tool_approval` 命令应答（oneshot channel）；拒绝则把失败结果
  回灌给模型让它换方案；10 分钟无应答按拒绝处理（防止运行永久挂起）。
- **全流程开放（open）**：自动批准全部工具调用（等价 DeepKing 旧行为）。
- 子智能体（subagents / 审查塔 / 审查员）复用同一把门，嵌套派遣同样受控。

## 五、成本与限制

- **单一运行时**：四种引擎全部通过 DeepSeek V4 API 完成推理，只消耗 DeepSeek Token；
  视觉识别（DeepSeek-OCR / ModLens）另计。
- **阶段开销**：DSK/DSA/DSF 的规划/委派/审查阶段会增加 1-5 次模型调用；小任务可关闭
  工具（纯对话模式仍走 `send_ai_message`，引擎仅在"带工具 Agent Loop"启用）。
- **单运行时诚实性**：DSA/DSF 的"选择模型/effort"与"跨家族审查"在单运行时下全部
  记录为 DeepSeek V4（observed）与 `same-family`（残余风险），与上游契约一致。
- **合法合规**：vendor/ 内上游源码均为 MIT 许可，保留原始 LICENSE 与本说明；
  Rust 引擎为参考上游算法的重写实现。

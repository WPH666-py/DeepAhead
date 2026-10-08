# DeepAhead

> 新一代多模态智能体 IDE · Next-Generation Multimodal Agentic IDE
>
> 用最简洁的架构，做最牛逼的产品！ — 水哥

DeepAhead 是一款面向现代开发者打造的多模态智能体集成开发环境（Multimodal Agentic IDE）。它由 DeepKing 继承升级而来——**除工作流模式外全部照抄 DeepKing**——将「代码编辑」「多模态 AI 辅助」「多智能体工具调用」「多语言运行」「文件解析」「版本控制」与「插件生态」深度融合到同一个桌面窗口。

底层采用 **Rust + Tauri 2** 构建，前端使用 **Vue 3 + TypeScript**，实现接近原生的启动速度与极低内存占用。

## 核心特性

- **跨平台桌面应用**：基于 Tauri 2，支持 Windows / macOS / Linux，Windows 端提供 NSIS 安装包。
- **四模式 AI 助手**：DSH / DSK / DSA / DSF 四种工作流，统一走 DeepSeek 运行时；DSK / DSA / DSF 由 Rust 移植的**厂商原装工作流引擎**（kimi-code / astra-advisor / fable-orchestrator+fablewright）驱动，而非仅风格模拟。
- **执行许可（对标 Harness 审批门）**：开启「工具（Agent）」后新增两种模式——**需分步确认**（每个工具调用先由用户批准）/ **全流程开放**（自动批准全部调用）。
- **内置多模态视觉引擎**：集成 DeepSeek-OCR 与 ModLens，让纯文本大模型也能"看懂"截图、设计稿、图表与扫描文档。
- **日志面板**：顶栏「📋 日志」汇总**模式切换、每轮提问与 AI 回复、全部工具调用结果、操作过程与上下文压缩**，可逐条展开查看详情，也可一键清空。日志独立于对话，清空会话后依然保留。
- **勾选工具即 Agent 模式**：AI 配置中勾选「工具」即默认走 9 工具 Agent 循环（自主调用工具直到得出结论）；取消勾选则退回单轮对话模式。
- **输入区增强**：更大的输入区，随图片数量向上增高；支持直接**复制粘贴图片**（纯文本模型未配视觉引擎时明确拦截并提示配置视觉引擎），每次提问最多 **6 张**，缩略图缩小展示。
- **清空会话**：「+ 添加文件」右侧新增「🗑 清空会话」，清空右侧 AI 对话内容与上下文统计，**日志内容保留**。
- **上下文占用与压缩**：实时显示当前对话上下文占用比例（Tokens / 窗口 / 百分比）。**手动压缩**模式下占用超过 **85%** 时提示建议压缩上下文或清空当前对话；**自动压缩**模式下占用超过 **85%** 时自动压缩用户上下文（保留最近轮次），**不清空对话**。
- **内置终端**：「本地终端」直接打开应用内置 Terminal 面板（不再拉起系统 CMD / Windows Terminal 黑框），支持结果导出、复制与清空。
- **界面皮肤系统**：内置三款鲸鱼娘主题皮肤（常规 / 女仆 / 广告），支持亮色 / 暗色随时切换，并可将 GitHub 仓库一键转换为自定义皮肤。
- **多标签编辑器**：内置 CodeMirror 编辑器，支持语法高亮与主题切换（经典纯白 / 护眼淡绿 / 深色专业）。
- **Agent Loop 工具调用**：Claude Code / Cursor 风格的九工具 Agent 循环，支持实时代码读写、命令执行、依赖安装。
- **多语言一键运行**：Python、JavaScript、TypeScript、Java、Go、Rust、C/C++、C#、PHP、SQL、MATLAB、Shell 等自动识别与运行。
- **智能文件解析**：纯 Rust 解析 Office（Excel / Word / PowerPoint），支持 PDF、CSV、图片预览。
- **Git 集成**：状态查看与一键推送，配合 GitHub Token 完成远程提交。
- **插件市场**：接入 VS Code 插件市场，支持搜索、安装与管理插件。
- **会话持久化**：AI 对话、思考过程与生成结果均本地持久化。

## 四种模式

DeepAhead 把 DeepSeek 与 K3、GPT-6 Astra、Claude Fable 5.1 的**原装工作流源码**相结合，能力上取长补短，但**只烧 DeepSeek 的 Token**：

| 模式 | 对应模型 | 原装工作流引擎（Open Source 上游） | 许可证 | 机制 |
| --- | --- | --- | --- | --- |
| **DSH** | DeepSeek Harness | DeepSeek Harness 原生 Agent | MIT | 稳健 Agent 循环，架构先行、长任务可追踪 |
| **DSK** | Kimi K3 | [MoonshotAI/kimi-code](https://github.com/MoonshotAI/kimi-code) | MIT | 任务规划 → 工具执行 → 塔式审查修复 |
| **DSA** | GPT-6 Astra | [DannyMac180/astra-advisor](https://github.com/DannyMac180/astra-advisor) | MIT | 总指挥拆解 → 有界交付物动态委派 → 完整 diff 复验 → 只读审查员 |
| **DSF** | Claude Fable 5.1 | [codejunkie99/fable-orchestrator](https://github.com/codejunkie99/fable-orchestrator) + [DivyamTalwar/fablewright](https://github.com/DivyamTalwar/fablewright) | MIT | CALL SHEET 路由 → 五段式委派 → 亲验 diff → 只读裁决员 |

四种模式共享同一个 DeepSeek V4 运行时与多模态视觉栈。DSH 是原生 Agent 循环基准；DSK / DSA / DSF 的编排算法**移植自厂商官方开源工作流源码**（原版源码随仓保存在 [`vendor/`](vendor/) 目录，含 LICENSE 与 PROVENANCE 锁定），与 DeepSeek 代码强强结合——**无 Persona 模拟层**：四种模式完全由 Rust 原装工作流引擎驱动（`personas/` 与 Persona 加载器已移除）。详见 [`docs/WORKFLOW-ENGINES.md`](docs/WORKFLOW-ENGINES.md)。

## 执行许可：需分步确认 / 全流程开放

在 AI 配置中开启「工具（Agent）」后，新增**执行许可**选择（对标 DeepSeek Harness 审批门）：

- **需分步确认**：每个工具调用前弹出审批卡片（工具名 + 参数），「允许执行」放行、「拒绝」则模型换方案继续；10 分钟无应答按拒绝处理。
- **全流程开放**：自动批准全部工具调用（等价 DeepKing 旧行为）。

## 多模态能力

- **DeepSeek-OCR**：以"上下文光学压缩"范式高效编码高分辨率页面，擅长长文档、复杂版式、公式、表格的结构化还原。
- **ModLens**：即插即用的视觉引擎，提供原生 `read_image` 工具，输出结构化 JSON 证据（OCR、版面、语义）。

两者输出均转译为结构化文本证据后注入 DeepSeek 上下文，形成从「文本 → 表格 → 文档 → 图片」的完整多模态闭环。

> **视觉引擎提示**：若当前连接的模型本身不具备多模态（识图）能力，AI 配置中会明确提示**需要搭配视觉引擎使用** —— 勾选「🖼 视觉引擎」并填写 Vision API Key / Base URL / Model 后，粘贴的图片会先经视觉引擎转译为结构化文本，再交给主模型推理。纯文本模型未配置视觉引擎时，粘贴图片会被拦截并给出配置指引。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2（Rust） |
| 前端 | Vue 3 + TypeScript + Vite |
| 编辑器 | CodeMirror 6 |
| 状态管理 | Pinia |
| 文件解析 | calamine、zip + XML、pymupdf |

## 快速开始

1. 下载并安装 DeepAhead 安装包（Windows 为 NSIS 安装程序）。
2. 启动后点击「开始 → 新建项目 / 打开项目」。
3. 在左侧文件树双击文件编辑，图片直接预览，Office / PDF 用系统默认程序打开。
4. 在右侧「AI 助手」面板配置 DeepSeek API Key，并选择 DSH / DSK / DSA / DSF 模式；开启「工具」后选择执行许可（需分步确认 / 全流程开放）。
5. 粘贴截图或设计稿，DeepAhead 会经内置视觉引擎完成多模态理解后再作答。
6. 顶部「📋 日志」可查看模式切换、每轮提问与 AI 回复、全部工具调用结果与操作过程；「🗑 清空会话」只清 AI 对话、保留日志。
7. 顶部选择「运行环境」与「运行文件」，点击「运行」，输出显示在底部内置终端（不再弹出系统 CMD 窗口）。

## 开发与构建

```bash
# 安装依赖
pnpm install

# 开发模式
pnpm tauri dev

# 构建安装包
pnpm tauri build
```

## 关于作者

- **昵称**：水哥
- **毕业院校**：青岛理工大学 · 2022 级毕业生
- **邮箱**：943050454@qq.com
- **项目理念**：DeepAhead，新一代多模态智能体 IDE。用最简洁的架构，做最牛逼的产品！

如有问题、建议或合作需求，欢迎通过邮箱联系。

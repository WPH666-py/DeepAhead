# DeepAhead 说明文档 / Documentation

> 新一代多模态智能体 IDE · Next-Generation Multimodal Agentic IDE
> 用最简洁的架构，做最牛逼的产品！— 水哥

---

# 一、中文说明

## 1. 项目简介

DeepAhead 是一款面向现代开发者打造的新一代多模态智能体集成开发环境（Multimodal Agentic IDE）。它由 DeepKing 继承升级而来（除工作流模式外全部照抄 DeepKing），由青岛理工大学 2022 级毕业生水哥独立设计与开发。DeepAhead 将「代码编辑」「多模态 AI 大模型辅助」「多智能体工具调用」「多语言运行环境」「文件解析」「版本控制」与「插件生态」深度融合为桌面级生产力工具。

DeepAhead 的核心理念是「简洁架构 + 极致体验」。底层采用 Rust 与 Tauri 2 构建，前端使用 Vue 3 与 TypeScript，实现了接近原生的启动速度与极低的内存占用。

与 DeepKing 的差异只有一处——四种模式重构为 **DSH、DSK、DSA、DSF**（分别对应 DeepSeek 原装工作流 Harness、DeepSeek+Kimi K3、DeepSeek+GPT-6 Astra、DeepSeek+Fable 5.1）。四种模式由 **Rust 移植的厂商原装工作流引擎**驱动，与 DeepSeek 运行时强强结合，**只烧 DeepSeek 的 Token**。

## 2. 核心特性

- **跨平台桌面应用**：基于 Tauri 2，支持 Windows、macOS 与 Linux，Windows 端提供原生 NSIS 安装包。
- **多标签代码编辑器**：内置 CodeMirror 编辑器，支持语法高亮、主题切换（经典纯白 / 护眼淡绿 / 深色专业）与多种编程语言。
- **文件树与资源管理**：完整的文件树浏览，新建 / 重命名 / 删除 / 复制 / 剪切 / 粘贴，支持拖拽调整面板宽度。
- **四模式 AI 助手**：DSH / DSK / DSA / DSF 四种工作流，统一走 DeepSeek 大模型运行时；DSK / DSA / DSF 由厂商原装工作流引擎（Rust 移植 kimi-code / astra-advisor / fable-orchestrator+fablewright 官方源码）驱动。
- **执行许可模式**：开启「工具（Agent）」后新增两种许可模式——**需分步确认**（每个工具调用先由用户批准）与 **全流程开放**（自动批准全部调用），对标 DeepSeek Harness 的审批机制。
- **内置多模态视觉引擎**：集成 DeepSeek-OCR 与 ModLens，可识别含文字截图、UI 设计稿、图表、公式与扫描文档，并把视觉内容转译为结构化文本供模型推理。
- **Agent Loop 工具调用**：提供 Claude Code / Cursor 风格的工具 Agent 循环，支持实时代码读写、命令行执行、依赖安装等自动化操作。
- **多语言一键运行**：支持 Python、JavaScript、TypeScript、Java、Go、Rust、C、C++、C#、PHP、SQL、MATLAB、Shell 等多种语言文件的自动识别与运行。
- **内置终端**：底部集成「终端 / 输出」面板，支持输入命令、查看运行结果、导出与复制输出。
- **智能文件解析**：纯 Rust 解析 Office（Excel / Word / PowerPoint），支持 PDF、CSV、图片预览，二进制文件用系统默认程序打开。
- **Git 集成**：内置 Git 状态查看与一键推送，配合 GitHub Token 完成远程仓库提交。
- **插件市场**：接入 VS Code 插件市场，支持搜索、安装与管理软件和插件。
- **界面皮肤系统**：内置三款鲸鱼娘主题皮肤（常规 / 女仆 / 广告），支持亮色 / 暗色随时切换；也可粘贴 GitHub 仓库地址一键转换为自定义皮肤。
- **会话持久化**：AI 对话、思考过程、生成结果均本地持久化，刷新页面不丢失。

## 3. 四模式架构

DeepAhead 只支持四种工作模式：**DSH、DSK、DSA、DSF**。四种模式共享同一个 DeepSeek 运行时与多模态视觉栈，**区别在于工作流引擎**：

| 模式 | 对应模型 | 原装工作流引擎 | 机制 |
| --- | --- | --- | --- |
| **DSH** | DeepSeek Harness | DeepSeek Harness 原生 Agent | 稳健的 Agent 循环，架构先行、长任务可追踪 |
| **DSK** | Kimi K3 | MoonshotAI/kimi-code（MIT） | 任务规划 → 工具执行 → 塔式审查修复 |
| **DSA** | GPT-6 Astra | DannyMac180/astra-advisor（MIT） | 总指挥拆解 → 有界交付物动态委派 → 完整 diff 复验 → 只读审查员 |
| **DSF** | Claude Fable 5.1 | codejunkie99/fable-orchestrator + DivyamTalwar/fablewright（MIT） | CALL SHEET 路由 → 五段式委派 → 亲验 diff → 只读裁决员 |

设计思路：DSH 作为「主模式」，提供最依赖原生 Agent 循环的基准体验；DSK、DSA 与 DSF 不需要单独拉模型——它们的**编排算法移植自各厂商官方开源工作流源码**，以 Rust 引擎的形式运行在 DeepSeek 运行时之上。原版源码（含 LICENSE 与 PROVENANCE 锁定）随仓保存在 `vendor/` 目录，映射关系与算法说明见 `docs/WORKFLOW-ENGINES.md`。这样既保留了真正的"原装工作流"，又把成本压到最低——**只消耗 DeepSeek Token**。

## 4. 无 Persona 注入层（原装工作流引擎驱动）

DeepAhead **不加载任何 Persona 文件、不模拟任何"人格"**。四种模式的编排完全由 Rust 原装工作流引擎负责：

- `src-tauri/src/ai/workflow/kimi.rs`（DSK）、`astra.rs`（DSA）、`fable.rs`（DSF）与原生 `agent_loop.rs`（DSH）各自在 `extra_preamble` / 阶段指令中注入**上游原装工作流内容**（kimi-code、astra-advisor、fable-orchestrator、fablewright）；
- 代码内置的"原生系统提示"（`src-tauri/src/ai/modes.rs`）只包含极简的模式身份说明、上下文文件内容块与通用安全规则——没有风格模拟、没有注释清单、没有按权重的知识注入；
- 模式元数据（名称 / 引擎 / 上游仓库 / 许可证 / 机制）由 `modes.rs` 静态表提供，不再读取磁盘上的任何 persona.toml / Markdown 知识文件（`personas/` 目录已整体移除）。

这是"原装工作流 + 单一 DeepSeek 运行时"的完整实现：引擎负责编排，DeepSeek 负责推理，只消耗 DeepSeek Token，其他厂商的模型与人格均不参与。

## 5. 多模态能力

DeepAhead 内置两套互补的视觉引擎：

- **DeepSeek-OCR**：以"上下文光学压缩"范式对高分辨率页面进行高效编码，擅长长文档、复杂版式、公式、五线谱、表格的结构化还原，输出带排版的 Markdown。
- **ModLens**：即插即用的视觉引擎，提供原生 `read_image` 工具，输出结构化 JSON 证据（OCR、版面、语义），适合截图理解、UI 还原与语义级看图问答。

两者的输出都会转译为结构化文本证据，再注入 DeepSeek 的上下文，形成从「文本 → 表格 → 文档 → 图片」的完整多模态上下文闭环。

## 6. 技术架构

DeepAhead 采用前后端分层架构。前端使用 Vue 3 + TypeScript + Vite 构建，编辑器基于 CodeMirror 6，状态管理使用 Pinia；后端使用 Rust 编写 Tauri 命令，通过 IPC 与前端通信。文件解析模块大量使用纯 Rust 库：Excel 采用 calamine，Word 与 PowerPoint 采用 zip + XML 解析，文本文件直接用 `std::fs` 读取并支持 UTF-8 / UTF-16 / GBK 编码自动识别；PDF 则通过内置的 Python pymupdf 兜底处理。所有子进程均通过隐藏窗口标志执行，避免弹出黑色命令行窗口。

## 7. 快速开始

1. 下载并安装 DeepAhead 安装包（Windows 为 NSIS 安装程序）。
2. 启动后点击「开始 → 新建项目 / 打开项目」，选择或创建工作目录。
3. 在左侧文件树中双击文件即可编辑；图片直接预览，Office / PDF 文件用系统默认程序打开。
4. 在右侧「AI 助手」面板点击「配置」填入 DeepSeek API Key，并选择 DSH / DSK / DSA / DSF 四种模式之一；开启「工具」后可在 AI 配置中选择执行许可（需分步确认 / 全流程开放）。
5. 上传截图或设计稿，DeepAhead 会通过内置视觉引擎完成多模态理解后再作答。
6. 顶部选择「运行环境」与「运行文件」，点击「运行」，输出自动显示在底部终端。

## 8. 功能详解

### 8.1 项目管理
通过「开始」菜单可新建或打开项目。项目列表清晰展示「对话 / 编程」模式，进入项目后顶部同样可以查看当前项目与模式，方便随时确认上下文。

### 8.2 文件编辑
编辑器支持多标签切换、保存、另存为。文件树右键菜单提供新建文件、新建文件夹、重命名、复制路径、剪切、复制、粘贴、删除等操作，所有文件操作均限定在项目目录内，安全可控。

### 8.3 AI 助手与四模式
AI 助手支持 DSH / DSK / DSA / DSF 四种工作流：
DSH 为 DeepSeek Harness 原生 Agent 循环；DSK 由 Kimi K3 原装工作流引擎驱动（计划 → 执行 → 塔式审查修复）；DSA 由 GPT-6 Astra 原装工作流引擎驱动（总指挥拆解 → 有界交付物动态委派 → 完整 diff 复验 → 只读审查员 ship/fix-first/rethink）；DSF 由 Claude Fable 5.1 原装工作流引擎驱动（CALL SHEET 路由 → 五段式委派 → 亲验 diff → 只读裁决）。所有模式统一走 DeepSeek 运行时并共享多模态视觉栈，仅消耗 DeepSeek Token。

### 8.4 执行许可（需分步确认 / 全流程开放）
在 AI 配置中开启「工具（Agent）」后，新增**执行许可**选择（对标 DeepSeek Harness 的审批门）：

- **需分步确认**：模型每次计划调用工具时，AI 面板弹出审批卡片（工具名 + 参数），由用户点击「允许执行」或「拒绝」；拒绝后模型会收到失败结果并换方案继续。超时（10 分钟）未应答按拒绝处理。
- **全流程开放**：工具调用自动放行（等价于 DeepKing 的原有行为）。

实现上由后端 `ApprovalGate`（`src-tauri/src/ai/approval.rs`）在每个工具执行前拦截：全流程开放直接放行；需分步确认则通过 `ai-agent-event`（kind=`tool_approval_required`）推送到前端，挂起等待 `respond_tool_approval` 命令应答（oneshot channel）。子智能体（subagents / 审查塔 / 审查员）复用同一把门，嵌套派遣同样受控。

### 8.5 多模态视觉问答
用户可直接粘贴或上传图片进对话，DeepAhead 自动调用 DeepSeek-OCR 或 ModLens 完成识别，把图片内容转译为结构化文本后交给模型推理。

### 8.6 工具调用 Agent Loop
开启「工具」后，AI 可进入 Agent Loop 模式，自动调用读写文件、执行命令、安装依赖等工具，并在「工具」下拉中实时展示工具名称、参数与执行结果（需分步确认模式下显示「等待审批」状态）。

### 8.7 运行文件
顶部可选运行环境与运行文件，系统根据扩展名自动选择解释器或编译器，结果实时输出到底部终端，Python 强制 UTF-8 输出避免中文乱码，所有子进程均隐藏窗口。

### 8.8 终端
底部终端支持输入命令、查看输出，并提供「结果导出」「复制」「清空」按钮，输入框自动聚焦便于连续操作。

### 8.9 Git 集成
「Git 提交」弹框支持填写 GitHub 用户名、Token、目标仓库、分支与提交信息，一键推送到远程仓库，并支持先「检查状态」。

### 8.10 插件市场
软件与插件市场接入 VS Code 插件市场，支持按相关性、下载量、评分等排序搜索，展示插件图标、名称、发布者与描述，一键安装并存到本地。

### 8.11 界面皮肤系统
设置面板提供「界面皮肤 / UI Skin」分区，内置三款鲸鱼娘主题皮肤——「鲸鱼娘·常规」「鲸鱼娘·女仆」「鲸鱼娘·广告」，三者不可删除，覆盖顶部工具栏、文件树、编辑区与 AI 面板，亮色 / 暗色可随时切换，并自动联动编辑器主题。用户还可粘贴任意 GitHub 仓库地址，抓取仓库中的 `skin.json` 与 CSS 配色变量，经内置插件样式转换器自动生成自定义皮肤。

## 9. 运行环境支持

DeepAhead 提供增强版运行时检测，自动扫描 PATH 及 C/D/E 盘常见安装目录，识别 Python、Node.js、npm、Java、Go、Rust、gcc、git、Docker、PHP、dotnet 等运行时及其版本，并在顶部下拉框中以「✓ / ✗」标识可用性。

## 10. 关于作者与联系方式

- **作者（昵称）**：水哥
- **毕业院校**：青岛理工大学，2022 级毕业生
- **联系方式**：943050454@qq.com
- **项目理念**：DeepAhead，新一代智能体 IDE。用最简洁的架构，做最牛逼的产品！

---

# 二、English Documentation

## 1. Introduction

DeepAhead is a next-generation multimodal agentic IDE built for modern developers. Evolved from DeepKing (everything except the workflow modes is copied verbatim), DeepAhead keeps the four modes but re-wires them to DSH / DSK / DSA / DSF — DeepSeek Harness native, DeepSeek+Kimi K3, DeepSeek+GPT-6 Astra, and DeepSeek+Fable 5.1 — all running on a single DeepSeek V4 runtime, burning only DeepSeek tokens.

## 2. Core Features

- **Cross-platform desktop app**: Tauri 2 (Windows / macOS / Linux), native NSIS installer on Windows.
- **Multi-tab editor**: CodeMirror 6 with syntax highlighting, theme switching, and many languages.
- **File tree & resource management**: create, rename, delete, copy, cut, paste; draggable panel resizing.
- **Four-mode AI assistant**: DSH (DeepSeek Harness native), DSK (Kimi K3 via MoonshotAI/kimi-code), DSA (GPT-6 Astra via DannyMac180/astra-advisor), DSF (Claude Fable 5.1 via fable-orchestrator + fablewright) — all driven by Rust ports of the official upstream workflow engines on one DeepSeek runtime.
- **Approval modes**: with Tools (Agent) enabled, two new permission modes — **step-by-step confirm** (each tool call needs the user's approval) and **full-open** (auto approve) — benchmarked against the DeepSeek Harness approval gate.
- **Built-in multimodal vision**: DeepSeek-OCR + ModLens translate screenshots, mockups, charts, formulas, and scans into structured text.
- **Agent Loop tool calling**: Claude Code / Cursor style tool loop with live file/command execution.
- **Multi-language one-click run** and **built-in terminal**.
- **Smart file parsing** (Office/PDF/CSV/images), **Git integration**, **VS Code plugin marketplace**, **whale-girl UI skins**, **session persistence**.

## 3. Quick Start

1. Install the DeepAhead installer (NSIS on Windows).
2. Start → New Project / Open Project.
3. Configure the DeepSeek API Key and choose DSH / DSK / DSA / DSF.
4. With Tools enabled, pick the execution permission: step-by-step confirm or full-open.
5. Upload screenshots for multimodal understanding; run files from the top bar.

## 4. About the Author

- **Nickname**: 水哥 (Brother Shui)
- **Graduation**: Qingdao University of Technology, Class of 2022
- **Contact**: 943050454@qq.com
- **Motto**: DeepAhead — Next-generation multimodal agentic IDE. Simple architecture, outstanding product!

---

# 三、联系 / Contact

| 项目 | 信息 |
| --- | --- |
| 作者 | 水哥 (Brother Shui) |
| 毕业院校 | 青岛理工大学 · 2022 级毕业生 |
| 邮箱 | 943050454@qq.com |
| 定位 | 新一代多模态智能体 IDE |

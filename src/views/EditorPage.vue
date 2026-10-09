<template>
  <div class="editor-page">
    <!-- 顶部header：五大模块 -->
    <div class="editor-header">
      <!-- 开始（下拉菜单） -->
      <div style="position:relative">
        <button class="menu-button" @click="toggleDropdown('startMenu')">开始 &#9662;</button>
        <div class="dropdown-menu" :class="{ show: openDropdown === 'startMenu' }">
          <div class="dropdown-item" @click="goNewProject">新建项目</div>
          <div class="dropdown-item" @click="goOpenProject">打开项目</div>
          <div class="dropdown-divider"></div>
          <div class="dropdown-item" @click="saveCurrentFile">保存</div>
          <div class="dropdown-item" @click="saveAsFile">另存为</div>
          <div class="dropdown-divider"></div>
          <div class="dropdown-item" @click="showMarketplace = true">软件和插件市场</div>
          <div class="dropdown-divider"></div>
          <div class="dropdown-item" @click="closeProject">关闭项目</div>
          <div class="dropdown-divider"></div>
          <div class="dropdown-item" style="color:#e74c3c" @click="uninstallApp">卸载 DeepAhead</div>
          <div class="dropdown-item" style="color:#e74c3c" @click="exitApp">退出</div>
        </div>
      </div>
      <button class="menu-button" @click="showSettings = true">设置</button>
      <button class="menu-button" @click="openLocalTerminal">本地终端</button>
      <!-- Git 面板：Git 提交 + 历史提交记录（集成 git-graph） -->
      <div style="position:relative">
        <button
          class="menu-button"
          :class="{ active: openDropdown === 'gitMenu' }"
          @click="toggleDropdown('gitMenu')"
        >
          🔀 Git 面板 &#9662;
        </button>
        <div class="dropdown-menu" :class="{ show: openDropdown === 'gitMenu' }">
          <div class="dropdown-item" @click="openGitCommitPanel">⬆ Git 提交</div>
          <div class="dropdown-item" @click="openGitHistoryPanel">🌳 历史提交记录</div>
        </div>
      </div>
      <div style="display:flex;align-items:center;gap:0.2rem">
        <select class="runtime-select" v-model="selectedRuntime" @change="onEnvRuntimeChange">
          <option value="">全部环境</option>
          <option v-for="rt in runtimes" :key="rt.name" :value="rt.name">{{ rt.available ? '✓' : '✗' }} {{ rt.name }} {{ rt.version || '' }}</option>
        </select>
        <button class="add-runtime-btn" @click="addCustomRuntime" title="添加自定义运行环境">＋</button>
        <select class="runfile-select" v-model="selectedRunFile" @change="onRunFileChange">
          <option value="">选择运行文件...</option>
          <option v-for="f in runnableFiles" :key="f.path" :value="f.path">{{ f.name }}</option>
        </select>
        <select class="browser-select" v-model="selectedBrowser" v-if="showBrowserSelect" @change="onRunBrowserChange">
          <option value="edge">Edge</option>
          <option value="chrome">Chrome</option>
          <option value="quark">夸克</option>
        </select>
        <button class="menu-button" style="color:#27ae60;font-weight:600" @click="runProject">运行</button>
        <!-- 日志菜单：模式切换 / 每轮提问与回复 / 工具调用结果 / 操作过程 -->
        <div style="position:relative">
          <button
            class="menu-button"
            :class="{ active: openDropdown === 'agentLogs' }"
            @click="toggleDropdown('agentLogs')"
          >
            📋 日志 <span v-if="store.sessionLogs.length > 0">({{ store.sessionLogs.length }})</span> &#9662;
          </button>
          <div class="dropdown-menu agent-logs-dropdown" :class="{ show: openDropdown === 'agentLogs' }">
            <div class="agent-logs-head">
              <span>运行日志（模式切换 · 提问与回复 · 工具调用 · 操作过程）</span>
              <button class="agent-logs-clear" @click.stop="clearLogs">清空日志</button>
            </div>
            <!-- Agent 运行进度 -->
            <div class="dropdown-item agent-progress-item" v-if="store.isLoading && store.useTools">
              <div class="agent-progress-bar"><div class="agent-progress-fill" :style="{ width: agentProgressPct + '%' }"></div></div>
              <span class="agent-progress-text">第 {{ store.agentIterations }}{{ store.agentMaxIterations > 0 ? '/' + store.agentMaxIterations : '' }} 步 · 已调 {{ store.toolCalls.length }} 个工具</span>
            </div>
            <div class="dropdown-divider" v-if="store.isLoading && store.useTools && store.sessionLogs.length > 0"></div>
            <!-- 日志条目 -->
            <div
              v-for="log in logsNewestFirst"
              :key="log.id"
              class="agent-log-item"
              :class="'log-' + log.kind"
            >
              <div class="agent-log-header" @click="toggleLogEntry(log.id)">
                <span class="agent-log-kind">{{ logKindLabel(log.kind) }}</span>
                <span class="agent-log-time">{{ formatLogTime(log.ts) }}</span>
                <span class="agent-log-title">{{ log.title }}</span>
                <span class="agent-log-expand" v-if="log.detail">{{ expandedLogs[log.id] ? '▾' : '▸' }}</span>
              </div>
              <pre v-if="log.detail && expandedLogs[log.id]" class="agent-log-detail">{{ log.detail }}</pre>
            </div>
            <div v-if="store.sessionLogs.length === 0" class="dropdown-item" style="color:#999">暂无日志</div>
          </div>
        </div>
        <!-- AI 驾驶舱：AI 配置 / 上下文占用 / 集成能力（下拉菜单） -->
        <div style="position:relative">
          <button
            class="menu-button"
            :class="{ active: openDropdown === 'aiCockpit' }"
            title="AI 驾驶舱：模型与视觉配置、上下文占用、集成能力"
            @click="toggleDropdown('aiCockpit')"
          >
            🧠 AI 驾驶舱 &#9662;
          </button>
          <div class="dropdown-menu" :class="{ show: openDropdown === 'aiCockpit' }">
            <div class="dropdown-item" @click="openAiConfigTab">⚙️ AI 配置</div>
            <div class="dropdown-item" @click="openContextTab">📊 上下文占用</div>
            <div class="dropdown-item" @click="openCapabilitiesTab">🧩 集成能力</div>
            <div class="dropdown-divider"></div>
            <div class="dropdown-item" @click="manualCheckUpdate">
              🔄 检查更新<template v-if="appVersion">（当前 {{ appVersion }}）</template>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- 三栏主体 -->
    <div class="editor-container">
      <!-- 左边：文件树 -->
      <div class="file-explorer" id="fileExplorerPanel">
        <div class="file-explorer-header">资源管理器</div>
        <div class="file-tree" @contextmenu.self="onExplorerContextMenu">
          <template v-if="store.fileTree.length">
            <FileTreeNode
              v-for="entry in store.fileTree"
              :key="entry.path"
              :entry="entry"
              :depth="0"
              :open-tabs="openTabs.map((t: TabInfo) => t.path)"
              @context-menu="onFileContextMenu"
              @open="openFile"
              @toggle="toggleFolder"
            />
          </template>
          <div v-else style="padding:1rem;color:#bbb;font-size:0.82rem;text-align:center">打开项目以查看文件</div>
        </div>
        <div class="resize-handle" @mousedown="startResize('explorer', $event)"></div>
      </div>

      <!-- 中间：代码编辑区 -->
      <div class="editor-area">
        <div class="tabs-bar" id="tabsBar" @contextmenu.self="onTabsContextMenu">
          <div
            v-for="tab in openTabs"
            :key="tab.path"
            class="tab"
            :class="{ active: activeTab === tab.path, open: tab.path === activeTab }"
            @click="switchTab(tab.path)"
            @contextmenu.stop.prevent="onTabContextMenu($event, tab.path)"
          >
            <span>{{ tab.kind === "table" ? "📊 " : tab.kind === "md" ? "📝 " : "" }}{{ tab.name }}</span>
            <span v-if="tab.dirty" class="tab-dirty">●</span>
            <span class="tab-close" @click.stop="closeTab(tab.path)">&times;</span>
          </div>
          <button
            v-if="activeTab && activeTabKind === 'md'"
            class="tab-preview-toggle"
            :title="showMdPreview ? '切换到编辑模式' : '切换到预览模式'"
            @click="toggleMdPreview"
          >{{ showMdPreview ? "✎ 编辑" : "👁 预览" }}</button>
        </div>
        <div class="editor-main-content" id="editorMainContent">
          <div class="code-editor" id="cm-editor" v-show="!showImagePreview && !showMdPreview && openTabs.length > 0" @contextmenu="onEditorContextMenu"></div>
          <div class="md-preview" id="mdPreview" v-show="showMdPreview" v-html="mdPreviewHtml"></div>
          <div class="editor-empty" v-show="!showImagePreview && !showMdPreview && openTabs.length === 0">
            <div class="editor-empty-icon">📂</div>
            <div class="editor-empty-title">未打开文件</div>
            <div class="editor-empty-desc">从左侧文件树双击打开文件，或拖拽文件到此处</div>
          </div>
        <div class="image-preview" v-if="showImagePreview" id="imagePreview">
            <button class="image-preview-close" @click="closeImagePreviewTab" title="关闭预览">×</button>
            <img :src="imagePreviewSrc" alt="预览" style="max-width:100%;max-height:100%;object-fit:contain;border-radius:4px;box-shadow:0 2px 12px rgba(0,0,0,0.1)">
          </div>
        </div>
        <!-- 内置终端面板 -->
        <div class="terminal-panel" id="terminalPanel" v-if="showTerminal">
          <div class="terminal-resize-handle" @mousedown="startResize('terminal', $event)"></div>
          <div class="terminal-header">
            <span>终端 / 输出</span>
            <div class="terminal-actions">
              <button class="terminal-action-btn" @click="exportTerminalOutput">结果导出</button>
              <button class="terminal-action-btn" @click="copyTerminalOutput">复制</button>
              <button class="terminal-action-btn" @click="clearTerminalOutput">清空</button>
              <button class="terminal-close-btn" @click="closeTerminalPanel" title="关闭">×</button>
            </div>
          </div>
          <div class="terminal-content" ref="terminalContent" @click="focusTerminalInput">
            <div v-for="(line, i) in terminalLines" :key="i" :class="line.type">{{ line.text }}</div>
            <div class="term-cmd" v-if="showTerminal"><input ref="termInputRef" v-model="termInput" @keyup.enter="execTermCmd" placeholder="输入命令..." style="background:transparent;border:none;color:#d4d4d4;font-family:inherit;font-size:inherit;outline:none;flex:1;width:100%" /></div>
          </div>
        </div>
      </div>

      <!-- 右边：AI + 插件 -->
      <div class="ai-panel">
        <div class="ai-panel-tabs">
          <div class="ai-tab" :class="{ active: aiTab === 'chat' }" @click="aiTab = 'chat'">AI 助手</div>
          <div class="ai-tab" :class="{ active: aiTab === 'plugins' }" @click="aiTab = 'plugins'">软件和插件</div>
        </div>
        <!-- AI问答区 -->
        <div class="ai-panel-content" :class="{ active: aiTab === 'chat' }" id="aiChatPanel">
          <div class="ai-chat" ref="aiChatRef">
            <div v-if="!store.displayMessages.length && !store.isLoading" class="message ai-message">欢迎使用AI助手！请先在下方选择或配置AI模型。</div>
            <div v-for="(msg, i) in store.displayMessages" :key="msg.id || i" class="message" :class="msgClass(msg.role)">
              <div class="msg-role">{{ roleLabel(msg.role) }}</div>
              <div class="msg-content">{{ msg.content }}</div>
              <button
                v-if="msg.role === 'user' && !store.isLoading"
                class="msg-withdraw"
                title="撤回该对话：内容回到输入框，并撤销本轮代码修改与结果"
                @click="withdrawMessage(i)"
              >↩ 撤回</button>
              <!-- 回合末裁决卡片（dsh-rule-engine-client 移植）：逐条独立判定，判例一次性锁定 -->
              <div v-if="msg.role === 'assistant' && msg.id && turnCardsByMsg[msg.id]" class="turn-card">
                <div class="turn-card-head">
                  <span class="turn-card-title">⚖️ 回合裁决</span>
                  <span class="turn-card-progress">{{ labeledCount(turnCardsByMsg[msg.id]) }}/{{ turnCardsByMsg[msg.id].blocks.length }} 已判</span>
                </div>
                <div
                  v-for="blk in turnCardsByMsg[msg.id].blocks"
                  :key="blk.i"
                  class="turn-card-block"
                >
                  <div class="turn-card-block-top">
                    <span class="turn-card-rule">规则 {{ blk.rule_id || '?' }}</span>
                    <span class="turn-card-tool">{{ blk.tool }}</span>
                    <span v-if="blk.label" class="turn-card-locked" :class="blk.label">
                      {{ blk.label === 'correct' ? '✓ 已判：拦对了' : '✗ 已判：拦错了' }}
                    </span>
                  </div>
                  <div class="turn-card-args">{{ blk.args }}</div>
                  <div class="turn-card-reason">{{ blk.reason }}</div>
                  <div v-if="!blk.label" class="turn-card-actions">
                    <button class="turn-card-btn ok" @click="store.rateTurnCard(turnCardsByMsg[msg.id].key, blk.i, 'correct')">✅ 拦对了</button>
                    <button class="turn-card-btn no" @click="store.rateTurnCard(turnCardsByMsg[msg.id].key, blk.i, 'incorrect')">❌ 拦错了</button>
                  </div>
                </div>
                <div class="turn-card-foot">判例一次性锁定；❌ 会使同指纹命令学习放行（可用 /guard label clear 撤销）</div>
              </div>
            </div>
            <div v-if="store.isLoading" class="message ai-message"><div class="msg-role">AI</div><div class="msg-content">{{ store.streamingContent || '思考中...' }}</div></div>
          </div>
          <!-- 上下文占用比例（实时显示当前对话上下文占用） -->
          <div class="ai-ctx-usage" :class="{ warn: store.contextWarning }" @click="openAIConfig" title="点击打开 AI 配置：上下文窗口 / 压缩模式">
            <div class="ai-ctx-usage-head">
              <span>上下文占用</span>
              <span class="ai-ctx-pct">{{ store.contextPercent }}%</span>
              <span class="ai-ctx-tokens">~{{ (store.contextTokens / 1000).toFixed(1) }}k / {{ (store.contextLimit / 1000).toFixed(0) }}k Tokens</span>
              <span class="ai-ctx-mode">{{ store.compressionMode === 'auto' ? '自动压缩' : '手动压缩' }}</span>
            </div>
            <div class="ai-ctx-bar">
              <div class="ai-ctx-fill" :class="{ warn: store.contextWarning }" :style="{ width: Math.min(store.contextPercent, 100) + '%' }"></div>
            </div>
          </div>
          <!-- 提示条：手动压缩模式超过 85% / 视觉引擎未配置 -->
          <div v-if="store.contextWarning && store.compressionMode === 'manual'" class="ai-input-notice warn">
            ⚠️ 上下文已占用 {{ store.contextPercent }}%（≥85%）：建议
            <a href="#" @click.prevent="doManualCompress">压缩上下文</a>
            或
            <a href="#" @click.prevent="doClearSession">清空当前对话</a>
          </div>
          <div class="ai-input-area">
            <!-- 上下文文件 + 操作按钮：添加文件 / 清空会话 -->
            <div class="ai-context-bar">
              <span v-for="(ctx, idx) in aiContextFiles" :key="idx" class="ai-context-chip">{{ ctx.name }}<span class="chip-remove" @click="removeContextFile(idx)">×</span></span>
              <button class="ai-context-add-btn" @click="showFilePicker = true">+ 添加文件</button>
              <button class="ai-context-clear-btn" @click="doClearSession" title="清空右侧 AI 对话内容（日志保留）">🗑 清空会话</button>
            </div>
            <!-- 已粘贴/添加的图片：缩小缩略图，随数量向上增加 -->
            <div v-if="store.pastedImages.length" class="pasted-images-strip">
              <div v-for="(img, idx) in store.pastedImages" :key="img.path" class="pasted-image-thumb">
                <img :src="img.preview" :alt="img.name" />
                <button class="chip-close" @click="store.removePastedImage(idx)" title="移除">&times;</button>
              </div>
              <span class="pasted-images-count">{{ store.pastedImages.length }}/{{ MAX_PASTE_IMAGES }}</span>
            </div>
            <textarea
              ref="aiInputRef"
              v-model="chatInput"
              :placeholder="inputPlaceholder"
              rows="5"
              @input="autoResizeAIInput"
              @contextmenu="onAIInputContextMenu"
              @keyup.enter.exact="handleSend"
              @paste="onPasteImage"
              :disabled="store.isLoading"
            ></textarea>
            <div class="ai-send-row">
              <select v-model="store.currentMode" @change="store.switchMode(store.currentMode)">
                <option value="">选择模式...</option>
                <option value="dsh">DSH (Harness)</option>
                <option value="dsk">DSK (Kimi K3)</option>
                <option value="dsa">DSA (GPT-6 Astra)</option>
                <option value="dsf">DSF (Fable 5.1)</option>
              </select>
              <span class="ai-mode-badge" :class="{ agent: store.useTools }">
                {{ store.useTools ? 'Agent 模式' : '对话模式' }}
              </span>
              <button class="ai-send-btn" :disabled="store.isLoading || (!chatInput.trim() && store.pastedImages.length === 0)" @click="handleSend">发送</button>
            </div>
          </div>
          <!-- 执行许可审批卡片（需分步确认模式：对标 Harness 审批门） -->
          <div v-if="store.pendingApproval && store.isLoading" class="tool-approval-overlay">
            <div class="tool-approval-card">
              <div class="tool-approval-head">🔐 工具调用待批准 <span class="tool-approval-mode">需分步确认</span></div>
              <div class="tool-approval-name">{{ store.pendingApproval.name }}</div>
              <pre class="tool-approval-args">{{ formatArgs(store.pendingApproval.arguments) }}</pre>
              <div class="tool-approval-actions">
                <button class="tool-approval-btn allow" @click="store.respondApproval(true)">✅ 允许执行</button>
                <button class="tool-approval-btn deny" @click="store.respondApproval(false)">⛔ 拒绝（模型将换方案）</button>
              </div>
            </div>
          </div>
        </div>
        <!-- 插件区 -->
        <div class="ai-panel-content" :class="{ active: aiTab === 'plugins' }" id="aiPluginPanel">
          <div class="plugin-list">
            <div v-if="installedExtensions.length === 0" style="text-align:center;padding:2rem;color:#bbb;font-size:0.82rem">
              暂无已安装的软件和插件<br><a href="#" @click.prevent="showMarketplace = true" style="color:#007acc">前往软件和插件市场下载</a>
            </div>
            <div v-for="ext in installedExtensions" :key="ext.id" class="plugin-item">
              <span class="plugin-icon">{{ ext.icon || '📦' }}</span>
              <div class="plugin-info">
                <div class="plugin-name">{{ ext.displayName }}</div>
                <div class="plugin-desc">{{ ext.description }}</div>
              </div>
              <div class="plugin-toggle" :class="{ on: !ext.disabled }" @click="toggleExtension(ext)"></div>
            </div>
          </div>
        </div>

        <!-- 浮动快捷菜单 -->
        <div class="ai-quick-actions">
          <button class="quick-action-btn chat-btn" title="向千问提问">💬</button>
        </div>
      </div>
    </div>

    <!-- 右键菜单 - 文件树 -->
    <div class="context-menu" :class="{ show: fileContextMenu.visible }" :style="{ left: fileContextMenu.x + 'px', top: fileContextMenu.y + 'px' }">
      <div class="context-item" @click="ctxNewFile">📁 新建文件</div>
      <div class="context-item" @click="ctxNewFolder">📂 新建文件夹</div>
      <div class="context-divider"></div>
      <div class="context-item" @click="ctxCopyPath">📄 复制路径</div>
      <div class="context-item" @click="ctxRename">✏ 重命名</div>
      <div class="context-divider"></div>
      <div class="context-item" @click="ctxCut">✂ 剪切</div>
      <div class="context-item" @click="ctxCopy">✅ 复制</div>
      <div class="context-item" @click="ctxPaste">📋 粘贴</div>
      <div class="context-divider"></div>
      <div class="context-item" style="color:#e74c3c" @click="ctxDelete">🗑 删除</div>
    </div>

    <!-- 右键菜单 - 编辑器 -->
    <div class="context-menu" :class="{ show: editorContextMenu.visible }" :style="{ left: editorContextMenu.x + 'px', top: editorContextMenu.y + 'px' }">
      <div class="context-item" @click="editorCtxAction('refactor')">🔄 重构</div>
      <div class="context-divider"></div>
      <div class="context-item" @click="editorCtxAction('cut')">✂ 剪切</div>
      <div class="context-item" @click="editorCtxAction('copy')">📄 复制</div>
      <div class="context-item" @click="editorCtxAction('paste')">📋 粘贴</div>
    </div>

    <!-- 右键菜单 - AI输入框 -->
    <div class="context-menu" :class="{ show: aiInputContextMenu.visible }" :style="{ left: aiInputContextMenu.x + 'px', top: aiInputContextMenu.y + 'px' }">
      <div class="context-item" :class="{ disabled: !aiInputHasSelection }" @click="aiCtxAction('cut')">✂ 剪切</div>
      <div class="context-item" :class="{ disabled: !aiInputHasSelection }" @click="aiCtxAction('copy')">📄 复制</div>
      <div class="context-item" @click="aiCtxAction('paste')">📋 粘贴</div>
      <div class="context-item" @click="aiCtxAction('selectAll')">☑ 全选</div>
      <div class="context-divider"></div>
      <div class="context-item" @click="showFilePicker = true">📎 添加文件到上下文</div>
      <div class="context-item" @click="showFilePicker = true"> 添加文件夹到上下文</div>
    </div>

    <!-- 右键菜单 - Tab 栏 -->
    <div class="context-menu" :class="{ show: tabContextMenu.visible }" :style="{ left: tabContextMenu.x + 'px', top: tabContextMenu.y + 'px' }">
      <div class="context-item" @click="tabCtxAction('close')">✕ 关闭</div>
      <div class="context-item" @click="tabCtxAction('closeOthers')">关闭其他</div>
      <div class="context-item" @click="tabCtxAction('closeAll')">关闭全部</div>
      <div class="context-divider"></div>
      <div class="context-item" @click="tabCtxAction('closeLeft')">关闭左侧</div>
      <div class="context-item" @click="tabCtxAction('closeRight')">关闭右侧</div>
    </div>

    <!-- 内联对话框：新建文件 / 文件夹 / 重命名 -->
    <div class="modal-overlay" :class="{ show: inlineInputModal.visible }" @click.self="inlineInputModal.visible = false">
      <div class="modal-box" style="width:400px">
        <div class="modal-header">
          <h3>{{ inlineInputModal.title }}</h3>
          <button class="modal-close" @click="inlineInputModal.visible = false">&times;</button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <input type="text" v-model="inlineInputModal.value" :placeholder="inlineInputModal.placeholder" @keyup.enter="confirmInlineInput" @keyup.esc="inlineInputModal.visible = false" ref="inlineInputRef">
          </div>
          <div class="form-actions" style="margin-top:1rem">
            <button class="btn btn-secondary" @click="inlineInputModal.visible = false">取消</button>
            <button class="btn btn-primary" @click="confirmInlineInput">确认</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 内联确认对话框：删除 -->
    <div class="modal-overlay" :class="{ show: inlineConfirmModal.visible }" @click.self="inlineConfirmModal.visible = false">
      <div class="modal-box" style="width:400px">
        <div class="modal-header">
          <h3>{{ inlineConfirmModal.title }}</h3>
          <button class="modal-close" @click="inlineConfirmModal.visible = false">&times;</button>
        </div>
        <div class="modal-body">
          <p style="font-size:0.9rem;color:#555;margin-bottom:1rem">{{ inlineConfirmModal.message }}</p>
          <div class="form-actions">
            <button class="btn btn-secondary" @click="inlineConfirmModal.visible = false">取消</button>
            <button class="btn btn-primary" style="background:#e74c3c" @click="confirmInlineConfirm">确认</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 文件选择器弹框 -->
    <div class="file-picker-overlay" :class="{ show: showFilePicker }" @click.self="showFilePicker = false">
      <div class="file-picker-box">
        <div class="file-picker-header">
          <span>选择文件或文件夹添加到上下文</span>
          <button class="modal-close" @click="showFilePicker = false">&times;</button>
        </div>
        <div class="file-picker-list">
          <div
            v-for="item in filePickerItems"
            :key="item.path"
            class="file-picker-item"
            :class="{ selected: filePickerSelections.has(item.path), dir: item.is_dir }"
            @click="toggleFilePickerSelection(item)"
          >
            {{ item.is_dir ? '📁' : '📄' }} {{ item.name }}
          </div>
        </div>
        <div class="file-picker-footer">
          <button class="btn btn-secondary" style="font-size:0.82rem;padding:0.35rem 0.8rem" @click="showFilePicker = false">取消</button>
          <button class="btn btn-primary" style="font-size:0.82rem;padding:0.35rem 0.8rem" @click="confirmFilePicker">确认添加</button>
        </div>
      </div>
    </div>

    <!-- 插件市场弹框 -->
    <div class="modal-overlay marketplace-modal" :class="{ show: showMarketplace }" @click.self="showMarketplace = false">
      <div class="modal-box">
        <div class="modal-header">
          <h3>软件和插件市场</h3>
          <button class="modal-close" @click="showMarketplace = false">&times;</button>
        </div>
        <div class="modal-body">
          <div class="marketplace-search">
            <input type="text" v-model="marketplaceSearch" placeholder="搜索 VS Code 插件..." @keydown.enter="searchMarketplace">
            <select v-model="marketplaceSortBy" @change="searchMarketplace">
              <option value="0">相关性</option>
              <option value="1">最近更新</option>
              <option value="2">名称</option>
              <option value="3">发布者</option>
              <option value="4">下载量</option>
              <option value="5">评分</option>
            </select>
            <button class="btn btn-primary" style="padding:0.5rem 1rem;font-size:0.84rem" @click="searchMarketplace">搜索</button>
          </div>
          <div class="marketplace-tabs">
            <div class="marketplace-tab" :class="{ active: marketplaceTab === 'popular' }" @click="marketplaceTab = 'popular'">热门推荐</div>
            <div class="marketplace-tab" :class="{ active: marketplaceTab === 'installed' }" @click="marketplaceTab = 'installed'">已安装</div>
            <div class="marketplace-tab" :class="{ active: marketplaceTab === 'disabled' }" @click="marketplaceTab = 'disabled'">已禁用</div>
          </div>
          <div class="marketplace-grid">
            <div v-if="marketplaceLoading" class="marketplace-loading"><span class="loading-spinner"></span> 正在加载插件列表...</div>
            <div v-else-if="marketplaceExtensions.length === 0" class="marketplace-empty">暂无插件</div>
            <div v-for="ext in marketplaceExtensions" :key="ext.id" class="ext-card">
              <div class="ext-card-header">
                <img v-if="ext.icon" :src="ext.icon" class="ext-icon-img" :alt="ext.displayName" @error="(e: any) => e.target.style.display='none'">
                <div v-else class="ext-icon-placeholder">📦</div>
                <div style="flex:1;min-width:0">
                  <div class="ext-name" :title="ext.displayName">{{ ext.displayName }}</div>
                  <div class="ext-publisher">{{ ext.publisher }}</div>
                </div>
              </div>
              <div class="ext-desc" :title="ext.description">{{ ext.description }}</div>
              <div class="ext-actions">
                <button class="ext-btn install" v-if="!isExtensionInstalled(ext.id)" @click="installExtension(ext)">安装</button>
                <button class="ext-btn installed" v-else>已安装</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Git 面板 · Git 提交弹框 -->
    <div class="modal-overlay" :class="{ show: showGitPushModal }" @click.self="showGitPushModal = false">
      <div class="modal-box">
        <div class="modal-header">
          <h3>⬆ Git 提交</h3>
          <button class="modal-close" @click="showGitPushModal = false">&times;</button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label>本地项目路径</label>
            <div style="display:flex;gap:0.4rem">
              <input type="text" v-model="gitLocalPath" placeholder="项目绝对路径" style="flex:1">
              <button class="btn btn-secondary" style="padding:0.35rem 0.6rem;font-size:0.78rem;white-space:nowrap" @click="selectGitLocalPath">选择</button>
            </div>
          </div>
          <div class="form-group"><label>GitHub 用户名</label><input type="text" v-model="gitUsername" placeholder="GitHub 用户名"></div>
          <div class="form-group"><label>GitHub Token</label><input type="password" v-model="gitToken" placeholder="ghp_xxx..."></div>
          <div class="form-group"><label>目标仓库 (用户名/仓库名)</label><input type="text" v-model="gitRemoteRepo" placeholder="如: myname/my-repo"></div>
          <div class="form-group"><label>分支名称</label><input type="text" v-model="gitBranch" placeholder="main"></div>
          <div class="form-group"><label>提交信息</label><input type="text" v-model="gitCommitMsg" placeholder="提交描述（可选）"></div>
          <div v-if="gitStatusArea" style="font-size:0.78rem;color:#888;margin-bottom:0.5rem;padding:0.4rem;background:#f9f9f9;border-radius:4px">{{ gitStatusArea }}</div>
        </div>
        <div class="modal-body" style="padding-top:0;display:flex;gap:0.5rem;justify-content:flex-end">
          <button class="btn btn-secondary" style="font-size:0.82rem;padding:0.4rem 1rem" @click="loadGitStatus">📋 检查状态</button>
          <button class="btn btn-primary" style="font-size:0.82rem;padding:0.4rem 1rem" @click="gitPush">⬆ 提交并推送</button>
        </div>
      </div>
    </div>

    <!-- Git 面板 · 历史提交记录（集成 git-graph 泳道图 + 提交详情） -->
    <div class="modal-overlay git-history-modal" :class="{ show: showGitHistoryModal }" @click.self="showGitHistoryModal = false">
      <div class="modal-box">
        <div class="modal-header">
          <h3>🌳 历史提交记录</h3>
          <button class="modal-close" @click="showGitHistoryModal = false">&times;</button>
        </div>
        <div class="git-history-toolbar">
          <input type="text" v-model="gitHistoryPath" placeholder="仓库路径" @keyup.enter="loadGitGraph">
          <button class="btn btn-secondary" style="font-size:0.78rem;padding:0.3rem 0.7rem;white-space:nowrap" @click="selectGitLocalPath">选择</button>
          <select v-model.number="gitGraphCount" @change="loadGitGraph" style="padding:0.3rem 0.4rem;border:1px solid #ddd;border-radius:4px;font-size:0.78rem">
            <option :value="50">最近 50 条</option>
            <option :value="200">最近 200 条</option>
            <option :value="500">最近 500 条</option>
            <option :value="1000">最近 1000 条</option>
          </select>
          <button class="btn btn-primary" style="font-size:0.78rem;padding:0.3rem 0.7rem;white-space:nowrap" @click="loadGitGraph">刷新</button>
        </div>
        <div class="git-history-body">
          <div v-if="gitGraphLoading" class="git-history-hint">正在读取提交历史…</div>
          <div v-else-if="gitGraphError" class="git-history-hint error">读取失败：{{ gitGraphError }}</div>
          <div v-else-if="gitGraphRows.length === 0" class="git-history-hint">该仓库暂无提交记录</div>
          <template v-else>
            <div class="git-graph-list">
              <div
                v-for="(row, ri) in gitGraphRows"
                :key="gitGraphCommits[ri].oid"
                class="git-graph-row"
                :class="{ selected: gitGraphSelected === gitGraphCommits[ri].oid }"
                :title="gitGraphCommits[ri].oid"
              >
                <!-- 泳道字形（对齐 dsh-git-graph：● 提交 / ◆ 合并 / │ 贯穿 / 空格） -->
                <span class="git-graph-lanes" :data-gitgraph-lanes="row.nodeColumn">
                  <span
                    v-for="(g, gi) in row.columns"
                    :key="gi"
                    class="git-lane"
                    :class="'git-lane-' + g"
                    :data-gitgraph-glyph="g"
                  >{{ laneGlyphChar(g) }}</span>
                </span>
                <!-- 提交信息 -->
                <div class="git-graph-info" @click="selectGitCommit(gitGraphCommits[ri])">
                  <div class="git-graph-subject">
                    <span
                      v-for="ref in gitGraphCommits[ri].refs"
                      :key="ref"
                      class="git-ref"
                      :class="{ 'git-ref-current': isCurrentRef(ref) }"
                    >{{ ref }}</span>
                    <span class="git-graph-subject-text" :title="gitGraphCommits[ri].subject">{{ gitGraphCommits[ri].subject || '(无提交信息)' }}</span>
                  </div>
                  <div class="git-graph-meta">
                    <code>{{ gitGraphCommits[ri].short }}</code>
                    <span>{{ gitGraphCommits[ri].author }}</span>
                    <span>·</span>
                    <span>{{ relativeTime(gitGraphCommits[ri].author_time) }}</span>
                    <span v-if="row.merge" class="git-graph-merge">merge</span>
                  </div>
                </div>
              </div>
              <div v-if="gitGraphHasMore" class="git-graph-more">
                <button class="btn btn-secondary" style="font-size:0.78rem;padding:0.35rem 1rem" @click="loadMoreGitGraph">加载更多</button>
              </div>
            </div>
            <!-- 提交详情 -->
            <div v-if="gitGraphSelected" class="git-commit-detail">
              <div v-if="!gitGraphDetail" class="git-history-hint">正在读取改动…</div>
              <template v-else>
                <div class="git-commit-detail-files">
                  <b>{{ gitGraphDetail.files.length }}</b> 个文件变更
                  <span v-for="f in gitGraphDetail.files.slice(0, 12)" :key="f" class="git-file-chip">{{ f }}</span>
                  <span v-if="gitGraphDetail.files.length > 12" class="git-file-chip">…</span>
                </div>
                <pre class="git-commit-patch">{{ gitGraphDetail.patch || '(无 diff 内容)' }}</pre>
              </template>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- 上下文占用（AI 驾驶舱 · 独立入口，原 AI 配置里的那一块移到这里） -->
    <div class="modal-overlay" :class="{ show: showContextModal }" @click.self="showContextModal = false">
      <div class="modal-box" style="width:520px">
        <div class="modal-header">
          <h3>📊 上下文占用</h3>
          <button class="modal-close" @click="showContextModal = false">&times;</button>
        </div>
        <div class="modal-body">
          <div class="ctx-config-box">
            <div class="ctx-config-head">
              <span class="ctx-config-pct" :class="{ warn: store.contextWarning }">{{ store.contextPercent }}%</span>
              <span class="ctx-config-tokens">
                当前对话 ~{{ (store.contextTokens / 1000).toFixed(1) }}k / 窗口 {{ (store.contextLimit / 1000).toFixed(0) }}k Tokens
              </span>
            </div>
            <div class="ai-ctx-bar">
              <div class="ai-ctx-fill" :class="{ warn: store.contextWarning }" :style="{ width: Math.min(store.contextPercent, 100) + '%' }"></div>
            </div>
            <div class="config-field" style="margin-top:0.6rem">
              <label>上下文窗口（Tokens）</label>
              <input type="number" min="1000" step="1000" v-model.number="contextLimitInput" placeholder="128000">
            </div>
            <div class="config-field">
              <label>压缩模式</label>
              <select :value="store.compressionMode" @change="onCompressionModeChange" style="width:100%;padding:0.45rem 0.5rem;border:1px solid #ccc;border-radius:5px">
                <option value="manual">手动压缩（超过 85% 提示建议压缩上下文或清空当前对话）</option>
                <option value="auto">自动压缩（超过 85% 自动压缩用户上下文，不清空对话）</option>
              </select>
            </div>
            <div class="ctx-config-hint">
              {{ store.compressionMode === 'auto'
                ? '自动模式：占用 ≥85% 时自动压缩较早的对话轮次，当前对话与最近轮次保留，不会被清空。'
                : '手动模式：占用 ≥85% 时只提示，不自动改动上下文；你可随时点击下方按钮手动压缩。' }}
            </div>
          </div>

          <!-- 压缩引擎阈值（billion-context 移植） -->
          <div v-if="ctxEngine" class="ctx-engine">
            <div class="cap-sub">压缩引擎阈值（billion-context 移植）</div>
            <div class="cap-row"><span>OVER-LIMIT</span><span class="cap-val">{{ (ctxEngine.max_context_limit_pct * 100).toFixed(0) }}%</span></div>
            <div class="cap-row"><span>EMERGENCY</span><span class="cap-val">{{ (ctxEngine.emergency_threshold_pct * 100).toFixed(0) }}%</span></div>
            <div class="cap-row"><span>nudge 增长步长</span><span class="cap-val">{{ (ctxEngine.nudge_growth_tokens / 1000).toFixed(0) }}k（恒定，不随窗口缩放）</span></div>
            <div class="cap-row"><span>增长门槛</span><span class="cap-val">{{ (ctxEngine.growth_floor / 1000).toFixed(1) }}k</span></div>
            <div class="cap-row"><span>保留最近</span><span class="cap-val">{{ ctxEngine.preserve_recent_messages }} 条 / {{ ctxEngine.preserve_recent_tokens }} Tokens</span></div>
          </div>

          <div class="form-actions" style="margin-top:1rem">
            <button class="btn btn-secondary" @click="showContextModal = false">取消</button>
            <button class="btn btn-secondary" :disabled="store.isLoading" @click="doManualCompress">🗜 立即压缩</button>
            <button class="btn btn-primary" @click="saveContextSettings">保存设置</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 自动更新弹框：发现新版本 → 暂不更新（10 分钟后再提醒）/ 立即更新 -->
    <div class="modal-overlay update-modal" :class="{ show: showUpdateModal }" @click.self="snoozeUpdate">
      <div class="modal-box" style="width:560px">
        <div class="modal-header">
          <h3>🎉 发现新版本</h3>
          <button class="modal-close" @click="snoozeUpdate">&times;</button>
        </div>
        <div class="modal-body">
          <div class="update-versions">
            <span class="update-cur">当前 {{ updateInfo?.current }}</span>
            <span class="update-arrow">→</span>
            <span class="update-new">{{ updateInfo?.latest }}</span>
          </div>
          <div v-if="updateInfo?.asset" class="update-asset">
            安装包：<code>{{ updateInfo.asset.name }}</code>
            <span v-if="updateInfo.asset.size">（{{ (updateInfo.asset.size / 1048576).toFixed(1) }} MB）</span>
            <span v-else>（大小未知）</span>
          </div>
          <div v-if="updateInfo?.published_at" class="update-date">发布时间：{{ updateInfo.published_at }}</div>

          <!-- 下载/安装进度 -->
          <div v-if="updateBusy" class="update-progress">
            <div class="update-progress-label">
              <span v-if="updatePhase === 'downloading'">
                正在从 Gitee 下载…（{{ (updateProgress.downloaded / 1048576).toFixed(1) }} MB<template v-if="updateProgress.total"> / {{ (updateProgress.total / 1048576).toFixed(1) }} MB</template>）
              </span>
              <span v-else>正在启动更新：应用即将退出，随后自动卸载旧版并安装新版…</span>
            </div>
            <div class="ai-ctx-bar">
              <div class="ai-ctx-fill" :style="{ width: updatePct + '%' }"></div>
            </div>
          </div>

          <div v-if="updateInfo?.notes" class="update-notes">
            <div class="update-notes-title">更新内容</div>
            <pre class="update-notes-body">{{ updateInfo.notes }}</pre>
          </div>

          <div class="update-hint">
            更新流程：从 Gitee 下载安装包 → 退出当前版本 → 静默卸载旧版 → 静默安装新版 → 自动重新启动。
            <br>选择「暂不更新」后每 10 分钟提醒一次，直到你点击「立即更新」。
          </div>
        </div>
        <div class="form-actions">
          <button class="btn btn-secondary" :disabled="updateBusy" @click="snoozeUpdate">暂不更新</button>
          <button class="btn btn-primary" :disabled="updateBusy" @click="startUpdate">
            {{ updateBusy ? (updatePhase === 'downloading' ? '下载中…' : '安装中…') : '立即更新' }}
          </button>
        </div>
      </div>
    </div>

    <!-- 设置弹框 -->
    <div class="modal-overlay" :class="{ show: showSettings }" @click.self="showSettings = false">
      <div class="modal-box">
        <div class="modal-header">
          <h3>设置</h3>
          <button class="modal-close" @click="showSettings = false">&times;</button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label>语言 / Language</label>
            <select v-model="settingsLanguage" @change="changeLanguage">
              <option value="zh-CN">中文</option>
              <option value="en-US">English</option>
            </select>
          </div>
          <div class="form-group" style="margin-top:1rem">
            <label>编辑器样式 / Editor Theme</label>
            <select v-model="store.editorTheme" @change="onEditorThemeChange">
              <option value="classic">经典纯白</option>
              <option value="green">护眼淡绿</option>
              <option value="dark">深色专业</option>
            </select>
          </div>
          <div class="form-group" style="margin-top:1rem">
            <label>界面皮肤 / UI Skin（覆盖文件树、编辑区、AI 区域）</label>
            <div class="skin-list">
              <div class="skin-item" :class="{ active: currentSkinId === null }" @click="onSkinSelect(null)">
                <div class="skin-info">
                  <div class="skin-name">默认</div>
                  <div class="skin-desc">DeepAhead 原生浅色界面</div>
                </div>
              </div>
              <div v-for="s in allSkins" :key="s.id" class="skin-item" :class="{ active: currentSkinId === s.id }" @click="onSkinSelect(s.id)">
                <div class="skin-info">
                  <div class="skin-name">
                    {{ s.name }}
                    <span v-if="s.builtin" class="skin-tag">内置</span>
                  </div>
                  <div class="skin-desc">{{ s.description }}</div>
                  <div v-if="s.source" class="skin-source">{{ s.source }}</div>
                </div>
                <div class="skin-actions" @click.stop>
                  <template v-if="s.palettes.dark">
                    <button class="skin-variant-btn" :class="{ on: currentSkinId === s.id && currentSkinVariant === 'light' }" @click="onSkinSelect(s.id, 'light')">亮</button>
                    <button class="skin-variant-btn" :class="{ on: currentSkinId === s.id && currentSkinVariant === 'dark' }" @click="onSkinSelect(s.id, 'dark')">暗</button>
                  </template>
                  <button v-if="!s.builtin" class="skin-delete-btn" title="删除此自定义皮肤" @click="onRemoveCustomSkin(s.id)">&times;</button>
                </div>
              </div>
            </div>
            <div class="skin-import">
              <input v-model="skinRepoUrl" type="text" placeholder="输入 GitHub 仓库地址，如 https://github.com/owner/repo" @keyup.enter="onImportSkin" />
              <button class="skin-import-btn" :disabled="skinImporting" @click="onImportSkin">
                {{ skinImporting ? "转换中..." : "转换并添加" }}
              </button>
            </div>
            <div v-if="skinImportMsg" class="skin-import-msg" :class="{ error: skinImportError }">{{ skinImportMsg }}</div>
          </div>
          <div class="form-group" style="margin-top:1.5rem">
            <label style="font-weight:500;color:#333">AI 模式说明</label>
            <div style="margin-top:0.5rem;font-size:0.82rem;color:#666;line-height:1.6">
              <div style="margin-bottom:0.6rem">
                <b style="color:#333">DSH</b> — DeepSeek Harness 原生 Agent 循环，架构先行，自主执行。适合长任务、工具调用。<br>
                <b style="color:#333">DSK</b> — Kimi K3 原装工作流引擎（MoonshotAI/kimi-code, MIT）：计划 → 执行 → 塔式子智能体审查修复。适合快速原型、功能开发。<br>
                <b style="color:#333">DSA</b> — GPT-6 Astra 原装工作流引擎（DannyMac180/astra-advisor, MIT）：总指挥拆解 → 有界交付物并行委派 → 完整 diff 复验 → 只读审查员。适合复杂任务、并行开发。<br>
                <b style="color:#333">DSF</b> — Claude Fable 5.1 原装工作流引擎（fable-orchestrator + fablewright, MIT）：CALL SHEET 路由 → 五段式委派 → 亲验 diff → 只读裁决。适合高波动、批量改造。
              </div>
              <div style="font-size:0.78rem;color:#999">
                DSH 为原生 Harness 工作流；DSK / DSA / DSF 由 Rust 移植的厂商原装工作流引擎驱动（原版源码见仓库 vendor/），与 DeepSeek V4 运行时强强结合（无 Persona 模拟层）。全部模式只消耗 DeepSeek Token；勾选「工具」即默认 Agent 模式，还可选择执行许可：需分步确认 / 全流程开放。日志面板可查看模式切换、每轮提问与回复、全部工具调用结果与操作过程。
              </div>
            </div>
          </div>
          <div class="form-group" style="margin-top:1.5rem">
            <label style="font-weight:500;color:#333">已安装插件</label>
            <div style="margin-top:0.5rem">
              <div v-if="installedExtensions.length === 0" style="color:#999;font-size:0.82rem">暂无已安装插件</div>
              <div v-for="ext in installedExtensions" :key="ext.id" class="plugin-item" style="border:1px solid #f0f0f0;border-radius:6px;margin-bottom:0.4px;padding:0.5rem">
                <span class="plugin-icon">{{ ext.icon || '📦' }}</span>
                <div class="plugin-info">
                  <div class="plugin-name">{{ ext.displayName }}</div>
                  <div class="plugin-desc">{{ ext.description }}</div>
                </div>
              </div>
            </div>
          </div>
          <div style="margin-top:2rem;padding-top:1.2rem;border-top:1px solid #eee">
            <div style="font-size:0.78rem;color:#aaa;margin-bottom:0.5rem">开发者信息</div>
            <div style="font-size:0.85rem;color:#555;line-height:1.7">
              <div>🏫 <b>青岛理工大学 2022级</b></div>
              <div>👤 <b>水哥</b></div>
              <div>💡 DeepAhead，新一代智能体IDE。用最简洁的架构，做最牛逼的产品！</div>
              <div>📞 <b>电话：18563982192</b></div>
              <div style="margin-top:0.4rem;font-size:0.78rem;color:#999">有什么问题随时跟我说</div>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- AI模型配置弹框 -->
    <div class="modal-overlay" :class="{ show: showAIConfigModal }" @click.self="showAIConfigModal = false">
      <div class="modal-box" style="width:480px">
        <div class="modal-header">
          <h3>DeepSeek V4 API 配置（唯一运行时）</h3>
          <button class="modal-close" @click="showAIConfigModal = false">&times;</button>
        </div>
        <div class="modal-body">
          <p style="font-size:0.78rem;color:#888;margin-bottom:0.8rem">
            DeepAhead 只有 DeepSeek V4 一个运行时模型。DSH 为原生 Harness 工作流，DSK / DSA / DSF 由厂商原装工作流引擎（kimi-code / astra-advisor / fable-orchestrator+fablewright 移植）驱动，均走 DeepSeek Token。
          </p>
          <!-- 能力开关：工具（Agent 模式） / 视觉引擎 / max -->
          <div class="config-field" style="display:flex;flex-direction:column;gap:0.55rem">
            <label style="display:flex;align-items:center;gap:0.5rem;font-weight:500;color:#333;width:100%;justify-content:flex-start;cursor:pointer">
              <input type="checkbox" :checked="store.useTools" @change="toggleTools" style="width:1rem;height:1rem;flex:none;cursor:pointer" />
              <span>🛠 工具</span>
              <span style="margin-left:auto;font-size:0.72rem;color:#999">勾选即默认 Agent 模式 · 9 工具 Agent Loop</span>
            </label>
            <!-- 勾选工具 = Agent 模式（默认），并显示执行许可（对标 Harness 审批） -->
            <div v-if="store.useTools" class="config-agent-badge">
              🤖 已启用 <b>Agent 模式</b>（勾选「工具」即默认走 Agent 循环：自主调用 9 个工具直到得出结论）
            </div>
            <div v-if="store.useTools" style="display:flex;align-items:center;gap:0.5rem;width:100%;justify-content:flex-start">
              <span style="font-weight:500;color:#333;flex:none;font-size:0.85rem">🔐 执行许可</span>
              <select v-model="store.approvalMode" @change="store.setApprovalMode(store.approvalMode)" style="flex:1;padding:0.35rem 0.45rem;border:1px solid #ccc;border-radius:5px;font-size:0.8rem">
                <option value="step">需分步确认（每个工具调用先由你批准）</option>
                <option value="open">全流程开放（自动批准全部调用）</option>
              </select>
            </div>
            <label style="display:flex;align-items:center;gap:0.5rem;font-weight:500;color:#333;width:100%;justify-content:flex-start;cursor:pointer">
              <input type="checkbox" :checked="multimodalEnabled" @change="toggleMultimodal" style="width:1rem;height:1rem;flex:none;cursor:pointer" />
              <span>🖼 视觉引擎</span>
              <span style="margin-left:auto;font-size:0.72rem;color:#999">识图（OCR/视觉）</span>
            </label>
            <!-- 当前模型本身不具备多模态能力时的提示 -->
            <div v-if="!modelHasNativeVision" class="config-vision-notice">
              ⚠️ 当前连接的模型 <b>{{ modelInput || 'deepseek-chat' }}</b> 本身不具备多模态（识图）能力，
              需要<b>搭配视觉引擎</b>使用：勾选「视觉引擎」并填写下方视觉模型配置，图片会先经视觉引擎转译为结构化文本再交给主模型。
            </div>
            <label style="display:flex;align-items:center;gap:0.5rem;font-weight:500;color:#333;width:100%;justify-content:flex-start;cursor:pointer">
              <input type="checkbox" :checked="maxMode" @change="toggleMaxMode" style="width:1rem;height:1rem;flex:none;cursor:pointer" />
              <span>max</span>
              <span style="margin-left:auto;font-size:0.72rem;color:#999">最大能力模式</span>
            </label>
          </div>
          <div class="config-field"><label>API Key</label><input type="password" v-model="apiKeyInput" placeholder="sk-..."></div>
          <div class="config-field"><label>Base URL</label><input v-model="baseUrlInput" placeholder="https://api.deepseek.com"></div>
          <div class="config-field">
            <label>Model（主模型只支持 DeepSeek，请手动输入）</label>
            <input v-model="modelInput" list="deepseek-model-hints" placeholder="如 deepseek-chat / deepseek-reasoner / deepseek-v4">
            <datalist id="deepseek-model-hints">
              <option v-for="m in DEEPSEEK_MODELS" :key="m.id" :value="m.id">{{ m.label }}</option>
            </datalist>
            <div v-if="modelInput.trim() && !isDeepSeekModel(modelInput)" class="config-vision-notice" style="margin-top:0.3rem">
              ⚠️ 主模型名必须包含 <b>deepseek</b>（大小写不限）。当前填写的是「{{ modelInput }}」，不是 DeepSeek 模型，保存时会被拒绝。
            </div>
            <div v-else-if="modelInput.trim()" class="config-model-locked" style="margin-top:0.3rem">
              ✅ 已识别为 DeepSeek 模型：<b>{{ modelInput }}</b>
            </div>
          </div>

          <!-- 视觉引擎开启时：显示视觉识别设置 -->
          <template v-if="multimodalEnabled">
            <div style="border-top:1px solid #eee;margin:0.9rem 0 0.6rem"></div>
            <label style="font-weight:500;color:#333">视觉引擎（DeepSeek-OCR / ModLens）</label>
            <div class="config-field">
              <label>引擎</label>
              <select v-model="visionProvider" style="width:100%;padding:0.45rem 0.5rem;border:1px solid #ccc;border-radius:5px">
                <option value="modlens">ModLens（截图/语义/结构化）</option>
                <option value="deepseek-ocr">DeepSeek-OCR（文档/公式/表格）</option>
              </select>
            </div>
            <div class="config-field"><label>Vision API Key</label><input type="password" v-model="visionKeyInput" placeholder="视觉模型 API Key"></div>
            <div class="config-field"><label>Vision Base URL</label><input v-model="visionBaseUrl" placeholder="https://api.openai.com/v1"></div>
            <div class="config-field"><label>Vision Model</label><input v-model="visionModel" placeholder="gpt-4o-mini / glm-4v-plus"></div>
          </template>
          <!-- 纯文本模式提示 -->
          <div v-else style="border-top:1px dashed #eee;margin:0.9rem 0 0.6rem;padding-top:0.4rem;font-size:0.72rem;color:#999">
            当前为纯文本模式，识图已禁用；开启「视觉引擎」可解锁视觉识别配置。
          </div>

          <div class="form-actions">
            <button class="btn btn-secondary" @click="showAIConfigModal = false">取消</button>
            <button class="btn btn-primary" @click="saveApiConfig">保存并测试连接</button>
            <button v-if="multimodalEnabled" class="btn btn-primary" style="margin-left:0.4rem" @click="saveVisionConfig">保存视觉引擎</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 集成能力面板：dsh-cost-meter / billion-context / dsh-memory-protocol / dsh-rule-engine / dsh-plugin-vet -->
    <div class="modal-overlay cap-modal" :class="{ show: showCapModal }" @click.self="showCapModal = false">
      <div class="modal-box">
        <div class="modal-header">
          <h3>🧩 集成能力</h3>
          <button class="modal-close" @click="showCapModal = false">&times;</button>
        </div>
        <div class="cap-tabs">
          <div class="cap-tab" :class="{ active: capTab === 'cost' }" @click="capTab = 'cost'">费用统计</div>
          <div class="cap-tab" :class="{ active: capTab === 'context' }" @click="capTab = 'context'">上下文压缩</div>
          <div class="cap-tab" :class="{ active: capTab === 'memory' }" @click="capTab = 'memory'">长期记忆</div>
          <div class="cap-tab" :class="{ active: capTab === 'rules' }" @click="capTab = 'rules'">规则引擎</div>
          <div class="cap-tab" :class="{ active: capTab === 'vet' }" @click="capTab = 'vet'">插件体检</div>
        </div>
        <div class="cap-body">
          <!-- 费用统计 -->
          <template v-if="capTab === 'cost'">
            <div v-if="!costSnapshot" class="cap-hint">未读取到费用数据。</div>
            <template v-else>
              <div class="cap-metrics">
                <div class="cap-metric"><span class="cap-metric-label">今日</span><b>{{ costSnapshot.today.display }}</b><span class="cap-metric-sub">{{ costSnapshot.today.calls }} 次 · {{ (costSnapshot.today.tokens / 1000).toFixed(1) }}k tok</span></div>
                <div class="cap-metric"><span class="cap-metric-label">本月</span><b>{{ costSnapshot.month.display }}</b><span class="cap-metric-sub">{{ costSnapshot.month.calls }} 次 · {{ (costSnapshot.month.tokens / 1000).toFixed(1) }}k tok</span></div>
                <div class="cap-metric"><span class="cap-metric-label">累计</span><b>{{ costSnapshot.all.display }}</b><span class="cap-metric-sub">{{ costSnapshot.all.calls }} 次 · {{ (costSnapshot.all.tokens / 1000).toFixed(1) }}k tok</span></div>
                <div class="cap-metric"><span class="cap-metric-label">缓存命中率</span><b>{{ (costSnapshot.cache_hit_rate * 100).toFixed(1) }}%</b><span class="cap-metric-sub">{{ costSnapshot.day_count }} 天账本</span></div>
              </div>
              <div class="cap-row">
                <span>币种</span>
                <span class="cap-val">{{ costSnapshot.currency }}（{{ costSnapshot.symbol }}，汇率 {{ costSnapshot.exchange_rate }}）</span>
              </div>
              <div v-if="costSnapshot.by_model?.length" class="cap-table">
                <div class="cap-th"><span>模型</span><span>成本</span><span>调用</span><span>Tokens</span></div>
                <div v-for="m in costSnapshot.by_model" :key="m.model" class="cap-tr">
                  <span class="cap-mono">{{ m.model }}</span>
                  <span>{{ costSnapshot.symbol }}{{ m.cost.toFixed(4) }}</span>
                  <span>{{ m.calls }}</span>
                  <span>{{ (m.tokens / 1000).toFixed(1) }}k</span>
                </div>
              </div>
              <div class="cap-hint">账本：{{ costSnapshot.ledger_path }}</div>
              <div class="cap-actions">
                <button class="btn btn-secondary" style="font-size:0.78rem" @click="refreshCost">刷新</button>
                <button class="btn btn-secondary" style="font-size:0.78rem" @click="switchCostCurrency">{{ costSnapshot.currency === 'USD' ? '切换为 CNY' : '切换为 USD' }}</button>
                <button class="btn btn-secondary" style="font-size:0.78rem" @click="doCostClear">清空费用历史</button>
              </div>
            </template>
          </template>

          <!-- 上下文压缩（billion-context） -->
          <template v-else-if="capTab === 'context'">
            <div v-if="!ctxEngine" class="cap-hint">未读取到上下文引擎配置。</div>
            <template v-else>
              <div class="cap-note">
                压缩引擎已替换为 <b>billion-context</b>（移植 acp-kernel）：摘要由模型自己写，
                引擎负责给消息分配稳定引用、判定何时提示、按成对完整性替换被消费的轮次。
              </div>
              <div class="cap-row"><span>上下文窗口</span><span class="cap-val">{{ (ctxEngine.model_context_limit / 1000).toFixed(0) }}k Tokens</span></div>
              <div class="cap-row"><span>OVER-LIMIT 阈值</span><span class="cap-val">{{ (ctxEngine.max_context_limit_pct * 100).toFixed(0) }}%</span></div>
              <div class="cap-row"><span>EMERGENCY 阈值</span><span class="cap-val">{{ (ctxEngine.emergency_threshold_pct * 100).toFixed(0) }}%</span></div>
              <div class="cap-row"><span>首次质量水位</span><span class="cap-val">{{ (ctxEngine.min_context_limit_pct * 100).toFixed(0) }}%</span></div>
              <div class="cap-row"><span>nudge 增长步长</span><span class="cap-val">{{ (ctxEngine.nudge_growth_tokens / 1000).toFixed(0) }}k Tokens（恒定，不随窗口缩放）</span></div>
              <div class="cap-row"><span>增长门槛</span><span class="cap-val">{{ (ctxEngine.growth_floor / 1000).toFixed(1) }}k Tokens</span></div>
              <div class="cap-row"><span>T1 / T2 目标</span><span class="cap-val">{{ (ctxEngine.tier_threshold_1 / 1000).toFixed(0) }}k / {{ (ctxEngine.tier_threshold_2 / 1000).toFixed(0) }}k</span></div>
              <div class="cap-row"><span>最小可压范围</span><span class="cap-val">{{ ctxEngine.min_compress_range }} 字符</span></div>
              <div class="cap-row"><span>摘要长度</span><span class="cap-val">{{ ctxEngine.min_summary_length }} ~ {{ ctxEngine.max_summary_length }} 字符</span></div>
              <div class="cap-row"><span>保留最近</span><span class="cap-val">{{ ctxEngine.preserve_recent_messages }} 条 / {{ ctxEngine.preserve_recent_tokens }} Tokens</span></div>
              <div class="cap-row"><span>分级压缩（T2/T3）</span><span class="cap-val">{{ ctxEngine.tiers_enabled ? '已开启' : '已关闭' }}</span></div>
              <div class="cap-actions"><button class="btn btn-secondary" style="font-size:0.78rem" @click="refreshCtxEngine">刷新</button></div>
            </template>
          </template>

          <!-- 长期记忆（dsh-memory-protocol） -->
          <template v-else-if="capTab === 'memory'">
            <div class="cap-note">
              记忆协议：<b>每轮先查记忆再动手</b>（轮首自动 weave 并把结果注入上下文），轮末自动归档本轮内容。
              记忆工具自身永远放行；后端不可用时按<b>失败开放</b>降级，不会把应用锁死。
            </div>
            <template v-if="memoryConfig">
              <label class="cap-check"><input type="checkbox" :checked="memoryConfig.enabled" @change="onMemoryToggle('enabled', $event)"><span>启用长期记忆</span></label>
              <label class="cap-check"><input type="checkbox" :checked="memoryConfig.inject_weave" @change="onMemoryToggle('inject_weave', $event)"><span>轮首自动查阅并注入</span></label>
              <label class="cap-check"><input type="checkbox" :checked="memoryConfig.enforce_weave" @change="onMemoryToggle('enforce_weave', $event)"><span>硬门：未查记忆则拒绝非记忆工具</span></label>
              <label class="cap-check"><input type="checkbox" :checked="memoryConfig.auto_ingest" @change="onMemoryToggle('auto_ingest', $event)"><span>轮末自动归档本轮内容</span></label>
              <label class="cap-check"><input type="checkbox" :checked="memoryConfig.fail_open" @change="onMemoryToggle('fail_open', $event)"><span>失败开放（后端不可用时放行并提示）</span></label>
            </template>
            <div class="cap-row"><span>记忆条数</span><span class="cap-val">{{ memoryStatus?.records ?? 0 }}</span></div>
            <div class="cap-row"><span>存储位置</span><span class="cap-val cap-mono">{{ memoryStatus?.file }}</span></div>
            <div v-if="memoryRecords.length" class="cap-list">
              <div v-for="r in memoryRecords" :key="r.id" class="cap-list-item">
                <span class="cap-badge">{{ r.kind }}</span>
                <span class="cap-list-text">{{ r.text }}</span>
              </div>
            </div>
            <div v-else class="cap-hint">暂无记忆记录。</div>
            <div class="cap-actions">
              <button class="btn btn-secondary" style="font-size:0.78rem" @click="refreshMemory">刷新</button>
              <button class="btn btn-secondary" style="font-size:0.78rem" @click="doMemoryClear">清空全部记忆</button>
            </div>
          </template>

          <!-- 规则引擎（dsh-rule-engine） -->
          <template v-else-if="capTab === 'rules'">
            <div v-if="!rulesInfo" class="cap-hint">未读取到规则。请在 {{ '$DSH_HOME' }}/AGENTS.md 中编写规则。</div>
            <template v-else>
              <div class="cap-note">
                规则引擎把 <b>AGENTS.md</b> 里的规则机械化：等级 → 动作（A 拒绝 / B 纠正 / C 询问 / D 自证 / M 元规则），
                低置信规则永不硬拦。已在 Agent 循环的<b>工具执行前</b>接入硬门。
              </div>
              <div class="cap-row"><span>规则总数</span><span class="cap-val">{{ rulesInfo.total }}</span></div>
              <div class="cap-row"><span>进入硬门</span><span class="cap-val">{{ rulesInfo.guard_rules }}</span></div>
              <div class="cap-row"><span>置信分布</span><span class="cap-val">high {{ rulesInfo.by_confidence?.high || 0 }} · medium {{ rulesInfo.by_confidence?.medium || 0 }} · low {{ rulesInfo.by_confidence?.low || 0 }}</span></div>
              <div class="cap-row"><span>规则文件</span><span class="cap-val cap-mono">{{ rulesInfo.rules_path }}</span></div>
              <div v-if="rulesInfo.rules?.length" class="cap-table">
                <div class="cap-th"><span>规则</span><span>等级</span><span>动作</span><span>置信</span></div>
                <div v-for="r in rulesInfo.rules.slice(0, 20)" :key="r.id" class="cap-tr">
                  <span>[{{ r.id }}] {{ r.title }}</span>
                  <span>{{ r.level || '—' }}</span>
                  <span>{{ (r.actions || []).join('/') }}</span>
                  <span>{{ r.confidence }}</span>
                </div>
              </div>
              <div class="cap-sub">最近审计</div>
              <div v-if="rulesAudit.length" class="cap-list">
                <div v-for="(a, i) in rulesAudit.slice(0, 12)" :key="i" class="cap-list-item">
                  <span class="cap-badge" :class="{ deny: a.kind === 'deny' }">{{ a.kind }}</span>
                  <span class="cap-list-text">[{{ a.rule }}] {{ a.tool }} — {{ a.reason }}</span>
                </div>
              </div>
              <div v-else class="cap-hint">暂无审计记录。</div>
              <!-- 回合末裁决卡片 / 任务契约：勾选只改草稿，底部「保存设置」统一提交 -->
              <div class="cap-sub">开关</div>
              <label class="cap-check">
                <input type="checkbox" v-model="rulesDraft.enabled">
                <span>启用规则引擎（关闭后硬门与裁决卡片都不生效）</span>
              </label>
              <label class="cap-check">
                <input type="checkbox" v-model="rulesDraft.turnCardEnabled">
                <span>回合末裁决卡片（默认关；开启后每轮被拦记录会生成 ✅/❌ 可判卡片）</span>
              </label>
              <label class="cap-check">
                <input type="checkbox" v-model="rulesDraft.taskContractEnabled">
                <span>任务契约（与裁决卡片同组，默认关）</span>
              </label>
              <!-- /guard 命令行 -->
              <div class="cap-sub">/guard 命令行</div>
              <div class="cap-cmdbar">
                <input
                  v-model="guardCommand"
                  placeholder="如 /guard status、/guard log 20、/guard unlock 10、/guard label clear <指纹>"
                  @keyup.enter="runGuard"
                >
                <button class="btn btn-primary" style="font-size:0.78rem;white-space:nowrap" :disabled="guardBusy" @click="runGuard">执行</button>
              </div>
              <pre v-if="guardOutput" class="cap-pre">{{ guardOutput }}</pre>
              <div v-if="rulesLabels.length" class="cap-sub">指纹放行（7 天判例）</div>
              <div v-if="rulesLabels.length" class="cap-list">
                <div v-for="l in rulesLabels" :key="l.fingerprint" class="cap-list-item">
                  <span class="cap-badge">{{ l.label }}</span>
                  <span class="cap-list-text cap-mono">{{ l.fingerprint }} · 剩余 {{ Math.max(0, Math.round((l.expires_at - Date.now() / 1000) / 3600)) }} 小时</span>
                  <button class="cap-link" @click="revokeLabel(l.fingerprint)">撤销</button>
                </div>
              </div>
            </template>
            <!-- 底部操作区：保存设置（提交整页配置） -->
            <div class="cap-actions">
              <button class="btn btn-secondary" style="font-size:0.78rem" @click="refreshRules">刷新</button>
              <button class="btn btn-primary" style="font-size:0.78rem" @click="saveRulesSettings">
                {{ rulesSaved ? '✅ 已保存' : '保存设置' }}
              </button>
            </div>
          </template>

          <!-- 插件体检（dsh-plugin-vet） -->
          <template v-else>
            <div class="cap-note">
              插件体检：确定性静态扫描 → 两段式评分卡（静态分与人工审计结论<b>刻意不合并</b>）。
              权重 critical 45 / high 20 / medium 8 / info 0，<b>只有非 heuristic 发现能改变判决</b>。
            </div>
            <div class="config-field">
              <label>待体检的插件包目录或文件</label>
              <input v-model="vetTarget" placeholder="如 D:\\projects\\some-plugin">
            </div>
            <div class="cap-actions">
              <button class="btn btn-primary" style="font-size:0.78rem" :disabled="vetBusy" @click="doVetScan">{{ vetBusy ? '扫描中…' : '开始体检' }}</button>
              <button class="btn btn-secondary" style="font-size:0.78rem" @click="vetTarget = store.currentProject || ''">用当前项目</button>
            </div>
            <pre v-if="vetScorecard" class="cap-pre">{{ vetScorecard }}</pre>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, nextTick, watch, computed } from "vue";
import { useAppStore, MAX_PASTE_IMAGES, type LogKind } from "../stores/app";
import { tauriAPI } from "../services/tauri-api";
import type { GitGraphCommit, GitGraphLaneRow, LaneGlyph, TurnCard, UpdateInfo } from "../services/tauri-api";
import FileTreeNode from "../components/layout/FileTreeNode.vue";
import { createEditor, destroyEditor, getEditorContent, setEditorContent, setEditorLanguage, setEditorTheme } from "../utils/codemirror";
import type { EditorView } from "@codemirror/view";
import type { EditorTheme } from "../utils/codemirror";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { readTextFile, writeTextFile, remove, rename, mkdir, exists } from "@tauri-apps/plugin-fs";
import { getAllSkins, getSkinById, addCustomSkin, removeCustomSkin, applySkin, type SkinDefinition, type SkinVariant } from "../utils/skins";
import { convertGitHubRepoToSkin } from "../utils/skinConverter";
import { markdownToHtml } from "../utils/markdown";

const emit = defineEmits<{ (e: "navigate", page: string): void }>();
const store = useAppStore();

// ─── 状态 ───
const openDropdown = ref("");
const aiTab = ref("chat");
const chatInput = ref("");
const showSettings = ref(false);
const showAIConfigModal = ref(false);
const showMarketplace = ref(false);
const showGitPushModal = ref(false);
const showGitHistoryModal = ref(false);
const showCapModal = ref(false);
const showContextModal = ref(false);
const showFilePicker = ref(false);
const showTerminal = ref(false);
const showImagePreview = ref(false);
const imagePreviewSrc = ref("");
const showBrowserSelect = ref(false);

// ─── 界面皮肤（内置三款鲸鱼娘 + GitHub 自定义转换） ───
// 说明：皮肤状态在组件内本地维护（localStorage + applySkin 直连），store 仅尽力同步，
// 避免 store 热更新滞后导致点击无反应
const allSkins = ref<SkinDefinition[]>(getAllSkins());
const currentSkinId = ref<string | null>(localStorage.getItem("DeepAhead-skin-id"));
const currentSkinVariant = ref<SkinVariant>((localStorage.getItem("DeepAhead-skin-variant") as SkinVariant) || "light");
const skinRepoUrl = ref("");
const skinImporting = ref(false);
const skinImportMsg = ref("");
const skinImportError = ref(false);

function refreshSkins() { allSkins.value = getAllSkins(); }

/** 选择皮肤；暗色皮肤联动深色编辑器主题，亮色联动经典纯白（符合视觉一致性） */
function onSkinSelect(id: string | null, variant?: SkinVariant) {
  let v: SkinVariant = variant ?? currentSkinVariant.value ?? "light";
  if (id && !getSkinById(id)?.palettes.dark) v = "light";
  currentSkinId.value = id;
  currentSkinVariant.value = v;
  if (id) {
    localStorage.setItem("DeepAhead-skin-id", id);
    localStorage.setItem("DeepAhead-skin-variant", v);
  } else {
    localStorage.removeItem("DeepAhead-skin-id");
    localStorage.removeItem("DeepAhead-skin-variant");
  }
  applySkin(id, v);
  try { store.setSkin(id, v); } catch (_) {}
  if (id) {
    const target: EditorTheme = v === "dark" ? "dark" : "classic";
    if (store.editorTheme !== target) {
      store.setEditorTheme(target);
      if (cmView.value && activeTab.value) {
        const tab = openTabs.value.find((t) => t.path === activeTab.value);
        if (tab) setEditorTheme(cmView.value, tab.name, target);
      }
      applyEditorThemeBg();
    }
  }
}

/** 从 GitHub 仓库转换并添加自定义皮肤（需要 VPN 能正常访问 GitHub） */
async function onImportSkin() {
  const url = skinRepoUrl.value.trim();
  if (!url || skinImporting.value) return;
  skinImporting.value = true;
  skinImportMsg.value = "";
  skinImportError.value = false;
  try {
    const { skin, warnings } = await convertGitHubRepoToSkin(url);
    addCustomSkin(skin);
    refreshSkins();
    onSkinSelect(skin.id);
    skinRepoUrl.value = "";
    skinImportMsg.value = warnings.length
      ? `已添加「${skin.name}」。提示：${warnings.join("；")}`
      : `已添加「${skin.name}」`;
  } catch (e: any) {
    skinImportError.value = true;
    skinImportMsg.value = String(e?.message || e);
  } finally {
    skinImporting.value = false;
  }
}

/** 删除自定义皮肤（内置皮肤不可删除，UI 不展示删除按钮） */
function onRemoveCustomSkin(id: string) {
  if (removeCustomSkin(id)) {
    if (currentSkinId.value === id) onSkinSelect(null);
    refreshSkins();
  }
}

// 工具调用详情展开状态（按 toolCall.id 索引）
const expandedToolCalls = ref<Record<string, boolean>>({});
function toggleToolCall(id: string) {
  expandedToolCalls.value[id] = !expandedToolCalls.value[id];
  expandedToolCalls.value = { ...expandedToolCalls.value };
}

// ─── 日志面板（模式切换 / 提问与回复 / 工具调用结果 / 操作过程）───
const expandedLogs = ref<Record<string, boolean>>({});
function toggleLogEntry(id: string) {
  expandedLogs.value[id] = !expandedLogs.value[id];
  expandedLogs.value = { ...expandedLogs.value };
}
/** 日志倒序（最新在最上） */
const logsNewestFirst = computed(() => [...store.sessionLogs].reverse());
function logKindLabel(kind: LogKind): string {
  return { mode: "模式", question: "提问", answer: "回复", tool: "工具", system: "系统", context: "上下文" }[kind] || kind;
}
function formatLogTime(ts: number): string {
  const d = new Date(ts);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
function clearLogs() {
  store.clearLogs();
  expandedLogs.value = {};
}
/** 回合裁决卡片：按助手消息 id 建索引，便于模板直接取用 */
const turnCardsByMsg = computed<Record<string, TurnCard>>(() => {
  const map: Record<string, TurnCard> = {};
  for (const c of Object.values(store.turnCards)) {
    if (c.message_id) map[c.message_id] = c;
  }
  return map;
});
/** 已判定条数（模板中避免内联箭头函数触发类型推断问题） */
function labeledCount(card: TurnCard): number {
  return card.blocks.filter((b) => b.label !== "").length;
}

/** Agent 进度百分比（不限步数时无固定上限，进度条保持进行态） */const agentProgressPct = computed(() => {
  const max = store.agentMaxIterations;
  if (!max || max <= 0) return 0;
  return Math.min((store.agentIterations / max) * 100, 100);
});

/**
 * 格式化工具参数：
 * - 如果是 JSON 字符串，先解析再用 JSON.stringify(..., null, 2) 美化，避免双重转义
 * - 如果是对象，直接格式化
 * - 解析失败时原样输出
 */
function formatArgs(args: unknown): string {
  if (args == null) return '(无参数)';
  if (typeof args === 'string') {
    try {
      return JSON.stringify(JSON.parse(args), null, 2);
    } catch {
      return args;
    }
  }
  try {
    return JSON.stringify(args, null, 2);
  } catch {
    return String(args);
  }
}

const aiInputRef = ref<HTMLTextAreaElement | null>(null);
const aiChatRef = ref<HTMLDivElement | null>(null);
const terminalContent = ref<HTMLDivElement | null>(null);
const termInputRef = ref<HTMLInputElement | null>(null);
const inlineInputRef = ref<HTMLInputElement | null>(null);

const apiKeyInput = ref("");
const baseUrlInput = ref("https://api.deepseek.com");
const modelInput = ref("deepseek-chat");

// ─── 主模型：只支持 DeepSeek 模型（用户手动输入，校验必须含 deepseek）───
const DEEPSEEK_MODELS = [
  { id: "deepseek-chat",     label: "DeepSeek V4 通用 · Agent/工具调用" },
  { id: "deepseek-reasoner", label: "DeepSeek V4 推理 · thinking" },
  { id: "deepseek-coder",    label: "代码专用" },
  { id: "deepseek-vl",       label: "多模态识图（可选）" },
];
/**
 * 是否接受为主模型：名称必须包含 deepseek（大小写不限，
 * 因此 deepseek / DeepSeek / DEEPSEEK 都通过）。
 * 允许代理前缀写法，如 deepseek-ai/DeepSeek-V4。
 */
function isDeepSeekModel(name: string): boolean {
  return /deepseek/i.test((name || "").trim());
}

const termInput = ref("");
const terminalLines = ref<{ type: string; text: string }[]>([]);
const runtimes = ref<{name:string;version:string|null;available:boolean;path:string|null}[]>([]);
const selectedRuntime = ref("");
const selectedRunFile = ref("");
const selectedBrowser = ref("edge");
const settingsLanguage = ref("zh-CN");

// ─── 视觉识别配置（DeepSeek-OCR / ModLens） ───
const visionProvider = ref("modlens");
const visionKeyInput = ref("");
const visionBaseUrl = ref("https://api.openai.com/v1");
const visionModel = ref("gpt-4o-mini");

// 视觉引擎（识图）开关：控制配置弹窗中视觉识别设置的可见性
const multimodalEnabled = ref(false);
// 最大能力模式：开启 9 工具 Agent Loop（本地 ref，切换时同步到 store.useTools）
const maxMode = ref(true);
// 上下文窗口输入（AI 配置面板）
const contextLimitInput = ref(store.contextLimit);

// ─── 自动更新（检测 Gitee 新版本 → 提示 → 下载 → 卸载重装）───
const showUpdateModal = ref(false);
const updateInfo = ref<UpdateInfo | null>(null);
const updateBusy = ref(false);
const updatePhase = ref<"idle" | "downloading" | "installing">("idle");
const updateProgress = ref({ downloaded: 0, total: 0 });
/** 10 分钟提醒定时器（"暂不更新"后持续提醒，直到用户选择立即更新） */
let updateRemindTimer: ReturnType<typeof setInterval> | null = null;
/** 长驻会话的定期复查定时器（6 小时） */
let updateCheckTimer: ReturnType<typeof setInterval> | null = null;
const UPDATE_REMIND_MS = 10 * 60 * 1000;
const appVersion = ref("");

/** 检查更新；发现新版本则弹框 */
async function checkForUpdate(): Promise<boolean> {
  try {
    const info = await tauriAPI.checkUpdate();
    updateInfo.value = info;
    // 把检查结果写进日志面板，便于排查"为什么没提示"
    appendUpdateLog(info);
    if (info.has_update && info.asset) {
      // 记录待更新版本，重启后仍会提示
      localStorage.setItem("deep-ide-pending-update", info.latest);
      showUpdateModal.value = true;
      return true;
    }
    // 已是最新 → 清掉待更新标记
    localStorage.removeItem("deep-ide-pending-update");
    return false;
  } catch (e: any) {
    store.appendLog("system", "检查更新失败", String(e));
    return false;
  }
}

/** 检查更新的日志（成功/失败/无附件都要留痕，否则用户无从判断） */
function appendUpdateLog(info: UpdateInfo) {
  if (info.error) {
    store.appendLog("system", `检查更新：${info.error}`, `当前 ${info.current}｜Gitee 最新 ${info.latest || "?"}`);
    return;
  }
  if (!info.has_update) {
    store.appendLog(
      "system",
      `检查更新：已是最新（当前 ${info.current}）`,
      `Gitee 最新发布：${info.latest || "(无发布)"}`
    );
    return;
  }
  store.appendLog(
    "system",
    `发现新版本 ${info.latest}（当前 ${info.current}）`,
    info.asset ? `安装包：${info.asset.name}` : "⚠️ 该发布下没有安装包附件，无法自动更新"
  );
}

/**
 * "暂不更新"：关闭弹框但**每 10 分钟再次提醒**，
 * 直到用户点击"立即更新"为止（跨重启也继续，因为待更新版本已落盘）。
 */
function snoozeUpdate() {
  showUpdateModal.value = false;
  store.appendLog("system", "已选择暂不更新（10 分钟后再次提醒）");
  if (updateRemindTimer) clearInterval(updateRemindTimer);
  updateRemindTimer = setInterval(async () => {
    const pending = localStorage.getItem("deep-ide-pending-update");
    if (!pending) { stopUpdateReminder(); return; }
    // 先刷新一次信息（版本可能又更新了）
    const found = await checkForUpdate();
    if (found) {
      showUpdateModal.value = true;
      store.appendLog("system", `提醒：仍有待安装的新版本 ${pending}`);
    }
  }, UPDATE_REMIND_MS);
}

function stopUpdateReminder() {
  if (updateRemindTimer) {
    clearInterval(updateRemindTimer);
    updateRemindTimer = null;
  }
}
/** 下载进度百分比（总大小未知时按 0 显示为不确定态） */
const updatePct = computed(() => {
  const { downloaded, total } = updateProgress.value;
  if (!total || total <= 0) return updatePhase.value === "downloading" ? 10 : 100;
  return Math.min(100, (downloaded / total) * 100);
});

/**
 * "立即更新"：下载新安装包 → 启动游离更新脚本 → 退出应用。
 * 卸载与重装由脚本在本进程退出后串行完成（运行中的 exe 无法自替换）。
 */
async function startUpdate() {
  const info = updateInfo.value;
  if (!info?.asset) { alert("没有可用的安装包。"); return; }
  updateBusy.value = true;
  updatePhase.value = "downloading";
  updateProgress.value = { downloaded: 0, total: 0 };
  try {
    const path = await tauriAPI.downloadUpdate(info.asset.download_url, info.asset.name);
    updatePhase.value = "installing";
    await tauriAPI.installUpdate(path);
    localStorage.removeItem("deep-ide-pending-update");
    stopUpdateReminder();
    store.appendLog("system", "更新已启动：应用将退出，随后自动卸载旧版并安装新版");
    // 给脚本一点启动时间，然后退出应用
    setTimeout(() => { tauriAPI.quitForUpdate(); }, 800);
  } catch (e: any) {
    updateBusy.value = false;
    updatePhase.value = "idle";
    alert(`更新失败：${e}\n\n可稍后重试，或手动从 Gitee Releases 下载安装包。`);
  }
}

// ─── 集成能力面板：费用统计 / 上下文引擎 / 长期记忆 / 规则引擎 / 插件体检 ───
const costSnapshot = ref<any>(null);
const ctxEngine = ref<any>(null);
const memoryConfig = ref<any>(null);
const memoryStatus = ref<any>(null);
const memoryRecords = ref<any[]>([]);
const rulesInfo = ref<any>(null);
const rulesAudit = ref<any[]>([]);
const vetTarget = ref("");
const vetReport = ref<any>(null);
const vetScorecard = ref("");
const vetBusy = ref(false);
const capTab = ref("cost");
// 规则引擎面板：配置 / /guard 命令行 / 指纹放行
const rulesCfg = ref<any>(null);
/**
 * 规则引擎开关的"草稿"状态。
 * 页面上的勾选只改草稿，点「保存设置」才写回后端 —— 这样一次保存的是整页配置。
 */
const rulesDraft = ref({ enabled: true, turnCardEnabled: false, taskContractEnabled: false });
const rulesSaved = ref(false);
const guardCommand = ref("");
const guardOutput = ref("");
const guardBusy = ref(false);
const rulesLabels = ref<any[]>([]);

async function refreshRulesConfig() {
  try {
    const cfg = await tauriAPI.rulesGetConfig();
    rulesCfg.value = cfg;
    rulesDraft.value = {
      enabled: cfg?.enabled !== false,
      turnCardEnabled: !!cfg?.turn_card_enabled,
      taskContractEnabled: !!cfg?.task_contract_enabled,
    };
    const l = await tauriAPI.rulesLabels();
    rulesLabels.value = l.labels || [];
  } catch (_) { rulesCfg.value = null; }
}

/** 勾选只改草稿（不落盘），底部「保存设置」才提交整页配置 */
function onRulesDraftToggle(key: "enabled" | "turnCardEnabled" | "taskContractEnabled", e: Event) {
  const val = (e.target as HTMLInputElement).checked;
  rulesDraft.value = { ...rulesDraft.value, [key]: val };
  rulesSaved.value = false;
}

/** 保存设置：把当前页面的规则引擎配置写回后端并本地持久化 */
async function saveRulesSettings() {
  try {
    const saved = await tauriAPI.rulesSetToggles({
      enabled: rulesDraft.value.enabled,
      turnCardEnabled: rulesDraft.value.turnCardEnabled,
      taskContractEnabled: rulesDraft.value.taskContractEnabled,
    });
    rulesCfg.value = saved;
    rulesDraft.value = {
      enabled: saved?.enabled !== false,
      turnCardEnabled: !!saved?.turn_card_enabled,
      taskContractEnabled: !!saved?.task_contract_enabled,
    };
    localStorage.setItem("deep-ide-rules-cfg", JSON.stringify(rulesDraft.value));
    rulesSaved.value = true;
    setTimeout(() => { rulesSaved.value = false; }, 2500);
    store.appendLog(
      "system",
      "规则引擎设置已保存",
      `裁决卡片 ${rulesDraft.value.turnCardEnabled ? "开" : "关"}｜任务契约 ${rulesDraft.value.taskContractEnabled ? "开" : "关"}｜引擎 ${rulesDraft.value.enabled ? "开" : "关"}`
    );
    store.addSystemMessage(
      `规则引擎设置已保存：回合末裁决卡片 ${rulesDraft.value.turnCardEnabled ? "开" : "关"}，任务契约 ${rulesDraft.value.taskContractEnabled ? "开" : "关"}`
    );
  } catch (e: any) {
    alert(`保存规则引擎设置失败：${e}`);
  }
}
async function runGuard() {
  const cmd = guardCommand.value.trim();
  if (!cmd) return;
  guardBusy.value = true;
  try {
    const r = await tauriAPI.rulesGuardCommand(cmd.startsWith("/guard") ? cmd : `/guard ${cmd}`);
    guardOutput.value = r.text;
    store.appendLog("system", `${cmd.startsWith("/guard") ? cmd : "/guard " + cmd}`, r.text.slice(0, 300));
    await refreshRules();
    await refreshRulesConfig();
  } catch (e: any) {
    guardOutput.value = `执行失败: ${e}`;
  } finally {
    guardBusy.value = false;
  }
}
async function revokeLabel(fp: string) {
  guardCommand.value = `/guard label clear ${fp}`;
  await runGuard();
}

async function refreshCost() {
  try { costSnapshot.value = await tauriAPI.costSnapshot(); } catch (_) { costSnapshot.value = null; }
}
async function refreshCtxEngine() {
  try { ctxEngine.value = await tauriAPI.contextEngineConfig(store.contextLimit); } catch (_) { ctxEngine.value = null; }
}
async function refreshMemory() {
  try {
    memoryConfig.value = await tauriAPI.getMemoryConfig();
    memoryStatus.value = await tauriAPI.memoryStatus();
    const r = await tauriAPI.memoryRecent(20);
    memoryRecords.value = r.records || [];
  } catch (_) { memoryConfig.value = null; }
}
async function refreshRules() {
  try {
    rulesInfo.value = await tauriAPI.rulesLoad();
    const a = await tauriAPI.rulesAudit(30);
    rulesAudit.value = a.records || [];
  } catch (_) { rulesInfo.value = null; }
}
/** 加载全部集成能力面板数据 */
async function refreshCapabilities() {
  await Promise.all([refreshCost(), refreshCtxEngine(), refreshMemory(), refreshRules(), refreshRulesConfig()]);
}
/**
 * 记忆配置开关。
 * 键名必须是 Tauri 期望的 camelCase（enforceWeave / injectWeave / autoIngest / failOpen），
 * 传 snake_case 会被静默忽略 —— 这正是"开关点了没反应"的原因。
 */
const MEMORY_TOGGLE_KEYS: Record<string, "enabled" | "enforceWeave" | "injectWeave" | "autoIngest" | "failOpen"> = {
  enabled: "enabled",
  enforce_weave: "enforceWeave",
  inject_weave: "injectWeave",
  auto_ingest: "autoIngest",
  fail_open: "failOpen",
};
async function onMemoryToggle(key: string, e: Event) {
  const val = (e.target as HTMLInputElement).checked;
  const wireKey = MEMORY_TOGGLE_KEYS[key] || "enabled";
  // 先乐观更新，避免勾选"弹回"
  memoryConfig.value = { ...(memoryConfig.value || {}), [key]: val };
  try {
    const saved = await tauriAPI.setMemoryConfig({ [wireKey]: val } as any);
    memoryConfig.value = saved;
    store.appendLog("system", `长期记忆 ${key} → ${val ? "开启" : "关闭"}`);
  } catch (err: any) {
    // 失败时回滚并明确告知，而不是静默
    memoryConfig.value = { ...(memoryConfig.value || {}), [key]: !val };
    store.addSystemMessage(`记忆配置写入失败: ${err}`);
  }
}
async function doMemoryClear() {
  const ok = await showInlineConfirm("清空长期记忆", "将删除全部长期记忆记录（不可恢复）。确定继续吗？");
  if (!ok) return;
  try {
    const n = await tauriAPI.memoryClear();
    store.appendLog("system", `已清空长期记忆（${n} 条）`);
    await refreshMemory();
  } catch (e: any) { alert("清空失败: " + e); }
}
async function doVetScan() {
  const t = vetTarget.value.trim() || store.currentProject || "";
  vetTarget.value = t;
  if (!t) { alert("请填写要体检的插件包目录或文件路径。"); return; }
  vetBusy.value = true;
  vetScorecard.value = "";
  vetReport.value = null;
  try {
    const r = await tauriAPI.vetScan(t);
    vetReport.value = r.report;
    vetScorecard.value = r.scorecard;
    store.appendLog(
      "system",
      `插件体检完成：${r.report.verdict}（staticScore ${r.report.static_score}）`,
      `${r.report.findings?.length || 0} 条静态发现 · ${r.report.source_count} 个文件`
    );
  } catch (e: any) {
    alert("体检失败: " + e);
  } finally {
    vetBusy.value = false;
  }
}
/** 切换计价币种（会触发按新币种重新计价） */
async function switchCostCurrency() {
  const next = costSnapshot.value?.currency === "USD" ? "CNY" : "USD";
  try {
    await tauriAPI.costSetConfig({ currency: next, symbol: next === "CNY" ? "¥" : "$" });
    await refreshCost();
    store.appendLog("system", `费用计价币种切换为 ${next}`);
  } catch (e: any) { alert("切换币种失败: " + e); }
}
/** 清空费用历史 */
async function doCostClear() {
  const ok = await showInlineConfirm("清空费用历史", "将删除账本中全部按天/按会话的费用记录（本地记忆与日志不受影响）。确定继续吗？");
  if (!ok) return;
  try {
    const r = await tauriAPI.costClear();
    store.appendLog("system", `已清空费用历史（${r.cleared_days} 天）`);
    await refreshCost();
  } catch (e: any) { alert("清空失败: " + e); }
}

/**
 * 当前连接的模型本身是否具备多模态（识图）能力。
 * DeepSeek V4 运行时为纯文本模型；若用户把 Model 改成已知的多模态模型
 * （gpt-4o / glm-4v / qwen-vl / claude-3 / gemini 等），则可直接粘贴图片。
 */
const NATIVE_VISION_HINTS = [
  "gpt-4o", "gpt-4.1", "gpt-4-turbo", "gpt-4-vision", "o1", "o3", "o4",
  "glm-4v", "qwen-vl", "qwen2-vl", "qwen2.5-vl", "internvl", "llava",
  "claude-3", "claude-4", "claude-sonnet", "claude-opus", "claude-haiku",
  "gemini", "deepseek-vl", "step-1v", "yi-vision", "minicpm-v", "moonshot-v1-vision",
];
const modelHasNativeVision = computed(() => {
  const m = (modelInput.value || store.model || "").toLowerCase();
  return NATIVE_VISION_HINTS.some(h => m.includes(h));
});
/** 是否允许直接粘贴图片：模型原生多模态 或 已开启视觉引擎 */
const canPasteImage = computed(() => modelHasNativeVision.value || multimodalEnabled.value);
/** 输入框占位提示：纯文本模型未配置视觉引擎时明确提示 */
const inputPlaceholder = computed(() =>
  canPasteImage.value
    ? "输入您的问题...可粘贴图片、右键添加文件到上下文"
    : "这是纯文本模型，请配置视觉引擎后再粘贴图片；输入您的问题..."
);

// Tab 管理
interface TabInfo { path: string; name: string; dirty: boolean; content?: string; kind?: "code" | "md" | "table"; previewHtml?: string; }
const openTabs = ref<TabInfo[]>([]);
const activeTab = ref("");
const cmView = ref<EditorView | null>(null);
const currentFile = ref<string | null>(null);
const isModified = ref(false);
// Markdown / 数据表格预览（基于 v-html 渲染）
const showMdPreview = ref(false);
const mdPreviewHtml = ref("");

// 右键菜单
const fileContextMenu = ref({ visible: false, x: 0, y: 0 });
const editorContextMenu = ref({ visible: false, x: 0, y: 0 });
const aiInputContextMenu = ref({ visible: false, x: 0, y: 0 });
const tabContextMenu = ref({ visible: false, x: 0, y: 0, tabPath: "" });
const contextTarget = ref<{ path: string; is_dir: boolean } | null>(null);

// 内联输入
const inlineInputModal = ref({ visible: false, title: "输入名称", value: "", placeholder: "请输入名称", resolve: null as ((v: string | null) => void) | null });
const inlineConfirmModal = ref({ visible: false, title: "确认操作", message: "", resolve: null as ((v: boolean) => void) | null });

// AI 上下文
const aiContextFiles = ref<{ path: string; name: string }[]>([]);

// 插件/市场
interface Extension { id: string; name: string; displayName: string; publisher: string; description: string; icon?: string; disabled: boolean; }
const installedExtensions = ref<Extension[]>([]);
const marketplaceExtensions = ref<Extension[]>([]);
const marketplaceSearch = ref("");
const marketplaceSortBy = ref("0");
const marketplaceTab = ref("popular");
const marketplaceLoading = ref(false);

// Git push
const gitLocalPath = ref(store.currentProject || "");
const gitUsername = ref("");
const gitToken = ref("");
const gitRemoteRepo = ref("");
const gitBranch = ref("main");
const gitCommitMsg = ref("");
const gitStatusArea = ref("");

// ─── Git 面板 · 历史提交记录（集成 dsh-git-graph）───
const gitHistoryPath = ref("");
const gitGraphCommits = ref<GitGraphCommit[]>([]);
const gitGraphRows = ref<GitGraphLaneRow[]>([]);
const gitGraphCount = ref(200);
const gitGraphHasMore = ref(false);
const gitGraphBranch = ref("");
const gitGraphLoading = ref(false);
const gitGraphError = ref("");
const gitGraphSelected = ref<string | null>(null);
const gitGraphDetail = ref<{ hash: string; stat: string; files: string[]; patch: string } | null>(null);

/**
 * 提交图泳道算法 —— 移植自 dsh-git-graph 的 `computeLanes`
 * （packages/dsh-git-graph/src/core/types.ts）。
 *
 * 单遍贪心"待定泳道"算法。前提：rows 已按拓扑序排列（子提交严格早于父提交出现），
 * 由 `git log --topo-order` 保证。
 *
 * 状态：`lanes[i]` = 第 i 条泳道正在等待出现的提交 oid（null = 空位/断口）；
 *       `later` = 所有行的全部父提交之并集。
 *
 * 每行处理：
 *  1. nodeColumn = lanes 中等于本行 oid 的位置；没有则**追加到最右**（分支头）
 *  2. 逐列判定字形：
 *       待定为 null              → 'gap'
 *       i === nodeColumn         → 父提交多于一个 ? 'merge' : 'node'
 *       待定 === 本行 oid（且不是 nodeColumn）→ 'gap'（合并汇入：另一条等待同一提交的泳道到此终止）
 *       later 含该待定提交        → 'pass'（该泳道继续向下）
 *       否则                     → 'gap'
 *  3. 过滤出存在于 later 中的父提交（first + rest）
 *  4. 清空汇入的重复泳道（等待本行 oid 但非 nodeColumn 的）
 *  5. lanes[nodeColumn] = first ?? null（首父继承当前泳道）
 *  6. rest 中尚未在任何泳道等待的父提交 → 追加新泳道
 *  7. 只裁剪**尾部**空位
 */
function computeLanes(rows: GitGraphCommit[]): GitGraphLaneRow[] {
  const lanes: (string | null)[] = [];
  // 所有行的全部父提交之并集
  const later = new Set<string>();
  for (const r of rows) for (const p of r.parents) later.add(p);

  const out: GitGraphLaneRow[] = [];
  for (const row of rows) {
    // 1. 定位本行提交所在泳道（分支头追加到最右）
    let nodeColumn = lanes.findIndex((p) => p === row.oid);
    if (nodeColumn === -1) {
      lanes.push(row.oid);
      nodeColumn = lanes.length - 1;
    }

    // 2. 逐列判定字形
    const columns: LaneGlyph[] = [];
    for (let i = 0; i < lanes.length; i++) {
      const pending = lanes[i];
      if (pending === null) columns.push("gap");
      else if (i === nodeColumn) columns.push(row.parents.length > 1 ? "merge" : "node");
      else if (pending === row.oid) columns.push("gap");
      else if (later.has(pending)) columns.push("pass");
      else columns.push("gap");
    }

    // 3. 只保留存在于 later 中的父提交
    const parents = row.parents.filter((p) => later.has(p));
    const first = parents.length > 0 ? parents[0] : null;
    const rest = parents.slice(1);

    // 4. 汇入本提交的重复泳道清空
    for (let i = 0; i < lanes.length; i++) {
      if (lanes[i] === row.oid && i !== nodeColumn) lanes[i] = null;
    }
    // 5. 首父继承当前泳道
    lanes[nodeColumn] = first;
    // 6. 其余父提交开新泳道
    for (const p of rest) {
      if (!lanes.includes(p)) lanes.push(p);
    }
    // 7. 只裁尾
    while (lanes.length > 0 && lanes[lanes.length - 1] === null) lanes.pop();

    out.push({ columns, nodeColumn, merge: parents.length > 1 });
  }
  return out;
}

/** 字形 → 字符（对齐上游 GraphDialog） */
function laneGlyphChar(g: LaneGlyph): string {
  return g === "node" ? "●" : g === "merge" ? "◆" : g === "pass" ? "│" : " ";
}

/**
 * 相对时间（对齐上游）：<60s 刚刚；<1h n 分钟；<24h n 小时；<30d n 天；否则 YYYY-MM-DD
 */
function relativeTime(unixSeconds: number): string {
  if (!unixSeconds) return "";
  const now = Date.now() / 1000;
  const diff = now - unixSeconds;
  if (diff < 60) return "刚刚";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  if (diff < 86400 * 30) return `${Math.floor(diff / 86400)} 天前`;
  const d = new Date(unixSeconds * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}


// 文件选择器
const filePickerItems = ref<any[]>([]);
const filePickerSelections = ref<Set<string>>(new Set());
const filePickerRoot = ref("");

const modes = [
  { id: "dsh", name: "DSH", desc: "DeepSeek Harness 原生 Agent", tags: "自主·架构先行·长任务" },
  { id: "dsk", name: "DSK", desc: "Kimi K3 原装工作流", tags: "计划·执行·塔式审查" },
  { id: "dsa", name: "DSA", desc: "GPT-6 Astra 原装工作流", tags: "总指挥·动态委派·只读审查" },
  { id: "dsf", name: "DSF", desc: "Fable 5.1 原装工作流", tags: "剧本制·五段式规格·独立裁决" },
];

// ─── 监听 ───
watch(() => store.currentProject, (newPath) => {
  if (newPath) {
    store.loadFileTree(newPath);
    detectRuntimes();
    gitLocalPath.value = newPath;
  }
});

// 打开市场弹窗时自动搜索
watch(showMarketplace, (val) => {
  if (val && marketplaceExtensions.value.length === 0) {
    // 默认搜索热门插件
    if (!marketplaceSearch.value) marketplaceSearch.value = "popular";
    searchMarketplace();
  }
});

onMounted(async () => {
  // 启动时恢复界面皮肤
  applySkin(currentSkinId.value, currentSkinVariant.value);
  // 恢复多模态 / max / 工具 开关状态
  multimodalEnabled.value = JSON.parse(localStorage.getItem("deep-ide-multimodal") || "false");
  maxMode.value = JSON.parse(localStorage.getItem("deep-ide-max-mode") || "true");
  try { store.useTools = JSON.parse(localStorage.getItem("deep-ide-tools") || "true"); } catch (_) {}
  // 恢复视觉识别配置（localStorage → 后端全局）
  try {
    const vcfg = JSON.parse(localStorage.getItem("deep-ide-vision-config") || "null");
    if (vcfg) {
      visionProvider.value = vcfg.provider || "modlens";
      visionBaseUrl.value = vcfg.baseUrl || "https://api.openai.com/v1";
      visionModel.value = vcfg.model || "gpt-4o-mini";
      visionKeyInput.value = vcfg.key || "";
      await store.configureVision(visionProvider.value, visionKeyInput.value, visionBaseUrl.value, visionModel.value);
    }
  } catch (_) {}
  // 监听流式 AI 响应事件
  const unlisten1 = await listen<string>("ai-stream-token", (event) => {
    store.appendStreamToken(event.payload);
    nextTick(() => { if (aiChatRef.value) aiChatRef.value.scrollTop = aiChatRef.value.scrollHeight; });
  });
  const unlisten2 = await listen<string>("ai-stream-done", (event) => {
    try {
      const data = JSON.parse(event.payload);
      store.streamingContent = "";
    } catch (_) {}
  });

  await store.loadAgents();
  if (store.apiKey) await store.switchMode("dsh");
  if (store.currentProject) {
    await store.loadFileTree(store.currentProject);
    detectRuntimes();
    gitLocalPath.value = store.currentProject;
  }
  const editorEl = document.getElementById("cm-editor");
  if (editorEl) {
    cmView.value = createEditor(editorEl, "", "untitled.txt", store.editorTheme);
    applyEditorThemeBg();
  }
  loadSettings();
  // 恢复 API 配置
  const apiConfig = localStorage.getItem("deep-ide-api-config");
  if (apiConfig) {
    try {
      const cfg = JSON.parse(apiConfig);
      apiKeyInput.value = cfg.apiKey || "";
      baseUrlInput.value = cfg.baseUrl || "https://api.deepseek.com";
      // 主模型只支持 DeepSeek 模型：历史配置里的非 DeepSeek 模型回退为默认值
      const savedModel = String(cfg.model || "");
      modelInput.value = isDeepSeekModel(savedModel) ? savedModel : "deepseek-chat";
      if (cfg.apiKey) {
        store.apiKey = cfg.apiKey;
        store.baseUrl = cfg.baseUrl || "https://api.deepseek.com";
        store.model = modelInput.value;
        await store.configureApiKey(cfg.apiKey);
      }
    } catch (_) {}
  }
  loadInstalledExtensions();
  // 恢复规则引擎开关（裁决卡片 / 任务契约），写回后端全局配置
  try {
    const saved = JSON.parse(localStorage.getItem("deep-ide-rules-cfg") || "null");
    if (saved) {
      rulesDraft.value = {
        enabled: saved.enabled !== false,
        turnCardEnabled: !!saved.turnCardEnabled,
        taskContractEnabled: !!saved.taskContractEnabled,
      };
      await tauriAPI.rulesSetToggles({
        enabled: rulesDraft.value.enabled,
        turnCardEnabled: rulesDraft.value.turnCardEnabled,
        taskContractEnabled: rulesDraft.value.taskContractEnabled,
      });
    }
  } catch (_) {}
  // 恢复已落盘的回合裁决卡片（重启后仍可跨回合补裁）
  await store.loadTurnCards();
  // 初始化上下文占用显示（无需等待首次 Agent 运行）
  store.recomputeContextUsage();

  // ─── 自动更新 ───
  // 下载进度事件
  await listen<{ downloaded: number; total: number }>("update-download-progress", (e) => {
    updateProgress.value = e.payload;
  });

  appVersion.value = await tauriAPI.appVersion().catch(() => "");

  // 启动后稍等再检查（不抢启动时的资源）。
  // ⚠️ 这里**绝不能**在发现更新后调用 snoozeUpdate()：那会把刚弹出来的弹框立刻关掉，
  //    变成"要等 10 分钟才提示"（0.5.0 的实际表现）。只有用户点了「暂不更新」才武装定时器。
  setTimeout(async () => {
    const found = await checkForUpdate();
    if (found) {
      // 立刻展示，不隐藏、不武装定时器（等用户自己选）
      return;
    }
    // 上次会话遗留的待更新版本：重新检查并直接提示
    const pending = localStorage.getItem("deep-ide-pending-update");
    if (pending) {
      const info = await tauriAPI.checkUpdate().catch(() => null);
      if (info) updateInfo.value = info;
      if (info?.has_update && info.asset) showUpdateModal.value = true;
      else localStorage.removeItem("deep-ide-pending-update");
    }
  }, 4000);

  // 长驻会话期间定期复查（默认 6 小时），这样挂着不关也能收到新版本提示
  if (updateCheckTimer) clearInterval(updateCheckTimer);
  updateCheckTimer = setInterval(() => { checkForUpdate(); }, 6 * 60 * 60 * 1000);
});

/** 手动检查更新（AI 驾驶舱菜单入口；也是排查"为什么没提示"的手段） */
async function manualCheckUpdate() {
  closeDropdowns();
  const info = await tauriAPI.checkUpdate().catch((e) => null);
  updateInfo.value = info;
  if (!info) {
    alert("检查更新失败：无法访问 Gitee（请检查网络）。");
    return;
  }
  if (info.error) {
    alert(`检查更新失败：${info.error}`);
    return;
  }
  if (info.has_update && info.asset) {
    showUpdateModal.value = true;
    return;
  }
  alert(
    `已是最新版本。\n\n当前版本：${info.current}\nGitee 最新：${info.latest || "(无发布)"}` +
    (info.asset ? `\n安装包：${info.asset.name}` : "\n（该发布下没有安装包附件）")
  );
}

// ─── 基础导航 ───
function toggleDropdown(n: string) { openDropdown.value = openDropdown.value === n ? "" : n; }
function closeDropdowns() { openDropdown.value = ""; }
function goNewProject() { closeDropdowns(); emit("navigate", "new-project"); }
function goOpenProject() { closeDropdowns(); emit("navigate", "open-project"); }
function closeProject() { closeDropdowns(); emit("navigate", "home"); store.currentProject = ""; }
function exitApp() { closeDropdowns(); invoke("exit_app"); }
function uninstallApp() { closeDropdowns(); alert("卸载功能请通过系统控制面板操作"); }

// ─── 运行环境 ───
async function detectRuntimes() {
  try { runtimes.value = await tauriAPI.detectRuntimes(); }
  catch (e: any) { console.error("Runtime detection failed:", e); }
}
function onEnvRuntimeChange() {}
function addCustomRuntime() {
  const name = prompt("自定义运行环境名称（如: python3.12）:");
  if (!name) return;
  const path = prompt("可执行文件路径（如: C:\\Python312\\python.exe）:");
  if (!path) return;
  const saved = localStorage.getItem("deep-ide-custom-runtimes");
  const customs = saved ? JSON.parse(saved) : [];
  customs.push({ name, path, available: true, version: "custom" });
  localStorage.setItem("deep-ide-custom-runtimes", JSON.stringify(customs));
  runtimes.value.push({ name, path, available: true, version: "custom" });
}

// ─── 文件打开/Tab ───
function ensureTab(path: string, name: string): TabInfo {
  let tab = openTabs.value.find(t => t.path === path);
  if (!tab) {
    tab = { path, name, dirty: false, content: "" };
    openTabs.value.push(tab);
  }
  return tab;
}
async function openFile(path: string) {
  const name = path.split(/[\\/]/).pop() || path;
  const ext = name.split(".").pop()?.toLowerCase() || "";
  const imageExts = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
  if (imageExts.includes(ext)) {
    try {
      const data = await invoke<number[]>("read_file_bytes", { path });
      const blob = new Blob([new Uint8Array(data)], { type: `image/${ext === "svg" ? "svg+xml" : ext}` });
      imagePreviewSrc.value = URL.createObjectURL(blob);
      showImagePreview.value = true;
      if (!openTabs.value.find(t => t.path === path)) openTabs.value.push({ path, name, dirty: false, content: "" });
      activeTab.value = path;
      return;
    } catch (e) { console.error(e); }
  }
  // CSV / Excel：在线渲染为表格（不再调用系统默认程序）
  if (ext === "csv") {
    try {
      const md = await tauriAPI.previewCsv(path);
      const tab = ensureTab(path, name);
      tab.kind = "table";
      tab.previewHtml = markdownToHtml(md);
      activeTab.value = path;
      switchTab(path);
      return;
    } catch (e: any) { alert("CSV 预览失败: " + e); return; }
  }
  if (ext === "xlsx" || ext === "xls") {
    try {
      const md = await tauriAPI.previewExcel(path);
      const tab = ensureTab(path, name);
      tab.kind = "table";
      tab.previewHtml = markdownToHtml(md);
      activeTab.value = path;
      switchTab(path);
      return;
    } catch (e: any) { alert("Excel 预览失败: " + e); return; }
  }
  // 二进制文件（Office/PDF）用系统默认程序打开
  const binaryExts = ["docx", "doc", "pptx", "ppt", "pdf"];
  if (binaryExts.includes(ext)) {
    try {
      await invoke("open_file_with_default_app", { path });
    } catch (e: any) { alert("无法打开文件: " + e); }
    return;
  }
  try {
    const content = await tauriAPI.readFile(path);
    currentFile.value = path;
    if (cmView.value) {
      setEditorContent(cmView.value, content);
      setEditorLanguage(cmView.value, name, store.editorTheme);
    }
    showImagePreview.value = false;
    const tab = ensureTab(path, name);
    tab.content = content;
    if (ext === "md") {
      // Markdown：默认进"预览"模式，可切换"编辑"
      tab.kind = "md";
      tab.previewHtml = markdownToHtml(content);
    } else {
      tab.kind = "code";
    }
    activeTab.value = path;
    isModified.value = false;
    switchTab(path);
  } catch (e: any) { alert("读取文件失败: " + e); }
}
function switchTab(path: string) {
  const tab = openTabs.value.find(t => t.path === path);
  if (!tab) return;
  activeTab.value = path;
  currentFile.value = path;
  const ext = tab.name.split(".").pop()?.toLowerCase() || "";
  const imageExts = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];
  // 图片 tab：显示图片预览
  if (imageExts.includes(ext) && tab.content === "") {
    showImagePreview.value = true;
    showMdPreview.value = false;
    // 重新生成 blob URL（之前的可能已被 revoke）
    invoke<number[]>("read_file_bytes", { path: tab.path }).then(data => {
      if (URL.revokeObjectURL) URL.revokeObjectURL(imagePreviewSrc.value);
      const blob = new Blob([new Uint8Array(data)], { type: `image/${ext === "svg" ? "svg+xml" : ext}` });
      imagePreviewSrc.value = URL.createObjectURL(blob);
    }).catch(e => console.error(e));
  } else if (tab.kind === "md" || tab.kind === "table") {
    // Markdown / 数据表格：渲染预览
    showImagePreview.value = false;
    showMdPreview.value = true;
    mdPreviewHtml.value = tab.previewHtml || "";
  } else {
    // 普通文件 tab：显示编辑器
    showImagePreview.value = false;
    showMdPreview.value = false;
    if (cmView.value) {
      setEditorContent(cmView.value, tab.content || "");
      setEditorLanguage(cmView.value, tab.name, store.editorTheme);
    }
  }
}
async function closeTab(path: string) {
  const tab = openTabs.value.find(t => t.path === path);
  if (tab?.dirty) {
    const ok = await showInlineConfirm("保存更改", `文件 ${tab.name} 已修改，是否保存？`);
    if (ok) await saveFile(path, getCurrentContent());
  }
  openTabs.value = openTabs.value.filter(t => t.path !== path);
  // 如果关闭的是图片预览 tab，隐藏预览
  if (showImagePreview.value && activeTab.value === path) {
    showImagePreview.value = false;
  }
  // 如果关闭的是 Markdown/表格预览 tab，隐藏预览
  if (showMdPreview.value && activeTab.value === path) {
    showMdPreview.value = false;
  }
  if (activeTab.value === path) {
    if (openTabs.value.length) {
      const last = openTabs.value[openTabs.value.length - 1];
      switchTab(last.path);
    } else {
      activeTab.value = "";
      currentFile.value = null;
      if (cmView.value) setEditorContent(cmView.value, "");
    }
  }
}
function closeImagePreviewTab() {
  showImagePreview.value = false;
  // 同时关闭图片对应的 tab
  if (activeTab.value) {
    closeTab(activeTab.value);
  }
}

// ─── 编辑器内容 ───
function getCurrentContent() { return cmView.value ? getEditorContent(cmView.value) : ""; }
async function saveCurrentFile() {
  if (!currentFile.value) return;
  await saveFile(currentFile.value, getCurrentContent());
}
async function saveFile(path: string, content: string) {
  try {
    await tauriAPI.writeFile(path, content);
    const tab = openTabs.value.find(t => t.path === path);
    if (tab) { tab.dirty = false; tab.content = content; }
    isModified.value = false;
  } catch (e: any) { alert("保存失败: " + e); }
}
async function saveAsFile() {
  if (!currentFile.value) { alert("请先打开一个文件"); return; }
  try {
    const selected = await open({
      title: "另存为",
      defaultPath: currentFile.value.split(/[\\/]/).pop(),
      filters: [{ name: "All Files", extensions: ["*"] }],
    });
    if (!selected) return;
    const content = getCurrentContent();
    await tauriAPI.writeFile(selected as string, content);
    alert("文件已保存到: " + selected);
  } catch (e: any) { alert("另存为失败: " + e); }
}

// ─── 运行 ───
const runnableFiles = computed(() => {
  const exts = ['.py', '.js', '.ts', '.java', '.go', '.rs', '.cpp', '.c', '.sh', '.bat', '.html'];
  const files: { path: string; name: string }[] = [];
  function walk(entries: any[]) {
    for (const e of entries) {
      if (e.is_dir && e.children) walk(e.children);
      else if (!e.is_dir && exts.some(ext => e.name.endsWith(ext))) files.push({ path: e.path, name: e.name });
    }
  }
  walk(store.fileTree);
  return files;
});
function onRunFileChange() {
  const ext = selectedRunFile.value.split(".").pop()?.toLowerCase();
  showBrowserSelect.value = ext === "html" || ext === "htm";
}
function onRunBrowserChange() {}
async function runProject() {
  const path = selectedRunFile.value || runnableFiles.value[0]?.path;
  if (!path) { alert("没有可运行文件"); return; }
  const ext = path.split(".").pop()?.toLowerCase() || "";
  if (ext === "html" || ext === "htm") {
    openHtmlInBrowser(path);
    return;
  }
  const runtime = selectedRuntime.value || undefined;
  showTerminal.value = true;
  focusTerminalInput();
  terminalLines.value.push({ type: "term-cmd", text: `运行 ${path}` });
  try {
    const result = await tauriAPI.runFile(path, runtime);
    for (const line of result.split("\n")) terminalLines.value.push({ type: "term-out", text: line });
  } catch (e: any) { terminalLines.value.push({ type: "term-err", text: e }); }
  nextTick(scrollTerminal);
}
function openHtmlInBrowser(path: string) {
  const browser = selectedBrowser.value || "edge";
  const browserCmd = browser === "chrome" ? "chrome" : browser === "quark" ? "quark" : "start msedge";
  tauriAPI.runCommand(".", `${browserCmd} "${path}"`);
  showTerminal.value = true;
  terminalLines.value.push({ type: "term-cmd", text: `在浏览器打开 ${path}` });
}

// ─── 终端 ───
/**
 * 「本地终端」只切换内置 Terminal 面板，不再拉起系统 CMD / Windows Terminal。
 * 命令在内置面板中执行（结果直接留在面板里，可导出/复制），避免弹出外部黑框。
 */
function openLocalTerminal() {
  closeDropdowns();
  showTerminal.value = true;
  terminalLines.value.push({ type: "term-info", text: `内置终端 · 工作目录 ${store.currentProject || "."}` });
  nextTick(() => { scrollTerminal(); focusTerminalInput(); });
}
function closeTerminalPanel() { showTerminal.value = false; }
function focusTerminalInput() { nextTick(() => termInputRef.value?.focus()); }
async function execTermCmd() {
  const cmd = termInput.value.trim(); if (!cmd) return;
  terminalLines.value.push({ type: "term-cmd", text: cmd });
  termInput.value = "";
  try {
    const result = await tauriAPI.runCommand(store.currentProject || ".", cmd);
    for (const line of result.split("\n")) terminalLines.value.push({ type: "term-out", text: line });
  } catch (e: any) { terminalLines.value.push({ type: "term-err", text: e }); }
  nextTick(() => { scrollTerminal(); focusTerminalInput(); });
}
function exportTerminalOutput() {
  const text = terminalLines.value.map(l => l.text).join("\n");
  const blob = new Blob([text], { type: "text/plain" });
  const a = document.createElement("a"); a.href = URL.createObjectURL(blob); a.download = "terminal-output.txt"; a.click();
}
function copyTerminalOutput() {
  const text = terminalLines.value.map(l => l.text).join("\n");
  navigator.clipboard.writeText(text);
}
function clearTerminalOutput() { terminalLines.value = []; }
function scrollTerminal() { if (terminalContent.value) terminalContent.value.scrollTop = terminalContent.value.scrollHeight; }

// ─── 右键菜单 ───
function onFileContextMenu(e: MouseEvent, entry: any) {
  e.preventDefault();
  contextTarget.value = { path: entry.path, is_dir: entry.is_dir };
  showMenu(fileContextMenu, e.clientX, e.clientY);
}
function onExplorerContextMenu(e: MouseEvent) {
  e.preventDefault();
  contextTarget.value = { path: store.currentProject || "", is_dir: true };
  showMenu(fileContextMenu, e.clientX, e.clientY);
}
function onEditorContextMenu(e: MouseEvent) { e.preventDefault(); showMenu(editorContextMenu, e.clientX, e.clientY); }
function onAIInputContextMenu(e: MouseEvent) { e.preventDefault(); showMenu(aiInputContextMenu, e.clientX, e.clientY); }
function onTabContextMenu(e: MouseEvent, tabPath: string) {
  e.preventDefault();
  tabContextMenu.value = { visible: true, x: e.clientX, y: e.clientY, tabPath };
  const close = () => { tabContextMenu.value.visible = false; document.removeEventListener("click", close); };
  setTimeout(() => document.addEventListener("click", close), 0);
}
function onTabsContextMenu(e: MouseEvent) {
  e.preventDefault();
  // 在空白区域右键，默认操作当前 tab
  const currentPath = activeTab.value;
  if (!currentPath) return;
  tabContextMenu.value = { visible: true, x: e.clientX, y: e.clientY, tabPath: currentPath };
  const close = () => { tabContextMenu.value.visible = false; document.removeEventListener("click", close); };
  setTimeout(() => document.addEventListener("click", close), 0);
}
function tabCtxAction(action: string) {
  const targetPath = tabContextMenu.value.tabPath;
  const idx = openTabs.value.findIndex(t => t.path === targetPath);
  if (idx < 0) return;
  switch (action) {
    case "close":
      closeTab(targetPath);
      break;
    case "closeOthers":
      openTabs.value.filter(t => t.path !== targetPath).forEach(t => closeTab(t.path));
      break;
    case "closeAll":
      [...openTabs.value].forEach(t => closeTab(t.path));
      break;
    case "closeLeft":
      openTabs.value.slice(0, idx).forEach(t => closeTab(t.path));
      break;
    case "closeRight":
      openTabs.value.slice(idx + 1).forEach(t => closeTab(t.path));
      break;
  }
  tabContextMenu.value.visible = false;
}
function showMenu(menu: any, x: number, y: number) {
  menu.value = { visible: true, x, y };
  const close = () => { menu.value.visible = false; document.removeEventListener("click", close); };
  setTimeout(() => document.addEventListener("click", close), 0);
}
const aiInputHasSelection = computed(() => {
  const input = aiInputRef.value;
  return input ? input.selectionStart !== input.selectionEnd : false;
});

// ─── 文件操作 ───
async function ctxNewFile() {
  if (!contextTarget.value) return;
  const dir = contextTarget.value.is_dir ? contextTarget.value.path : contextTarget.value.path.replace(/\\[^\\]+$/, "").replace(/\/[^\/]+$/, "");
  const name = await showInlineInput("新建文件", "", "输入文件名");
  if (!name) return;
  const path = dir + (dir.endsWith("\\") || dir.endsWith("/") ? "" : "/") + name;
  try {
    await writeTextFile(path, "");
    await store.loadFileTree(store.currentProject!);
  } catch (e: any) { alert("创建失败: " + e); }
}
async function ctxNewFolder() {
  if (!contextTarget.value) return;
  const dir = contextTarget.value.is_dir ? contextTarget.value.path : contextTarget.value.path.replace(/\\[^\\]+$/, "").replace(/\/[^\/]+$/, "");
  const name = await showInlineInput("新建文件夹", "", "输入文件夹名");
  if (!name) return;
  const path = dir + (dir.endsWith("\\") || dir.endsWith("/") ? "" : "/") + name;
  try {
    await mkdir(path);
    await store.loadFileTree(store.currentProject!);
  } catch (e: any) { alert("创建失败: " + e); }
}
function ctxCopyPath() { if (contextTarget.value) navigator.clipboard.writeText(contextTarget.value.path); }
async function ctxRename() {
  if (!contextTarget.value) return;
  const oldPath = contextTarget.value.path;
  const oldName = oldPath.split(/[\\/]/).pop() || "";
  const newName = await showInlineInput("重命名", oldName, "新名称");
  if (!newName || newName === oldName) return;
  const newPath = oldPath.substring(0, oldPath.length - oldName.length) + newName;
  try {
    await rename(oldPath, newPath);
    await store.loadFileTree(store.currentProject!);
  } catch (e: any) { alert("重命名失败: " + e); }
}
let clipboardData: { type: "cut" | "copy"; path: string; name: string } | null = null;
function ctxCut() { if (contextTarget.value) clipboardData = { type: "cut", path: contextTarget.value.path, name: contextTarget.value.path.split(/[\\/]/).pop() || "" }; }
function ctxCopy() { if (contextTarget.value) clipboardData = { type: "copy", path: contextTarget.value.path, name: contextTarget.value.path.split(/[\\/]/).pop() || "" }; }
async function ctxPaste() {
  if (!clipboardData || !contextTarget.value) return;
  const dir = contextTarget.value.is_dir ? contextTarget.value.path : contextTarget.value.path.replace(/\\[^\\]+$/, "").replace(/\/[^\/]+$/, "");
  const dest = dir + "/" + clipboardData.name;
  try {
    if (clipboardData.type === "copy") {
      const content = await readTextFile(clipboardData.path);
      await writeTextFile(dest, content);
    } else {
      await rename(clipboardData.path, dest);
    }
    clipboardData = null;
    await store.loadFileTree(store.currentProject!);
  } catch (e: any) { alert("粘贴失败: " + e); }
}
async function ctxDelete() {
  if (!contextTarget.value) return;
  const name = contextTarget.value.path.split(/[\\/]/).pop();
  const ok = await showInlineConfirm("确认删除", `确定要删除 "${name}" 吗？`);
  if (!ok) return;
  try {
    await remove(contextTarget.value.path, { recursive: true });
    await store.loadFileTree(store.currentProject!);
  } catch (e: any) { alert("删除失败: " + e); }
}
async function editorCtxAction(action: string) {
  if (action === "refactor") {
    // 将选中的代码发送到 AI 进行重构
    const content = getCurrentContent();
    if (!content.trim()) { alert("请先在编辑器中选中要重构的代码"); return; }
    const tab = openTabs.value.find(t => t.path === activeTab.value);
    const fileName = tab?.name || "current file";
    const ext = fileName.split(".").pop()?.toLowerCase();
    const langMap: Record<string,string> = { py:"Python", js:"JavaScript", ts:"TypeScript", java:"Java", go:"Go", rs:"Rust", cpp:"C++", c:"C", cs:"C#", php:"PHP", rb:"Ruby", sql:"SQL", html:"HTML", css:"CSS", vue:"Vue", json:"JSON", md:"Markdown", xml:"XML", sh:"Shell", bat:"Batch" };
    const lang = ext ? (langMap[ext] || ext.toUpperCase()) : "";
    chatInput.value = `请对以下${lang ? " " + lang : ""}代码进行重构优化，提升可读性和可维护性:\n\n\`\`\`\n${content}\n\`\`\``;
    aiTab.value = "chat";
    return;
  }
  const input = document.querySelector(".code-editor textarea") as HTMLTextAreaElement | null;
  if (!input) return;
  if (action === "cut") { input.setRangeText("", input.selectionStart, input.selectionEnd, "end"); }
  else if (action === "copy") { navigator.clipboard.writeText(input.value.substring(input.selectionStart, input.selectionEnd)); }
  else if (action === "paste") { navigator.clipboard.readText().then(t => { input.setRangeText(t, input.selectionStart, input.selectionEnd, "end"); }); }
}
function aiCtxAction(action: string) {
  const input = aiInputRef.value; if (!input) return;
  if (action === "cut") { navigator.clipboard.writeText(input.value.substring(input.selectionStart, input.selectionEnd)); input.setRangeText("", input.selectionStart, input.selectionEnd, "end"); }
  else if (action === "copy") { navigator.clipboard.writeText(input.value.substring(input.selectionStart, input.selectionEnd)); }
  else if (action === "paste") { navigator.clipboard.readText().then(t => { input.setRangeText(t, input.selectionStart, input.selectionEnd, "end"); }); }
  else if (action === "selectAll") { input.select(); }
}

// ─── 内联对话框 ───
function showInlineInput(title: string, defaultValue: string, placeholder: string): Promise<string | null> {
  return new Promise((resolve) => {
    inlineInputModal.value = { visible: true, title, value: defaultValue, placeholder, resolve };
    nextTick(() => { inlineInputRef.value?.focus(); inlineInputRef.value?.select(); });
  });
}
function confirmInlineInput() {
  const v = inlineInputModal.value.value.trim();
  inlineInputModal.value.visible = false;
  if (inlineInputModal.value.resolve) { inlineInputModal.value.resolve(v || null); inlineInputModal.value.resolve = null; }
}
function showInlineConfirm(title: string, message: string): Promise<boolean> {
  return new Promise((resolve) => {
    inlineConfirmModal.value = { visible: true, title, message, resolve };
  });
}
function confirmInlineConfirm() {
  inlineConfirmModal.value.visible = false;
  if (inlineConfirmModal.value.resolve) { inlineConfirmModal.value.resolve(true); inlineConfirmModal.value.resolve = null; }
}

// ─── 文件选择器 ───
async function loadFilePicker(path: string) {
  filePickerRoot.value = path;
  try {
    const result = await tauriAPI.listDirectory(path, 1);
    filePickerItems.value = result.entries;
  } catch (e) { filePickerItems.value = []; }
}
function toggleFilePickerSelection(item: any) {
  if (filePickerSelections.value.has(item.path)) filePickerSelections.value.delete(item.path);
  else filePickerSelections.value.add(item.path);
}
function confirmFilePicker() {
  for (const p of filePickerSelections.value) {
    const name = p.split(/[\\/]/).pop() || p;
    aiContextFiles.value.push({ path: p, name });
  }
  showFilePicker.value = false;
  filePickerSelections.value.clear();
}
watch(showFilePicker, async (v) => { if (v) await loadFilePicker(store.currentProject || "."); });
function removeContextFile(idx: number) { aiContextFiles.value.splice(idx, 1); }

// ─── Git 面板：Git 提交 / 历史提交记录（集成 git-graph）───
/** Git 提交面板（原「Git 推送」） */
function openGitCommitPanel() {
  closeDropdowns();
  gitLocalPath.value = gitLocalPath.value || store.currentProject || "";
  showGitPushModal.value = true;
}
/** 历史提交记录面板：加载提交图并计算泳道布局 */
async function openGitHistoryPanel() {
  closeDropdowns();
  gitHistoryPath.value = gitLocalPath.value || store.currentProject || "";
  showGitHistoryModal.value = true;
  await loadGitGraph();
}
async function loadGitGraph() {
  gitGraphLoading.value = true;
  gitGraphError.value = "";
  try {
    const view = await tauriAPI.gitLogGraph(gitHistoryPath.value || ".", gitGraphCount.value, true);
    gitGraphCommits.value = view.commits || [];
    gitGraphBranch.value = view.branch || "";
    gitGraphHasMore.value = !!view.has_more;
    gitGraphRows.value = computeLanes(gitGraphCommits.value);
    gitGraphSelected.value = null;
    gitGraphDetail.value = null;
  } catch (e: any) {
    gitGraphError.value = String(e);
    gitGraphCommits.value = [];
    gitGraphRows.value = [];
    gitGraphHasMore.value = false;
  } finally {
    gitGraphLoading.value = false;
  }
}
/** 加载更多（对齐上游：初始 200，每次 +100） */
async function loadMoreGitGraph() {
  gitGraphCount.value = Math.min(gitGraphCount.value + 100, 5000);
  await loadGitGraph();
}
/** 当前分支/HEAD 指向的引用高亮（对齐上游 graphRefCurrent） */
function isCurrentRef(ref: string): boolean {
  return !!gitGraphBranch.value && ref === gitGraphBranch.value;
}
/** 展开某条提交：读取改动详情 */
async function selectGitCommit(row: GitGraphCommit) {
  if (gitGraphSelected.value === row.oid) {
    gitGraphSelected.value = null;
    gitGraphDetail.value = null;
    return;
  }
  gitGraphSelected.value = row.oid;
  gitGraphDetail.value = null;
  try {
    gitGraphDetail.value = await tauriAPI.gitCommitDetail(gitHistoryPath.value || ".", row.oid);
  } catch (e: any) {
    gitGraphDetail.value = { hash: row.oid, stat: String(e), files: [], patch: "" };
  }
}

async function selectGitLocalPath() {
  const dir = await open({ directory: true });
  if (dir) gitLocalPath.value = dir;
}
async function loadGitStatus() {
  try {
    const s = await tauriAPI.gitStatus(gitLocalPath.value || ".");
    gitStatusArea.value = `分支: ${s.branch} | 干净: ${s.clean ? '是' : '否'} | 变更: ${s.changes.length} | 暂存: ${s.staged.length} | 未跟踪: ${s.untracked.length}`;
  } catch (e: any) { gitStatusArea.value = "检查状态失败: " + e; }
}
async function gitPush() {
  const path = gitLocalPath.value || store.currentProject || ".";
  try {
    const result = await invoke<string>("git_push", {
      path,
      username: gitUsername.value,
      token: gitToken.value,
      repo: gitRemoteRepo.value,
      branch: gitBranch.value,
      message: gitCommitMsg.value || "update from DeepAhead"
    });
    gitStatusArea.value = result;
  } catch (e: any) { gitStatusArea.value = "推送失败: " + e; }
}

// ─── AI ───
function roleLabel(r: string) { return { user: "你", assistant: "AI", system: "系统" }[r] || r; }
function msgClass(r: string) { return { "user-message": r === "user", "ai-message": r === "assistant", "system-message": r === "system" }; }
async function handleSend() {
  // 有粘贴图片：先经视觉引擎识别，再连问题一起发送（无文本也可用默认问题）
  if (store.pastedImages.length > 0) {
    if (!canPasteImage.value) { alert("这是纯文本模型，请配置视觉引擎后再使用图片。"); return; }
    const q = chatInput.value.trim();
    chatInput.value = "";
    if (aiInputRef.value) aiInputRef.value.style.height = "auto";
    const paths = store.pastedImages.map(i => i.path);
    store.clearPastedImage();
    await store.sendWithImages(q, paths);
    nextTick(() => { if (aiChatRef.value) aiChatRef.value.scrollTop = aiChatRef.value.scrollHeight; });
    return;
  }
  const t = chatInput.value.trim(); if (!t || store.isLoading) return;
  chatInput.value = "";
  if (aiInputRef.value) aiInputRef.value.style.height = "auto";
  const ctxPaths = aiContextFiles.value.map(f => f.path);
  aiContextFiles.value = [];
  if (store.useTools) {
    await store.sendMessageWithTools(t, ctxPaths);
  } else {
    await store.sendMessageStream(t, ctxPaths);
  }
  nextTick(() => { if (aiChatRef.value) aiChatRef.value.scrollTop = aiChatRef.value.scrollHeight; });
}

// ─── 清空会话 / 上下文压缩 ───
/** 清空会话：清空右侧 AI 对话内容与上下文统计，日志内容保留 */
async function doClearSession() {
  if (store.isLoading) { alert("AI 正在运行中，请等待完成后再清空会话。"); return; }
  if (store.messages.length === 0) return;
  const ok = await showInlineConfirm("清空会话", "将清空右侧 AI 对话内容与上下文占用统计（日志面板内容保留）。确定继续吗？");
  if (!ok) return;
  store.clearSession();
  chatInput.value = "";
  aiContextFiles.value = [];
  store.appendLog("system", "已清空会话（AI 对话内容已清空，日志保留）");
}
/** 手动压缩上下文（不新建对话，只压缩较早轮次） */
async function doManualCompress() {
  if (store.isLoading) { alert("AI 正在运行中，请等待完成后再压缩上下文。"); return; }
  await store.compressContextManually();
}
/** 切换压缩模式（自动 / 手动） */
function onCompressionModeChange(e: Event) {
  const mode = (e.target as HTMLSelectElement).value as "auto" | "manual";
  store.setCompressionMode(mode);
  store.addSystemMessage(mode === "auto" ? "已开启自动压缩上下文（≥85% 自动压缩，不清空对话）" : "已切换为手动压缩上下文（≥85% 仅提示）");
}
/** 应用上下文窗口大小 */
function applyContextLimit() {
  store.setContextLimit(contextLimitInput.value);
  contextLimitInput.value = store.contextLimit;
  store.appendLog("context", `上下文窗口设置为 ${(store.contextLimit / 1000).toFixed(0)}k Tokens`);
}
/** 保存上下文占用页设置（窗口 + 压缩模式）并刷新引擎阈值 */
async function saveContextSettings() {
  applyContextLimit();
  await refreshCtxEngine();
  store.addSystemMessage(
    `上下文设置已保存：窗口 ${(store.contextLimit / 1000).toFixed(0)}k Tokens，压缩模式 ${store.compressionMode === "auto" ? "自动" : "手动"}`
  );
  showContextModal.value = false;
}

// 当前激活 Tab 的类型（用于显示预览/编辑切换按钮）
const activeTabKind = computed(() => {
  const tab = openTabs.value.find(t => t.path === activeTab.value);
  return tab?.kind || "code";
});

/** Markdown Tab：预览 ⇄ 编辑 切换 */
function toggleMdPreview() {
  const tab = openTabs.value.find(t => t.path === activeTab.value);
  if (!tab || tab.kind !== "md") return;
  // 切到编辑：把当前编辑器内容同步回 tab.content
  if (showMdPreview.value) {
    showMdPreview.value = false;
  } else {
    if (!tab.previewHtml || tab.content !== undefined) {
      tab.previewHtml = markdownToHtml(tab.content || "");
    }
    showMdPreview.value = true;
  }
}

/** 撤回对话：用户消息回到输入框，撤销该轮 Agent 的文件修改，删除后续消息 */
async function withdrawMessage(i: number) {
  if (store.isLoading) { alert("AI 正在运行中，请等待完成后再撤回。"); return; }
  const msg = store.messages[i];
  if (!msg || msg.role !== "user") return;

  // 1. 收集被删除消息对应的 Agent 运行 ID
  const runIds: string[] = [];
  for (let j = i; j < store.messages.length; j++) {
    const rid = store.runIdForMsg(store.messages[j].id || "");
    if (rid) runIds.push(rid);
  }
  // 2. 撤销文件变更（write/edit 恢复原样，新建文件删除）
  const undoNotes: string[] = [];
  for (const rid of runIds) {
    try {
      const notes = await tauriAPI.undoRunChanges(rid);
      undoNotes.push(...notes);
    } catch (e: any) { console.warn("undo failed", rid, e); }
  }
  // 3. 删除消息 + 清理 run 追踪
  store.clearRunIdsFrom(i);
  store.removeMessagesFrom(i);
  // 4. 消息内容回填输入框
  chatInput.value = msg.content;
  aiContextFiles.value = [];
  store.clearPastedImage();
  nextTick(() => {
    if (aiInputRef.value) { aiInputRef.value.focus(); autoResizeAIInput(); }
  });
  // 5. 刷新文件树 + 重载已打开的 Tab（内容可能被回滚）
  if (store.currentProject) await store.loadFileTree(store.currentProject);
  await reloadOpenTabs();
  if (undoNotes.length) store.addSystemMessage(`↩ 已撤回对话并撤销 ${undoNotes.length} 处文件变更`);
}

/** 重新从磁盘读取已打开 Tab 的内容（撤回文件变更后调用） */
async function reloadOpenTabs() {
  for (const tab of openTabs.value) {
    const ext = tab.name.split(".").pop()?.toLowerCase() || "";
    if (["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"].includes(ext)) continue;
    try {
      const content = await tauriAPI.readFile(tab.path);
      tab.content = content;
      if (tab.kind === "md" || tab.kind === "table") {
        tab.previewHtml = tab.kind === "md"
          ? markdownToHtml(content)
          : tab.previewHtml; // 表格预览由命令生成，保持现状
      }
      if (activeTab.value === tab.path) {
        if (tab.kind === "md") {
          mdPreviewHtml.value = tab.previewHtml || "";
        } else if (tab.kind !== "table") {
          if (cmView.value) {
            setEditorContent(cmView.value, content);
            setEditorLanguage(cmView.value, tab.name, store.editorTheme);
          }
        }
      }
    } catch (e) { /* 文件可能已被删除 */ }
  }
}

/**
 * 粘贴图片：
 * - 模型原生支持多模态，或已开启视觉引擎 → 读剪贴板图片 → 存临时文件 → 加入待发送图片（最多 6 张）
 * - 纯文本模型且未配置视觉引擎 → 拦截并提示"请配置视觉引擎使用"
 */
async function onPasteImage(e: ClipboardEvent) {
  const items = e.clipboardData?.items; if (!items) return;
  // 找出剪贴板中的图片项
  const imageItems: DataTransferItem[] = [];
  for (let i = 0; i < items.length; i++) {
    const it = items[i];
    if (it.kind === "file" && it.type.startsWith("image/")) imageItems.push(it);
  }
  if (imageItems.length === 0) return;

  // 纯文本模型 + 未开启视觉引擎：明确提示
  if (!canPasteImage.value) {
    e.preventDefault();
    alert("这是纯文本模型，请配置视觉引擎使用：\n\n打开「AI配置」→ 勾选「视觉引擎」→ 填写 Vision API Key / Base URL / Model。\n配置后粘贴的图片会先由视觉引擎转译为文本，再交给主模型推理。");
    store.appendLog("system", "粘贴图片被拦截：纯文本模型且未配置视觉引擎");
    return;
  }

  e.preventDefault();
  let added = 0;
  for (const it of imageItems) {
    if (store.pastedImages.length >= MAX_PASTE_IMAGES) {
      alert(`每次提问最多 ${MAX_PASTE_IMAGES} 张图片。`);
      break;
    }
    const file = it.getAsFile(); if (!file) continue;
    const ext = (file.type.split("/")[1] || "png").replace("jpeg", "jpg");
    const dataUrl = await new Promise<string>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result || ""));
      reader.readAsDataURL(file);
    });
    if (!dataUrl) continue;
    if (await store.addPastedImage(dataUrl, ext)) added++;
  }
  if (added > 0) {
    await nextTick();
    autoResizeAIInput();
  }
}
function autoResizeAIInput() {
  const input = aiInputRef.value; if (!input) return;
  input.style.height = "auto";
  input.style.height = Math.min(Math.max(input.scrollHeight, 120), 420) + "px";
}
function saveAIConfig() { saveApiConfig(); } // deprecated, kept for ref
async function saveApiConfig() {
  // 主模型只支持 DeepSeek 模型
  const m = (modelInput.value || "").trim();
  if (!m) { alert("请选择 DeepSeek 主模型。"); return; }
  if (!isDeepSeekModel(m)) {
    alert(`主模型只支持 DeepSeek 模型，当前填写的是「${m}」。\n请从下拉列表选择，或填写包含 deepseek 的模型名（如 deepseek-chat / deepseek-reasoner）。`);
    return;
  }
  modelInput.value = m;
  store.baseUrl = baseUrlInput.value; store.model = m;
  localStorage.setItem("deep-ide-api-config", JSON.stringify({
    apiKey: apiKeyInput.value, baseUrl: baseUrlInput.value, model: m,
  }));
  await store.configureApiKey(apiKeyInput.value);
  await store.switchMode(store.currentMode);
  showAIConfigModal.value = false;
}

// ─── 视觉引擎 / 上下文配置 ───
/** 勾选「工具」= 默认 Agent 模式（走 9 工具 Agent Loop） */
function toggleTools(e: Event) {
  store.useTools = (e.target as HTMLInputElement).checked;
  localStorage.setItem("deep-ide-tools", JSON.stringify(store.useTools));
  store.addSystemMessage(
    store.useTools
      ? "已开启工具 → 默认 Agent 模式（9 工具 Agent Loop，自主调用直到得出结论）"
      : "已关闭工具 → 对话模式（仅单轮回复，不调用工具）"
  );
  store.appendLog("mode", store.useTools ? "启用 Agent 模式（工具已勾选）" : "切换为对话模式（工具已关闭）");
}
function toggleMaxMode(e: Event) {
  maxMode.value = (e.target as HTMLInputElement).checked;
  localStorage.setItem("deep-ide-max-mode", JSON.stringify(maxMode.value));
  store.addSystemMessage(maxMode.value ? "已开启最大能力模式（9 工具 Agent Loop）" : "已关闭最大能力模式");
  store.appendLog("mode", maxMode.value ? "开启 max 最大能力模式" : "关闭 max 最大能力模式");
}
function toggleMultimodal(e: Event) {
  multimodalEnabled.value = (e.target as HTMLInputElement).checked;
  localStorage.setItem("deep-ide-multimodal", JSON.stringify(multimodalEnabled.value));
  store.addSystemMessage(multimodalEnabled.value ? "已开启视觉引擎（识图）" : "已切换为纯文本模式");
  store.appendLog("mode", multimodalEnabled.value ? "开启视觉引擎（识图）" : "关闭视觉引擎，切换为纯文本模式");
  // 纯文本模型 + 未配置视觉引擎：提示需要搭配视觉引擎使用
  if (multimodalEnabled.value && visionKeyInput.value.trim() === "" && !modelHasNativeVision.value) {
    alert("已开启视觉引擎，但尚未填写 Vision API Key。\n请填写下方「视觉引擎」配置（引擎 / Vision API Key / Base URL / Model）后点击「保存视觉引擎」。");
  }
}
/** 打开集成能力面板并加载全部数据 */
async function openCapabilities() {
  showCapModal.value = true;
  await refreshCapabilities();
}
// ─── AI 驾驶舱：三个下拉入口 ───
function openAiConfigTab() {
  closeDropdowns();
  openAIConfig();
}
function openContextTab() {
  closeDropdowns();
  contextLimitInput.value = store.contextLimit;
  refreshCtxEngine();
  showContextModal.value = true;
}
function openCapabilitiesTab() {
  closeDropdowns();
  capTab.value = "cost";
  openCapabilities();
}
async function openAIConfig() {
  contextLimitInput.value = store.contextLimit;
  await loadVisionConfig();
  await refreshCapabilities();
  showAIConfigModal.value = true;
}
async function saveVisionConfig() {
  try {
    await store.configureVision(visionProvider.value, visionKeyInput.value, visionBaseUrl.value, visionModel.value);
    localStorage.setItem("deep-ide-vision-config", JSON.stringify({
      provider: visionProvider.value, key: visionKeyInput.value, baseUrl: visionBaseUrl.value, model: visionModel.value,
    }));
    if (visionKeyInput.value.trim()) multimodalEnabled.value = true;
    alert("视觉引擎已保存：" + visionProvider.value + " / " + visionModel.value);
  } catch (e: any) {
    alert("保存视觉引擎失败: " + e);
  }
}
async function loadVisionConfig() {
  try {
    const cfg = await tauriAPI.getVisionConfig();
    visionProvider.value = cfg.provider;
    visionBaseUrl.value = cfg.base_url;
    visionModel.value = cfg.model;
  } catch (_) {}
}

// ─── 设置/插件 ───
function loadSettings() {
  const saved = localStorage.getItem("deep-ide-settings");
  if (saved) { const s = JSON.parse(saved); settingsLanguage.value = s.language || "zh-CN"; }
}
function changeLanguage() {
  localStorage.setItem("deep-ide-settings", JSON.stringify({ language: settingsLanguage.value }));
}
function onEditorThemeChange() {
  store.setEditorTheme(store.editorTheme);
  // 立即应用到当前编辑器
  if (cmView.value && activeTab.value) {
    const tab = openTabs.value.find(t => t.path === activeTab.value);
    if (tab) {
      setEditorTheme(cmView.value, tab.name, store.editorTheme);
    }
  }
  // 应用编辑器区域的背景色
  applyEditorThemeBg();
}
function applyEditorThemeBg() {
  const editorEl = document.getElementById("cm-editor");
  if (!editorEl) return;
  // CodeMirror 生成的 .cm-editor 元素在 #cm-editor 容器内
  const cmEditor = editorEl.querySelector(".cm-editor") as HTMLElement | null;
  switch (store.editorTheme) {
    case "classic": editorEl.style.backgroundColor = "#ffffff"; cmEditor?.classList.remove("dark-theme"); break;
    case "green": editorEl.style.backgroundColor = "#dff2e2"; cmEditor?.classList.remove("dark-theme"); break;
    case "dark": editorEl.style.backgroundColor = "#1e1e2e"; cmEditor?.classList.add("dark-theme"); break;
  }
}
function loadInstalledExtensions() {
  const saved = localStorage.getItem("deep-ide-extensions");
  installedExtensions.value = saved ? JSON.parse(saved) : [];
}
function saveInstalledExtensions() {
  localStorage.setItem("deep-ide-extensions", JSON.stringify(installedExtensions.value));
}
function isExtensionInstalled(id: string) { return installedExtensions.value.some(e => e.id === id); }
function toggleExtension(ext: Extension) { ext.disabled = !ext.disabled; saveInstalledExtensions(); }
async function searchMarketplace() {
  marketplaceLoading.value = true;
  try {
    const result = await invoke<any[]>("search_vscode_marketplace", { query: marketplaceSearch.value, sortBy: parseInt(marketplaceSortBy.value) });
    marketplaceExtensions.value = result.map(r => ({
      id: r.extension_id || r.extensionId || r.extension_name || r.extensionName,
      name: r.extension_name || r.extensionName || '',
      displayName: r.display_name || r.displayName || r.extension_name || r.extensionName || '',
      publisher: r.publisher || '',
      description: r.short_description || r.shortDescription || '',
      icon: r.icon || null,
      disabled: false,
    }));
  } catch (e) { marketplaceExtensions.value = []; }
  marketplaceLoading.value = false;
}
function installExtension(ext: Extension) {
  installedExtensions.value.push({ ...ext, disabled: false });
  saveInstalledExtensions();
}

// ─── 缩放 ───
function startResize(type: "explorer" | "terminal", e: MouseEvent) {
  e.preventDefault();
  const startX = e.clientX, startY = e.clientY;
  const panel = type === "explorer" ? document.getElementById("fileExplorerPanel") : document.getElementById("terminalPanel");
  const editorArea = document.querySelector(".editor-area") as HTMLElement;
  if (!panel || !editorArea) return;
  const startW = panel.offsetWidth, startH = panel.offsetHeight;
  const onMove = (ev: MouseEvent) => {
    if (type === "explorer") {
      const newW = Math.max(160, Math.min(480, startW + ev.clientX - startX));
      panel.style.width = newW + "px";
    } else {
      const newH = Math.max(120, Math.min(editorArea.offsetHeight * 0.5, startH - (ev.clientY - startY)));
      (panel as HTMLElement).style.flex = "none"; (panel as HTMLElement).style.height = newH + "px";
    }
  };
  const onUp = () => { document.removeEventListener("mousemove", onMove); document.removeEventListener("mouseup", onUp); };
  document.addEventListener("mousemove", onMove); document.addEventListener("mouseup", onUp);
}
function toggleFolder(entry: any) { entry.expanded = !entry.expanded; }
</script>

<style scoped>
.editor-page { display:flex; flex-direction:column; width:100%; height:100%; }
.editor-main-content { flex:1; min-height:0; display:flex; flex-direction:column; position:relative; }

/* 运行时/运行文件选择器 */
.runtime-select, .runfile-select, .browser-select {
  padding: 0.25rem 0.3rem; border: 1px solid #ddd; border-radius: 4px;
  font-size: 0.76rem; color: #555; outline: none; background: #fff;
  max-width: 180px;
}
.runtime-select { min-width: 120px; max-width: 220px; }
.add-runtime-btn {
  padding: 0.22rem 0.38rem; border: 1px solid #ddd; border-radius: 4px;
  background: #fff; color: #007acc; font-size: 0.76rem; cursor: pointer; line-height: 1;
}

/* AI 消息 */
.msg-role { font-weight: 600; font-size: 0.72rem; color: #888; margin-bottom: 0.2rem; }
.msg-content { white-space: pre-wrap; word-break: break-word; }
.system-message { background: #fffbe6; border: 1px solid #ffe58f; color: #876800; font-size: 0.78rem; }

/* 浮动快捷菜单 */
.ai-quick-actions {
  position: absolute; right: 0.5rem; top: 50%; transform: translateY(-50%);
  display: flex; flex-direction: column; gap: 0.3rem; z-index: 10;
}
.quick-action-btn {
  display: flex; align-items: center; gap: 0.3rem;
  padding: 0.3rem 0.6rem; border: 1px solid #e8e8e8; border-radius: 20px;
  background: #fff; color: #555; font-size: 0.72rem; cursor: pointer;
  box-shadow: 0 1px 4px rgba(0,0,0,0.06); transition: all 0.15s;
  white-space: nowrap;
}
.quick-action-btn:hover { border-color: #bbb; box-shadow: 0 2px 8px rgba(0,0,0,0.1); }
.quick-action-btn.chat-btn { border-radius: 50%; width: 32px; height: 32px; padding: 0; justify-content: center; font-size: 1rem; }

/* 右键菜单 */
.context-menu { display: none; position: fixed; background: #fff; border: 1px solid #ddd; border-radius: 6px; box-shadow: 0 4px 16px rgba(0,0,0,0.12); z-index: 2000; min-width: 160px; padding: 0.3rem 0; }
.context-menu.show { display: block; }
.context-item { padding: 0.4rem 1rem; font-size: 0.82rem; color: #333; cursor: pointer; display: flex; align-items: center; gap: 0.5rem; }
.context-item:hover { background: #f0f0f0; }
.context-item.disabled { color: #999; pointer-events: none; }
.context-divider { height: 1px; background: #eee; margin: 0.2rem 0; }

/* 弹框 */
.modal-overlay { display: none; position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0,0,0,0.3); z-index: 3000; align-items: center; justify-content: center; }
.modal-overlay.show { display: flex; }
.modal-box { background: #fff; border-radius: 10px; width: 520px; max-width: 90vw; max-height: 80vh; overflow-y: auto; box-shadow: 0 8px 32px rgba(0,0,0,0.15); }
.modal-header { display: flex; justify-content: space-between; align-items: center; padding: 1rem 1.2rem; border-bottom: 1px solid #eee; }
.modal-header h3 { font-size: 1rem; font-weight: 500; color: #333; }
.modal-close { width: 28px; height: 28px; border: none; background: #f0f0f0; border-radius: 50%; cursor: pointer; font-size: 1rem; color: #888; display: flex; align-items: center; justify-content: center; }
.modal-close:hover { background: #e0e0e0; color: #333; }
.modal-body { padding: 1.2rem; }

/* 文件选择器 */
.file-picker-overlay { display: none; position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0,0,0,0.2); z-index: 4000; align-items: center; justify-content: center; }
.file-picker-overlay.show { display: flex; }
.file-picker-box { background: #fff; border-radius: 8px; width: 400px; max-width: 90vw; max-height: 70vh; display: flex; flex-direction: column; box-shadow: 0 8px 32px rgba(0,0,0,0.15); }
.file-picker-header { display: flex; justify-content: space-between; align-items: center; padding: 0.8rem 1rem; border-bottom: 1px solid #eee; }
.file-picker-list { flex: 1; overflow-y: auto; padding: 0.5rem; max-height: 50vh; }
.file-picker-item { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 0.5rem; cursor: pointer; border-radius: 4px; font-size: 0.8rem; color: #333; }
.file-picker-item:hover { background: #f0f0f0; }
.file-picker-item.selected { background: #e8f0fe; color: #1a73e8; }
.file-picker-item.dir { font-weight: 500; }
.file-picker-footer { display: flex; gap: 0.5rem; padding: 0.8rem 1rem; border-top: 1px solid #eee; justify-content: flex-end; }

/* 插件市场 */
.marketplace-modal .modal-box { width: 900px; max-width: 95vw; max-height: 85vh; }
.marketplace-search { display: flex; gap: 0.6rem; margin-bottom: 1rem; }
.marketplace-search input { flex: 1; padding: 0.55rem 0.8rem; border: 1px solid #e0e0e0; border-radius: 6px; font-size: 0.88rem; outline: none; }
.marketplace-search select { padding: 0.55rem 0.6rem; border: 1px solid #e0e0e0; border-radius: 6px; font-size: 0.84rem; outline: none; color: #555; background: #fff; }
.marketplace-tabs { display: flex; gap: 0; border-bottom: 1px solid #eee; margin-bottom: 0.8rem; }
.marketplace-tab { padding: 0.4rem 1rem; font-size: 0.82rem; color: #888; cursor: pointer; border-bottom: 2px solid transparent; transition: all 0.2s; }
.marketplace-tab.active { color: #333; border-bottom-color: #007acc; }
.marketplace-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 0.8rem; max-height: 55vh; overflow-y: auto; padding-right: 0.3rem; }
.ext-card { background: #fff; border: 1px solid #eee; border-radius: 8px; padding: 0.8rem; display: flex; flex-direction: column; gap: 0.4rem; transition: box-shadow 0.2s; }
.ext-card:hover { box-shadow: 0 2px 8px rgba(0,0,0,0.08); }
.ext-card-header { display: flex; align-items: center; gap: 0.5rem; }
.ext-icon { width: 36px; height: 36px; border-radius: 6px; background: #f0f0f0; display: flex; align-items: center; justify-content: center; font-size: 1.1rem; flex-shrink: 0; overflow: hidden; }
.ext-icon-img { width: 36px; height: 36px; border-radius: 6px; object-fit: cover; flex-shrink: 0; background: #f0f0f0; }
.ext-icon-placeholder { width: 36px; height: 36px; border-radius: 6px; background: #f0f0f0; display: flex; align-items: center; justify-content: center; font-size: 1.1rem; flex-shrink: 0; }
.ext-name { font-weight: 500; font-size: 0.85rem; color: #333; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.ext-publisher { font-size: 0.72rem; color: #999; }
.ext-desc { font-size: 0.76rem; color: #777; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.ext-actions { display: flex; gap: 0.4rem; margin-top: auto; }
.ext-btn { flex: 1; padding: 0.35rem 0.5rem; border: 1px solid #e0e0e0; border-radius: 4px; font-size: 0.76rem; cursor: pointer; background: #fff; color: #555; transition: all 0.2s; text-align: center; }
.ext-btn:hover { border-color: #aaa; color: #333; }
.ext-btn.install { background: #007acc; color: #fff; border-color: #007acc; }
.ext-btn.install:hover { background: #005a9e; }
.ext-btn.installed { background: #e8f5e9; color: #2e7d32; border-color: #c8e6c9; cursor: default; }

/* AI配置 */
.config-card { background: #fff; border: 1px solid #eee; border-radius: 8px; padding: 0.8rem; margin-bottom: 0.6rem; }
.config-card-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem; }
.config-card-header span { font-size: 0.85rem; font-weight: 500; color: #333; }
.model-status { font-size: 0.72rem; padding: 0.15rem 0.4rem; border-radius: 3px; }
.model-status.connected { background: #e8f5e9; color: #2e7d32; }
.model-status.disconnected { background: #fce4ec; color: #c62828; }

/* 已粘贴图片：输入框内缩略图预览 */
.pasted-image-inline { position: relative; display: inline-block; margin: 0.4rem 0 0; }
.pasted-image-inline img { display: block; width: 88px; height: 88px; object-fit: cover; border: 1px solid #c9d8f2; border-radius: 8px; }
.pasted-image-inline .chip-close { position: absolute; top: -6px; right: -6px; width: 20px; height: 20px; line-height: 18px; text-align: center; border-radius: 50%; background: #e74c3c; color: #fff; border: 1px solid #fff; font-size: 1rem; cursor: pointer; padding: 0; }
.config-field { margin-bottom: 0.5rem; }
.config-field label { display: block; font-size: 0.76rem; color: #777; margin-bottom: 0.2rem; }
.config-field input:not([type="checkbox"]):not([type="radio"]) { width: 100%; padding: 0.4rem 0.5rem; border: 1px solid #e0e0e0; border-radius: 4px; font-size: 0.82rem; outline: none; }
.config-field input[type="checkbox"], .config-field input[type="radio"] { width: 1rem; height: 1rem; flex: none; cursor: pointer; margin: 0; }

/* 插件列表 */
.plugin-list { padding: 0.8rem; overflow-y: auto; }
.plugin-item { display: flex; align-items: center; gap: 0.5rem; padding: 0.5rem 0.6rem; border-bottom: 1px solid #f0f0f0; font-size: 0.83rem; color: #555; }
.plugin-item:last-child { border-bottom: none; }
.plugin-icon { font-size: 1.1rem; }
.plugin-info { flex: 1; }
.plugin-name { font-weight: 500; color: #333; }
.plugin-desc { font-size: 0.75rem; color: #999; }
.plugin-toggle { width: 32px; height: 18px; border-radius: 9px; background: #ccc; position: relative; cursor: pointer; transition: background 0.2s; flex-shrink: 0; }
.plugin-toggle.on { background: #27ae60; }
.plugin-toggle::after { content: ''; position: absolute; width: 14px; height: 14px; border-radius: 50%; background: #fff; top: 2px; left: 2px; transition: transform 0.2s; }
.plugin-toggle.on::after { transform: translateX(14px); }

/* loading spinner */
.loading-spinner { display: inline-block; width: 14px; height: 14px; border: 2px solid #ddd; border-top-color: #999; border-radius: 50%; animation: spin 0.6s linear infinite; vertical-align: middle; margin-right: 0.4rem; }
@keyframes spin { to { transform: rotate(360deg); } }

/* 市场 */
.marketplace-loading { text-align: center; padding: 2rem; color: #999; font-size: 0.88rem; }
.marketplace-empty { text-align: center; padding: 2rem; color: #bbb; font-size: 0.88rem; }

/* 界面皮肤 */
.skin-list { margin-top: 0.5rem; max-height: 260px; overflow-y: auto; border: 1px solid #f0f0f0; border-radius: 8px; }
.skin-item { display: flex; align-items: center; gap: 0.5rem; padding: 0.55rem 0.7rem; cursor: pointer; border-bottom: 1px solid #f5f5f5; transition: background 0.15s; }
.skin-item:last-child { border-bottom: none; }
.skin-item:hover { background: #f8f9fb; }
.skin-item.active { background: #eef4ff; }
.skin-info { flex: 1; min-width: 0; }
.skin-name { font-size: 0.85rem; font-weight: 500; color: #333; display: flex; align-items: center; gap: 0.4rem; }
.skin-tag { font-size: 0.68rem; color: #5aa7d8; border: 1px solid #a8d1df; border-radius: 4px; padding: 0 0.3rem; line-height: 1.4; }
.skin-desc { font-size: 0.74rem; color: #999; margin-top: 0.1rem; }
.skin-source { font-size: 0.68rem; color: #bbb; margin-top: 0.1rem; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.skin-actions { display: flex; align-items: center; gap: 0.3rem; flex-shrink: 0; }
.skin-variant-btn { border: 1px solid #ddd; background: #fff; color: #888; font-size: 0.72rem; border-radius: 4px; padding: 0.1rem 0.45rem; cursor: pointer; }
.skin-variant-btn.on { background: #333; color: #fff; border-color: #333; }
.skin-delete-btn { border: none; background: #f0f0f0; color: #999; width: 20px; height: 20px; border-radius: 50%; cursor: pointer; font-size: 0.8rem; line-height: 1; }
.skin-delete-btn:hover { background: #e74c3c; color: #fff; }
.skin-import { display: flex; gap: 0.4rem; margin-top: 0.6rem; }
.skin-import input { flex: 1; border: 1px solid #e0e0e0; border-radius: 6px; padding: 0.4rem 0.6rem; font-size: 0.8rem; outline: none; }
.skin-import input:focus { border-color: #aaa; }
.skin-import-btn { border: none; background: #333; color: #fff; border-radius: 6px; padding: 0.4rem 0.8rem; font-size: 0.8rem; cursor: pointer; white-space: nowrap; }
.skin-import-btn:disabled { background: #bbb; cursor: not-allowed; }
.skin-import-msg { margin-top: 0.4rem; font-size: 0.74rem; color: #27ae60; line-height: 1.5; }
.skin-import-msg.error { color: #e74c3c; }

/* ─── 执行许可审批卡片（需分步确认模式，对标 Harness 审批门）─── */
#aiChatPanel { position: relative; }
.tool-approval-overlay { position: absolute; left: 0; right: 0; bottom: 0; top: 0; background: rgba(255,255,255,0.78); backdrop-filter: blur(2px); display: flex; align-items: flex-end; justify-content: center; padding: 1rem; z-index: 60; }
.tool-approval-card { width: 100%; max-width: 420px; background: #fff; border: 1px solid #f1c40f; border-radius: 10px; box-shadow: 0 8px 28px rgba(0,0,0,0.18); padding: 0.9rem 1rem; }
.tool-approval-head { font-size: 0.85rem; font-weight: 600; color: #333; margin-bottom: 0.45rem; }
.tool-approval-mode { display: inline-block; background: #fff7e0; color: #b8860b; border: 1px solid #f1c40f; border-radius: 10px; padding: 0 0.45rem; font-size: 0.7rem; font-weight: 500; margin-left: 0.35rem; vertical-align: middle; }
.tool-approval-name { font-size: 0.9rem; font-weight: 600; color: #0b3d91; margin-bottom: 0.4rem; word-break: break-all; }
.tool-approval-args { background: #f7f7f8; border: 1px solid #eee; border-radius: 6px; padding: 0.5rem 0.6rem; font-size: 0.72rem; color: #555; max-height: 160px; overflow: auto; white-space: pre-wrap; word-break: break-all; margin-bottom: 0.7rem; }
.tool-approval-actions { display: flex; gap: 0.5rem; }
.tool-approval-btn { flex: 1; border: none; border-radius: 7px; padding: 0.5rem 0; font-size: 0.85rem; font-weight: 600; cursor: pointer; }
.tool-approval-btn.allow { background: #27ae60; color: #fff; }
.tool-approval-btn.allow:hover { background: #219653; }
.tool-approval-btn.deny { background: #e74c3c; color: #fff; }
.tool-approval-btn.deny:hover { background: #d84332; }
</style>

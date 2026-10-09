//! ─── 规则引擎（移植自 jilian-dsh/dsh-rule-engine）───
//!
//! 原插件把用户写的 `AGENTS.md` 规则**机械地**变成运行时约束，而不是靠模型自觉。
//! 它挂在 DSH 的四个点上：`tools.guard` 同步硬门、`tools/pre-execute` 中间件、
//! `session/event` 文本审计、`/guard` 命令面。
//!
//! 本模块移植**可移植核心**（语言中立的机制层）：
//!   1. 规则容器解析（可配置格式契约：章节 / 规则头 / free-zone / 执行等级 / 四要素）
//!   2. 等级 → 动作与置信度派生（低置信规则永不硬拦）
//!   3. 工具分类表（analysis / artifact / mutating / unknown + 前缀规则）
//!   4. 只读命令判定（引用感知分段 + 有序管线）
//!   5. `guard_decision`：旁路窗口 → 自保护 → 内联命令禁令 → 重试熔断 →
//!      授权匹配（意图直判 + 敏感操作审批）→ 允许
//!   6. 审计账本（JSONL + 2MB 轮转）+ 带 ERR 编号的拒绝信封（供打标与指纹放行）
//!
//! 未移植（原插件自身也未实现或依赖 DSH 专有钩子）：LLM 意图兜底、
//! 文本注入纠正通道、任务契约、turn card、远程服务面板。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// ════════════════════════════════════════════════════════
// 规则容器解析
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub index: usize,
    pub id: String,
    pub title: String,
    pub section: String,
    pub start_line: usize,
    pub end_line: usize,
    /// 执行等级原始文本（如 "A" / "B + D"）
    pub level: String,
    pub body: String,
    /// 派生：动作集合
    pub actions: Vec<String>,
    pub confidence: String,
    pub handler: String,
    pub trigger: String,
    pub check: String,
    pub action: String,
    pub exemption: String,
}

/// 等级 → 动作（对齐 understander.js:actionsForLevel）
pub fn actions_for_level(level: &str) -> Vec<String> {
    let l = level.replace(' ', "");
    let mut out = Vec::new();
    let push = |v: &mut Vec<String>, a: &str| {
        if !v.iter().any(|x| x == a) {
            v.push(a.to_string());
        }
    };
    for part in l.split(['+', '＋', '、']) {
        let head: String = part.chars().take(1).collect();
        match head.as_str() {
            "A" => push(&mut out, "deny"),
            "B" => push(&mut out, "correct"),
            "C" => push(&mut out, "ask"),
            "D" => push(&mut out, "self-certify"),
            "M" => push(&mut out, "meta"),
            _ => {}
        }
    }
    if out.is_empty() {
        push(&mut out, "self-certify");
    }
    out
}

/// 置信度：high = 等级 ∧ 触发 ∧ 检查 ∧ (动作 ∨ 等级含 D)；medium = 有等级；否则 low
fn confidence_of(level: &str, trigger: &str, check: &str, action: &str, actions: &[String]) -> String {
    if level.is_empty() {
        return "low".into();
    }
    let has_d = level.contains('D') || actions.iter().any(|a| a == "self-certify");
    if !trigger.is_empty() && !check.is_empty() && (!action.is_empty() || has_d) {
        "high".into()
    } else {
        "medium".into()
    }
}

/// 解析 `AGENTS.md`（格式契约与上游一致：`### [规则 N] 标题` / `执行等级：A` / `**触发**：…`）
pub fn parse_rules(text: &str) -> Vec<Rule> {
    let lines: Vec<&str> = text.lines().collect();
    let mut rules: Vec<Rule> = Vec::new();
    let mut section = String::new();
    let mut in_free_zone = false;
    let mut current: Option<Rule> = None;
    let mut label: Option<&'static str> = None;

    let flush = |cur: &mut Option<Rule>, out: &mut Vec<Rule>| {
        if let Some(r) = cur.take() {
            out.push(r);
        }
    };

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end();
        let trimmed = line.trim();

        if trimmed.contains("<!--") && trimmed.contains("free-zone:start") {
            in_free_zone = true;
            continue;
        }
        if trimmed.contains("<!--") && trimmed.contains("free-zone:end") {
            in_free_zone = false;
            continue;
        }
        if in_free_zone {
            continue;
        }

        // 章节
        if let Some(rest) = trimmed.strip_prefix("## ") {
            if !trimmed.starts_with("###") {
                flush(&mut current, &mut rules);
                section = rest.trim().to_string();
                label = None;
                continue;
            }
        }

        // 规则头：### [规则 22] 标题
        if trimmed.starts_with("###") {
            if let Some(open) = trimmed.find("[规则") {
                let after = &trimmed[open..];
                if let Some(close) = after.find(']') {
                    let id = after["[规则".len()..close].trim().to_string();
                    let title = after[close + 1..]
                        .trim()
                        .trim_end_matches(|c| c == '）' || c == ')')
                        .split('（')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    flush(&mut current, &mut rules);
                    current = Some(Rule {
                        index: rules.len(),
                        id,
                        title,
                        section: section.clone(),
                        start_line: i + 1,
                        end_line: i + 1,
                        level: String::new(),
                        body: String::new(),
                        actions: vec![],
                        confidence: "low".into(),
                        handler: String::new(),
                        trigger: String::new(),
                        check: String::new(),
                        action: String::new(),
                        exemption: String::new(),
                    });
                    label = None;
                    continue;
                }
            }
        }

        if let Some(r) = current.as_mut() {
            r.end_line = i + 1;
            r.body.push_str(line);
            r.body.push('\n');

            // 执行等级（按字符边界切分，避免多字节 '：' 撕裂 UTF-8）
            if let Some(pos) = trimmed.find("执行等级") {
                let after = &trimmed[pos..];
                if let Some((colon, ch)) = after.char_indices().find(|(_, c)| *c == '：' || *c == ':') {
                    let v: String = after[colon + ch.len_utf8()..]
                        .trim()
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '＋' | '、' | ' '))
                        .collect();
                    let v = v.trim().to_string();
                    if !v.is_empty() {
                        r.level = v;
                    }
                }
            }

            // 四要素：**触发**：…
            if trimmed.starts_with("**") {
                for (key, name) in [("触发", "trigger"), ("检查", "check"), ("动作", "action"), ("豁免", "exemption")] {
                    if trimmed.contains(key) {
                        if let Some((colon, ch)) = trimmed.char_indices().find(|(_, c)| *c == '：' || *c == ':') {
                            let value = trimmed[colon + ch.len_utf8()..].trim().to_string();
                            match name {
                                "trigger" => r.trigger = value,
                                "check" => r.check = value,
                                "action" => r.action = value,
                                _ => r.exemption = value,
                            }
                        }
                        label = None;
                        break;
                    }
                }
            } else if !trimmed.is_empty() && label.is_none() {
                // 续行并入上一要素（这里只做简化：并入 body）
            }
        }
    }
    flush(&mut current, &mut rules);

    // 派生动作 / 置信度
    for r in rules.iter_mut() {
        r.actions = actions_for_level(&r.level);
        r.confidence = confidence_of(&r.level, &r.trigger, &r.check, &r.action, &r.actions);
    }
    rules
}

// ════════════════════════════════════════════════════════
// 工具分类表
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolClass {
    /// 无条件允许
    Analysis,
    /// 允许 + 记账
    Artifact,
    /// 走完整严格链
    Mutating,
    Unknown,
}

const ANALYSIS_TOOLS: [&str; 24] = [
    "read", "grep", "glob", "read_image", "web_fetch", "web_search", "present",
    "ask_user_question", "job_list", "job_output", "list_agents", "get_goal",
    "todo_write", "memory_search", "memory_status", "memory_weave",
    "search_context", "acp_status", "acp_rule", "decompress", "skill",
    "list_subagent_models", "session_search", "run_safety_check",
];

const ARTIFACT_TOOLS: [&str; 10] = [
    "todo_write", "create_goal", "update_goal", "subagent", "subagent_fork",
    "send_message", "workflow", "exit_plan_mode", "vision_store",
    "install_python_package",
];

const MUTATING_TOOLS: [&str; 19] = [
    "pwsh", "bash", "run_command", "run_file", "str_replace_editor", "upload_file",
    "job_kill", "terminal_open", "terminal_send", "terminal_close", "compress",
    "interrupt_agent", "delete_file", "git_push", "git_clone",
    // 写文件类工具属于 mutating（上游分类：edit/write 走完整严格链）
    "write", "edit", "batch_write", "batch_edit",
];

/// 工具分类（显式表 → 前缀规则 → unknown）
pub fn classify_tool(name: &str) -> ToolClass {
    let n = name.to_lowercase();
    if MUTATING_TOOLS.contains(&n.as_str()) {
        return ToolClass::Mutating;
    }
    if ANALYSIS_TOOLS.contains(&n.as_str()) {
        return ToolClass::Analysis;
    }
    if ARTIFACT_TOOLS.contains(&n.as_str()) {
        return ToolClass::Artifact;
    }
    for p in ["mcp__", "dev_", "terminal_", "cordis_", "team_"] {
        if n.starts_with(p) {
            return ToolClass::Mutating;
        }
    }
    for p in ["vision_", "job_", "session_", "cordis_inspect"] {
        if n.starts_with(p) {
            return ToolClass::Analysis;
        }
    }
    ToolClass::Unknown
}

/// 是否属于需要用户逐项确认的**风险操作**。
///
/// 用于「仅确认风险操作」档位：只有这里判定为 true 的调用才会弹审批卡片，
/// 只读 / 分析类调用直接放行，不被无谓打扰。
pub fn is_risk_operation(tool: &str, _args: &serde_json::Value) -> bool {
    let by_class = match classify_tool(tool) {
        // 变更类与未知类一律视为风险
        ToolClass::Mutating | ToolClass::Unknown => true,
        // 分析类里只有少数带外发副作用的算风险
        ToolClass::Analysis => matches!(tool, "web_fetch" | "web_search"),
        ToolClass::Artifact => matches!(
            tool,
            "create_goal" | "update_goal" | "send_message" | "install_python_package"
        ),
    };
    if by_class {
        return true;
    }
    // 形似命令 / 推代码 / 删盘的工具名兜底（含 MCP 前缀工具）
    let n = tool.to_lowercase();
    n.starts_with("git_") || n.starts_with("terminal_") || n.starts_with("mcp__")
}

// ════════════════════════════════════════════════════════
// 只读命令判定
// ════════════════════════════════════════════════════════

/// PowerShell 别名归一
fn normalize_aliases(seg: &str) -> String {
    let mut s = seg.trim().to_string();
    let pairs = [
        ("rm ", "remove-item "), ("rd ", "remove-item "), ("ri ", "remove-item "),
        ("del ", "remove-item "), ("erase ", "remove-item "),
        ("cp ", "copy-item "), ("copy ", "copy-item "),
        ("mv ", "move-item "), ("move ", "move-item "),
        ("iex ", "invoke-expression "), ("ii ", "invoke-item "),
        ("cat ", "get-content "), ("gc ", "get-content "),
        ("ls ", "get-childitem "), ("dir ", "get-childitem "),
    ];
    let lower = s.to_lowercase();
    for (from, to) in pairs {
        if lower.starts_with(from) {
            s = format!("{}{}", to, &s[from.len()..]);
            break;
        }
    }
    s
}

/// 引用感知分段：分隔符 `;` `\n` `&&` `||` `|`，
/// 但**数字后的 `|` 保留**（否则 `2>&1` 会被误判成管道）
fn split_segments(cmd: &str) -> Vec<String> {
    let mut segs = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = cmd.chars().collect();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' && !in_double {
            in_single = !in_single;
        } else if c == '"' && !in_single {
            in_double = !in_double;
        }
        if !in_single && !in_double {
            if c == ';' || c == '\n' {
                segs.push(std::mem::take(&mut cur));
                i += 1;
                continue;
            }
            if c == '|' {
                let prev_is_digit = i > 0 && chars[i - 1].is_ascii_digit();
                let next_is_amp = i + 1 < chars.len() && chars[i + 1] == '&';
                if !prev_is_digit && !next_is_amp {
                    segs.push(std::mem::take(&mut cur));
                    i += 1;
                    continue;
                }
            }
            if c == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
                segs.push(std::mem::take(&mut cur));
                i += 2;
                continue;
            }
            if c == '|' && i + 1 < chars.len() && chars[i + 1] == '|' {
                segs.push(std::mem::take(&mut cur));
                i += 2;
                continue;
            }
        }
        cur.push(c);
        i += 1;
    }
    segs.push(cur);
    segs.into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

const READONLY_HEADS: [&str; 47] = [
    "get-childitem", "get-content", "get-item", "get-location", "get-date",
    "get-command", "get-process", "get-service", "select-string", "select-object",
    "where-object", "foreach-object", "measure-object", "sort-object", "format-table",
    "test-path", "resolve-path", "split-path", "join-path", "compare-object",
    "git status", "git log", "git diff", "git show", "git branch", "git remote",
    "node --version", "npm --version", "python --version", "cargo --version",
    // 常见 Unix 只读工具（上游 READONLY_CMD_RE 同为长白名单）
    "head", "tail", "cat", "grep", "wc", "sort", "uniq", "ls", "pwd",
    "echo", "which", "type", "stat", "file", "jq", "less", "find",
];

const MUTATING_MARKERS: [&str; 15] = [
    "remove-item", "copy-item", "move-item", "rename-item", "new-item",
    "set-content", "add-content", "out-file", "start-process", "invoke-expression",
    "rm ", "del ", "rmdir", "git push", "git commit",
];

/// 单段是否只读
fn segment_is_readonly(seg: &str) -> bool {
    let s = normalize_aliases(seg);
    let lower = s.to_lowercase();
    if lower.starts_with('#') {
        return true;
    }
    if MUTATING_MARKERS.iter().any(|m| lower.starts_with(m) || lower.contains(m)) {
        return false;
    }
    if READONLY_HEADS.iter().any(|h| lower.starts_with(h)) {
        return true;
    }
    false
}

/// 整条命令是否只读：**每一段都必须只读**，未知段 ⇒ 非只读
pub fn is_readonly_command(cmd: &str) -> bool {
    let segs = split_segments(cmd);
    if segs.is_empty() {
        return false;
    }
    segs.iter().all(|s| segment_is_readonly(s))
}

// ════════════════════════════════════════════════════════
// 授权与决策
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authorization {
    pub at: i64,
    pub expires_at: i64,
    /// delete / write / backup / git / command / skill / any
    pub r#type: String,
    pub path_prefix: String,
    pub source: String,
}

impl Authorization {
    pub fn expired(&self, now: i64) -> bool {
        self.expires_at > 0 && now > self.expires_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardDecision {
    pub allow: bool,
    pub rule_id: String,
    pub kind: String,
    pub reason: String,
    pub err_id: String,
}

fn err_id() -> String {
    // 6 位 base36 大写
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
        .unwrap_or(0);
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut n = nanos.wrapping_mul(2654435761);
    let mut s = String::new();
    for _ in 0..6 {
        s.push(ALPHABET[(n % 36) as usize] as char);
        n /= 36;
    }
    s
}

/// 拒绝信封（对齐上游 makeHit 的形状，供打标与指纹放行使用）
pub fn denial(rule_id: &str, kind: &str, reason: &str) -> GuardDecision {
    let e = err_id();
    let prefix = if rule_id.starts_with("__") {
        "[guardian:contract]".to_string()
    } else {
        format!("[guardian:rule{}]", rule_id)
    };
    GuardDecision {
        allow: false,
        rule_id: rule_id.to_string(),
        kind: kind.to_string(),
        reason: format!(
            "{} {}（规则 {}｜误判可打标：/guard label ERR-{} incorrect）",
            prefix, reason, rule_id, e
        ),
        err_id: e,
    }
}

/// 规则引擎运行时状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleEngineConfig {
    pub enabled: bool,
    pub correct_inject: bool,
    /// off / ask / deny
    pub unknown_policy: String,
    /// 免检工具
    pub allowlist: Vec<String>,
    /// 旁路截止时间（Unix 秒）
    pub bypass_until: i64,
    /// 解锁截止时间（自保护豁免）
    pub unlock_until: i64,
    /// 需要保护的文件（自保护）
    pub protected_files: Vec<String>,
    /// 回合末裁决卡片开关（默认关闭，对齐上游）
    pub turn_card_enabled: bool,
    /// 任务契约开关（默认关闭，对齐上游；本移植仅保留开关位）
    pub task_contract_enabled: bool,
}

impl Default for RuleEngineConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            correct_inject: true,
            unknown_policy: "off".into(),
            allowlist: vec![],
            bypass_until: 0,
            unlock_until: 0,
            protected_files: vec![
                "AGENTS.md".into(),
                "rule-engine.json".into(),
                "rule-understanding.json".into(),
                ".credentials.yaml".into(),
                "settings.yaml".into(),
                "cordis.patch.yml".into(),
            ],
            // 上游：卡片默认关闭（面向大众/通用性，用户按需打开）
            turn_card_enabled: false,
            task_contract_enabled: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct RuleEngineState {
    pub rules: Vec<Rule>,
    pub authorizations: Vec<Authorization>,
    /// 重试计数：tool:args → 次数
    pub retry_counts: HashMap<String, u32>,
    /// 引擎自身拒绝过的 key（不计入失败次数，避免"自己把自己熔断"）
    pub denied_keys: HashSet<String>,
    /// 本轮是否见过真实用户消息
    pub real_user_seen: bool,
    /// 本轮用户文本
    pub user_text: String,
    /// 本轮是否含执行意图（简化：命中动作词）
    pub has_execute_clause: bool,
    /// 本会话已发过纠正的规则
    pub injected_rules: HashSet<String>,
    /// 任务契约（/guard mode|budget|contract 操作它）
    pub contract: TaskContract,
    /// 质量账本趋势窗口
    pub quality_window: usize,
}

/// 判断是否受保护配置路径
fn is_protected_path(cfg: &RuleEngineConfig, path: &str) -> bool {
    let p = path.replace('\\', "/").to_lowercase();
    cfg.protected_files
        .iter()
        .any(|f| p.ends_with(&f.to_lowercase()))
}

/// 从用户文本粗判执行意图（对齐上游 intent 分类的简化版：
/// 出现动作词且不是纯提问 ⇒ 视为有执行子句）
pub fn has_execute_clause(user_text: &str) -> bool {
    const ACTION_WORDS: [&str; 34] = [
        "写", "改", "删", "添加", "新增", "创建", "实现", "修复", "重构", "优化",
        "安装", "运行", "执行", "提交", "推送", "打包", "生成", "替换", "调整",
        "write", "edit", "delete", "remove", "create", "add", "fix", "refactor",
        "implement", "install", "run", "execute", "commit", "push", "build",
    ];
    let t = user_text.to_lowercase();
    let has_action = ACTION_WORDS.iter().any(|w| t.contains(w));
    // 纯提问：以问号结尾且无祈使语气
    let question_only = (t.trim_end().ends_with('?') || t.trim_end().ends_with('？'))
        && !t.contains("请")
        && !t.contains("帮我")
        && !t.contains("please");
    has_action && !question_only
}

/// 授权匹配：类型相等（或任一为 any）+ 路径前缀边界匹配
pub fn auth_matches(auth: &Authorization, op_type: &str, path: &str, now: i64) -> bool {
    if auth.expired(now) {
        return false;
    }
    if auth.r#type != "any" && op_type != "any" && auth.r#type != op_type {
        return false;
    }
    if auth.path_prefix.is_empty() {
        return true;
    }
    let a = auth.path_prefix.replace('\\', "/").to_lowercase();
    let p = path.replace('\\', "/").to_lowercase();
    p == a || p.starts_with(&format!("{}/", a.trim_end_matches('/')))
}

/// 从工具调用提取操作类型与路径
pub fn operation_of(tool: &str, args: &serde_json::Value) -> (String, String) {
    let path = args
        .get("file_path")
        .or_else(|| args.get("path"))
        .or_else(|| args.get("target"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let ty = match tool {
        "delete_file" => "delete",
        "write" | "edit" | "batch_write" | "batch_edit" | "str_replace_editor" => "write",
        "git_push" | "git_clone" => "git",
        "pwsh" | "bash" | "run_command" | "run_file" => "command",
        "skill" => "skill",
        _ => "any",
    };
    (ty.to_string(), path)
}

/// 硬门决策（对齐 guardDecision 的顺序）
pub fn guard_decision(
    cfg: &RuleEngineConfig,
    state: &mut RuleEngineState,
    tool: &str,
    args: &serde_json::Value,
    now: i64,
) -> GuardDecision {
    let allow = |rule: &str, kind: &str, reason: &str| GuardDecision {
        allow: true,
        rule_id: rule.to_string(),
        kind: kind.to_string(),
        reason: reason.to_string(),
        err_id: String::new(),
    };

    if !cfg.enabled {
        return allow("", "disabled", "规则引擎已关闭");
    }
    // 旁路窗口：全部放行（仍会记账为 bypass-action）
    if cfg.bypass_until > now {
        return allow("__escape-gate", "bypass-action", "处于 /guard bypass 窗口内");
    }
    if cfg.allowlist.iter().any(|a| a == tool) {
        return allow("__allowlist", "allow", "工具在免检名单中");
    }

    let class = classify_tool(tool);
    let (op_type, path) = operation_of(tool, args);

    // 只读命令与只读工具：无条件放行
    if class == ToolClass::Analysis {
        return allow("", "allow", "只读/分析类工具");
    }
    if matches!(tool, "pwsh" | "bash" | "run_command") {
        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if is_readonly_command(cmd) {
            return allow("", "allow", "只读命令");
        }
        // 判例指纹放行：某条命令被判"拦错了"后，同类命令直接放行
        if let Some(fp) = label_allows(cmd) {
            audit("label-hits", "__label", tool, &format!("指纹 {} 已放行（7 天判例）", fp), "");
            return allow("__label", "label-hits", "命中已登记的指纹放行（判例）");
        }
    }

    // 自保护：受保护配置路径的写入需要 /guard unlock
    if matches!(tool, "write" | "edit" | "str_replace_editor" | "batch_write" | "batch_edit")
        && is_protected_path(cfg, &path)
        && cfg.unlock_until <= now
    {
        return denial(
            "__self-protect",
            "deny",
            &format!("{} 是规则引擎的保护文件，需先执行 /guard unlock", path),
        );
    }

    // 内联命令禁令（node -e / pwsh -c 等）
    if matches!(tool, "pwsh" | "bash" | "run_command") {
        let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let inline = cmd.contains("node -e")
            || cmd.contains("node --eval")
            || cmd.contains("pwsh -c")
            || cmd.contains("powershell -c")
            || cmd.contains("python -c");
        if inline {
            return denial(
                "9",
                "deny",
                "检测到内联命令执行（node -e / -c 形式），请写成脚本文件后运行",
            );
        }
    }

    // 重试熔断：同一 (工具, 参数) 连续第 3 次失败
    let key = format!("{}:{}", tool, args);
    let count = *state.retry_counts.get(&key).unwrap_or(&0);
    if count >= 2 {
        state.denied_keys.insert(key.clone());
        return denial(
            "1",
            "deny",
            &format!(
                "{} 已连续失败 {} 次，请先分析失败原因、改变方案后再试（第 3 次相同调用被熔断）",
                tool,
                count + 1
            ),
        );
    }

    // 意图直判（规则 22）：本轮没有执行子句 ⇒ 拒绝一切变更类工具
    if state.real_user_seen
        && !state.has_execute_clause
        && op_type != "any"
        && class == ToolClass::Mutating
    {
        // 有匹配授权则放行
        if state
            .authorizations
            .iter()
            .any(|a| auth_matches(a, &op_type, &path, now))
        {
            return allow("22", "allow", "命中本轮授权范围");
        }
        return denial(
            "22",
            "deny",
            "本轮用户消息没有执行子句 —— 计划/确认不等于落盘授权，请先征得用户明确同意（得到「落盘/确认/保存」后即可执行）",
        );
    }

    // 敏感操作审批（规则 12A）：变更类工具需要授权
    if class == ToolClass::Mutating && op_type != "any" {
        if state
            .authorizations
            .iter()
            .any(|a| auth_matches(a, &op_type, &path, now))
        {
            return allow("12A", "allow", "命中授权范围");
        }
        return denial(
            "12A",
            "deny",
            &format!(
                "{} 属于 {} 操作，当前没有覆盖 {} 的有效授权。请先向用户征得同意（批准后我会记入授权范围）",
                tool, op_type, path
            ),
        );
    }

    allow("", "allow", "允许")
}

// ════════════════════════════════════════════════════════
// 审计账本
// ════════════════════════════════════════════════════════

pub fn dsh_home() -> PathBuf {
    if let Ok(h) = std::env::var("DSH_HOME") {
        if !h.trim().is_empty() {
            return PathBuf::from(h);
        }
    }
    if let Some(home) = dirs_next::home_dir() {
        return home.join(".dsh");
    }
    PathBuf::from(".")
}

pub fn audit_path() -> PathBuf {
    dsh_home().join("rule-engine.log.jsonl")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    pub ts: String,
    pub kind: String,
    pub rule: String,
    pub tool: String,
    pub reason: String,
    pub err_id: String,
}

static AUDIT_LOCK: Mutex<()> = Mutex::new(());

/// 追加审计（2MB 轮转，保留最后 400 行）
pub fn audit(kind: &str, rule: &str, tool: &str, reason: &str, err_id: &str) {
    let _g = AUDIT_LOCK.lock();
    let path = audit_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let rec = AuditRecord {
        ts: chrono::Utc::now().to_rfc3339(),
        kind: kind.to_string(),
        rule: rule.to_string(),
        tool: tool.to_string(),
        reason: reason.to_string(),
        err_id: err_id.to_string(),
    };
    let Ok(line) = serde_json::to_string(&rec) else { return };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", line);
    }
    // 轮转
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 2 * 1024 * 1024 {
            if let Ok(text) = std::fs::read_to_string(&path) {
                let lines: Vec<&str> = text.lines().collect();
                let keep = lines.len().saturating_sub(400);
                let tail: String = lines[keep..].join("\n") + "\n";
                let _ = std::fs::write(&path, tail);
            }
        }
    }
}

/// 读取最近 N 条审计
pub fn read_audit(n: usize) -> Vec<AuditRecord> {
    let Ok(text) = std::fs::read_to_string(audit_path()) else {
        return vec![];
    };
    let mut out: Vec<AuditRecord> = text
        .lines()
        .rev()
        .take(n)
        .filter_map(|l| serde_json::from_str::<AuditRecord>(l).ok())
        .collect();
    out.reverse();
    out
}

/// 规则参数是否受保护（供后端命令使用）
pub fn path_is_protected(cfg: &RuleEngineConfig, path: &str) -> bool {
    is_protected_path(cfg, path)
}

// ════════════════════════════════════════════════════════
// 全局配置（进程内，供 IPC 与 agent loop 共享）
// ════════════════════════════════════════════════════════

static ENGINE_CONFIG: std::sync::OnceLock<Mutex<RuleEngineConfig>> = std::sync::OnceLock::new();

fn config_lock() -> &'static Mutex<RuleEngineConfig> {
    ENGINE_CONFIG.get_or_init(|| Mutex::new(RuleEngineConfig::default()))
}

/// 读取当前配置（界面开关会即时影响 agent loop 的硬门行为）
pub fn get_config() -> RuleEngineConfig {
    config_lock().lock().map(|c| c.clone()).unwrap_or_default()
}

/// 应用界面上的开关
pub fn set_ui_toggles(turn_card_enabled: Option<bool>, task_contract_enabled: Option<bool>, enabled: Option<bool>) -> RuleEngineConfig {
    let mut cfg = get_config();
    if let Some(v) = turn_card_enabled { cfg.turn_card_enabled = v; }
    if let Some(v) = task_contract_enabled { cfg.task_contract_enabled = v; }
    if let Some(v) = enabled { cfg.enabled = v; }
    if let Ok(mut g) = config_lock().lock() {
        *g = cfg.clone();
    }
    audit(
        "mode-link",
        "__ui",
        "-",
        &format!(
            "规则引擎开关 → 引擎 {}｜裁决卡片 {}｜任务契约 {}",
            if cfg.enabled { "开" } else { "关" },
            if cfg.turn_card_enabled { "开" } else { "关" },
            if cfg.task_contract_enabled { "开" } else { "关" },
        ),
        "",
    );
    cfg
}

/// **执行许可档位 → 规则引擎开关**的唯一联动入口。
///
/// 规则引擎的开关**不交给用户自选**，而是由执行许可档位唯一决定：
///   - `需逐步确认`：规则引擎所有开关全开（硬门 + 裁决卡片 + 任务契约），
///     每一步工具调用都要查看；
///   - `仅确认风险操作`：规则引擎所有开关全开（硬门 + 裁决卡片 + 任务契约），
///     只有风险操作才会弹卡片；
///   - `全流程开放`：规则引擎所有开关**全关**，所有操作**永久放行**
///     （不产生硬门、不产生裁决卡片）。
pub fn link_ui_mode(mode: &crate::ai::approval::ApprovalMode) -> RuleEngineConfig {
    let on = mode.rule_engine_enabled();
    let cfg = set_ui_toggles(Some(mode.turn_card_enabled()), Some(on), Some(on));
    crate::ai::runtime_log::info(
        "rules",
        &format!("执行许可 → {}；规则引擎配置：{}", mode.label(), mode.describe()),
    );
    cfg
}

/// 用 /guard 命令执行期间可能改动的字段（unlock/bypass）回写全局配置
pub fn apply_runtime_windows(unlock_until: i64, bypass_until: i64) {
    if let Ok(mut g) = config_lock().lock() {
        g.unlock_until = unlock_until;
        g.bypass_until = bypass_until;
    }
}


/// 供 UI 展示的规则概览
pub fn rules_summary(rules: &[Rule]) -> serde_json::Value {
    let mut by_confidence: HashMap<&str, usize> = HashMap::new();
    let mut guard_rules = 0;
    for r in rules {
        *by_confidence.entry(r.confidence.as_str()).or_insert(0) += 1;
        if r.actions.iter().any(|a| a == "deny" || a == "ask" || a == "meta") {
            guard_rules += 1;
        }
    }
    serde_json::json!({
        "total": rules.len(),
        "guard_rules": guard_rules,
        "by_confidence": by_confidence,
        "rules": rules.iter().map(|r| serde_json::json!({
            "id": r.id, "title": r.title, "section": r.section,
            "level": r.level, "actions": r.actions, "confidence": r.confidence,
        })).collect::<Vec<_>>(),
    })
}

/// 从 DSH_HOME/AGENTS.md 载入规则
pub fn load_rules_from_home() -> Vec<Rule> {
    let p = dsh_home().join("AGENTS.md");
    load_rules_from_path(&p)
}

pub fn load_rules_from_path(p: &Path) -> Vec<Rule> {
    std::fs::read_to_string(p).map(|t| parse_rules(&t)).unwrap_or_default()
}

// ════════════════════════════════════════════════════════
// 回合末裁决卡片 + 判例登记（移植自 dsh-rule-engine-client v0.1.0）
//
// 契约（与上游一致）：
//   - 一张卡片按"回合"聚合本轮全部被拦记录；同一回合多条 → 卡内逐条分组
//   - 每条独立展开/收起 + **独立 ✅/❌**（不共用按钮）+ 独立一次性锁定
//   - 标题显示已判进度：`已判 2/3（✅1，❌1）`
//   - ❌（拦错了）会登记**命令指纹** → 同类命令下次直接放行（7 天 TTL）
//   - 危险命令**永不产生指纹**（因此不可能被"学习放行"）
//   - 判例一次性：已判即锁定；改判走 `/guard label <ERR-码> <correct|incorrect>`
//   - 落盘 `$DSH_HOME/rule-engine-turn-cards.json`，上限 200 条
//   - 默认**关闭**（由设置页开关控制）
// ════════════════════════════════════════════════════════

pub const TURN_CARDS_MAX: usize = 200;
pub const LABEL_TTL_SECONDS: i64 = 7 * 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCardBlock {
    pub i: usize,
    pub tool: String,
    /// 参数摘要（≤80 字符，对齐上游 args(≤80)）
    pub args: String,
    pub rule_id: String,
    pub title: String,
    pub reason: String,
    pub err_id: String,
    /// "" | "correct" | "incorrect" —— 非空即锁定（一次性）
    pub label: String,
    pub labeled_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCard {
    /// 卡片唯一键（前端据此取/评）
    pub key: String,
    pub session_id: String,
    /// 所属助手消息 id（前端在创建消息时回填）
    pub message_id: String,
    pub turn: u64,
    /// 用户原文（≤200 字符）
    pub user_text: String,
    pub blocks: Vec<TurnCardBlock>,
    /// "denied" | "clear"
    pub verdict: String,
    pub at: i64,
}

impl TurnCard {
    /// 进度文案（对齐上游：`已判 2/3（✅1，❌1）`）
    pub fn progress(&self) -> String {
        let labeled: Vec<&TurnCardBlock> = self
            .blocks
            .iter()
            .filter(|b| !b.label.is_empty())
            .collect();
        let ok = labeled.iter().filter(|b| b.label == "correct").count();
        let no = labeled.iter().filter(|b| b.label == "incorrect").count();
        let mut s = format!("已判 {}/{}", labeled.len(), self.blocks.len());
        if ok > 0 || no > 0 {
            s.push('（');
            if ok > 0 {
                s.push_str(&format!("✅{}", ok));
            }
            if ok > 0 && no > 0 {
                s.push_str(", ");
            }
            if no > 0 {
                s.push_str(&format!("❌{}", no));
            }
            s.push('）');
        }
        s
    }
    pub fn all_labeled(&self) -> bool {
        !self.blocks.is_empty() && self.blocks.iter().all(|b| !b.label.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct TurnCardStore {
    cards: Vec<TurnCard>,
}

pub fn turn_cards_path() -> PathBuf {
    rule_engine_dir().join("rule-engine-turn-cards.json")
}

/// 规则引擎的落盘根目录。
/// 可用 `DEEPAHEAD_RULE_ENGINE_DIR` 覆盖（测试与多profile隔离用），
/// 默认 `$DSH_HOME`（与上游一致，便于与真实 dsh-rule-engine 共享判例文件）。
pub fn rule_engine_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DEEPAHEAD_RULE_ENGINE_DIR") {
        if !d.trim().is_empty() {
            return PathBuf::from(d);
        }
    }
    dsh_home()
}

static CARD_LOCK: Mutex<()> = Mutex::new(());

fn load_cards() -> Vec<TurnCard> {
    let Ok(text) = std::fs::read_to_string(turn_cards_path()) else {
        return vec![];
    };
    serde_json::from_str::<TurnCardStore>(&text)
        .map(|s| s.cards)
        .unwrap_or_default()
}

fn save_cards(cards: &[TurnCard]) -> Result<(), String> {
    let p = turn_cards_path();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    // 保留上限：裁掉最老的
    let mut keep: Vec<TurnCard> = cards.to_vec();
    if keep.len() > TURN_CARDS_MAX {
        keep.sort_by_key(|c| c.at);
        let drop_n = keep.len() - TURN_CARDS_MAX;
        keep.drain(0..drop_n);
    }
    let json = serde_json::to_string_pretty(&TurnCardStore { cards: keep })
        .map_err(|e| format!("序列化裁决卡片失败: {}", e))?;
    std::fs::write(&p, json).map_err(|e| format!("写裁决卡片失败: {}", e))
}

/// 新建一张回合裁决卡片（无被拦记录时不应调用）
pub fn record_turn_card(
    session_id: &str,
    turn: u64,
    user_text: &str,
    blocks: Vec<TurnCardBlock>,
) -> Result<TurnCard, String> {
    if blocks.is_empty() {
        return Err("没有可裁决的拦截记录".into());
    }
    let _g = CARD_LOCK.lock().map_err(|_| "裁决卡片锁中毒".to_string())?;
    let mut cards = load_cards();
    let key = format!(
        "tc_{}_{}_{}",
        session_id.chars().take(16).collect::<String>(),
        turn,
        chrono::Utc::now().timestamp_millis()
    );
    let card = TurnCard {
        key: key.clone(),
        session_id: session_id.to_string(),
        message_id: String::new(),
        turn,
        user_text: user_text.chars().take(200).collect(),
        blocks,
        verdict: "denied".into(),
        at: chrono::Utc::now().timestamp(),
    };
    cards.push(card.clone());
    save_cards(&cards)?;
    Ok(card)
}

/// 把卡片挂到某条助手消息上（前端创建消息后回填）
pub fn attach_turn_card(key: &str, message_id: &str) -> Result<TurnCard, String> {
    let _g = CARD_LOCK.lock().map_err(|_| "裁决卡片锁中毒".to_string())?;
    let mut cards = load_cards();
    let Some(c) = cards.iter_mut().find(|c| c.key == key) else {
        return Err(format!("找不到裁决卡片 {}", key));
    };
    c.message_id = message_id.to_string();
    let out = c.clone();
    save_cards(&cards)?;
    Ok(out)
}

pub fn list_turn_cards(session_id: Option<&str>, limit: Option<usize>) -> Vec<TurnCard> {
    let mut cards = load_cards();
    if let Some(sid) = session_id {
        cards.retain(|c| c.session_id == sid);
    }
    cards.sort_by_key(|c| std::cmp::Reverse(c.at));
    cards.truncate(limit.unwrap_or(50));
    cards
}

pub fn get_turn_card(key: &str) -> Option<TurnCard> {
    load_cards().into_iter().find(|c| c.key == key)
}

/// 判例登记（一次性）。返回更新后的卡片。
/// expected_verdict：界面"期望的裁决"（默认 "clear"，即认为本不该拦）。
pub fn rate_turn_card(
    key: &str,
    block_index: usize,
    verdict: &str,
    expected_verdict: Option<&str>,
) -> Result<TurnCard, String> {
    let label = match verdict {
        "correct" | "incorrect" => verdict,
        other => return Err(format!("非法判定：{}（应为 correct / incorrect）", other)),
    };
    let _g = CARD_LOCK.lock().map_err(|_| "裁决卡片锁中毒".to_string())?;
    let mut cards = load_cards();
    let Some(card) = cards.iter_mut().find(|c| c.key == key) else {
        return Err(format!("找不到裁决卡片 {}", key));
    };
    let Some(block) = card.blocks.iter_mut().find(|b| b.i == block_index) else {
        return Err(format!("卡片 {} 没有第 {} 条", key, block_index));
    };
    // 一次性：已判即锁定（改判走 /guard label）
    if !block.label.is_empty() {
        return Err(format!(
            "第 {} 条已判定为 {}，判例一次性锁定；如需改判请使用 /guard label ERR-{} {}",
            block_index, block.label, block.err_id, label
        ));
    }
    block.label = label.to_string();
    block.labeled_at = chrono::Utc::now().timestamp();
    let snapshot = card.clone();
    save_cards(&cards)?;

    // ❌ 拦错了 → 登记命令指纹（同类命令下次直接放行）
    if label == "incorrect" {
        if let Some(block) = snapshot.blocks.iter().find(|b| b.i == block_index) {
            if matches!(block.tool.as_str(), "pwsh" | "bash" | "run_command") {
                if let Some(fp) = fingerprint_of(&block.args) {
                    let _ = upsert_label(&fp, "incorrect", expected_verdict);
                    audit(
                        "label-hits",
                        &block.rule_id,
                        &block.tool,
                        &format!("判例登记：拦错了 → 指纹 {} 学习放行（7 天）", fp),
                        &block.err_id,
                    );
                }
            }
        }
    } else {
        if let Some(block) = snapshot.blocks.iter().find(|b| b.i == block_index) {
            audit(
                "turn-card-verdict",
                &block.rule_id,
                &block.tool,
                &format!("判例登记：拦对了（{}）", block.reason),
                &block.err_id,
            );
        }
    }

    get_turn_card(key).ok_or_else(|| "卡片在登记后消失".to_string())
}

// ════════════════════════════════════════════════════════
// 命令指纹（判例学习放行）
// ════════════════════════════════════════════════════════

/// 会让指纹置空的危险命令标记——**危险形状永不被学习放行**
const DANGEROUS_MARKERS: [&str; 22] = [
    "rm -rf", "rm -r", "remove-item", "del ", "erase ", "rmdir", "rd /s",
    "drop table", "truncate table", "mkfs", "format ", "diskpart",
    "shutdown", "reboot", "> /dev/sd", "git push --force", "git push -f",
    "git reset --hard", "curl | sh", "curl|sh", "wget | sh", "invoke-expression",
];

/// 归一化命令 → 稳定指纹。
/// 会剥离：绝对路径、数字、引号内容中的具体值；危险命令返回 None。
pub fn fingerprint_of(command: &str) -> Option<String> {
    let lower = command.to_lowercase();
    if DANGEROUS_MARKERS.iter().any(|m| lower.contains(m)) {
        return None; // 危险命令不产生指纹
    }
    let mut normalized = String::new();
    for token in lower.split_whitespace() {
        // 数字 → <n>
        let mut t = String::new();
        let mut prev_digit = false;
        for ch in token.chars() {
            if ch.is_ascii_digit() {
                if !prev_digit {
                    t.push_str("<n>");
                }
                prev_digit = true;
            } else {
                t.push(ch);
                prev_digit = false;
            }
        }
        // 绝对路径 → <path>
        let t = if t.starts_with('/')
            || t.contains(":/")
            || t.contains(":\\")
            || t.starts_with("\\\\")
        {
            "<path>".to_string()
        } else {
            t
        };
        if !normalized.is_empty() {
            normalized.push(' ');
        }
        normalized.push_str(&t);
    }
    if normalized.trim().is_empty() {
        return None;
    }
    // FNV-1a 64 → 12 位十六进制（与上游"12 hex"口径一致）
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in normalized.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    Some(format!("{:012x}", h & 0xffff_ffff_ffff))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelRow {
    pub fingerprint: String,
    pub label: String,
    pub at: i64,
    pub expires_at: i64,
}

pub fn labels_path() -> PathBuf {
    rule_engine_dir().join("rule-engine-labels.json")
}

fn labels_path_in(base: &Path) -> PathBuf {
    base.join("rule-engine-labels.json")
}

static LABEL_LOCK: Mutex<()> = Mutex::new(());

fn load_labels_in(base: &Path) -> Vec<LabelRow> {
    let Ok(text) = std::fs::read_to_string(labels_path_in(base)) else {
        return vec![];
    };
    let mut rows: Vec<LabelRow> = serde_json::from_str(&text).unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    rows.retain(|r| r.expires_at == 0 || r.expires_at > now);
    rows
}

pub fn load_labels() -> Vec<LabelRow> {
    load_labels_in(&rule_engine_dir())
}

fn save_labels_in(base: &Path, rows: &[LabelRow]) -> Result<(), String> {
    let p = labels_path_in(base);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(&p, serde_json::to_string_pretty(rows).unwrap_or_else(|_| "[]".into()))
        .map_err(|e| format!("写判例失败: {}", e))
}

fn upsert_label_in(base: &Path, fingerprint: &str, label: &str) -> Result<LabelRow, String> {
    let _g = LABEL_LOCK.lock().map_err(|_| "判例锁中毒".to_string())?;
    let mut rows = load_labels_in(base);
    let now = chrono::Utc::now().timestamp();
    rows.retain(|r| r.fingerprint != fingerprint);
    let row = LabelRow {
        fingerprint: fingerprint.to_string(),
        label: label.to_string(),
        at: now,
        expires_at: now + LABEL_TTL_SECONDS,
    };
    rows.push(row.clone());
    save_labels_in(base, &rows)?;
    Ok(row)
}

fn clear_label_in(base: &Path, fingerprint: &str) -> Result<bool, String> {
    let _g = LABEL_LOCK.lock().map_err(|_| "判例锁中毒".to_string())?;
    let mut rows = load_labels_in(base);
    let before = rows.len();
    rows.retain(|r| r.fingerprint != fingerprint);
    let removed = rows.len() != before;
    save_labels_in(base, &rows)?;
    Ok(removed)
}

fn label_allows_in(base: &Path, command: &str) -> Option<String> {
    let fp = fingerprint_of(command)?;
    load_labels_in(base)
        .into_iter()
        .find(|r| r.fingerprint == fp && r.label == "incorrect")
        .map(|r| r.fingerprint)
}

/// 登记或刷新一条判例
pub fn upsert_label(fingerprint: &str, label: &str, _expected: Option<&str>) -> Result<LabelRow, String> {
    upsert_label_in(&rule_engine_dir(), fingerprint, label)
}

/// 撤销一条指纹放行
pub fn clear_label(fingerprint: &str) -> Result<bool, String> {
    clear_label_in(&rule_engine_dir(), fingerprint)
}

/// 该命令是否命中"已判拦错"的指纹（命中则直接放行）
pub fn label_allows(command: &str) -> Option<String> {
    label_allows_in(&rule_engine_dir(), command)
}

// ════════════════════════════════════════════════════════
// /guard 命令面（移植自上游 18 个子命令的可移植子集）
// ════════════════════════════════════════════════════════

pub const GUARD_USAGE: &str = "\
/guard 子命令：
  status            开关与当前状态摘要
  rules             已解析规则清单（等级/动作/置信/handler）
  active            最近活跃规则
  log [N]           最近 N 条审计（默认 20）
  cards [N]         最近 N 张回合裁决卡片
  labels            已登记的指纹放行（7 天）
  unlock [N]        解锁保护文件 N 分钟（默认 10）
  bypass [N]        旁路全部硬门 N 分钟（默认 5）
  lock              立即取消 unlock/bypass
  revoke            清空全部判例与指纹放行
  reload            重新解析 AGENTS.md
  label <ERR-码|指纹> <correct|incorrect>
  label clear <指纹> 撤销指纹放行
  mode <observe|review|answer|change|monitor|watch|off>   任务契约模式
  budget [N]        查看/设置委派预算
  contract [arm|disarm|hash|deps|path|categories|clear]   任务契约细项
  quality           质量账本趋势
  approve <类型> <路径> [分钟]   签发一条授权（不允许 any）
  hotword [词]      查看/学习热词
  freedom           盲区自述（当前未实现的能力）
";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardCommandResult {
    pub ok: bool,
    pub text: String,
}

/// 执行一条 /guard 命令
pub fn run_guard_command(cfg: &mut RuleEngineConfig, state: &mut RuleEngineState, input: &str) -> GuardCommandResult {
    let raw = input.trim();
    let body = raw.strip_prefix("/guard").unwrap_or(raw).trim();
    if body.is_empty() {
        return GuardCommandResult { ok: true, text: GUARD_USAGE.into() };
    }
    let parts: Vec<&str> = body.split_whitespace().collect();
    let sub = parts.first().copied().unwrap_or("status");
    let now = chrono::Utc::now().timestamp();
    let ok = |t: String| GuardCommandResult { ok: true, text: t };
    let err = |t: String| GuardCommandResult { ok: false, text: t };

    match sub {
        "status" | "state" => {
            let mut s = String::new();
            s.push_str(&format!("规则引擎：{}\n", if cfg.enabled { "已启用" } else { "已关闭" }));
            s.push_str(&format!("已解析规则：{} 条（进入硬门 {} 条）\n", state.rules.len(),
                state.rules.iter().filter(|r| r.actions.iter().any(|a| a == "deny" || a == "ask" || a == "meta")).count()));
            s.push_str(&format!("未知工具策略：{}\n", cfg.unknown_policy));
            s.push_str(&format!("保护文件：{} 个\n", cfg.protected_files.len()));
            if cfg.unlock_until > now {
                s.push_str(&format!("解锁窗口剩余 {} 秒\n", cfg.unlock_until - now));
            }
            if cfg.bypass_until > now {
                s.push_str(&format!("旁路窗口剩余 {} 秒\n", cfg.bypass_until - now));
            }
            s.push_str(&format!("有效授权：{} 条\n", state.authorizations.iter().filter(|a| !a.expired(now)).count()));
            s.push_str(&format!("指纹放行：{} 条\n", load_labels().len()));
            s.push_str(&format!("裁决卡片：{} 张\n", load_cards().len()));
            s.push_str(&format!("审计账本：{}", audit_path().to_string_lossy()));
            ok(s)
        }
        "rules" | "list" | "ls" => {
            if state.rules.is_empty() {
                return ok("（未解析到规则；请在 $DSH_HOME/AGENTS.md 中编写）".into());
            }
            let mut s = String::new();
            for r in &state.rules {
                s.push_str(&format!(
                    "[{}] {} ｜ 等级 {} ｜ 动作 {} ｜ 置信 {} ｜ handler {}\n",
                    r.id, r.title,
                    if r.level.is_empty() { "—" } else { &r.level },
                    r.actions.join("/"),
                    r.confidence,
                    if r.handler.is_empty() { "—" } else { &r.handler }
                ));
            }
            ok(s)
        }
        "active" => {
            let recs = read_audit(200);
            let mut seen: Vec<String> = Vec::new();
            for r in recs.iter().rev() {
                if r.kind == "deny" && !r.rule.is_empty() && !seen.contains(&r.rule) {
                    seen.push(r.rule.clone());
                }
                if seen.len() >= 10 {
                    break;
                }
            }
            if seen.is_empty() {
                ok("最近没有命中规则的拦截。".into())
            } else {
                ok(format!("最近活跃规则：{}", seen.join(", ")))
            }
        }
        "log" => {
            let n: usize = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(20);
            let recs = read_audit(n);
            if recs.is_empty() {
                return ok("暂无审计记录。".into());
            }
            let mut s = String::new();
            for r in recs.iter().rev() {
                s.push_str(&format!(
                    "[{}] {} ｜ 规则 {} ｜ {}（ERR-{}）\n    {}\n",
                    r.ts, r.kind, if r.rule.is_empty() { "—" } else { &r.rule },
                    r.tool, r.err_id, r.reason
                ));
            }
            ok(s)
        }
        "cards" => {
            let n: usize = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(10);
            let cards = list_turn_cards(None, Some(n));
            if cards.is_empty() {
                return ok("暂无回合裁决卡片。".into());
            }
            let mut s = String::new();
            for c in cards {
                s.push_str(&format!("{} ｜ {} ｜ {} 条 ｜ {}\n", c.key, c.verdict, c.blocks.len(), c.progress()));
            }
            ok(s)
        }
        "labels" => {
            let rows = load_labels();
            if rows.is_empty() {
                return ok("暂无指纹放行记录。".into());
            }
            let mut s = String::new();
            for r in rows {
                let left = if r.expires_at == 0 { 0 } else { (r.expires_at - now).max(0) };
                s.push_str(&format!("{} ｜ {} ｜ 剩余 {} 小时\n", r.fingerprint, r.label, left / 3600));
            }
            ok(s)
        }
        "unlock" => {
            let n: i64 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(10);
            let n = n.clamp(1, 60);
            cfg.unlock_until = now + n * 60;
            audit("guard-command", "__escape-gate", "unlock", &format!("解锁 {} 分钟", n), "");
            ok(format!("已解锁保护文件 {} 分钟（自保护豁免）", n))
        }
        "bypass" => {
            let n: i64 = parts.get(1).and_then(|v| v.parse().ok()).unwrap_or(5);
            let n = n.clamp(1, 60);
            cfg.bypass_until = now + n * 60;
            audit("guard-command", "__escape-gate", "bypass", &format!("旁路 {} 分钟", n), "");
            ok(format!("已旁路全部硬门 {} 分钟（期间所有变更类调用仍会记账）", n))
        }
        "lock" => {
            cfg.unlock_until = 0;
            cfg.bypass_until = 0;
            audit("guard-command", "__escape-gate", "lock", "取消 unlock/bypass", "");
            ok("已取消 unlock 与 bypass 窗口。".into())
        }
        "revoke" => {
            let n = state.authorizations.len();
            state.authorizations.clear();
            let base = rule_engine_dir();
            let m = load_labels_in(&base).len();
            let _ = save_labels_in(&base, &[]);
            audit("guard-command", "__revoke", "revoke", &format!("清空 {} 条授权 / {} 条判例", n, m), "");
            ok(format!("已清空 {} 条授权与全部指纹放行。", n))
        }
        "reload" => {
            state.rules = load_rules_from_home();
            audit("guard-command", "", "reload", &format!("重新解析 {} 条规则", state.rules.len()), "");
            ok(format!("已重新解析 AGENTS.md：{} 条规则。", state.rules.len()))
        }
        "label" => {
            let a = parts.get(1).copied().unwrap_or("");
            let b = parts.get(2).copied().unwrap_or("");
            if a == "clear" {
                let fp = b;
                if fp.is_empty() {
                    return err("用法：/guard label clear <指纹>".into());
                }
                match clear_label(fp) {
                    Ok(true) => ok(format!("已撤销指纹放行：{}", fp)),
                    Ok(false) => ok(format!("未找到指纹：{}", fp)),
                    Err(e) => err(e),
                }
            } else {
                if a.is_empty() || b.is_empty() {
                    return err("用法：/guard label <ERR-码|指纹> <correct|incorrect>".into());
                }
                if b != "correct" && b != "incorrect" {
                    return err("判定只能是 correct 或 incorrect".into());
                }
                // 按 ERR 码找卡片；找不到则按指纹处理
                if let Some(card) = load_cards().into_iter().find(|c| {
                    c.blocks.iter().any(|bl| bl.err_id == a || format!("ERR-{}", bl.err_id) == a)
                }) {
                    let idx = card
                        .blocks
                        .iter()
                        .find(|bl| bl.err_id == a || format!("ERR-{}", bl.err_id) == a)
                        .map(|bl| bl.i)
                        .unwrap_or(0);
                    // 命令行改判：允许覆盖（与卡片的一次性锁定不同）
                    let _g = CARD_LOCK.lock().ok();
                    let mut cards = load_cards();
                    if let Some(c) = cards.iter_mut().find(|c| c.key == card.key) {
                        if let Some(bl) = c.blocks.iter_mut().find(|bl| bl.i == idx) {
                            bl.label = b.to_string();
                            bl.labeled_at = now;
                        }
                    }
                    let _ = save_cards(&cards);
                    if b == "incorrect" {
                        if let Some(bl) = card.blocks.iter().find(|bl| bl.i == idx) {
                            if let Some(fp) = fingerprint_of(&bl.args) {
                                let _ = upsert_label(&fp, "incorrect", None);
                            }
                        }
                    }
                    return ok(format!("已改判 {} 第 {} 条为 {}", card.key, idx, b));
                }
                match upsert_label(a, b, None) {
                    Ok(r) => ok(format!("已登记判例：{} → {}（7 天）", r.fingerprint, r.label)),
                    Err(e) => err(e),
                }
            }
        }
        "freedom" | "blindspots" | "free" => {
            let s = "\
已实现（机制层）：
  ✓ AGENTS.md 格式契约解析（free-zone 跳过、等级、四要素）
  ✓ 等级 → 动作与置信度（低置信永不硬拦）
  ✓ 工具分类表（analysis / artifact / mutating / unknown + 前缀规则）
  ✓ 只读命令判定（引用感知分段，2>&1 不误判）
  ✓ 硬门：旁路 / 自保护 / 内联命令禁令 / 重试熔断 / 意图直判 / 授权边界
  ✓ 回合裁决卡片 + 判例一次性登记 + 指纹学习放行（危险命令永不指纹化）
  ✓ 文本审计与纠正注入（12 项检测 + 交付门：每规则一次、每小时 ≤3）
  ✓ 任务契约（modes / levels / hash 与依赖策略 / 路径与类别约束 / 委派预算）
  ✓ 质量账本（任务签名归一化 + 趋势）
  ✓ 热词学习（长度受限、上限 500、原子写）
  ✓ 审计账本 JSONL（2MB 轮转）

未实现（诚实清单）：
  ✗ LLM 意图兜底（裁决器需要 LLM 路由；DeepAhead 未接入异步裁决）
  ✗ 技能授权实时对账（DeepAhead 无技能目录注册表）
  ✗ 挂载/装配完整性审计（依赖宿主插件加载器）
  ✗ 版本守卫回滚（依赖工具调用/结果的成对钩子）
";
            ok(s.into())
        }
        // ─── 任务契约 ───
        "mode" => {
            let m = parts.get(1).copied().unwrap_or("");
            const MODES: [&str; 7] = ["observe", "review", "answer", "change", "monitor", "watch", "off"];
            if !MODES.contains(&m) {
                return err(format!("用法：/guard mode <{}>", MODES.join("|")));
            }
            state.contract.mode = m.to_string();
            match m {
                "off" => state.contract.armed = false,
                "change" => {
                    state.contract.armed = true;
                    state.contract.level = "guard".into();
                }
                _ => {}
            }
            audit("guard-command", "__contract", "mode", m, "");
            ok(format!(
                "任务契约模式 → {}（armed={}，等级 {}）",
                m, state.contract.armed, state.contract.level
            ))
        }
        "budget" => {
            let n = parts.get(1).copied().unwrap_or("");
            if n.is_empty() {
                return ok(format!(
                    "委派预算：{}/{}（0 表示不限制；armed 且为 0 时按 2 处理）",
                    state.contract.agent_spent, state.contract.agent_budget
                ));
            }
            let v: u32 = match n.parse() {
                Ok(v) => v,
                Err(_) => return err("用法：/guard budget <次数>".into()),
            };
            state.contract.agent_budget = v;
            state.contract.agent_spent = 0;
            audit("guard-command", "__contract", "budget", &format!("设为 {}", v), "");
            ok(format!("委派预算设为 {}（已重置用量）", v))
        }
        "contract" => {
            let sub2 = parts.get(1).copied().unwrap_or("");
            match sub2 {
                "" => {
                    let c = &state.contract;
                    ok(format!(
                        "任务契约：\n  模式 {} ｜ 等级 {} ｜ armed {}\n  委派预算 {}/{}\n  哈希策略 {} ｜ 依赖策略 {}\n  允许路径 {:?}\n  允许类别 {:?}\n（破坏性类别 {} 结构性不可授权）",
                        c.mode, c.level, c.armed, c.agent_spent, c.agent_budget,
                        c.hash_policy, c.dependency_policy, c.allowed_paths, c.categories,
                        DESTRUCTIVE_CATEGORIES.join("/")
                    ))
                }
                "arm" => {
                    state.contract.armed = true;
                    if state.contract.level == "off" {
                        state.contract.level = "guard".into();
                    }
                    audit("guard-command", "__contract", "arm", "", "");
                    ok("任务契约已启用（armed）".into())
                }
                "disarm" => {
                    state.contract.armed = false;
                    audit("guard-command", "__contract", "disarm", "", "");
                    ok("任务契约已停用".into())
                }
                "hash" | "deps" => {
                    let v = parts.get(2).copied().unwrap_or("");
                    if !["deny", "ask", "allow"].contains(&v) {
                        return err(format!("用法：/guard contract {} <deny|ask|allow>", sub2));
                    }
                    if sub2 == "hash" {
                        state.contract.hash_policy = v.to_string();
                    } else {
                        state.contract.dependency_policy = v.to_string();
                    }
                    audit("guard-command", "__contract", sub2, v, "");
                    ok(format!("{} 策略 → {}", if sub2 == "hash" { "哈希" } else { "依赖" }, v))
                }
                "path" => {
                    let v = parts.get(2).copied().unwrap_or("");
                    if v.is_empty() {
                        return err("用法：/guard contract path <路径前缀>".into());
                    }
                    state.contract.allowed_paths.push(v.to_string());
                    audit("guard-command", "__contract", "path", v, "");
                    ok(format!("已追加允许路径：{}", v))
                }
                "categories" => {
                    let v = parts.get(2).copied().unwrap_or("");
                    if v.is_empty() {
                        return err("用法：/guard contract categories <a,b,c>".into());
                    }
                    state.contract.categories = v
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    audit("guard-command", "__contract", "categories", v, "");
                    ok(format!(
                        "允许类别 → {:?}（破坏性类别 {} 会被契约拒绝）",
                        state.contract.categories,
                        DESTRUCTIVE_CATEGORIES.join("/")
                    ))
                }
                "clear" => {
                    state.contract = TaskContract::default();
                    audit("guard-command", "__contract", "clear", "", "");
                    ok("任务契约已重置为默认（未启用）".into())
                }
                other => err(format!(
                    "未知：/guard contract {}\n用法：arm|disarm|hash|deps|path|categories|clear",
                    other
                )),
            }
        }
        // ─── 质量账本 ───
        "quality" => {
            let rows = load_quality_ledger();
            if rows.is_empty() {
                return ok(format!(
                    "质量账本为空（{}）。",
                    quality_ledger_path().to_string_lossy()
                ));
            }
            let mut s = format!("质量账本：{} 条记录\n任务签名趋势（前 → 后，越低越好）：\n", rows.len());
            for (sig, prev, recent, n) in quality_trend(state.quality_window) {
                let arrow = if recent < prev { "↓" } else if recent > prev { "↑" } else { "→" };
                s.push_str(&format!("  {} {} {:.2} → {:.2}（{} 次）\n", sig, arrow, prev, recent, n));
            }
            ok(s)
        }
        // ─── 授权（approve）───
        "approve" => {
            let ty = parts.get(1).copied().unwrap_or("");
            let path = parts.get(2).copied().unwrap_or("");
            if ty.is_empty() || ty == "any" {
                return err(
                    "用法：/guard approve <delete|write|backup|git|command|skill> <路径> [分钟]（不允许 any 通配）"
                        .into(),
                );
            }
            let mins: i64 = parts.get(3).and_then(|v| v.parse().ok()).unwrap_or(10);
            let mins = mins.clamp(1, 720);
            state.authorizations.push(Authorization {
                at: now,
                expires_at: now + mins * 60,
                r#type: ty.to_string(),
                path_prefix: path.to_string(),
                source: "physical-confirm".into(),
            });
            audit("guard-command", "12A", "approve", &format!("{} {} {} 分钟", ty, path, mins), "");
            ok(format!("已授权：{} 操作，路径 {}，有效期 {} 分钟", ty, path, mins))
        }
        // ─── 热词 ───
        "hotword" => {
            let w = parts.get(1).copied().unwrap_or("");
            if w.is_empty() {
                let hw = load_hotwords();
                return ok(format!(
                    "已学习热词 {} 个（上限 {}，长度 {}-{}）：\n{}",
                    hw.words.len(),
                    HOTWORD_CAP,
                    HOTWORD_MIN_LEN,
                    HOTWORD_MAX_LEN,
                    if hw.words.is_empty() { "（空）".to_string() } else { hw.words.join(", ") }
                ));
            }
            match learn_hotword(w) {
                Ok(hw) => ok(format!("已学习热词「{}」，当前共 {} 个", w, hw.words.len())),
                Err(e) => err(e),
            }
        }
        "help" | "?" | "" => ok(GUARD_USAGE.into()),
        other => err(format!("未知子命令：{}\n\n{}", other, GUARD_USAGE)),
    }
}

/// 从卡片的被拦记录构造一条 block
pub fn make_card_block(
    index: usize,
    tool: &str,
    args: &serde_json::Value,
    decision: &GuardDecision,
    title: &str,
) -> TurnCardBlock {
    let args_text = args
        .get("command")
        .or_else(|| args.get("file_path"))
        .or_else(|| args.get("path"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| args.to_string());
    TurnCardBlock {
        i: index,
        tool: tool.to_string(),
        args: args_text.chars().take(80).collect(),
        rule_id: decision.rule_id.clone(),
        title: title.to_string(),
        reason: decision.reason.clone(),
        err_id: decision.err_id.clone(),
        label: String::new(),
        labeled_at: 0,
    }
}

// ════════════════════════════════════════════════════════
// ① 文本审计 + 纠正注入（移植自 dsh-rule-engine text-detect / semantic）
//
// 上游的边界必须保留：**助手输出无法被阻断**，文本审计只做"审计 + 注入纠正"。
// 交付门（delivery gate）：每个 (会话, 规则) 只注入一次；每会话每小时 ≤3 次；
// 文本若长得像命令则拒绝注入。
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextHit {
    pub rule_id: String,
    pub title: String,
    /// 命中的片段（≤120 字符）
    pub evidence: String,
    /// correct = 词典直判，立即投递；self_certify = 疑似，需 LLM 裁决
    pub kind: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TextAuditState {
    /// 已注入过的规则（(会话, 规则) 只注入一次）
    pub injected_rules: HashSet<String>,
    /// 注入时间戳，用于"每会话每小时 ≤3 次"预算
    pub inject_at: Vec<i64>,
    /// 本会话建议类表达次数（规则 16）
    pub suggestion_count: u32,
    /// 本轮是否调用过 get-date 类工具（规则 2）
    pub get_date_seen: bool,
    /// 本轮只读工具调用次数（规则 31）
    pub readonly_calls: u32,
    /// 本轮是否出现过验证动作（规则 23）
    pub verify_seen: bool,
}

pub const MAX_INJECT_PER_HOUR: usize = 3;

const PROMISE_WORDS: [&str; 16] = [
    "一定", "保证", "绝对", "肯定没问题", "万无一失", "包你", "必然",
    "definitely", "guaranteed", "i promise", "for sure", "certainly will",
    "100%", "完美解决", "彻底解决", "绝不",
];
const EMPTY_TALK: [&str; 12] = [
    "got it", "好的", "收到", "明白了", "了解了", "没问题",
    "understood", "noted", "sure thing", "will do", "okay", "ok",
];
const DELIVERY_CLAIM: [&str; 18] = [
    "已完成", "已修复", "已实现", "已添加", "已更新", "已发布", "搞定", "做好了",
    "解决了", "修好了", "改好了", "处理完了",
    "done", "completed", "fixed", "implemented", "finished", "shipped",
];
const VERIFY_EVIDENCE: [&str; 14] = [
    "测试通过", "已验证", "跑通", "编译通过", "构建成功", "cargo test", "npm test",
    "pnpm test", "通过测试", "verified", "tests pass", "build succeeded", "assert", "check passed",
];
const SOURCE_MARK: [&str; 10] = [
    "来源", "参考", "见 ", "依据", "文档", "source", "参考链接", "http", "https", "根据",
];
const TIME_WORDS: [&str; 10] = [
    "今天", "现在", "目前", "当前", "刚刚", "本日", "today", "now", "currently", "right now",
];
const APOLOGY_ONLY: [&str; 8] = [
    "抱歉", "对不起", "不好意思", "很遗憾", "sorry", "apologies", "my bad", "i apologize",
];
const APOLOGY_WITH_CAUSE: [&str; 8] = [
    "原因", "因为", "由于", "根因", "修正", "改进", "避免", "because",
];

/// 审计助手文本，返回命中项（对齐上游 detectViolations 的主要检测器）
pub fn text_audit(assistant_text: &str, user_text: &str, st: &mut TextAuditState) -> Vec<TextHit> {
    let text = assistant_text;
    let lower = text.to_lowercase();
    let mut hits: Vec<TextHit> = Vec::new();
    let ev = |needle: &str| -> String {
        text.lines()
            .find(|l| l.to_lowercase().contains(&needle.to_lowercase()))
            .unwrap_or("")
            .trim()
            .chars()
            .take(120)
            .collect()
    };

    // 规则 7：承诺/大话
    for w in PROMISE_WORDS {
        if lower.contains(&w.to_lowercase()) {
            hits.push(TextHit {
                rule_id: "7".into(),
                title: "承诺性表达（无证据的大话）".into(),
                evidence: ev(w),
                kind: "correct".into(),
            });
            break;
        }
    }

    // 规则 23：交付声明缺验证证据
    let has_claim = DELIVERY_CLAIM.iter().any(|w| lower.contains(&w.to_lowercase()));
    let has_verify = VERIFY_EVIDENCE.iter().any(|w| lower.contains(&w.to_lowercase())) || st.verify_seen;
    if has_claim && !has_verify {
        hits.push(TextHit {
            rule_id: "23".into(),
            title: "声称已完成但没有验证证据".into(),
            evidence: DELIVERY_CLAIM
                .iter()
                .find(|w| lower.contains(&w.to_lowercase()))
                .map(|w| ev(w))
                .unwrap_or_default(),
            kind: "correct".into(),
        });
    }

    // 规则 2：时间表达但未取时间
    if !st.get_date_seen {
        for w in TIME_WORDS {
            if lower.contains(&w.to_lowercase()) {
                hits.push(TextHit {
                    rule_id: "2".into(),
                    title: "使用当前时间词但没有取时间证据".into(),
                    evidence: ev(w),
                    kind: "self_certify".into(),
                });
                break;
            }
        }
    }

    // 规则 5：URL/内部引用缺来源标注
    let has_url = text.contains("http://") || text.contains("https://");
    let has_source = SOURCE_MARK.iter().any(|w| lower.contains(&w.to_lowercase()));
    if has_url && !has_source {
        hits.push(TextHit {
            rule_id: "5".into(),
            title: "引用链接但没有标注来源".into(),
            evidence: "（含 URL，未见来源标注）".into(),
            kind: "self_certify".into(),
        });
    }

    // 规则 14：散文里用路径缩写
    for pat in ["~/.dsh", "%USERPROFILE%", "reports\\", "…\\"] {
        if text.contains(pat) {
            hits.push(TextHit {
                rule_id: "14".into(),
                title: "正文中使用路径缩写（应给完整路径）".into(),
                evidence: ev(pat),
                kind: "self_certify".into(),
            });
            break;
        }
    }

    // 规则 22：空洞回应
    let trimmed = lower.trim().trim_end_matches(['。', '.', '!', '！', '~']);
    if EMPTY_TALK.iter().any(|w| trimmed == w.to_lowercase()) {
        hits.push(TextHit {
            rule_id: "22".into(),
            title: "空洞回应（没有实质内容）".into(),
            evidence: ev(&trimmed.chars().take(20).collect::<String>()),
            kind: "correct".into(),
        });
    }

    // 规则 22：只道歉不给原因/改进
    let has_apology = APOLOGY_ONLY.iter().any(|w| lower.contains(&w.to_lowercase()));
    let has_cause = APOLOGY_WITH_CAUSE.iter().any(|w| lower.contains(&w.to_lowercase()));
    if has_apology && !has_cause {
        hits.push(TextHit {
            rule_id: "22".into(),
            title: "只道歉未说明原因与改进".into(),
            evidence: APOLOGY_ONLY
                .iter()
                .find(|w| lower.contains(&w.to_lowercase()))
                .map(|w| ev(w))
                .unwrap_or_default(),
            kind: "correct".into(),
        });
    }

    // 规则 16：本会话第 3 次建议类表达
    if lower.contains("建议") || lower.contains("suggest") || lower.contains("recommend") {
        st.suggestion_count += 1;
        if st.suggestion_count >= 3 {
            hits.push(TextHit {
                rule_id: "16".into(),
                title: "同一会话第 3 次给建议（应直接执行）".into(),
                evidence: "（建议类表达第 3 次）".into(),
                kind: "self_certify".into(),
            });
        }
    }

    // 规则 31：同一轮只读工具 ≥3 次且无验证意图
    if st.readonly_calls >= 3 && !st.verify_seen {
        hits.push(TextHit {
            rule_id: "31".into(),
            title: "重复只读调用 ≥3 次且没有验证意图".into(),
            evidence: format!("（本轮只读调用 {} 次）", st.readonly_calls),
            kind: "self_certify".into(),
        });
    }

    // 规则 11：用户中文、回复全英文
    let user_is_cjk = user_text.chars().any(is_cjk_char);
    let reply_cjk = text.chars().filter(|c| is_cjk_char(*c)).count();
    let reply_total = text.chars().filter(|c| !c.is_whitespace()).count().max(1);
    if user_is_cjk && reply_cjk * 20 < reply_total && reply_total > 40 {
        hits.push(TextHit {
            rule_id: "11".into(),
            title: "用户使用中文但回复以英文为主".into(),
            evidence: "（回复中文字符占比 < 5%）".into(),
            kind: "self_certify".into(),
        });
    }

    hits
}

fn is_cjk_char(c: char) -> bool {
    matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF)
}

/// 投递门：是否允许把纠正文本注入到会话里。
/// 三条闸门与上游一致：真实用户轮、每 (会话,规则) 一次、每小时 ≤3 次、
/// 且注入文本不得"看起来像命令"。
pub fn should_deliver_injection(
    st: &mut TextAuditState,
    hits: &[TextHit],
    now: i64,
) -> (bool, Vec<TextHit>) {
    if hits.is_empty() {
        return (false, vec![]);
    }
    // 清理 1 小时前的记录
    st.inject_at.retain(|t| now - *t < 3600);
    if st.inject_at.len() >= MAX_INJECT_PER_HOUR {
        return (false, vec![]);
    }
    let fresh: Vec<TextHit> = hits
        .iter()
        .filter(|h| !st.injected_rules.contains(&h.rule_id))
        .cloned()
        .collect();
    if fresh.is_empty() {
        return (false, vec![]);
    }
    // 注入文本不得像命令
    let looks_like_command = fresh
        .iter()
        .any(|h| h.title.contains("rm ") || h.title.contains("pwsh") || h.title.contains("node -e"));
    if looks_like_command {
        return (false, vec![]);
    }
    for h in &fresh {
        st.injected_rules.insert(h.rule_id.clone());
    }
    st.inject_at.push(now);
    (true, fresh)
}

/// 渲染纠正注入文本（对齐上游 `[规则引擎] …（规则 N，…）` 形状）
pub fn render_injection(hits: &[TextHit]) -> String {
    if hits.is_empty() {
        return String::new();
    }
    if hits.len() == 1 {
        let h = &hits[0];
        return format!(
            "[规则引擎] {}（规则 {}，已记入 /guard log；下次回复请自证/纠正）",
            h.title, h.rule_id
        );
    }
    let list = hits
        .iter()
        .map(|h| format!("规则 {}：{}", h.rule_id, h.title))
        .collect::<Vec<_>>()
        .join("；");
    format!(
        "[规则引擎] 本轮检出 {} 项：{}（已记入 /guard log；下次回复请自证/纠正）",
        hits.len(),
        list
    )
}

// ════════════════════════════════════════════════════════
// ② 任务契约 + 反过度设计（移植自 dsh-rule-engine contract.js）
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContract {
    /// review | answer | change | monitor | watch | off
    pub mode: String,
    /// watch | guard | lock | off
    pub level: String,
    /// 委派预算（0 = 不限制；armed 且为 0 时按 2 处理）
    pub agent_budget: u32,
    pub agent_spent: u32,
    /// deny | ask | allow
    pub hash_policy: String,
    /// deny | ask | allow
    pub dependency_policy: String,
    /// 允许写入的路径前缀（空 = 不限制）
    pub allowed_paths: Vec<String>,
    /// 允许的命令类别（空 = 门未启用）
    pub categories: Vec<String>,
    pub armed: bool,
}

impl Default for TaskContract {
    fn default() -> Self {
        Self {
            mode: "observe".into(),
            level: "off".into(),
            agent_budget: 0,
            agent_spent: 0,
            hash_policy: "deny".into(),
            dependency_policy: "ask".into(),
            allowed_paths: vec![],
            categories: vec![],
            armed: false,
        }
    }
}

/// 结构性不可加入 allow 列表的破坏性类别
pub const DESTRUCTIVE_CATEGORIES: [&str; 4] = ["delete", "move", "replace", "purge"];
/// 非破坏性类别
pub const NON_DESTRUCTIVE_CATEGORIES: [&str; 8] = [
    "install", "build", "test", "audit", "analyze", "naming", "sync", "restore",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDecision {
    pub allow: bool,
    pub reason_code: String,
    pub message: String,
}

/// 从命令文本推断动作类别
pub fn classify_action(tool: &str, command: &str) -> String {
    let c = command.to_lowercase();
    if c.contains("rm ") || c.contains("remove-item") || c.contains("del ") || c.contains("rmdir") {
        return "delete".into();
    }
    if c.contains("move-item") || c.contains("mv ") {
        return "move".into();
    }
    if c.contains("npm install") || c.contains("pnpm add") || c.contains("pip install") || c.contains("cargo add") {
        return "install".into();
    }
    if c.contains("build") || c.contains("cargo build") || c.contains("pnpm build") {
        return "build".into();
    }
    if c.contains("test") {
        return "test".into();
    }
    match tool {
        "write" | "edit" | "batch_write" | "batch_edit" | "str_replace_editor" => "write".into(),
        "subagent" | "subagent_fork" | "workflow" => "delegate".into(),
        _ => "unknown".into(),
    }
}

/// 任务契约决策（对齐上游 decideContractAction 的 reason code 体系）
pub fn decide_contract_action(
    contract: &mut TaskContract,
    tool: &str,
    args: &serde_json::Value,
) -> ContractDecision {
    let allow = |code: &str, msg: &str| ContractDecision {
        allow: true,
        reason_code: code.into(),
        message: msg.into(),
    };
    let deny = |code: &str, msg: &str| ContractDecision {
        allow: false,
        reason_code: code.into(),
        message: msg.into(),
    };

    if !contract.armed || contract.level == "off" {
        return allow("CONTROL_INACTIVE", "任务契约未启用");
    }
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
    let path = args
        .get("file_path")
        .or_else(|| args.get("path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let action = classify_action(tool, command);

    // 委派预算必须**先于**"非变更类动作"的放行判定：
    // 否则 subagent / workflow 这类不是 write 的动作会绕过预算。
    if action == "delegate" {
        let budget = if contract.agent_budget == 0 { 2 } else { contract.agent_budget };
        if contract.agent_spent >= budget {
            return deny(
                "AGENT_BUDGET_EXHAUSTED",
                &format!("委派预算已用尽（{}/{}）", contract.agent_spent, budget),
            );
        }
        contract.agent_spent += 1;
        return allow(
            "WITHIN_CONTRACT",
            &format!("委派已消耗预算 {}/{}", contract.agent_spent, budget),
        );
    }

    let writing = matches!(
        tool,
        "write" | "edit" | "batch_write" | "batch_edit" | "str_replace_editor" | "pwsh" | "bash" | "run_command" | "delete_file"
    );
    if !writing {
        return allow("WITHIN_CONTRACT", "非变更类动作");
    }

    // 模式禁止变更
    if matches!(contract.mode.as_str(), "review" | "answer" | "monitor" | "watch") {
        return deny(
            "MODE_FORBIDS_MUTATION",
            &format!("当前任务契约模式为 {}，不允许变更类操作", contract.mode),
        );
    }
    // 可变更性未证实
    if contract.mode == "observe" {
        return deny(
            "MUTABILITY_UNPROVEN",
            "任务契约处于 observe 模式：变更未被证实授权，请先明确告知用户并取得同意",
        );
    }
    // 破坏性类别永不放行
    if DESTRUCTIVE_CATEGORIES.contains(&action.as_str()) {
        return deny(
            "DESTRUCTIVE_NOT_ALLOWED",
            &format!("破坏性类别 {} 结构性不可授权", action),
        );
    }
    // 路径越界
    if !contract.allowed_paths.is_empty() && !path.is_empty() {
        let p = path.replace('\\', "/").to_lowercase();
        let ok = contract
            .allowed_paths
            .iter()
            .any(|a| p.starts_with(&a.replace('\\', "/").to_lowercase()));
        if !ok {
            return deny("PATH_OUTSIDE_CONTRACT", &format!("{} 不在契约允许路径内", path));
        }
    } else if path.is_empty() && !command.is_empty() {
        return deny("WRITE_PATH_UNPROVEN", "命令写入路径无法从参数证明");
    }
    // 类别不在契约内
    if !contract.categories.is_empty()
        && !NON_DESTRUCTIVE_CATEGORIES.contains(&action.as_str())
        && !contract.categories.iter().any(|c| c == &action)
    {
        return deny(
            "CATEGORY_NOT_IN_CONTRACT",
            &format!("动作类别 {} 不在契约允许列表内", action),
        );
    }
    // 委派预算
    if action == "delegate" {
        let budget = if contract.agent_budget == 0 { 2 } else { contract.agent_budget };
        if contract.agent_spent >= budget {
            return deny("AGENT_BUDGET_EXHAUSTED", &format!("委派预算已用尽（{}/{}）", contract.agent_spent, budget));
        }
        contract.agent_spent += 1;
        return allow("WITHIN_CONTRACT", &format!("委派已消耗预算 {}/{}", contract.agent_spent, budget));
    }
    allow("WITHIN_CONTRACT", "在契约范围内")
}

// ════════════════════════════════════════════════════════
// ③ 质量账本（移植自 dsh-rule-engine quality-ledger.js）
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRow {
    pub sig: String,
    pub ts: i64,
    pub rework: u32,
    pub interventions: u32,
    pub frictions: u32,
    pub tokens: u64,
}

pub fn quality_ledger_path() -> PathBuf {
    rule_engine_dir().join("quality-ledger.jsonl")
}

/// 任务签名：去引号 → 盘符绝对路径 → <path> → 数字 → <n> → 折叠空白 → 小写 → FNV-1a 前 12 位
pub fn task_signature(text: &str) -> String {
    let mut s = String::new();
    let mut in_quote: Option<char> = None;
    for ch in text.chars() {
        if let Some(q) = in_quote {
            if ch == q {
                in_quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            in_quote = Some(ch);
            continue;
        }
        s.push(ch);
    }
    // 路径归一化
    let mut out = String::new();
    for token in s.split_whitespace() {
        let t = if token.starts_with('/')
            || token.contains(":/")
            || token.contains(":\\")
            || token.starts_with("\\\\")
        {
            "<path>"
        } else {
            token
        };
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
    }
    // 数字归一化
    let mut normalized = String::new();
    let mut prev_digit = false;
    for ch in out.chars() {
        if ch.is_ascii_digit() {
            if !prev_digit {
                normalized.push_str("<n>");
            }
            prev_digit = true;
        } else {
            normalized.push(ch);
            prev_digit = false;
        }
    }
    let normalized = normalized.to_lowercase();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in normalized.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

/// 记一条质量记录
pub fn record_quality(row: QualityRow) -> Result<(), String> {
    let p = quality_ledger_path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .map_err(|e| format!("打开质量账本失败: {}", e))?;
    let line = serde_json::to_string(&row).map_err(|e| e.to_string())?;
    writeln!(f, "{}", line).map_err(|e| format!("写质量账本失败: {}", e))
}

pub fn load_quality_ledger() -> Vec<QualityRow> {
    let Ok(text) = std::fs::read_to_string(quality_ledger_path()) else {
        return vec![];
    };
    text.lines().filter_map(|l| serde_json::from_str::<QualityRow>(l).ok()).collect()
}

/// 按签名聚合趋势（最近 window 条 vs 之前 window 条）
pub fn quality_trend(window: usize) -> Vec<(String, f64, f64, usize)> {
    let rows = load_quality_ledger();
    let mut by_sig: HashMap<String, Vec<&QualityRow>> = HashMap::new();
    for r in &rows {
        by_sig.entry(r.sig.clone()).or_default().push(r);
    }
    let w = window.max(1);
    let mut out: Vec<(String, f64, f64, usize)> = Vec::new();
    for (sig, list) in by_sig {
        if list.len() < 2 {
            continue;
        }
        let score = |rs: &[&QualityRow]| -> f64 {
            if rs.is_empty() {
                return 0.0;
            }
            let n = rs.len() as f64;
            rs.iter().map(|r| r.rework as f64 + r.frictions as f64).sum::<f64>() / n
        };
        let split = list.len().saturating_sub(w);
        let recent = score(&list[split..]);
        let prev = score(&list[..split]);
        out.push((sig, prev, recent, list.len()));
    }
    out.sort_by(|a, b| b.3.cmp(&a.3));
    out.truncate(8);
    out
}

// ════════════════════════════════════════════════════════
// ④ 热词学习（移植自 dsh-rule-engine hotwords.js）
// ════════════════════════════════════════════════════════

pub const HOTWORD_CAP: usize = 500;
pub const HOTWORD_MIN_LEN: usize = 2;
pub const HOTWORD_MAX_LEN: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HotWords {
    pub versions: u32,
    pub words: Vec<String>,
    pub updated_at: i64,
}

pub fn hotwords_path() -> PathBuf {
    rule_engine_dir().join("rule-engine-hotwords.json")
}

pub fn load_hotwords() -> HotWords {
    std::fs::read_to_string(hotwords_path())
        .ok()
        .and_then(|t| serde_json::from_str::<HotWords>(&t).ok())
        .unwrap_or(HotWords { versions: 1, ..Default::default() })
}

/// 学习一个动作词（长度受限、去重、上限 500，原子写）
pub fn learn_hotword(word: &str) -> Result<HotWords, String> {
    let w = word.trim().to_lowercase();
    let n = w.chars().count();
    if n < HOTWORD_MIN_LEN || n > HOTWORD_MAX_LEN {
        return Err(format!("热词长度需在 {}~{} 之间", HOTWORD_MIN_LEN, HOTWORD_MAX_LEN));
    }
    let mut hw = load_hotwords();
    hw.versions = 1;
    if !hw.words.contains(&w) {
        hw.words.push(w);
        if hw.words.len() > HOTWORD_CAP {
            let drop_n = hw.words.len() - HOTWORD_CAP;
            hw.words.drain(0..drop_n);
        }
    }
    hw.updated_at = chrono::Utc::now().timestamp();
    let p = hotwords_path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&hw).unwrap_or_default())
        .map_err(|e| format!("写热词失败: {}", e))?;
    std::fs::rename(&tmp, &p).map_err(|e| format!("替换热词失败: {}", e))?;
    Ok(hw)
}

/// 结合内置动作词 + 学习到的热词判断执行子句
pub fn has_execute_clause_with_hotwords(user_text: &str) -> bool {
    if has_execute_clause(user_text) {
        return true;
    }
    let t = user_text.to_lowercase();
    load_hotwords().words.iter().any(|w| t.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SAMPLE: &str = r#"
## 安全
### [规则 22] 不要擅自执行
执行等级：A
**触发**：用户提出需求
**检查**：本轮是否含执行子句
**动作**：无执行子句时拒绝变更类工具

## 文档
### [规则 19] 版本记录
执行等级：D
**触发**：提到版本
**检查**：是否同步正文

<!-- free-zone:start -->
### [规则 99] 这一条不应被解析
执行等级：A
<!-- free-zone:end -->
"#;

    #[test]
    fn parses_rules_and_skips_free_zone() {
        let rules = parse_rules(SAMPLE);
        assert_eq!(rules.len(), 2, "free-zone 内的规则不应被解析");
        assert_eq!(rules[0].id, "22");
        assert_eq!(rules[0].title, "不要擅自执行");
        assert_eq!(rules[0].section, "安全");
        assert_eq!(rules[0].level, "A");
        assert!(rules[0].actions.contains(&"deny".to_string()));
        assert_eq!(rules[1].id, "19");
        assert_eq!(rules[1].level, "D");
    }

    #[test]
    fn level_maps_to_actions_and_confidence() {
        assert!(actions_for_level("A").contains(&"deny".to_string()));
        assert!(actions_for_level("B + D").contains(&"correct".to_string()));
        assert!(actions_for_level("B + D").contains(&"self-certify".to_string()));
        assert_eq!(actions_for_level("").len(), 1); // 默认 self-certify
        let rules = parse_rules(SAMPLE);
        assert_eq!(rules[0].confidence, "high", "等级+触发+检查+动作 → high");
    }

    #[test]
    fn tool_classification() {
        assert_eq!(classify_tool("read"), ToolClass::Analysis);
        assert_eq!(classify_tool("pwsh"), ToolClass::Mutating);
        assert_eq!(classify_tool("mcp__foo__bar"), ToolClass::Mutating);
        assert_eq!(classify_tool("vision_read"), ToolClass::Analysis);
        assert_eq!(classify_tool("something_else"), ToolClass::Unknown);
    }

    #[test]
    fn readonly_command_analysis_preserves_2_and_1() {
        assert!(is_readonly_command("git status"));
        assert!(is_readonly_command("git log --oneline | head -20"));
        // 2>&1 的 | 前面是数字 → 不当成分段符
        assert!(is_readonly_command("node --version 2>&1"));
        assert!(!is_readonly_command("rm -rf /tmp/x"));
        assert!(!is_readonly_command("git status; rm -rf x"));
        // 未知段 ⇒ 非只读
        assert!(!is_readonly_command("someunknowncmd --flag"));
    }

    #[test]
    fn guard_denies_mutation_without_execute_clause() {
        let cfg = RuleEngineConfig::default();
        let mut st = RuleEngineState {
            real_user_seen: true,
            has_execute_clause: false,
            ..Default::default()
        };
        let d = guard_decision(&cfg, &mut st, "write", &json!({"file_path":"a.txt"}), 1000);
        assert!(!d.allow);
        assert_eq!(d.rule_id, "22");
        assert!(d.reason.contains("ERR-"));
    }

    #[test]
    fn guard_allows_readonly_and_bypass() {
        let cfg = RuleEngineConfig::default();
        let mut st = RuleEngineState::default();
        // 只读工具无条件放行
        assert!(guard_decision(&cfg, &mut st, "read", &json!({"file_path":"a"}), 1000).allow);
        // 只读命令放行
        assert!(guard_decision(&cfg, &mut st, "bash", &json!({"command":"git status"}), 1000).allow);
        // 旁路窗口全放行
        let mut cfg2 = RuleEngineConfig::default();
        cfg2.bypass_until = 2000;
        assert!(guard_decision(&cfg2, &mut st, "write", &json!({"file_path":"a"}), 1000).allow);
    }

    #[test]
    fn authorization_scoping_is_boundary_aware() {
        let now = 1000;
        let auth = Authorization {
            at: 900,
            expires_at: now + 600,
            r#type: "write".into(),
            path_prefix: "D:/proj/src".into(),
            source: "user".into(),
        };
        assert!(auth_matches(&auth, "write", "D:/proj/src/a.rs", now));
        assert!(auth_matches(&auth, "write", "d:\\proj\\src\\b.rs", now));
        // 前缀边界：src2 不应命中
        assert!(!auth_matches(&auth, "write", "D:/proj/src2/a.rs", now));
        // 类型不符
        assert!(!auth_matches(&auth, "delete", "D:/proj/src/a.rs", now));
        // any 通配
        assert!(auth_matches(&auth, "any", "D:/proj/src/a.rs", now));
        // 过期
        assert!(!auth_matches(&auth, "write", "D:/proj/src/a.rs", now + 700));
    }

    #[test]
    fn self_protect_and_inline_command_are_denied() {
        let cfg = RuleEngineConfig::default();
        let mut st = RuleEngineState::default();
        let d = guard_decision(&cfg, &mut st, "write", &json!({"file_path":"D:/x/AGENTS.md"}), 1000);
        assert!(!d.allow);
        assert_eq!(d.rule_id, "__self-protect");

        let d = guard_decision(&cfg, &mut st, "bash", &json!({"command":"node -e \"console.log(1)\""}), 1000);
        assert!(!d.allow);
        assert_eq!(d.rule_id, "9");
    }

    #[test]
    fn retry_breaker_trips_on_third_attempt() {
        let cfg = RuleEngineConfig::default();
        let mut st = RuleEngineState {
            has_execute_clause: true,
            ..Default::default()
        };
        let args = json!({"command":"npm test"});
        st.authorizations.push(Authorization {
            at: 0, expires_at: 9999, r#type: "command".into(),
            path_prefix: "".into(), source: "user".into(),
        });
        // 前两次放行（计数由调用方在失败时 +1）
        assert!(guard_decision(&cfg, &mut st, "bash", &args, 1000).allow);
        let key = format!("bash:{}", args);
        st.retry_counts.insert(key.clone(), 1);
        assert!(guard_decision(&cfg, &mut st, "bash", &args, 1000).allow);
        st.retry_counts.insert(key.clone(), 2);
        let d = guard_decision(&cfg, &mut st, "bash", &args, 1000);
        assert!(!d.allow, "第 3 次相同调用应被熔断");
        assert_eq!(d.rule_id, "1");
    }

    #[test]
    fn execute_clause_detection() {
        assert!(has_execute_clause("请帮我实现登录功能"));
        assert!(has_execute_clause("write the file now"));
        assert!(!has_execute_clause("这个功能怎么实现？"));
        assert!(!has_execute_clause("what does this do?"));
    }

    // ─── 回合裁决卡片 / 指纹 / /guard ───

    #[test]
    fn fingerprint_normalises_and_never_covers_dangerous_commands() {
        // 数字与绝对路径被归一化 → 同类命令同指纹
        let a = fingerprint_of("git status --porcelain -b").unwrap();
        let b = fingerprint_of("git status --porcelain -b").unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 12, "指纹应为 12 位十六进制");
        // 路径差异不影响
        let p1 = fingerprint_of("git add D:/proj/a.txt").unwrap();
        let p2 = fingerprint_of("git add D:/proj/b.txt").unwrap();
        assert_eq!(p1, p2, "绝对路径应被归一化为 <path>");
        // 危险命令 → 无指纹（永不被学习放行）
        assert!(fingerprint_of("rm -rf /tmp/x").is_none());
        assert!(fingerprint_of("Remove-Item -Recurse -Force D:/x").is_none());
        assert!(fingerprint_of("git reset --hard HEAD~1").is_none());
        assert!(fingerprint_of("git push --force origin main").is_none());
    }

    #[test]
    fn turn_card_progress_matches_upstream_wording() {
        let mk = |labels: &[&str]| TurnCard {
            key: "k".into(), session_id: "s".into(), message_id: String::new(),
            turn: 1, user_text: "u".into(), verdict: "denied".into(), at: 0,
            blocks: labels.iter().enumerate().map(|(i, l)| TurnCardBlock {
                i, tool: "bash".into(), args: "npm test".into(),
                rule_id: "22".into(), title: "t".into(), reason: "r".into(),
                err_id: "ABC123".into(), label: l.to_string(), labeled_at: 0,
            }).collect(),
        };
        assert_eq!(mk(&["", "", ""]).progress(), "已判 0/3");
        assert_eq!(mk(&["correct", "", ""]).progress(), "已判 1/3（✅1）");
        assert_eq!(mk(&["correct", "incorrect", ""]).progress(), "已判 2/3（✅1, ❌1）");
        assert!(mk(&["correct", "correct"]).all_labeled());
        assert!(!mk(&["correct", ""]).all_labeled());
    }

    #[test]
    fn label_allows_only_when_incorrect_and_clears() {
        // 用独立临时目录，绝不触碰真实 ~/.dsh 状态
        let base = std::env::temp_dir().join(format!("da_rules_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);

        let fp = fingerprint_of("git add D:/proj/a.txt").unwrap();
        let _ = clear_label_in(&base, &fp);
        assert!(
            label_allows_in(&base, "git add D:/proj/other.txt").is_none(),
            "未登记时不应放行"
        );
        upsert_label_in(&base, &fp, "incorrect").unwrap();
        // 同类命令（路径不同）应命中同一指纹
        assert!(
            label_allows_in(&base, "git add D:/proj/b.txt").is_some(),
            "登记后同类命令应放行"
        );
        // correct 标签不放行
        let fp2 = fingerprint_of("npm run build").unwrap();
        let _ = clear_label_in(&base, &fp2);
        upsert_label_in(&base, &fp2, "correct").unwrap();
        assert!(
            label_allows_in(&base, "npm run build").is_none(),
            "correct 不应产生放行"
        );
        // 撤销
        assert!(clear_label_in(&base, &fp).unwrap());
        assert!(
            label_allows_in(&base, "git add D:/proj/c.txt").is_none(),
            "撤销后不应再放行"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn dangerous_command_never_gets_a_label_even_if_rated_incorrect() {
        let base = std::env::temp_dir().join(format!("da_rules_danger_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&base);
        // 危险命令指纹为 None → 无法登记 → 永远不可能被学习放行
        assert!(fingerprint_of("rm -rf D:/important").is_none());
        assert!(label_allows_in(&base, "rm -rf D:/important").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    // ─── 文本审计 / 契约 / 质量账本 / 热词 ───

    #[test]
    fn text_audit_detects_promise_and_missing_verification() {
        let mut st = TextAuditState::default();
        let hits = text_audit("这个问题一定没问题，已经彻底解决了。", "修一下", &mut st);
        assert!(hits.iter().any(|h| h.rule_id == "7"), "应检出承诺性表达");
        assert!(
            hits.iter().any(|h| h.rule_id == "23"),
            "声称已完成但无验证证据应被检出"
        );
        // 有验证证据时不再报 23
        let mut st2 = TextAuditState::default();
        let hits2 = text_audit("已完成，测试通过。", "修一下", &mut st2);
        assert!(!hits2.iter().any(|h| h.rule_id == "23"));
    }

    #[test]
    fn text_audit_detects_empty_talk_and_bare_apology() {
        let mut st = TextAuditState::default();
        assert!(text_audit("好的", "做这个", &mut st).iter().any(|h| h.rule_id == "22"));
        let mut st2 = TextAuditState::default();
        let hits = text_audit("非常抱歉给您带来困扰。", "做这个", &mut st2);
        assert!(hits.iter().any(|h| h.rule_id == "22"), "只道歉未给原因应被检出");
        // 给出原因则不报
        let mut st3 = TextAuditState::default();
        let hits3 = text_audit("抱歉，原因是配置写错了，我会修正并避免。", "做这个", &mut st3);
        assert!(!hits3.iter().any(|h| h.rule_id == "22"));
    }

    #[test]
    fn injection_gate_is_once_per_rule_and_budgeted() {
        let mut st = TextAuditState::default();
        let now = 1000;
        let hits = vec![TextHit {
            rule_id: "7".into(), title: "t".into(), evidence: "e".into(), kind: "correct".into(),
        }];
        let (ok1, fresh) = should_deliver_injection(&mut st, &hits, now);
        assert!(ok1 && fresh.len() == 1);
        // 同一规则不重复投递
        let (ok2, _) = should_deliver_injection(&mut st, &hits, now + 1);
        assert!(!ok2, "同一规则只投递一次");
        // 换规则：受"每小时 ≤3"预算限制（已有 1 次）
        for (i, rid) in ["8", "9"].iter().enumerate() {
            let h = vec![TextHit {
                rule_id: rid.to_string(), title: "t".into(), evidence: "e".into(), kind: "correct".into(),
            }];
            let (ok, _) = should_deliver_injection(&mut st, &h, now + 2 + i as i64);
            assert!(ok, "第 {} 条新规则应在预算内", i + 2);
        }
        // 第 4 条应被预算拦下
        let h4 = vec![TextHit {
            rule_id: "10".into(), title: "t".into(), evidence: "e".into(), kind: "correct".into(),
        }];
        let (ok4, _) = should_deliver_injection(&mut st, &h4, now + 10);
        assert!(!ok4, "每小时最多 3 次注入");
        // 一小时后预算恢复
        let (ok5, _) = should_deliver_injection(&mut st, &h4, now + 3601);
        assert!(ok5, "超过一小时预算应恢复");
    }

    #[test]
    fn injection_text_matches_upstream_shape() {
        let one = vec![TextHit {
            rule_id: "7".into(), title: "承诺性表达".into(), evidence: "".into(), kind: "correct".into(),
        }];
        let s = render_injection(&one);
        assert!(s.starts_with("[规则引擎]"));
        assert!(s.contains("规则 7"));
        // 多条聚合
        let many = vec![
            TextHit { rule_id: "7".into(), title: "a".into(), evidence: "".into(), kind: "correct".into() },
            TextHit { rule_id: "23".into(), title: "b".into(), evidence: "".into(), kind: "correct".into() },
        ];
        let s2 = render_injection(&many);
        assert!(s2.contains("本轮检出 2 项"));
    }

    #[test]
    fn task_contract_blocks_and_allows() {
        // 未启用 → 放行
        let mut c = TaskContract::default();
        let d = decide_contract_action(&mut c, "write", &json!({"file_path": "a.txt"}));
        assert!(d.allow && d.reason_code == "CONTROL_INACTIVE");

        // observe 模式 → 可变更性未证实
        c.armed = true;
        c.level = "guard".into();
        c.mode = "observe".into();
        let d = decide_contract_action(&mut c, "write", &json!({"file_path": "a.txt"}));
        assert!(!d.allow && d.reason_code == "MUTABILITY_UNPROVEN");

        // change 模式 + 路径约束
        c.mode = "change".into();
        c.allowed_paths = vec!["D:/proj".into()];
        let d = decide_contract_action(&mut c, "write", &json!({"file_path": "D:/proj/a.rs"}));
        assert!(d.allow, "契约内路径应放行：{}", d.message);
        let d = decide_contract_action(&mut c, "write", &json!({"file_path": "D:/other/a.rs"}));
        assert!(!d.allow && d.reason_code == "PATH_OUTSIDE_CONTRACT");

        // 破坏性类别结构性不可授权
        let d = decide_contract_action(&mut c, "bash", &json!({"command": "rm -rf D:/proj/x"}));
        assert!(!d.allow && d.reason_code == "DESTRUCTIVE_NOT_ALLOWED");
    }

    #[test]
    fn task_contract_delegation_budget() {
        let mut c = TaskContract {
            armed: true,
            level: "guard".into(),
            mode: "change".into(),
            agent_budget: 2,
            ..Default::default()
        };
        for _ in 0..2 {
            let d = decide_contract_action(&mut c, "subagent", &json!({}));
            assert!(d.allow, "预算内应放行");
        }
        let d = decide_contract_action(&mut c, "subagent", &json!({}));
        assert!(!d.allow && d.reason_code == "AGENT_BUDGET_EXHAUSTED");
        assert_eq!(c.agent_spent, 2);
    }
    #[test]
    fn quality_signature_normalises_and_stays_stable() {
        // 路径与数字归一化 → 同签名
        assert_eq!(
            task_signature("修复 D:/proj/a.rs 的第 42 行"),
            task_signature("修复 D:/proj/b.rs 的第 99 行")
        );
        // 引号内容被剥离
        assert_eq!(
            task_signature("执行 \"rm -rf x\" 命令"),
            task_signature("执行 命令")
        );
        assert_eq!(task_signature("x").len(), 12);
        assert_ne!(task_signature("修复登录"), task_signature("新增支付"));
    }

    #[test]
    fn hotword_learning_validates_length_and_dedupes() {
        let base = rule_engine_dir().join("hw_test");
        std::env::set_var("DEEPAHEAD_RULE_ENGINE_DIR", base.to_string_lossy().to_string());
        let _ = std::fs::remove_dir_all(&base);
        // 过短
        assert!(learn_hotword("x").is_err());
        // 过长
        assert!(learn_hotword(&"a".repeat(20)).is_err());
        // 正常
        let hw = learn_hotword("部署").unwrap();
        assert!(hw.words.contains(&"部署".to_string()));
        // 去重
        let hw2 = learn_hotword("部署").unwrap();
        assert_eq!(hw2.words.iter().filter(|w| *w == "部署").count(), 1);
        // 学到的热词参与执行子句判定
        assert!(has_execute_clause_with_hotwords("帮我部署一下"));
        std::env::remove_var("DEEPAHEAD_RULE_ENGINE_DIR");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn guard_command_extended_surface() {
        let mut cfg = RuleEngineConfig::default();
        let mut st = RuleEngineState::default();
        // mode
        assert!(run_guard_command(&mut cfg, &mut st, "/guard mode change").ok);
        assert!(st.contract.armed && st.contract.mode == "change");
        assert!(!run_guard_command(&mut cfg, &mut st, "/guard mode nonsense").ok);
        // budget
        assert!(run_guard_command(&mut cfg, &mut st, "/guard budget 5").ok);
        assert_eq!(st.contract.agent_budget, 5);
        // contract 细项
        assert!(run_guard_command(&mut cfg, &mut st, "/guard contract hash allow").ok);
        assert_eq!(st.contract.hash_policy, "allow");
        assert!(run_guard_command(&mut cfg, &mut st, "/guard contract path D:/proj").ok);
        assert!(st.contract.allowed_paths.contains(&"D:/proj".to_string()));
        assert!(run_guard_command(&mut cfg, &mut st, "/guard contract").text.contains("任务契约"));
        // approve 不允许 any
        assert!(!run_guard_command(&mut cfg, &mut st, "/guard approve any D:/x").ok);
        assert!(run_guard_command(&mut cfg, &mut st, "/guard approve write D:/x 5").ok);
        assert_eq!(st.authorizations.len(), 1);
        // quality（空账本也应正常返回）
        assert!(run_guard_command(&mut cfg, &mut st, "/guard quality").ok);
        // freedom 现在应列出新实现项
        let f = run_guard_command(&mut cfg, &mut st, "/guard freedom").text;
        assert!(f.contains("任务契约") && f.contains("质量账本"));
    }

    #[test]
    fn guard_command_surface_covers_key_subcommands() {
        let mut cfg = RuleEngineConfig::default();
        let mut st = RuleEngineState::default();
        // help
        assert!(run_guard_command(&mut cfg, &mut st, "/guard").text.contains("子命令"));
        // status
        let r = run_guard_command(&mut cfg, &mut st, "/guard status");
        assert!(r.ok && r.text.contains("规则引擎"));
        // unlock / bypass / lock
        let r = run_guard_command(&mut cfg, &mut st, "/guard unlock 3");
        assert!(r.ok && cfg.unlock_until > 0);
        let r = run_guard_command(&mut cfg, &mut st, "/guard bypass 2");
        assert!(r.ok && cfg.bypass_until > 0);
        let r = run_guard_command(&mut cfg, &mut st, "/guard lock");
        assert!(r.ok && cfg.unlock_until == 0 && cfg.bypass_until == 0);
        // 未知子命令 → 错误并给用法
        let r = run_guard_command(&mut cfg, &mut st, "/guard nonsense");
        assert!(!r.ok && r.text.contains("子命令"));
        // label 参数校验
        let r = run_guard_command(&mut cfg, &mut st, "/guard label ABC wrong-value");
        assert!(!r.ok);
        // freedom 诚实清单
        let r = run_guard_command(&mut cfg, &mut st, "/guard freedom");
        assert!(r.ok && r.text.contains("未实现"));
    }
}

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

fn turn_cards_path_in(base: &Path) -> PathBuf {
    base.join("rule-engine-turn-cards.json")
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
  ✓ 审计账本 JSONL（2MB 轮转）

未实现（诚实清单）：
  ✗ LLM 意图兜底（上游的「非对称救援」需要 LLM 路由）
  ✗ 文本注入纠正通道（助手文本审计：交付声明/时间证据/批评冻结等 20 项）
  ✗ 任务契约与反过度设计（modes/budgets/hash 策略）
  ✗ 质量账本、热词学习、技能授权实时对账
  ✗ /guard 的 approve / mode / budget / contract / quality 子命令
";
            ok(s.into())
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

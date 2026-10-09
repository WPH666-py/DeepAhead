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
}

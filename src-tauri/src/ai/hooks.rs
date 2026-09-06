use serde::{Deserialize, Serialize};

/// ─── 安全钩子（从 e-ide hooks.ts 移植）───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub action: HookAction,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum HookAction {
    Confirm,  // 需要用户确认
    Warn,     // 警告但允许
    Block,    // 阻止执行
    Log,      // 仅记录
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResult {
    pub triggered: bool,
    pub rule_id: String,
    pub message: String,
    pub action: HookAction,
}

pub struct SafetyHooks {
    rules: Vec<HookRule>,
}

impl SafetyHooks {
    pub fn new() -> Self {
        Self {
            rules: vec![
                HookRule {
                    id: "confirm-dangerous-rm".into(),
                    name: "确认危险删除".into(),
                    description: "在执行 rm -rf 或递归删除前要求用户确认".into(),
                    action: HookAction::Confirm,
                    patterns: vec![
                        r"rm\s+-rf".into(),
                        r"rm\s+-r\s+/".into(),
                        r"del\s+/[fsq]".into(),
                        r"Remove-Item\s+-Recurse".into(),
                        r"DROP\s+(TABLE|DATABASE)".into(),
                    ],
                },
                HookRule {
                    id: "warn-sensitive-files".into(),
                    name: "敏感文件警告".into(),
                    description: "编辑 .env、credentials、密钥文件时警告".into(),
                    action: HookAction::Warn,
                    patterns: vec![
                        r"\.env\b".into(),
                        r"credentials".into(),
                        r"\.pem\b".into(),
                        r"\.key\b".into(),
                        r"secret".into(),
                        r"\.htpasswd".into(),
                        r"id_rsa".into(),
                    ],
                },
                HookRule {
                    id: "warn-console-log".into(),
                    name: "调试代码警告".into(),
                    description: "检测 console.log、debugger 等调试语句".into(),
                    action: HookAction::Warn,
                    patterns: vec![
                        r"console\.(log|debug|warn)\(".into(),
                        r"\bdebugger\b".into(),
                        r"print\(\s*$".into(),
                        r"fmt\.Println".into(),
                        r"System\.out\.println".into(),
                    ],
                },
                HookRule {
                    id: "confirm-force-push-main".into(),
                    name: "确认强制推送".into(),
                    description: "对 main/master 分支执行 force push 时确认".into(),
                    action: HookAction::Confirm,
                    patterns: vec![
                        r"push\s+--force".into(),
                        r"push\s+-f\b".into(),
                        r"push\s+--force-with-lease".into(),
                        r"reset\s+--hard".into(),
                        r"branch\s+-D".into(),
                    ],
                },
                HookRule {
                    id: "warn-secrets-in-code".into(),
                    name: "代码中的密钥检测".into(),
                    description: "检测硬编码的 API Key、Token、密码".into(),
                    action: HookAction::Warn,
                    patterns: vec![
                        r"sk-[a-zA-Z0-9]{20,}".into(),
                        r"AIza[0-9A-Za-z\-_]{35}".into(),
                        r"ghp_[a-zA-Z0-9]{36}".into(),
                        r#"api[_-]?key\s*[:=]\s*['"]"#.into(),
                        r#"password\s*[:=]\s*['"]"#.into(),
                        r#"token\s*[:=]\s*['"]"#.into(),
                        r#"secret\s*[:=]\s*['"]"#.into(),
                    ],
                },
            ],
        }
    }

    /// 对所有规则检查文本内容
    pub fn evaluate_all(&self, content: &str) -> Vec<HookResult> {
        let mut results = Vec::new();

        for rule in &self.rules {
            for pattern in &rule.patterns {
                if let Ok(re) = regex::Regex::new(pattern) {
                    if re.is_match(content) {
                        if results.iter().any(|r: &HookResult| r.rule_id == rule.id) {
                            continue; // 同一规则只触发一次
                        }
                        results.push(HookResult {
                            triggered: true,
                            rule_id: rule.id.clone(),
                            message: format!("[{}] {}", rule.name, rule.description),
                            action: rule.action.clone(),
                        });
                    }
                }
            }
        }

        results
    }

    /// 检查是否有需要确认的操作
    pub fn has_confirmations(&self, results: &[HookResult]) -> bool {
        results.iter().any(|r| r.action == HookAction::Confirm || r.action == HookAction::Block)
    }

    /// 获取所有规则定义
    pub fn get_rules(&self) -> &[HookRule] {
        &self.rules
    }
}

impl Default for SafetyHooks {
    fn default() -> Self {
        Self::new()
    }
}

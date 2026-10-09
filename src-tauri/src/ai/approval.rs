// ═══════════════════════════════════════════════════════════════════
// 执行许可门（Approval Gate）—— 对标 DeepSeek Harness 的审批机制
//
// DeepAhead 提供三档执行许可，档位**同时决定规则引擎的开关**：
//
//   | 档位            | key     | 规则引擎 | 逐调用审批 | 裁决卡片 |
//   |-----------------|---------|----------|------------|----------|
//   | 需逐步确认      | step    | 全开     | 是         | 是       |
//   | 仅确认风险操作  | risk    | 全开     | 否（仅风险）| 是      |
//   | 全流程开放      | open    | 全关     | 否         | 否       |
//
// 卡片语义（三档统一，用户只回答一次）：
//   ❌ 放行 → 该操作立即放行，前端自动回「继续」，Agent 继续跑
//   ✅ 拦截 → 该操作被拦下，模型换方案或向用户确认
//
// 实现方式：工具被执行前调用 ApprovalGate::request()。
//   - 全流程开放：直接放行，不产生事件，规则引擎同步全关（所有操作永久放行）；
//   - 仅确认风险操作：规则引擎决定哪些调用是"风险操作"，只有命中硬门的调用
//     才发 `ai-agent-event`（kind = tool_approval_required）并挂起等待；
//   - 需逐步确认：每一个工具调用都发审批事件，由用户逐步把关。
//
// 等待期超时（默认 10 分钟）按拦截处理，保证前端崩溃时循环不永久挂起。
//
// 子智能体（subagents / 审查塔 / 审查员）复用同一把门：审批事件与
// 主循环走同一条事件通道，因此三档模式下嵌套派遣同样受控。
// ═══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Emitter;

use crate::ai::agent_loop::{AgentEvent, AgentEventKind};

/// 执行许可模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// 需逐步确认：每个工具调用都必须等待用户批准
    StepConfirm,
    /// 仅确认风险操作：只对规则引擎判定为风险的操作弹卡片
    RiskOnly,
    /// 全流程开放：自动批准全部工具调用，规则引擎全部开关关闭（永久放行）
    FullOpen,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        Self::StepConfirm
    }
}

impl ApprovalMode {
    /// 解析前端下发的模式字符串（"step" / "risk" / "open"）
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "step" | "step_confirm" | "confirm" | "every" => Self::StepConfirm,
            "risk" | "risk_only" | "risky" | "confirm_risk" => Self::RiskOnly,
            "open" | "full" | "full_open" | "all" => Self::FullOpen,
            _ => Self::FullOpen,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StepConfirm => "step",
            Self::RiskOnly => "risk",
            Self::FullOpen => "open",
        }
    }

    /// 中文展示名（前端 / 事件元数据共用）
    pub fn label(&self) -> &'static str {
        match self {
            Self::StepConfirm => "需逐步确认",
            Self::RiskOnly => "仅确认风险操作",
            Self::FullOpen => "全流程开放",
        }
    }

    /// 该档位下规则引擎是否参与：全流程开放时规则引擎的所有开关全关，
    /// 所有操作**永久放行**（硬门不生效、裁决卡片不产生）。
    pub fn rule_engine_enabled(&self) -> bool {
        !matches!(self, Self::FullOpen)
    }

    /// 该档位下是否产生回合裁决卡片（全流程开放时不产生）
    pub fn turn_card_enabled(&self) -> bool {
        self.rule_engine_enabled()
    }

    /// 该档位下是否需要逐调用审批
    pub fn gate_every_call(&self) -> bool {
        matches!(self, Self::StepConfirm)
    }

    /// 配置摘要（写日志用）
    pub fn describe(&self) -> String {
        format!(
            "{}（规则引擎 {}，逐调用审批 {}，裁决卡片 {}）",
            self.label(),
            if self.rule_engine_enabled() { "全开" } else { "全关·永久放行" },
            if self.gate_every_call() { "开启" } else { "关闭" },
            if self.turn_card_enabled() { "开启" } else { "关闭" },
        )
    }
}

/// 审批门：Task 持有 AppHandle + 待决审批表，负责发事件与等待用户应答。
/// 以 `Arc<ApprovalGate>` 形式作为 Tauri State 管理，运行中输入按 Arc 共享。
pub struct ApprovalGate {
    app: Mutex<Option<tauri::AppHandle>>,
    pending: tokio::sync::Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
}

impl Default for ApprovalGate {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalGate {
    pub fn new() -> Self {
        Self {
            app: Mutex::new(None),
            pending: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    /// 绑定 AppHandle（在 send_ai_message_with_tools 中设置）
    pub fn set_app(&self, app: tauri::AppHandle) {
        if let Ok(mut guard) = self.app.lock() {
            *guard = Some(app);
        }
    }

    /// 发起一次工具调用审批。
    /// - `FullOpen`：立即返回 Ok(true)，不产生任何事件。
    /// - `StepConfirm` / `RiskOnly`：发出 tool_approval_required 事件并等待用户应答；
    ///   超时（600s）或通道中断按拦截处理。
    ///
    /// 调用方（agent_loop）负责判断当前调用是否属于"风险操作"：
    /// 仅确认风险操作档位下，非风险调用根本不进这把门。
    pub async fn request(
        &self,
        approval_id: &str,
        tool_id: &str,
        name: &str,
        arguments: &str,
        mode: ApprovalMode,
    ) -> Result<bool, String> {
        if mode == ApprovalMode::FullOpen {
            return Ok(true);
        }
        let app = self
            .app
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| "审批门未初始化（AppHandle 缺失）".to_string())?;

        let (tx, rx) = tokio::sync::oneshot::channel::<bool>();
        self.pending.lock().await.insert(approval_id.to_string(), tx);

        let ev = AgentEvent::new(AgentEventKind::ToolApprovalRequired {
            approval_id: approval_id.to_string(),
            id: tool_id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        });
        let _ = app.emit("ai-agent-event", serde_json::to_value(&ev).unwrap_or_default());

        // 等待前端应答；超时按拦截处理（防止前端关停后循环永久挂起）
        let approved = tokio::time::timeout(std::time::Duration::from_secs(600), rx)
            .await
            .map(|r| r.unwrap_or(false))
            .unwrap_or(false);

        self.pending.lock().await.remove(approval_id);
        crate::ai::runtime_log::info(
            "approval",
            &format!(
                "审批应答 approval={} tool={} → {}",
                approval_id,
                name,
                if approved { "❌ 放行（继续）" } else { "✅ 拦截" }
            ),
        );
        Ok(approved)
    }

    /// 前端应答：approval_id 对应的待决请求写入 true/false
    pub async fn resolve(&self, approval_id: String, approved: bool) -> Result<(), String> {
        let mut pending = self.pending.lock().await;
        let tx = pending
            .remove(&approval_id)
            .ok_or_else(|| "审批请求不存在或已超时".to_string())?;
        let _ = tx.send(approved);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mode_strings() {
        assert_eq!(ApprovalMode::parse("step"), ApprovalMode::StepConfirm);
        assert_eq!(ApprovalMode::parse("step_confirm"), ApprovalMode::StepConfirm);
        assert_eq!(ApprovalMode::parse("risk"), ApprovalMode::RiskOnly);
        assert_eq!(ApprovalMode::parse("risk_only"), ApprovalMode::RiskOnly);
        assert_eq!(ApprovalMode::parse("open"), ApprovalMode::FullOpen);
        assert_eq!(ApprovalMode::parse(""), ApprovalMode::FullOpen);
        assert_eq!(ApprovalMode::parse("unknown"), ApprovalMode::FullOpen);
    }

    #[test]
    fn default_is_step_confirm() {
        assert_eq!(ApprovalMode::default(), ApprovalMode::StepConfirm);
    }

    /// 全流程开放：规则引擎所有开关全关 ⇒ 所有操作永久放行
    #[test]
    fn full_open_turns_every_rule_switch_off() {
        let m = ApprovalMode::FullOpen;
        assert!(!m.rule_engine_enabled());
        assert!(!m.turn_card_enabled());
        assert!(!m.gate_every_call());
    }

    /// 仅确认风险操作 / 需逐步确认：规则引擎全开
    #[test]
    fn confirm_modes_switch_rule_engine_on() {
        for m in [ApprovalMode::StepConfirm, ApprovalMode::RiskOnly] {
            assert!(m.rule_engine_enabled(), "{:?} 应开启规则引擎", m);
            assert!(m.turn_card_enabled(), "{:?} 应产生裁决卡片", m);
        }
        assert!(ApprovalMode::StepConfirm.gate_every_call());
        assert!(!ApprovalMode::RiskOnly.gate_every_call());
    }

    #[test]
    fn as_str_roundtrip() {
        for m in [
            ApprovalMode::StepConfirm,
            ApprovalMode::RiskOnly,
            ApprovalMode::FullOpen,
        ] {
            assert_eq!(ApprovalMode::parse(m.as_str()), m);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// 执行许可门（Approval Gate）—— 对标 DeepSeek Harness 的审批机制
//
// 在 AI 配置中开启「工具（Agent）」后，DeepAhead 新增两种许可模式：
//   - 需分步确认（step）：每个工具调用在执行前向 UI 弹出审批请求，
//     用户「允许」后才执行，用户「拒绝」则把拒绝结果回灌给模型；
//   - 全流程开放（open）：自动批准全部工具调用（原 DeepKing 行为）。
//
// 实现方式：工具被执行前调用 ApprovalGate::request()。
//   open 模式直接放行；step 模式通过 AppHandle 发出
//   `ai-agent-event`（kind = tool_approval_required），然后挂起等待
//   前端调用 `respond_tool_approval` 命令写入应答（oneshot channel）。
//   超时（默认 10 分钟）按拒绝处理，保证前端崩溃时循环不永久挂起。
//
// 子智能体（subagents / 审查塔 / 审查员）复用同一把门：审批事件与
// 主循环走同一条事件通道，因此 step 模式下嵌套派遣同样受控。
// ═══════════════════════════════════════════════════════════════════

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Emitter;

use crate::ai::agent_loop::{AgentEvent, AgentEventKind};

/// 执行许可模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// 需分步确认：每个工具调用都必须等待用户批准
    StepConfirm,
    /// 全流程开放：自动批准全部工具调用
    FullOpen,
}

impl Default for ApprovalMode {
    fn default() -> Self {
        Self::StepConfirm
    }
}

impl ApprovalMode {
    /// 解析前端下发的模式字符串（"step" / "open"）
    pub fn parse(s: &str) -> Self {
        match s {
            "step" | "step_confirm" | "confirm" => Self::StepConfirm,
            _ => Self::FullOpen,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StepConfirm => "step",
            Self::FullOpen => "open",
        }
    }

    /// 中文展示名（前端 / 事件元数据共用）
    pub fn label(&self) -> &'static str {
        match self {
            Self::StepConfirm => "需分步确认",
            Self::FullOpen => "全流程开放",
        }
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
    /// - `FullOpen`：立即返回 Ok(true)，不产生任何事件（保持旧行为）。
    /// - `StepConfirm`：发出 tool_approval_required 事件并等待用户应答；
    ///   超时（600s）或通道中断按拒绝处理。
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

        // 等待前端应答；超时按拒绝处理（防止前端关停后循环永久挂起）
        let approved = tokio::time::timeout(std::time::Duration::from_secs(600), rx)
            .await
            .map(|r| r.unwrap_or(false))
            .unwrap_or(false);

        self.pending.lock().await.remove(approval_id);
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
        assert_eq!(ApprovalMode::parse("open"), ApprovalMode::FullOpen);
        assert_eq!(ApprovalMode::parse(""), ApprovalMode::FullOpen);
        assert_eq!(ApprovalMode::parse("unknown"), ApprovalMode::FullOpen);
    }

    #[test]
    fn default_is_step_confirm() {
        assert_eq!(ApprovalMode::default(), ApprovalMode::StepConfirm);
    }
}

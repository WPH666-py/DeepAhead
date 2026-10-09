//! ─── 待决规则裁决队列（回合裁决卡片 → 放行 → 自动继续）───
//!
//! 规则引擎在工具执行前拦下一次调用时，会在这里登记一条**待决记录**；
//! 前端渲染成裁决卡片后，用户在卡片上只回答一次：
//!
//!   - **❌ 放行**：登记一条一次性放行（工具名 / 操作类型 / 路径前缀三维匹配），
//!     并把该 run 标记为「待继续」。Agent 重试同一调用时规则引擎直接放行；
//!     若这一轮循环已经结束，前端读走「待继续」标记后自动回「继续」再跑一轮。
//!   - **✅ 拦截**：不放行。本轮不再重试该操作，模型须换方案或先向用户确认。
//!
//! 放行是**一次性**的：命中一次后即消费，避免一张卡片等于永久白名单。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

/// 单个 run 最多保留的待决记录数（防内存无界增长）
const MAX_PENDING_PER_RUN: usize = 64;
/// 全局最多保留的放行条目数
const MAX_ALLOWS: usize = 512;
/// 待继续标记的保留时长（秒）：超过则认为用户已离开，不再自动续跑
const CONTINUE_TTL_SECONDS: i64 = 30 * 60;

/// 一条待决的规则裁决
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOp {
    pub run_id: String,
    pub i: usize,
    pub tool: String,
    /// delete / write / git / command / skill / any
    pub op_type: String,
    pub path: String,
    /// 被拦调用的完整参数 JSON（续跑时原样重放该调用，保证一次性放行能被消费）
    pub args_full: String,
    /// 参数摘要（≤80 字符，给卡片展示）
    pub args: String,
    pub reason: String,
    pub err_id: String,
    /// 用户是否已处理（放行 / 拦截）
    pub resolved: bool,
    /// 放行 = true，拦截 = false（未处理时为 false，靠 resolved 区分）
    pub allowed: bool,
    pub at: i64,
}

/// 一条已签发的一次性放行
#[derive(Debug, Clone)]
struct AllowEntry {
    /// 工具名（空 = 不限工具）/ 操作类型 / 路径前缀
    tool: String,
    op_type: String,
    path_prefix: String,
}

#[derive(Debug, Default)]
struct Store {
    pending: HashMap<String, Vec<PendingOp>>,
    allows: Vec<AllowEntry>,
    /// run_id → 待继续时间戳
    continue_at: HashMap<String, i64>,
    /// run_id → 最近一次"放行"的操作（前端据此自动回复「继续」并告知模型重试哪个调用）
    last_allowed: HashMap<String, PendingOp>,
}

static STORE: Mutex<Option<Store>> = Mutex::new(None);

fn with_store<T>(f: impl FnOnce(&mut Store) -> T) -> T {
    let mut g = match STORE.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    let store = g.get_or_insert_with(Store::default);
    f(store)
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 登记一条待决记录（规则引擎拦下调用时调用）。
///
/// **去重**：模型有可能在用户尚未裁决时反复重试同一个调用。同一个
/// (工具, 完整参数) 只保留一条待决记录，否则卡片上会出现一堆一模一样的条目，
/// 而用户只需要回答一次。
#[allow(clippy::too_many_arguments)]
pub fn record(
    run_id: &str,
    i: usize,
    tool: &str,
    op_type: &str,
    path: &str,
    args_full: &str,
    reason: &str,
    err_id: &str,
) -> PendingOp {
    let existing = with_store(|s| {
        s.pending
            .get(run_id)
            .and_then(|list| {
                list.iter()
                    .find(|p| p.tool == tool && p.args_full == args_full)
                    .cloned()
            })
    });
    if let Some(op) = existing {
        crate::ai::runtime_log::info(
            "rules",
            &format!(
                "同一调用重复被拦，复用已有待决记录 run={} #{} tool={}（等待用户裁决）",
                run_id, op.i, tool
            ),
        );
        return op;
    }

    let op = PendingOp {
        run_id: run_id.to_string(),
        i,
        tool: tool.to_string(),
        op_type: op_type.to_string(),
        path: path.to_string(),
        args_full: args_full.to_string(),
        args: args_full.chars().take(80).collect(),
        reason: reason.to_string(),
        err_id: err_id.to_string(),
        resolved: false,
        allowed: false,
        at: now(),
    };
    with_store(|s| {
        let list = s.pending.entry(run_id.to_string()).or_default();
        list.push(op.clone());
        if list.len() > MAX_PENDING_PER_RUN {
            let drop_n = list.len() - MAX_PENDING_PER_RUN;
            list.drain(0..drop_n);
        }
    });
    crate::ai::runtime_log::info(
        "rules",
        &format!(
            "待决裁决登记 run={} #{} tool={} type={} path={}（等待用户在卡片上选择 ❌ 放行 / ✅ 拦截）",
            run_id, i, tool, op_type, path
        ),
    );
    op
}

/// 用户判定：❌ 放行（approved=true）/ ✅ 拦截（approved=false）
pub fn resolve(run_id: &str, i: usize, approved: bool) -> Result<PendingOp, String> {
    let out = with_store(|s| {
        let Some(list) = s.pending.get_mut(run_id) else {
            return Err(format!("run {} 没有待决记录", run_id));
        };
        let Some(op) = list.iter_mut().find(|p| p.i == i) else {
            return Err(format!("run {} 没有第 {} 条待决记录", run_id, i));
        };
        op.resolved = true;
        op.allowed = approved;
        let snapshot = op.clone();
        if approved {
            s.allows.push(AllowEntry {
                tool: snapshot.tool.clone(),
                op_type: snapshot.op_type.clone(),
                // 命令类操作不带路径语义；文件类操作按文件精确匹配
                path_prefix: if snapshot.op_type == "command" || snapshot.op_type == "any" {
                    String::new()
                } else {
                    snapshot.path.clone()
                },
            });
            if s.allows.len() > MAX_ALLOWS {
                let drop_n = s.allows.len() - MAX_ALLOWS;
                s.allows.drain(0..drop_n);
            }
            s.continue_at.insert(run_id.to_string(), now());
            s.last_allowed.insert(run_id.to_string(), snapshot.clone());
        }
        Ok(snapshot)
    })?;
    crate::ai::runtime_log::info(
        "rules",
        &format!(
            "用户裁决 run={} #{} tool={} → {}",
            run_id,
            i,
            out.tool,
            if approved {
                "❌ 放行该操作（自动回复「继续」，Agent 继续跑）"
            } else {
                "✅ 拦截该操作（Agent 换方案或先向用户确认）"
            }
        ),
    );
    Ok(out)
}

/// 路径前缀边界匹配
fn path_matches(prefix: &str, path: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let a = prefix.replace('\\', "/").to_lowercase();
    let p = path.replace('\\', "/").to_lowercase();
    p == a || p.starts_with(&format!("{}/", a.trim_end_matches('/')))
}

/// 该调用是否命中一条一次性放行；命中则**消费**该条目并返回 true
pub fn consume_allow(tool: &str, op_type: &str, path: &str) -> bool {
    with_store(|s| {
        let idx = s.allows.iter().position(|a| {
            let tool_ok = a.tool.is_empty() || a.tool.eq_ignore_ascii_case(tool);
            let ty_ok = a.op_type.is_empty() || a.op_type == "any" || a.op_type == op_type;
            tool_ok && ty_ok && path_matches(&a.path_prefix, path)
        });
        match idx {
            Some(i) => {
                s.allows.remove(i);
                true
            }
            None => false,
        }
    })
}

/// 取走「待继续」标记（有则 true，并清除）
pub fn take_continue(run_id: &str) -> bool {
    with_store(|s| {
        let hit = s
            .continue_at
            .get(run_id)
            .map(|t| now() - *t <= CONTINUE_TTL_SECONDS)
            .unwrap_or(false);
        s.continue_at.remove(run_id);
        hit
    })
}

/// 是否存在待继续标记（只读，供界面轮询）
pub fn has_continue(run_id: &str) -> bool {
    with_store(|s| {
        s.continue_at
            .get(run_id)
            .map(|t| now() - *t <= CONTINUE_TTL_SECONDS)
            .unwrap_or(false)
    })
}

/// 清理某个 run 的全部状态（Agent 循环正常收尾时调用）
pub fn clear_run(run_id: &str) {
    with_store(|s| {
        s.pending.remove(run_id);
        s.continue_at.remove(run_id);
        s.last_allowed.remove(run_id);
    });
}

/// 取走"最近一次放行"的记录（前端自动回复「继续」时读取，用于告知模型重试哪个调用）
pub fn take_last_allowed(run_id: &str) -> Option<PendingOp> {
    with_store(|s| s.last_allowed.remove(run_id))
}

/// 状态摘要（供界面显示 / 诊断）
pub fn snapshot(run_id: &str) -> serde_json::Value {
    let (pending, pending_unresolved, allows, cont) = with_store(|s| {
        let list = s.pending.get(run_id).cloned().unwrap_or_default();
        let unresolved = list.iter().filter(|p| !p.resolved).count();
        let cont = s
            .continue_at
            .get(run_id)
            .map(|t| now() - *t <= CONTINUE_TTL_SECONDS)
            .unwrap_or(false);
        (list, unresolved, s.allows.len(), cont)
    });
    serde_json::json!({
        "run_id": run_id,
        "pending": pending,
        "pending_unresolved": pending_unresolved,
        "allows": allows,
        "has_continue": cont,
    })
}

/// 该 run 的待决记录（供界面 / 诊断）
pub fn list(run_id: &str) -> Vec<PendingOp> {
    with_store(|s| s.pending.get(run_id).cloned().unwrap_or_default())
}

/// 已签发但未消费的放行条数（诊断用）
pub fn allow_count() -> usize {
    with_store(|s| s.allows.len())
}

/// 规则引擎第二次遇到同一调用时给模型的回灌文本：
/// 用户已经放行，**立即重试该调用即可通过**。
pub fn approved_retry_message(tool: &str) -> String {
    format!(
        "❌ 用户在回合裁决卡片上放行了这个操作（{}）。\
         该操作已在规则引擎登记一次性放行，**请立即用完全相同的工具与参数重试一次**，\
         这次会直接通过硬门，不需要再向用户确认。重试成功后继续完成原任务。",
        tool
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_and_consume_allow() {
        record("run_t1", 0, "write", "write", "C:/a/b.txt", "{\"file_path\":\"C:/a/b.txt\"}", "缺授权", "ERR1");
        assert!(!consume_allow("write", "write", "C:/a/b.txt"), "未放行前不应通过");

        let op = resolve("run_t1", 0, true).expect("放行应成功");
        assert!(op.allowed && op.resolved);
        assert!(op.args_full.contains("file_path"), "应保留完整参数供续跑重放");
        assert!(has_continue("run_t1"), "放行后应产生待继续标记");
        assert!(consume_allow("write", "write", "C:/a/b.txt"), "放行后应命中");
        assert!(!consume_allow("write", "write", "C:/a/b.txt"), "放行是一次性的");

        assert!(take_continue("run_t1"), "应取走待继续标记");
        assert!(!take_continue("run_t1"), "待继续标记只能取一次");
        clear_run("run_t1");
        assert!(list("run_t1").is_empty());
    }

    #[test]
    fn deny_does_not_allow() {
        record("run_t2", 1, "pwsh", "command", "", "{\"command\":\"ls\"}", "缺授权", "ERR2");
        resolve("run_t2", 1, false).expect("拦截应成功");
        assert!(!consume_allow("pwsh", "command", ""), "拦截不应放行");
        assert!(!has_continue("run_t2"), "拦截不应产生待继续标记");
        clear_run("run_t2");
    }

    #[test]
    fn path_prefix_boundary() {
        record("run_t3", 0, "write", "write", "C:/proj/src", "{}", "x", "E");
        resolve("run_t3", 0, true).unwrap();
        assert!(consume_allow("write", "write", "C:/proj/src/a.rs"));
        assert!(!consume_allow("write", "write", "C:/proj/src-other/a.rs"), "前缀必须按目录边界匹配");
        clear_run("run_t3");
    }

    #[test]
    fn command_allow_matches_any_path() {
        record("run_t4", 0, "pwsh", "command", "", "{}", "x", "E");
        resolve("run_t4", 0, true).unwrap();
        assert!(consume_allow("pwsh", "command", "whatever"));
        clear_run("run_t4");
    }

    #[test]
    fn resolve_unknown_is_error() {
        assert!(resolve("run_missing", 9, true).is_err());
    }

    /// 同一调用反复被拦 → 只保留一条待决记录（用户只需回答一次）
    #[test]
    fn duplicate_record_is_deduped() {
        let args = "{\"command\":\"rm x\"}";
        let a = record("run_t5", 0, "pwsh", "command", "", args, "缺授权", "E1");
        let b = record("run_t5", 1, "pwsh", "command", "", args, "缺授权", "E2");
        assert_eq!(a.i, b.i, "重复登记应复用同一条记录（i 不变）");
        assert_eq!(list("run_t5").len(), 1, "只应有一条待决记录");

        // 不同参数则是两条
        let c = record("run_t5", 1, "pwsh", "command", "", "{\"command\":\"rm y\"}", "缺授权", "E3");
        assert_eq!(c.i, 1);
        assert_eq!(list("run_t5").len(), 2);
        clear_run("run_t5");
    }
}

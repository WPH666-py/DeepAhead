use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// 一次文件变更的撤销记录（变更前状态）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndoEntry {
    /// 变更文件的绝对路径
    pub path: String,
    /// 变更前文件是否存在
    pub existed: bool,
    /// 变更前文件内容（不存在时为空 Vec）
    pub original: Vec<u8>,
}

/// 撤销日志存储（按 run_id 分组，会话窗口内存活）
#[derive(Clone)]
pub struct UndoStore {
    map: Arc<Mutex<HashMap<String, Vec<UndoEntry>>>>,
}

impl UndoStore {
    pub fn new() -> Self {
        Self { map: Arc::new(Mutex::new(HashMap::new())) }
    }

    /// 记录一条撤销条目
    pub fn record(&self, run_id: &str, entry: UndoEntry) {
        if let Ok(mut map) = self.map.lock() {
            map.entry(run_id.to_string()).or_default().push(entry);
        }
    }

    /// 取走某个 run 的所有撤销记录（取走后清空，防止重复撤销）
    pub fn take(&self, run_id: &str) -> Vec<UndoEntry> {
        if let Ok(mut map) = self.map.lock() {
            map.remove(run_id).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    /// 某个 run 的撤销记录数量
    pub fn count(&self, run_id: &str) -> usize {
        if let Ok(map) = self.map.lock() {
            map.get(run_id).map(|v| v.len()).unwrap_or(0)
        } else {
            0
        }
    }
}

/// 按撤销记录恢复文件（倒序执行，等价于回滚这一轮的全部文件变更）
/// 返回值：恢复/删除的文件路径列表
pub fn apply_undo(entries: &[UndoEntry]) -> Vec<String> {
    let mut applied = Vec::new();
    for e in entries.iter().rev() {
        let path = std::path::Path::new(&e.path);
        if e.existed {
            if std::fs::write(path, &e.original).is_ok() {
                applied.push(format!("已恢复 {}", e.path));
            } else {
                applied.push(format!("恢复失败(可能被占用) {}", e.path));
            }
        } else {
            let _ = std::fs::remove_file(path);
            applied.push(format!("已删除新增文件 {}", e.path));
        }
    }
    applied
}

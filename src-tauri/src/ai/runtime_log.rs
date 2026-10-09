//! ─── 运行时日志（实时落盘，用户随时可查）───
//!
//! 目标：AI 驾驶舱里的「📋 日志」面板看到的每一条记录，都**实时**写入用户本机
//! 安装目录下的磁盘文件，用户不必打开面板也能随时查看、复现、报障。
//!
//! 落盘位置（Windows）：
//!   `%LOCALAPPDATA%\DeepAhead\logs\deepahead-YYYY-MM-DD.log`
//! 即 `C:\Users\<用户>\AppData\Local\DeepAhead\logs\`；macOS / Linux 走
//! `dirs_next::data_dir()` 的对应目录。可用环境变量 `DEEPAHEAD_LOG_DIR` 覆盖。
//!
//! 设计约束：
//!   - **写入失败绝不拖垮主流程**：所有 IO 错误都被吞掉（日志是旁路，不是依赖）；
//!   - **跨线程安全**：一把全局 `Mutex` 串行化写入，保证行不撕裂；
//!   - **单文件 8MB 上限**：超限时轮转为 `.log.1`（只保留一个历史文件）；
//!   - 前端 `appendLog` 与后端 agent loop 共用同一个文件，时间线自然对齐。

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// 单个日志文件的体积上限（超过后轮转为 .log.1）
const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// 日志根目录：用户本机数据目录下的 `DeepAhead/logs`
pub fn log_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DEEPAHEAD_LOG_DIR") {
        if !d.trim().is_empty() {
            return PathBuf::from(d);
        }
    }
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DeepAhead")
        .join("logs")
}

/// 当天日志文件（按本地日期切分，跨天自动新建）
pub fn log_path() -> PathBuf {
    let day = chrono::Local::now().format("%Y-%m-%d").to_string();
    log_dir().join(format!("deepahead-{}.log", day))
}

/// 时间戳前缀（本地时间，便于用户肉眼比对操作时刻）
fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// 单行内的换行折叠：一条日志必须占一行，否则时间线会被撑断
fn one_line(s: &str) -> String {
    s.replace("\r\n", " \\n ").replace('\n', " \\n ").replace('\r', " \\n ")
}

/// 写入一条日志。`level` 建议 info / warn / error；`scope` 建议 frontend /
/// agent / rules / approval / tools / app。
pub fn write(level: &str, scope: &str, message: &str) {
    write_in(&log_path(), level, scope, message)
}

/// 写入到指定文件（测试与多 profile 隔离用；生产走 `write`）。
fn write_in(path: &std::path::Path, level: &str, scope: &str, message: &str) {
    let _g = match WRITE_LOCK.lock() {
        Ok(g) => g,
        Err(_) => return, // 锁中毒：宁可丢日志也不 panic
    };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    // 轮转：超过上限时把当前文件挪成 .log.1（覆盖上一份历史）
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > MAX_FILE_BYTES {
            let mut old = path.to_path_buf().into_os_string();
            old.push(".1");
            let _ = std::fs::rename(path, PathBuf::from(old));
        }
    }
    let line = format!(
        "[{}] [{}] [{}] {}\n",
        stamp(),
        level.to_uppercase(),
        scope,
        one_line(message)
    );
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn info(scope: &str, message: &str) {
    write("info", scope, message);
}

pub fn warn(scope: &str, message: &str) {
    write("warn", scope, message);
}

pub fn error(scope: &str, message: &str) {
    write("error", scope, message);
}

/// 读取最近 N 行（给界面"查看磁盘日志"用；文件不存在返回空串）
pub fn tail(n: usize) -> String {
    tail_in(&log_path(), n)
}

fn tail_in(path: &std::path::Path, n: usize) -> String {
    let Ok(text) = std::fs::read_to_string(path) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// 日志文件信息（路径 / 大小 / 是否存在），供界面展示
pub fn status() -> serde_json::Value {
    let path = log_path();
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    serde_json::json!({
        "dir": log_dir().to_string_lossy().to_string(),
        "file": path.to_string_lossy().to_string(),
        "exists": path.exists(),
        "size": size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 写入/读取必须走**显式路径**：日志是全局单文件，用环境变量改目录会和
    /// 其它并行测试互相污染（曾经因此假失败）。
    #[test]
    fn writes_and_reads_back() {
        let tmp = std::env::temp_dir().join(format!("deepahead-log-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let file = tmp.join("deepahead-test.log");

        write_in(&file, "info", "test", "第一行\n第二行");
        write_in(&file, "info", "test", "带中文的消息");

        let text = tail_in(&file, 10);
        assert!(text.contains("第一行 \\n 第二行"), "换行应被折叠为一行: {}", text);
        assert!(text.contains("带中文的消息"));
        assert_eq!(text.lines().count(), 2, "两条日志应占两行: {}", text);
        assert!(text.contains("[INFO] [test]"), "应带级别与 scope: {}", text);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 落盘目录必须在用户本机数据目录下（安装目录），且可被环境变量覆盖
    #[test]
    fn log_dir_is_under_user_data_dir() {
        let d = log_dir();
        assert!(d.ends_with("logs"), "日志目录应以 logs 结尾: {}", d.display());
        let st = status();
        assert!(st["file"].as_str().unwrap_or("").contains("deepahead-"));
    }
}

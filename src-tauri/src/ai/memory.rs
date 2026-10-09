//! ─── 长期记忆协议（移植自 baaai123/dsh-memory-protocol）───
//!
//! 原插件是 DSH host-only 的"记忆强制协议"：每轮先查记忆再动手，轮末归档本轮内容。
//! 它自身不存任何数据（数据在 Python `memory-skill` MCP 服务里），插件只负责三件事：
//!   1. `tools/pre-execute` 硬门：未 weave 就调用非记忆工具 → deny，并给出补救指引
//!   2. `agent/pre-step`   轮首自动 weave，把检索结果注入上下文
//!   3. `agent/turn-stopping` 轮末自动 ingest 本轮缓冲
//!
//! 这里做**无 MCP、无 Python、无外部依赖**的最小可用移植：
//!   - 存储：每用户固定路径下的 JSONL（原插件特意修正了 cwd 漂移问题，这里沿用固定路径）
//!   - 检索：BM25 + 字符二元组（对应上游在无 embedding 时的 `embedder.mode=fallback`）
//!   - 门：按轮次的前置门 + **失败开放**（后端不可用时不阻塞，只提示）
//!
//! 保真要点（与上游一致）：
//!   - 记忆工具自身永远放行（否则死锁）
//!   - 拒绝发生在工具执行**之前**，且拒绝理由写明补救动作
//!   - 检索"成功但为空"同样满足门；"失败"不满足门
//!   - 门是**每轮**（不是每会话）重置
//!   - 截断上限：查询 2000 / 注入 8000 / 归档 4000 字符

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// 常量（对齐上游）
pub const MAX_QUERY_CHARS: usize = 2000;
pub const MAX_INJECT_CHARS: usize = 8000;
pub const MAX_INGEST_CHARS: usize = 4000;
pub const DEFAULT_TOP_K: usize = 8;

/// 记忆条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    /// 分类：turn / note / skill / conclusion
    pub kind: String,
    pub text: String,
    /// Unix 秒
    pub created_at: i64,
    /// 来源会话
    pub session_id: String,
    /// 来源角色（user / assistant）
    pub role: String,
}

/// 记忆配置（对齐上游配置项）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// 总开关
    pub enabled: bool,
    /// 硬门：未 weave 时是否拒绝非记忆工具
    pub enforce_weave: bool,
    /// 轮首自动 weave 并注入上下文
    pub inject_weave: bool,
    /// 轮末自动归档本轮内容
    pub auto_ingest: bool,
    /// 免检工具名（完整工具名）
    pub allowlist: Vec<String>,
    /// 后端不可用时失败开放（true = 放行 + 一次性提示；false = 拒绝）
    pub fail_open: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        // 与上游默认值一致：enforceWeave=true, injectWeave=true, autoIngest=true, bootFailOpen=true
        Self {
            enabled: true,
            enforce_weave: true,
            inject_weave: true,
            auto_ingest: true,
            allowlist: vec![],
            fail_open: true,
        }
    }
}

/// 记忆工具名（自身永远放行，避免死锁）
pub const MEMORY_TOOLS: [&str; 4] = ["memory_weave", "memory_ingest", "memory_search", "memory_status"];

pub fn is_memory_tool(name: &str) -> bool {
    MEMORY_TOOLS.contains(&name)
}

/// 轮首注入的介绍语（对齐上游 activation card 的语义）
pub const MEMORY_INTRO: &str = "\
[长期记忆协议] 本轮已先查阅长期记忆。以下是检索到的相关记忆；
请把它们当作既有事实与约束使用，不要重复询问用户已知信息。";

/// 未 weave 时的拒绝理由（对齐上游 remediation 文案）
pub const MEMORY_DENY_REASON: &str = "\
Memory protocol: 每轮必须先 consult 长期记忆再动手。
请先调用 memory_weave（传入 user_message），然后再重试本次工具调用。";

// ════════════════════════════════════════════════════════
// 存储
// ════════════════════════════════════════════════════════

/// 每用户固定存储目录（对齐上游"固定路径、不随 cwd 漂移"的修正）
pub fn memory_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("DEEPAHEAD_MEMORY_DIR") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    let base = dirs_next::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("DeepAhead").join("memory")
}

fn memory_file() -> PathBuf {
    memory_dir().join("memory.jsonl")
}

static STORE_LOCK: Mutex<()> = Mutex::new(());

fn load_records() -> Vec<MemoryRecord> {
    let path = memory_file();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<MemoryRecord>(l).ok())
        .collect()
}

fn save_records(records: &[MemoryRecord]) -> Result<(), String> {
    let dir = memory_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建记忆目录失败: {}", e))?;
    let mut buf = String::new();
    for r in records {
        let line = serde_json::to_string(r).map_err(|e| format!("序列化记忆失败: {}", e))?;
        buf.push_str(&line);
        buf.push('\n');
    }
    std::fs::write(memory_file(), buf).map_err(|e| format!("写入记忆失败: {}", e))
}

// ════════════════════════════════════════════════════════
// 检索：BM25 + 字符二元组（CJK 友好）
// ════════════════════════════════════════════════════════

/// 分词：ASCII 按空白/标点切词并小写；CJK 生成字符二元组。
/// 无 embedding 时的 fallback 检索（对齐上游 embedder fallback 模式）。
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut ascii_word = String::new();
    let mut cjk_run: Vec<char> = Vec::new();

    let flush_ascii = |w: &mut String, out: &mut Vec<String>| {
        if !w.is_empty() {
            out.push(std::mem::take(w));
        }
    };
    let flush_cjk = |run: &mut Vec<char>, out: &mut Vec<String>| {
        if run.len() == 1 {
            out.push(run[0].to_string());
        } else {
            for w in run.windows(2) {
                out.push(format!("{}{}", w[0], w[1]));
            }
        }
        run.clear();
    };

    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            flush_cjk(&mut cjk_run, &mut tokens);
            ascii_word.push(ch.to_ascii_lowercase());
        } else if is_cjk(ch) {
            flush_ascii(&mut ascii_word, &mut tokens);
            cjk_run.push(ch);
        } else {
            flush_ascii(&mut ascii_word, &mut tokens);
            flush_cjk(&mut cjk_run, &mut tokens);
        }
    }
    flush_ascii(&mut ascii_word, &mut tokens);
    flush_cjk(&mut cjk_run, &mut tokens);
    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

/// BM25 打分（k1=1.2, b=0.75 标准取值）+ 时间近因加权
fn bm25_scores(query: &str, records: &[MemoryRecord]) -> Vec<(usize, f64)> {
    const K1: f64 = 1.2;
    const B: f64 = 0.75;

    let q_tokens = tokenize(query);
    if q_tokens.is_empty() || records.is_empty() {
        return vec![];
    }
    let docs: Vec<Vec<String>> = records.iter().map(|r| tokenize(&r.text)).collect();
    let avg_len = docs.iter().map(|d| d.len()).sum::<usize>() as f64 / docs.len().max(1) as f64;

    // 文档频率
    let mut df: HashMap<&str, usize> = HashMap::new();
    for d in &docs {
        let mut seen: Vec<&str> = d.iter().map(|s| s.as_str()).collect();
        seen.sort_unstable();
        seen.dedup();
        for t in seen {
            *df.entry(t).or_insert(0) += 1;
        }
    }
    let n = docs.len() as f64;
    let now = chrono::Utc::now().timestamp();

    let mut scored: Vec<(usize, f64)> = Vec::new();
    for (i, d) in docs.iter().enumerate() {
        let dl = d.len() as f64;
        let mut score = 0.0f64;
        for qt in &q_tokens {
            let f = d.iter().filter(|t| *t == qt).count() as f64;
            if f == 0.0 {
                continue;
            }
            let dfi = *df.get(qt.as_str()).unwrap_or(&0) as f64;
            let idf = ((n - dfi + 0.5) / (dfi + 0.5) + 1.0).ln();
            let denom = f + K1 * (1.0 - B + B * dl / avg_len.max(1.0));
            score += idf * (f * (K1 + 1.0)) / denom;
        }
        if score > 0.0 {
            // 近因加权：30 天半衰
            let age_days = ((now - records[i].created_at).max(0) as f64) / 86400.0;
            let recency = 0.5f64.powf(age_days / 30.0);
            score *= 0.7 + 0.3 * recency;
            scored.push((i, score));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

// ════════════════════════════════════════════════════════
// 公开操作
// ════════════════════════════════════════════════════════

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{}…", cut)
}

fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("mem_{}_{}", nanos, std::process::id())
}

/// 轮首检索（weave）：返回注入用的记忆上下文文本。
/// 「成功但为空」也返回 Ok（满足门），与上游一致。
pub fn weave(query: &str, session_id: &str, top_k: Option<usize>) -> Result<String, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "记忆存储锁中毒".to_string())?;
    let records = load_records();
    if records.is_empty() {
        return Ok(String::new());
    }
    let q = truncate_chars(query, MAX_QUERY_CHARS);
    let k = top_k.unwrap_or(DEFAULT_TOP_K);

    let mut picked: Vec<(usize, f64)> = bm25_scores(&q, &records);
    // 无命中时回退到最近记忆，保证"先查记忆"这一步有实际内容
    if picked.is_empty() {
        let mut idx: Vec<usize> = (0..records.len()).collect();
        idx.sort_by_key(|i| -records[*i].created_at);
        picked = idx.into_iter().take(k).map(|i| (i, 0.0)).collect();
    }
    picked.truncate(k);

    let mut out = String::new();
    for (i, _) in picked {
        let r = &records[i];
        let when = chrono::DateTime::from_timestamp(r.created_at, 0)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        out.push_str(&format!("- [{} · {} · {}] {}\n", r.kind, when, r.role, r.text));
    }
    let _ = session_id;
    Ok(truncate_chars(&out, MAX_INJECT_CHARS))
}

/// 归档一条记忆（ingest）
pub fn ingest(content: &str, role: &str, session_id: &str, kind: Option<&str>) -> Result<MemoryRecord, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("内容为空，未写入记忆".to_string());
    }
    let _guard = STORE_LOCK.lock().map_err(|_| "记忆存储锁中毒".to_string())?;
    let mut records = load_records();
    let rec = MemoryRecord {
        id: new_id(),
        kind: kind.unwrap_or("turn").to_string(),
        text: truncate_chars(trimmed, MAX_INGEST_CHARS),
        created_at: chrono::Utc::now().timestamp(),
        session_id: session_id.to_string(),
        role: role.to_string(),
    };
    records.push(rec.clone());
    // 保留最近 5000 条，避免无限增长
    if records.len() > 5000 {
        let drop_n = records.len() - 5000;
        records.drain(0..drop_n);
    }
    save_records(&records)?;
    Ok(rec)
}

/// 检索记忆（供模型/界面搜索）
pub fn search(query: &str, limit: Option<usize>) -> Result<Vec<(MemoryRecord, f64)>, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "记忆存储锁中毒".to_string())?;
    let records = load_records();
    let q = truncate_chars(query, MAX_QUERY_CHARS);
    let k = limit.unwrap_or(20);
    let scored = bm25_scores(&q, &records);
    Ok(scored
        .into_iter()
        .take(k)
        .map(|(i, s)| (records[i].clone(), s))
        .collect())
}

/// 记忆统计
pub fn status() -> serde_json::Value {
    let records = load_records().len();
    serde_json::json!({
        "records": records,
        "dir": memory_dir().to_string_lossy().to_string(),
        "file": memory_file().to_string_lossy().to_string(),
        "available": true,
    })
}

/// 清空全部记忆
pub fn clear() -> Result<usize, String> {
    let _guard = STORE_LOCK.lock().map_err(|_| "记忆存储锁中毒".to_string())?;
    let n = load_records().len();
    save_records(&[])?;
    Ok(n)
}

/// 最近的记忆（供界面展示）
pub fn recent(limit: Option<usize>) -> Vec<MemoryRecord> {
    let mut records = load_records();
    records.sort_by_key(|r| -r.created_at);
    records.truncate(limit.unwrap_or(50));
    records
}

// ════════════════════════════════════════════════════════
// 全局配置（进程内，供 IPC 与 agent loop 共享）
// ════════════════════════════════════════════════════════

static MEMORY_CONFIG: std::sync::OnceLock<Mutex<MemoryConfig>> = std::sync::OnceLock::new();

fn config_lock() -> &'static Mutex<MemoryConfig> {
    MEMORY_CONFIG.get_or_init(|| Mutex::new(MemoryConfig::default()))
}

/// 读取当前记忆配置
pub fn get_config() -> MemoryConfig {
    config_lock().lock().map(|c| c.clone()).unwrap_or_default()
}

/// 更新记忆配置，返回更新后的值
pub fn set_config(cfg: MemoryConfig) -> MemoryConfig {
    if let Ok(mut guard) = config_lock().lock() {
        *guard = cfg.clone();
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_handles_cjk_bigrams_and_ascii_words() {
        let t = tokenize("Hello 世界 memory_protocol");
        assert!(t.contains(&"hello".to_string()));
        assert!(t.contains(&"memory_protocol".to_string()));
        // "世界" → 单个二元组
        assert!(t.contains(&"世界".to_string()));
    }

    #[test]
    fn bm25_ranks_matching_doc_first() {
        let now = chrono::Utc::now().timestamp();
        let recs = vec![
            MemoryRecord { id: "1".into(), kind: "turn".into(), text: "用户偏好使用 pnpm 而不是 npm".into(), created_at: now, session_id: "s".into(), role: "user".into() },
            MemoryRecord { id: "2".into(), kind: "turn".into(), text: "今天天气不错，出门散步".into(), created_at: now, session_id: "s".into(), role: "user".into() },
        ];
        let scored = bm25_scores("pnpm 包管理器", &recs);
        assert!(!scored.is_empty());
        assert_eq!(scored[0].0, 0, "应命中第 1 条（含 pnpm）");
    }

    #[test]
    fn truncation_caps() {
        let long = "字".repeat(MAX_QUERY_CHARS + 500);
        let out = truncate_chars(&long, MAX_QUERY_CHARS);
        assert_eq!(out.chars().count(), MAX_QUERY_CHARS + 1); // +1 是省略号
    }

    #[test]
    fn memory_tools_are_exempt() {
        assert!(is_memory_tool("memory_weave"));
        assert!(is_memory_tool("memory_search"));
        assert!(!is_memory_tool("write"));
        assert!(!is_memory_tool("bash"));
    }
}

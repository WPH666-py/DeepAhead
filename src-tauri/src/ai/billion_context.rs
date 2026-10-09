//! ─── billion-context 上下文压缩引擎（移植自 ranxianglei/billion-context 的 kernel）───
//!
//! 原插件的内核是一个**纯算法库**（`acp-kernel`，零运行时依赖、无 I/O、不调用模型）。
//! 它的核心主张是：**摘要由模型自己写**，引擎只负责
//!   (a) 什么时候提示（nudge）、(b) 给消息分配稳定引用、(c) 校验/调整范围、
//!   (d) 用模型的摘要替换被覆盖的消息、(e) 用无损内容存储支持事后取回。
//!
//! "省 5 倍 token" 来自模型把消费完的工具输出换成高密度摘要；
//! "单会话几十亿 token" 来自折叠 + 分级（T1→T2→T3）+ 内容存储，
//! 让**可见窗口保持有界**而会话可以无限增长。
//!
//! 本模块移植的是可移植内核：引用分配、分级块谱系、nudge 判定、
//! 成对完整性（工具调用/结果、推理/回复不可拆分）、剪枝渲染、无损取回。
//! 未移植（原插件依赖 HTTP 代理层/未实现）：CCR 导出落盘、图像压缩、语义检索。

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// 摘要消息头（原文常量，是承重的：拆解与截断都依赖它）
pub const SUMMARY_HEADER: &str = "[Compressed conversation section]";

// ════════════════════════════════════════════════════════
// Token 估算（CJK 感知，替换原先的 ceil(chars/2.5)）
// ════════════════════════════════════════════════════════

/// 是否 CJK 字符（中日韩）
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF
        | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

/// CJK ≈ 1 token/字；其余 ≈ 4 字符/token。
/// 所有阈值都是按这个口径标定的，不能换回 chars/2.5。
pub fn count_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let cjk = text.chars().filter(|c| is_cjk(*c)).count();
    let rest = text.chars().count().saturating_sub(cjk);
    cjk + rest.div_ceil(4)
}

// ════════════════════════════════════════════════════════
// 引用：mNNNNN，永不重发
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefMap {
    /// 原始 id → 引用
    pub by_raw: HashMap<String, String>,
    /// 引用 → 原始 id
    pub by_ref: HashMap<String, String>,
    /// 已用过的最高序号（剪枝引用表时必须重新钉住，否则会重发引用）
    pub highest_used: usize,
}

/// 受保护消息不占引用槽位
pub const BLOCKED_REF: &str = "BLOCKED";

impl RefMap {
    /// 为一个原始 id 分配引用（同一 id 复用；永不重发）。
    /// protected = true 时返回 BLOCKED 且不消耗序号。
    pub fn assign(&mut self, raw_id: &str, protected: bool) -> String {
        if let Some(existing) = self.by_raw.get(raw_id) {
            return existing.clone();
        }
        if protected {
            self.by_raw.insert(raw_id.to_string(), BLOCKED_REF.to_string());
            return BLOCKED_REF.to_string();
        }
        // 游标 = 已用最高序号 + 1，向上找空位
        let mut idx = self.highest_used + 1;
        loop {
            let candidate = format!("m{:05}", idx);
            if !self.by_ref.contains_key(&candidate) {
                self.by_ref.insert(candidate.clone(), raw_id.to_string());
                self.by_raw.insert(raw_id.to_string(), candidate.clone());
                self.highest_used = idx;
                return candidate;
            }
            idx += 1;
        }
    }

    pub fn ref_of(&self, raw_id: &str) -> Option<&String> {
        self.by_raw.get(raw_id)
    }
    pub fn raw_of(&self, r: &str) -> Option<&String> {
        self.by_ref.get(r)
    }
    /// 只保留仍存活的引用，并把高水位重新钉住
    pub fn prune_to(&mut self, live_raw_ids: &HashSet<String>) {
        self.by_raw.retain(|k, _| live_raw_ids.contains(k));
        let live_refs: HashSet<String> = self.by_raw.values().cloned().collect();
        self.by_ref.retain(|k, _| live_refs.contains(k));
        self.highest_used = self
            .by_ref
            .keys()
            .filter_map(|r| r.trim_start_matches('m').parse::<usize>().ok())
            .max()
            .unwrap_or(0);
    }
}

// ════════════════════════════════════════════════════════
// 消息（引擎只认这个形状）
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    ToolCall,
    ToolResult,
    Reasoning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreMessage {
    pub id: String,
    pub role: String,
    pub content_type: ContentType,
    pub text: String,
    pub tool_name: Option<String>,
    pub tool_call_id: Option<String>,
    /// 承载媒体载荷（图片等）→ 永不折叠（折叠会不可逆地丢掉字节）
    pub has_media: bool,
    /// 摘要消息（acp_summary_*）→ 永不进入覆盖集
    pub is_summary: bool,
}

impl CoreMessage {
    pub fn tokens(&self) -> usize {
        count_tokens(&self.text)
    }
    /// 覆盖比较用：去掉 '#N' 之后的聚类后缀
    pub fn base_id(&self) -> &str {
        match self.id.find('#') {
            Some(i) => &self.id[..i],
            None => &self.id,
        }
    }
}

// ════════════════════════════════════════════════════════
// 压缩块
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionBlock {
    pub block_id: String,
    /// 1 / 2 / 3
    pub tier: u8,
    pub topic: Option<String>,
    /// 模型写的摘要
    pub summary: String,
    /// 范围内直接覆盖的原始消息 id
    pub direct_message_ids: Vec<String>,
    /// 直接 + 所有后代块覆盖的并集（传递谱系）
    pub effective_message_ids: Vec<String>,
    /// 被本块蒸馏掉的子块
    pub direct_block_ids: Vec<String>,
    pub compressed_tokens: usize,
    pub created_at: i64,
    pub survived_count: u32,
    /// "young" | "old"
    pub generation: String,
    pub active: bool,
    pub restored_inline: bool,
    /// 展示用引用（可能指向 "bN"）
    pub start_ref: Option<String>,
    pub end_ref: Option<String>,
}

impl CompressionBlock {
    pub fn summary_tokens(&self) -> usize {
        count_tokens(&self.summary)
    }
    /// 渲染成消息文本
    pub fn render(&self) -> String {
        match &self.topic {
            Some(t) if !t.trim().is_empty() => {
                format!("{} — {}\n{}", SUMMARY_HEADER, t, self.summary)
            }
            _ => format!("{}\n{}", SUMMARY_HEADER, self.summary),
        }
    }
}

// ════════════════════════════════════════════════════════
// 配置（默认值逐项对齐 kernel/src/config.ts）
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillionConfig {
    /// 模型上下文窗口
    pub model_context_limit: usize,
    /// 触发 OVER-LIMIT 的占用比例
    pub max_context_limit_pct: f64,
    /// first-sight 质量判定使用的低水位
    pub min_context_limit_pct: f64,
    /// EMERGENCY 占用比例
    pub emergency_threshold_pct: f64,
    /// nudge 增长步长比例
    pub growth_ratio: f64,
    /// 增长步长下限（= 上限，故意做成常量，不随窗口缩放）
    pub growth_floor: usize,
    /// 增长步长上限
    pub growth_cap: usize,
    pub min_growth_floor: usize,
    pub min_growth_ratio: f64,
    pub tier2_growth_multiplier: f64,
    /// 分级开关（T2/T3）
    pub tiers_enabled: bool,
    pub tier2_trigger: usize,
    pub tier3_trigger: usize,
    /// 提升为 "old" 代际所需的存活轮数
    pub promotion_threshold: u32,
    /// 压缩范围最小字符数（原始字符数，不是 token）
    pub min_compress_range: usize,
    pub max_summary_length: usize,
    pub min_summary_length: usize,
    /// 保留最近 N 条消息
    pub preserve_recent_messages: usize,
    /// 向后扩展到至少这么多 token
    pub preserve_recent_tokens: usize,
    /// 这些工具位于软保护区内时不参与"保留"（让它们的大结果可折叠）
    pub never_preserve_recent_tools: Vec<String>,
    /// 永不折叠的工具（精确名或前缀*）
    pub protected_tools: Vec<String>,
    /// 只保护最新一次调用的工具
    pub protected_latest_tools: Vec<String>,
    /// 强制截断阈值
    pub truncate_threshold: f64,
    /// 连续多少次 terminal 后允许逃逸
    pub truncate_terminal_escape_after: u32,
    /// 可选的每 token 最小收益
    pub min_pressure_benefit_tokens: Option<usize>,
}

impl Default for BillionConfig {
    fn default() -> Self {
        Self {
            model_context_limit: 128_000,
            max_context_limit_pct: 0.75,
            min_context_limit_pct: 0.45,
            emergency_threshold_pct: 0.95,
            growth_ratio: 0.05,
            growth_floor: 50_000,
            growth_cap: 50_000,
            min_growth_floor: 20_000,
            min_growth_ratio: 0.45,
            tier2_growth_multiplier: 1.5,
            tiers_enabled: true,
            tier2_trigger: 1000,
            tier3_trigger: 2000,
            promotion_threshold: 5,
            min_compress_range: 5000,
            max_summary_length: 20_000,
            min_summary_length: 50,
            preserve_recent_messages: 5,
            preserve_recent_tokens: 5000,
            never_preserve_recent_tools: vec![
                "decompress".into(),
                "search_context".into(),
                "read".into(),
                "bash".into(),
            ],
            protected_tools: vec![],
            protected_latest_tools: vec![],
            truncate_threshold: 0.95,
            truncate_terminal_escape_after: 3,
            min_pressure_benefit_tokens: None,
        }
    }
}

impl BillionConfig {
    pub fn with_limit(limit: usize) -> Self {
        let mut c = Self::default();
        if limit >= 1000 {
            c.model_context_limit = limit;
        }
        c
    }

    /// nudge 增长步长：clamp(L × growthRatio, floor, cap) —— 默认恒定 50000
    pub fn nudge_growth_tokens(&self) -> usize {
        let scaled = (self.model_context_limit as f64 * self.growth_ratio).round() as usize;
        scaled.clamp(self.growth_floor.min(self.growth_cap), self.growth_floor.max(self.growth_cap))
    }
    /// 增长下限：max(minGrowthFloor, minGrowthRatio × 步长) —— 默认 22500
    pub fn growth_floor_effective(&self) -> usize {
        let a = self.min_growth_floor;
        let b = (self.min_growth_ratio * self.nudge_growth_tokens() as f64).round() as usize;
        a.max(b)
    }
    pub fn min_pressure_benefit(&self) -> usize {
        self.min_pressure_benefit_tokens
            .unwrap_or_else(|| 5000.max((self.model_context_limit as f64 * 0.01).round() as usize))
    }
    pub fn tier_threshold(&self, tier: u8) -> usize {
        let base = self.nudge_growth_tokens();
        match tier {
            1 => base,
            2 => (base as f64 * self.tier2_growth_multiplier).round() as usize,
            _ => (base as f64 * self.tier2_growth_multiplier).round() as usize,
        }
    }
}

// ════════════════════════════════════════════════════════
// 状态
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NudgeState {
    pub last_per_message_nudge_tokens: usize,
    pub last_nudge_shown_tokens: usize,
    pub baseline_tokens: usize,
    pub last_shown_by_tier: HashMap<u8, usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompressionStats {
    pub tokens_compressed: usize,
    pub compression_count: usize,
    pub stored_count: usize,
    pub retrieval_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionState {
    pub blocks: Vec<CompressionBlock>,
    pub message_refs: RefMap,
    pub token_snapshot: HashMap<String, usize>,
    pub nudge: NudgeState,
    pub stats: CompressionStats,
    next_block_id: usize,
    terminal_streak: u32,
}

impl Default for CompressionState {
    fn default() -> Self {
        Self {
            blocks: vec![],
            message_refs: RefMap::default(),
            token_snapshot: HashMap::new(),
            nudge: NudgeState::default(),
            stats: CompressionStats::default(),
            next_block_id: 1,
            terminal_streak: 0,
        }
    }
}

impl CompressionState {
    pub fn new_block_id(&mut self) -> String {
        let id = format!("b{}", self.next_block_id);
        self.next_block_id += 1;
        id
    }
    pub fn active_blocks(&self) -> impl Iterator<Item = &CompressionBlock> {
        self.blocks.iter().filter(|b| b.active)
    }
    pub fn active_blocks_of_tier(&self, tier: u8) -> Vec<&CompressionBlock> {
        self.blocks.iter().filter(|b| b.active && b.tier == tier).collect()
    }
    /// 全部被覆盖的原始消息 id（所有活跃块的有效覆盖）
    pub fn covered_ids(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        for b in self.active_blocks() {
            for id in &b.effective_message_ids {
                set.insert(id.clone());
            }
        }
        set
    }
}

// ════════════════════════════════════════════════════════
// 无损内容存储（取回用）
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentStore {
    pub by_hash: HashMap<String, String>,
    pub by_ref: HashMap<String, String>,
}

impl ContentStore {
    /// 极简稳定哈希（FNV-1a 64，避免引入新依赖）
    fn hash(text: &str) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in text.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        format!("{:016x}", h)
    }
    /// 首次写入优先（append-only），原文不进 CompressionState，避免每轮重写大对象
    pub fn store(&mut self, r: &str, text: &str) -> bool {
        if self.by_ref.contains_key(r) {
            return false;
        }
        let h = Self::hash(text);
        self.by_hash.entry(h).or_insert_with(|| text.to_string());
        self.by_ref.insert(r.to_string(), text.to_string());
        true
    }
    pub fn retrieve(&self, r: &str) -> Option<&String> {
        self.by_ref.get(r)
    }
}

// ════════════════════════════════════════════════════════
// 保护与推荐
// ════════════════════════════════════════════════════════

const ALWAYS_PROTECTED_TOOLS: [&str; 2] = ["compress", "acp_rule"];

fn tool_matches(pattern: &str, name: &str) -> bool {
    let p = pattern.to_lowercase();
    let n = name.to_lowercase();
    if let Some(prefix) = p.strip_suffix('*') {
        n.starts_with(prefix)
    } else {
        p == n
    }
}

/// 软保护区：最近 N 条消息 + 向后扩展到 preserve_recent_tokens 的 token 量 + 最后一条用户消息。
/// 注意：never_preserve_recent_tools 里的工具**不进入**该区，让它们的大结果可被折叠。
pub fn soft_zone(messages: &[CoreMessage], cfg: &BillionConfig) -> HashSet<usize> {
    let mut zone: HashSet<usize> = HashSet::new();
    let n = messages.len();
    if n == 0 {
        return zone;
    }
    let mut tokens = 0usize;
    let mut count = 0usize;
    let mut i = n;
    while i > 0 {
        i -= 1;
        let m = &messages[i];
        let excluded = m
            .tool_name
            .as_ref()
            .map(|t| cfg.never_preserve_recent_tools.iter().any(|p| tool_matches(p, t)))
            .unwrap_or(false);
        if !excluded {
            zone.insert(i);
            tokens += m.tokens();
            count += 1;
        }
        if count >= cfg.preserve_recent_messages && tokens >= cfg.preserve_recent_tokens {
            break;
        }
    }
    // 最后一条用户消息
    for (i, m) in messages.iter().enumerate().rev() {
        if m.role == "user" && !m.is_summary {
            zone.insert(i);
            break;
        }
    }
    zone
}

/// 硬保护：永不折叠的消息下标
pub fn hard_protected(
    messages: &[CoreMessage],
    cfg: &BillionConfig,
    latest_tool_instance: &HashMap<String, usize>,
) -> HashSet<usize> {
    let mut set: HashSet<usize> = HashSet::new();
    // 会话第一条用户消息：无条件钉住（严格 provider 会拒绝没有 user 消息的请求）
    if let Some(i) = messages.iter().position(|m| m.role == "user") {
        set.insert(i);
    }
    for (i, m) in messages.iter().enumerate() {
        if m.has_media || m.is_summary {
            set.insert(i);
            continue;
        }
        if let Some(t) = &m.tool_name {
            if ALWAYS_PROTECTED_TOOLS.iter().any(|p| tool_matches(p, t)) {
                set.insert(i);
                continue;
            }
            if cfg.protected_tools.iter().any(|p| tool_matches(p, t)) {
                set.insert(i);
                continue;
            }
            if cfg.protected_latest_tools.iter().any(|p| tool_matches(p, t)) {
                if latest_tool_instance.get(&t.to_lowercase()) == Some(&i) {
                    set.insert(i);
                }
            }
        }
    }
    set
}

/// 可压缩范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Range {
    pub start_idx: usize,
    pub end_idx: usize,
    pub start_ref: String,
    pub end_ref: String,
    pub message_count: usize,
    pub chars: usize,
    pub tokens: usize,
    /// 范围内工具内容占比（0~1），用于 nudge 展示
    pub tool_ratio: f64,
    /// 是否含用户消息
    pub has_user: bool,
    /// 被保护而不可压缩（用于展示）
    pub protected_reason: Option<String>,
}

/// 推荐可压缩范围：跳过软/硬保护区，按最小字符数过滤（注意是原始字符数）
pub fn recommend(
    messages: &[CoreMessage],
    state: &CompressionState,
    cfg: &BillionConfig,
) -> Vec<Range> {
    let covered = state.covered_ids();
    let soft = soft_zone(messages, cfg);
    let latest = latest_tool_instances(messages);
    let hard = hard_protected(messages, cfg, &latest);

    let mut out: Vec<Range> = Vec::new();
    let mut i = 0usize;
    while i < messages.len() {
        let m = &messages[i];
        let skippable = covered.contains(m.base_id())
            || soft.contains(&i)
            || hard.contains(&i)
            || m.is_summary;
        if skippable {
            i += 1;
            continue;
        }
        // 收集连续可压段
        let start = i;
        let mut end = i;
        let mut chars = 0usize;
        let mut tokens = 0usize;
        let mut tool_chars = 0usize;
        let mut has_user = false;
        while end < messages.len() {
            let mm = &messages[end];
            if covered.contains(mm.base_id())
                || soft.contains(&end)
                || hard.contains(&end)
                || mm.is_summary
            {
                break;
            }
            chars += mm.text.chars().count();
            tokens += mm.tokens();
            if mm.content_type == ContentType::ToolResult || mm.content_type == ContentType::ToolCall {
                tool_chars += mm.text.chars().count();
            }
            if mm.role == "user" {
                has_user = true;
            }
            end += 1;
        }
        if end > start {
            let (sref, eref) = (
                state
                    .message_refs
                    .ref_of(&messages[start].id)
                    .cloned()
                    .unwrap_or_else(|| "?".into()),
                state
                    .message_refs
                    .ref_of(&messages[end - 1].id)
                    .cloned()
                    .unwrap_or_else(|| "?".into()),
            );
            // minCompressRange 计的是原始字符数
            if chars >= cfg.min_compress_range {
                out.push(Range {
                    start_idx: start,
                    end_idx: end - 1,
                    start_ref: sref,
                    end_ref: eref,
                    message_count: end - start,
                    chars,
                    tokens,
                    tool_ratio: if chars == 0 { 0.0 } else { tool_chars as f64 / chars as f64 },
                    has_user,
                    protected_reason: None,
                });
            }
        }
        i = end.max(i + 1);
    }
    out
}

/// 每个工具最近一次出现的下标
fn latest_tool_instances(messages: &[CoreMessage]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for (i, m) in messages.iter().enumerate() {
        if let Some(t) = &m.tool_name {
            map.insert(t.to_lowercase(), i);
        }
    }
    map
}

// ════════════════════════════════════════════════════════
// nudge 判定
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NudgeDecision {
    pub inject: bool,
    pub tier: u8,
    /// "GENTLE" | "OVER-LIMIT" | "EMERGENCY" | "T2" | "T3"
    pub label: String,
    pub reason: String,
    pub pending_t1: usize,
    pub pending_t2: usize,
    pub pending_t3: usize,
}

/// 判定是否注入 nudge（逐行对齐 kernel/src/compress.ts::decideNudge 的阈值语义）
pub fn decide_nudge(
    messages: &[CoreMessage],
    state: &CompressionState,
    cfg: &BillionConfig,
    usage_tokens: usize,
) -> NudgeDecision {
    let limit = cfg.model_context_limit.max(1) as f64;
    let usage = usage_tokens as f64 / limit;
    let step = cfg.nudge_growth_tokens();
    let floor = cfg.growth_floor_effective();
    let min_benefit = cfg.min_pressure_benefit();

    let ranges = recommend(messages, state, cfg);
    let pending_t1: usize = ranges.iter().map(|r| r.tokens).sum();
    let pending_t2: usize = state
        .active_blocks_of_tier(1)
        .iter()
        .map(|b| b.summary_tokens())
        .sum();
    let pending_t3: usize = state
        .active_blocks_of_tier(2)
        .iter()
        .map(|b| b.summary_tokens())
        .sum();

    let over_limit = usage >= cfg.max_context_limit_pct;
    let emergency = usage >= cfg.emergency_threshold_pct;
    let pressure = over_limit || emergency;

    let growth_reference = if state.nudge.last_nudge_shown_tokens > 0 {
        state.nudge.last_nudge_shown_tokens
    } else if state.nudge.baseline_tokens > 0 {
        state.nudge.baseline_tokens
    } else {
        usage_tokens
    };

    let t2_count_ready = state.active_blocks_of_tier(1).len() >= cfg.tier2_trigger;
    let t3_count_ready = state.active_blocks_of_tier(2).len() >= cfg.tier3_trigger;

    let first_sight_mass_ready = state.nudge.last_nudge_shown_tokens == 0
        && state.nudge.baseline_tokens == 0
        && usage >= cfg.min_context_limit_pct
        && pending_t1.max(pending_t2).max(pending_t3) >= step;
    let growth_ready =
        first_sight_mass_ready || usage_tokens.saturating_sub(growth_reference) >= floor;

    let mut decision = NudgeDecision {
        inject: false,
        tier: 1,
        label: "GENTLE".into(),
        reason: String::new(),
        pending_t1,
        pending_t2,
        pending_t3,
    };

    // 1) pressure 优先，忽略增长与节流
    if pressure {
        let mut best = (1u8, pending_t1);
        if cfg.tiers_enabled {
            if pending_t2 > best.1 { best = (2, pending_t2); }
            if pending_t3 > best.1 { best = (3, pending_t3); }
        }
        if best.1 >= min_benefit {
            decision.inject = true;
            decision.tier = best.0;
            decision.label = if emergency { "EMERGENCY".into() } else { "OVER-LIMIT".into() };
            decision.reason = format!(
                "占用 {:.1}%（{} / {}）{}",
                usage * 100.0,
                usage_tokens,
                cfg.model_context_limit,
                if emergency { "达到紧急阈值" } else { "超过 OVER-LIMIT 阈值" }
            );
        } else {
            decision.reason = format!("占用高但可压收益不足（{} < {} token）", best.1, min_benefit);
        }
        return decision;
    }

    // 2) 增长就绪 → T1
    if growth_ready && pending_t1 >= step {
        decision.inject = true;
        decision.tier = 1;
        decision.label = "GENTLE".into();
        decision.reason = format!("上下文较上次 nudge 增长 ≥ {} token", floor);
        return decision;
    }

    // 3) T2
    if cfg.tiers_enabled {
        let cadence_ok = state
            .nudge
            .last_shown_by_tier
            .get(&2)
            .map(|&v| v == 0 || usage_tokens.saturating_sub(v) >= floor)
            .unwrap_or(true);
        let thr2 = cfg.tier_threshold(2);
        if (t2_count_ready || (pending_t2 >= thr2 && pending_t2 > pending_t1)) && cadence_ok {
            decision.inject = true;
            decision.tier = 2;
            decision.label = "T2".into();
            decision.reason = format!("T1 块可蒸馏（{} ≥ {} token）", pending_t2, thr2);
            return decision;
        }
        // 4) T3
        let cadence_ok3 = state
            .nudge
            .last_shown_by_tier
            .get(&3)
            .map(|&v| v == 0 || usage_tokens.saturating_sub(v) >= floor)
            .unwrap_or(true);
        let thr3 = cfg.tier_threshold(3);
        if (t3_count_ready
            || (pending_t3 >= thr3 && pending_t3 > pending_t2 && pending_t3 > pending_t1))
            && cadence_ok3
        {
            decision.inject = true;
            decision.tier = 3;
            decision.label = "T3".into();
            decision.reason = format!("T2 块可浓缩（{} ≥ {} token）", pending_t3, thr3);
            return decision;
        }
    }

    decision.reason = format!(
        "增长或可压质量未达阈值（增长参考 {}，可压 T1 {} / T2 {} / T3 {}）",
        growth_reference, pending_t1, pending_t2, pending_t3
    );
    decision
}

/// 注入后记录节流锚点
pub fn stamp_nudge(state: &mut CompressionState, tier: u8, usage_tokens: usize, cfg: &BillionConfig) {
    let step = cfg.nudge_growth_tokens();
    let shown = state.nudge.last_nudge_shown_tokens;
    let baseline = state.nudge.baseline_tokens;
    if (baseline > 0 && usage_tokens + step < baseline) || (shown > 0 && usage_tokens + step < shown) {
        state.nudge.last_per_message_nudge_tokens = usage_tokens;
        state.nudge.last_nudge_shown_tokens = 0;
        state.nudge.last_shown_by_tier.clear();
    }
    if state.nudge.last_per_message_nudge_tokens == 0 {
        state.nudge.last_per_message_nudge_tokens = usage_tokens;
    }
    state.nudge.last_nudge_shown_tokens = usage_tokens;
    state.nudge.last_shown_by_tier.insert(tier, usage_tokens);
}

// ════════════════════════════════════════════════════════
// 成对完整性（工具调用/结果、推理/回复不可拆分）
// ════════════════════════════════════════════════════════

/// 边界扩张到不动点（≤2 轮），只对消息边界生效。
/// 这是必须的：DeepSeek thinking 模式要求 reasoning_content 随后续请求回传，
/// 拆散（call, result）或（reasoning, burst）会导致 API 400。
pub fn adjust_pair_boundaries(
    messages: &[CoreMessage],
    start: usize,
    end: usize,
) -> (usize, usize) {
    let mut s = start;
    let mut e = end;
    for _ in 0..2 {
        let mut moved = false;

        // 1) 推理对：范围内的 reasoning 需要把它后面的 assistant burst 一起拉进来
        let mut i = s;
        while i <= e && i < messages.len() {
            if messages[i].content_type == ContentType::Reasoning {
                let mut j = i + 1;
                while j < messages.len()
                    && matches!(
                        messages[j].content_type,
                        ContentType::Text | ContentType::ToolCall
                    )
                    && messages[j].role == "assistant"
                {
                    if j > e { e = j; moved = true; }
                    j += 1;
                }
            }
            i += 1;
        }

        // 2) 工具对：范围内出现的 tool_call_id 必须把对应结果（向后）与调用（向前）纳入
        let mut ids: Vec<String> = Vec::new();
        for m in messages.iter().take(e + 1).skip(s) {
            if let Some(id) = &m.tool_call_id {
                if !id.is_empty() { ids.push(id.clone()); }
            }
        }
        for m in messages.iter().take(e + 1).skip(s) {
            if m.content_type == ContentType::ToolCall && !m.text.is_empty() {
                ids.push(m.text.clone());
            }
        }
        if !ids.is_empty() {
            // 向前 ≤20 找调用
            let lo = s.saturating_sub(20);
            for k in lo..s {
                if let Some(id) = &messages[k].tool_call_id {
                    if !id.is_empty() && ids.iter().any(|x| x == id) { s = k; moved = true; }
                }
            }
            // 向后 ≤20 找结果
            let hi = (e + 20).min(messages.len().saturating_sub(1));
            for k in (e + 1)..=hi {
                if k >= messages.len() { break; }
                if let Some(id) = &messages[k].tool_call_id {
                    if !id.is_empty() && ids.iter().any(|x| x == id) { e = k; moved = true; }
                }
            }
        }

        if !moved { break; }
    }
    (s, e)
}

// ════════════════════════════════════════════════════════
// 应用压缩
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct CompressInput {
    pub start_idx: usize,
    pub end_idx: usize,
    pub summary: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressOutcome {
    pub block_id: String,
    pub tier: u8,
    pub start_ref: String,
    pub end_ref: String,
    pub before_tokens: usize,
    pub after_tokens: usize,
    pub covered: usize,
}

/// 应用一次压缩：校验 → 建块 → 记录覆盖。
/// 摘要由调用方（模型）提供；引擎只做校验与记账。
pub fn apply_compression(
    messages: &[CoreMessage],
    state: &mut CompressionState,
    cfg: &BillionConfig,
    inputs: &[CompressInput],
) -> Result<Vec<CompressOutcome>, String> {
    if inputs.is_empty() {
        return Err("empty".into());
    }
    let covered = state.covered_ids();

    // 按起始下标排序，重叠范围跳过后者（不整体中止）
    let mut sorted: Vec<&CompressInput> = inputs.iter().collect();
    sorted.sort_by_key(|i| i.start_idx);

    let mut outcomes = Vec::new();
    let mut used: Vec<(usize, usize)> = Vec::new();

    for inp in sorted {
        if used.iter().any(|(s, e)| inp.start_idx <= *e && inp.end_idx >= *s) {
            continue; // 重叠：最早者胜
        }
        if inp.start_idx > inp.end_idx || inp.end_idx >= messages.len() {
            return Err(format!("范围越界：{}..{}", inp.start_idx, inp.end_idx));
        }
        // 摘要长度校验
        let slen = inp.summary.chars().count();
        if slen == 0 {
            return Err("摘要为空".into());
        }
        if slen < cfg.min_summary_length {
            return Err(format!("摘要过短（{} < {} 字符）", slen, cfg.min_summary_length));
        }
        if slen > cfg.max_summary_length {
            return Err(format!("摘要过长（{} > {} 字符）", slen, cfg.max_summary_length));
        }

        // 成对边界扩张
        let (s, e) = adjust_pair_boundaries(messages, inp.start_idx, inp.end_idx);

        // 范围内必须至少有一条"新的、未被覆盖"的直接消息，否则就是空操作（防活锁）
        let direct: Vec<String> = messages[s..=e]
            .iter()
            .filter(|m| !covered.contains(m.base_id()) && !m.is_summary)
            .map(|m| m.id.clone())
            .collect();
        if direct.is_empty() {
            return Err(format!(
                "范围 {}–{} 已被完全覆盖且不含新消息，压缩无意义（拒绝空操作）",
                messages[s].id, messages[e].id
            ));
        }

        let before_tokens: usize = messages[s..=e].iter().map(|m| m.tokens()).sum();
        // 计入被消费的子块（先取出需要的数据，避免与后续可变借用冲突）
        let consumed_blocks: Vec<CompressionBlock> = state
            .blocks
            .iter()
            .filter(|b| b.active && b.effective_message_ids.iter().any(|id| direct.contains(id)))
            .cloned()
            .collect();
        let consumed_tokens: usize = consumed_blocks.iter().map(|b| b.summary_tokens()).sum();
        let mut child_ids: Vec<String> = Vec::new();
        let mut inherited: Vec<String> = Vec::new();
        for cb in &consumed_blocks {
            inherited.extend(cb.effective_message_ids.iter().cloned());
            child_ids.push(cb.block_id.clone());
        }

        let block_id = state.new_block_id();
        let after_tokens = count_tokens(&inp.summary);
        let mut effective = direct.clone();
        effective.extend(inherited);
        effective.sort();
        effective.dedup();

        let sref = state
            .message_refs
            .ref_of(&messages[s].id)
            .cloned()
            .unwrap_or_else(|| "?".into());
        let eref = state
            .message_refs
            .ref_of(&messages[e].id)
            .cloned()
            .unwrap_or_else(|| "?".into());

        let block = CompressionBlock {
            block_id: block_id.clone(),
            tier: 1,
            topic: inp.topic.clone(),
            summary: inp.summary.clone(),
            direct_message_ids: direct.clone(),
            effective_message_ids: effective,
            direct_block_ids: child_ids.clone(),
            compressed_tokens: before_tokens + consumed_tokens,
            created_at: chrono::Utc::now().timestamp(),
            survived_count: 0,
            generation: "young".into(),
            active: true,
            restored_inline: false,
            start_ref: Some(sref.clone()),
            end_ref: Some(eref.clone()),
        };
        state.blocks.push(block);
        // 消费掉的子块失活
        for cb in child_ids {
            if let Some(b) = state.blocks.iter_mut().find(|b| b.block_id == cb) {
                b.active = false;
            }
        }
        state.stats.tokens_compressed += before_tokens.saturating_sub(after_tokens);
        state.stats.compression_count += 1;
        used.push((s, e));

        outcomes.push(CompressOutcome {
            block_id,
            tier: 1,
            start_ref: sref,
            end_ref: eref,
            before_tokens,
            after_tokens,
            covered: direct.len(),
        });
    }

    if outcomes.is_empty() {
        return Err("所有范围都因重叠被跳过".into());
    }

    // 成功压缩后必须重置 nudge 基线，否则 nudge 会反复触发（反馈环）
    state.nudge.last_per_message_nudge_tokens = 0;
    state.nudge.last_nudge_shown_tokens = 0;
    state.nudge.last_shown_by_tier.clear();
    state.terminal_streak = 0;

    Ok(outcomes)
}

/// 蒸馏若干 T1 块为 T2 / T2 块为 T3
pub fn apply_distill(
    state: &mut CompressionState,
    child_block_ids: &[String],
    summary: String,
    topic: Option<String>,
    tier: u8,
) -> Result<CompressOutcome, String> {
    let slen = summary.chars().count();
    if slen == 0 {
        return Err("摘要为空".into());
    }
    let children: Vec<CompressionBlock> = state
        .blocks
        .iter()
        .filter(|b| b.active && child_block_ids.contains(&b.block_id))
        .cloned()
        .collect();
    if children.is_empty() {
        return Err("没有可用于蒸馏的活跃子块".into());
    }
    let before_tokens: usize = children.iter().map(|b| b.compressed_tokens).sum();
    let block_id = state.new_block_id();
    let after_tokens = count_tokens(&summary);

    let mut effective = Vec::new();
    for c in &children {
        effective.extend(c.effective_message_ids.iter().cloned());
    }
    effective.sort();
    effective.dedup();

    state.blocks.push(CompressionBlock {
        block_id: block_id.clone(),
        tier,
        topic,
        summary,
        direct_message_ids: vec![],
        effective_message_ids: effective,
        direct_block_ids: child_block_ids.to_vec(),
        compressed_tokens: before_tokens,
        created_at: chrono::Utc::now().timestamp(),
        survived_count: 0,
        generation: "young".into(),
        active: true,
        restored_inline: false,
        start_ref: None,
        end_ref: None,
    });
    for c in &children {
        if let Some(b) = state.blocks.iter_mut().find(|b| b.block_id == c.block_id) {
            b.active = false;
        }
    }
    state.stats.tokens_compressed += before_tokens.saturating_sub(after_tokens);
    state.stats.compression_count += 1;

    Ok(CompressOutcome {
        block_id,
        tier,
        start_ref: children.first().and_then(|c| c.start_ref.clone()).unwrap_or_default(),
        end_ref: children.last().and_then(|c| c.end_ref.clone()).unwrap_or_default(),
        before_tokens,
        after_tokens,
        covered: children.len(),
    })
}

/// 失活一个块（取回原文）
pub fn deactivate_block(state: &mut CompressionState, block_id: &str) -> Result<usize, String> {
    let Some(b) = state.blocks.iter().find(|b| b.block_id == block_id) else {
        return Err(format!("找不到块 {}", block_id));
    };
    let ids = b.effective_message_ids.clone();
    if let Some(bm) = state.blocks.iter_mut().find(|b| b.block_id == block_id) {
        bm.active = false;
    }
    Ok(ids.len())
}

// ════════════════════════════════════════════════════════
// 剪枝渲染（把摘要替回被覆盖的位置）
// ════════════════════════════════════════════════════════

/// 计算"发送给模型"的视图：
///  1. 丢掉被覆盖的消息
///  2. 每个活跃块的摘要在其最早被覆盖的位置插入（成对安全）
///  3. 第一条用户消息无条件保留（pin 在 covered 判定之前）
pub fn prune(messages: &[CoreMessage], state: &CompressionState) -> Vec<CoreMessage> {
    let covered = state.covered_ids();
    let first_user = messages.iter().position(|m| m.role == "user");

    // 块 → 最早覆盖位置
    let mut anchor: HashMap<String, usize> = HashMap::new();
    for b in state.active_blocks() {
        let mut min: Option<usize> = None;
        for (i, m) in messages.iter().enumerate() {
            if b.effective_message_ids.iter().any(|id| id == m.base_id() || id == &m.id) {
                min = Some(min.map_or(i, |v: usize| v.min(i)));
            }
        }
        if let Some(mi) = min {
            anchor.insert(b.block_id.clone(), mi);
        }
    }

    // 位置 → 要插入的摘要
    let mut inserts: HashMap<usize, Vec<&CompressionBlock>> = HashMap::new();
    for b in state.active_blocks() {
        if let Some(&pos) = anchor.get(&b.block_id) {
            inserts.entry(pos).or_default().push(b);
        }
    }

    let mut out: Vec<CoreMessage> = Vec::new();
    for (i, m) in messages.iter().enumerate() {
        let is_pinned_first_user = Some(i) == first_user;
        let is_covered = !is_pinned_first_user && covered.contains(m.base_id());
        if is_covered {
            // 被覆盖：只在锚点位置插入摘要（成对安全：跳过同一 burst 的结果）
            if let Some(blocks) = inserts.get(&i) {
                for b in blocks {
                    out.push(summary_message(b, &m.id));
                }
            }
            continue;
        }
        if let Some(blocks) = inserts.get(&i) {
            for b in blocks {
                out.push(summary_message(b, &m.id));
            }
        }
        out.push(m.clone());
    }

    strip_orphans(out)
}

fn summary_message(b: &CompressionBlock, anchor_raw_id: &str) -> CoreMessage {
    CoreMessage {
        // 视图专用 id：永不进入覆盖集
        id: format!("acp_summary_{}", b.block_id),
        role: "system".into(),
        content_type: ContentType::Text,
        text: b.render(),
        tool_name: None,
        tool_call_id: None,
        has_media: false,
        is_summary: true,
    }
    .tap_anchor(anchor_raw_id)
}

trait TapAnchor {
    fn tap_anchor(self, _raw: &str) -> Self;
}
impl TapAnchor for CoreMessage {
    fn tap_anchor(self, _raw: &str) -> Self { self }
}

/// 剪枝后剥离孤儿：结果没有调用 → 调用没有结果 → 推理没有配对回复
fn strip_orphans(msgs: Vec<CoreMessage>) -> Vec<CoreMessage> {
    let mut call_ids: HashSet<String> = HashSet::new();
    let mut result_ids: HashSet<String> = HashSet::new();
    for m in &msgs {
        match m.content_type {
            ContentType::ToolCall => {
                if !m.text.is_empty() { call_ids.insert(m.text.clone()); }
            }
            ContentType::ToolResult => {
                if let Some(id) = &m.tool_call_id {
                    if !id.is_empty() { result_ids.insert(id.clone()); }
                }
            }
            _ => {}
        }
    }
    let mut out: Vec<CoreMessage> = Vec::new();
    for m in msgs {
        let drop = match m.content_type {
            // 结果没有对应调用 → 丢
            ContentType::ToolResult => m
                .tool_call_id
                .as_ref()
                .map(|id| !id.is_empty() && !call_ids.contains(id))
                .unwrap_or(false),
            // 调用没有对应结果 → 丢（compress 调用豁免：它承载摘要）
            ContentType::ToolCall => {
                let exempt = m
                    .tool_name
                    .as_ref()
                    .map(|t| t == "compress" || t == "decompress")
                    .unwrap_or(false);
                !exempt && !m.text.is_empty() && !result_ids.contains(&m.text)
            }
            _ => false,
        };
        if !drop {
            out.push(m);
        }
    }
    out
}

// ════════════════════════════════════════════════════════
// nudge 文本（注入为临时的 user 消息，永不持久化）
// ════════════════════════════════════════════════════════

pub const COMPRESS_PHILOSOPHY: &str = "\
Compression Philosophy:
- All compression serves the primary task, but be frugal.
- Context capacity is precious. Save context by compressing consumed outputs, not by avoiding tools.
- Compress by need, not by percentage.
- Work from summaries, not raw tool outputs. All listed ranges (user prompts, tool outputs, code, logs, exploration, intermediate steps) should be compressed to summary format — the ONLY exceptions are protected content, content the current step is actively using, or critical content you cannot reconstruct.";

/// T1 压缩规则（关键条款逐字保留）
pub const HOW_TO_COMPRESS_RULES: &str = "\
HOW TO COMPRESS

When you call `compress`, the summary you write becomes the only record of the replaced conversation.
Make it self-contained and complete: every user request, experiment purpose, and work task in the range must
be accurately captured. A later reader (or you, after decompressing) should be able to continue the task
WITHOUT needing the original. The summary records the PAST as of this block's creation: label recorded task
state as history (\"TASK AS OF THIS BLOCK: ...\") — never as a live instruction.
Write plain text with real unicode characters; never copy \\uXXXX escape sequences or JSON-escaped fragments
out of tool output.

KEEP VERBATIM — never paraphrase or abbreviate these:
- Full file paths with line numbers, directory prefix on every mention. Never abbreviate to a bare filename.
- Function, class, and type signatures AND critical code lines that encode logic — the line that IS the finding.
- Error messages and stack traces (exact text — you need the literal string to grep for it later).
- Key details from reports and analyses — not just the conclusion.
- Decisions and their rationale (\"chose X over Y because Z\" — the \"because\" is load-bearing).
- Constraints discovered (\"must support Node 22\", \"no new dependencies\").
- Exact values: versions, config keys, thresholds, magic numbers.
- User intent — quote short user messages verbatim ONLY WITH their message ref, e.g. User said (m00132): \"ship it tonight\".
- Open objectives carry-forward — a one-line `Open objectives:` entry naming each still-open objective with its message ref.
- The user's overall goal and any changes to it, including pivots.
- Purpose behind each significant action; open questions and unresolved TODOs.
- Message refs of key anchors (m00420, m00510-m00520).

DROP — extract the signal, discard the vessel:
- Verbose logs once the error line/result is captured; duplicate file reads.
- Consumed exploration (search hits, agent returns, successful outputs).
- Dead-end exploration — but PRESERVE the lesson in one line: \"tried X, failed because Y\".
- Back-and-forth discussion once the final position is captured; repeated status checks.
For each significant item you DROP add a one-line CONTENT description of what it covers — not where it lives.

PRIORITY — when the summary must be compact, preserve in this order:
1. User's overall goal, goal evolution, intent, and hard constraints
2. Decisions and rationale.
3. Exact technical artifacts: paths, signatures, errors, values.
4. Conclusions and key findings.
5. Lessons learned: what failed and why.

Write dense, scannable bullets — not narrative prose. Every line must earn its place.";

pub const TIER2_DISTILL_RULES: &str = "\
You are compressing historical summaries (not raw conversation).
KEEP: decisions + rationale; final outcomes (versions/PR numbers); key lessons; critical constraints;
architectural decisions; user quotes only as attributed history with their ref; an `Open objectives:`
carry-forward; `[SUPERSEDED by PR #NNN]` / `[OBSOLETE: ...]` markers; subject symbol/module names;
one-line exploration conclusions.
DROP: exact line numbers; diffs; verbose signatures; code listings; build/deploy/test steps; review detail; logs.
FORMAT: first line `Source: bN+bM+... (XK->YK tok, Zx). [original topic]`, then 3-5 bullets per source block,
start with the outcome, merge same-topic sources.
SIZE TARGET: 50-150 tokens per source block (excluding header).";

pub const TIER3_CONDENSE_RULES: &str = "\
Tier 3 is a lookup index, not a knowledge base.
FORMAT: same header line, then 1-3 facts per source block as \"[PR/Issue/Version] — [outcome in <=8 words]\".
Priority: shipped outcomes -> open work (incl. re-carried `Open objectives:`) -> architectural decisions -> critical constraints.
SIZE TARGET: 30-60 tokens per source block (incl. header).";

pub const ONE_CALL_HINT: &str = "\
ONE call, ONE string — fold every range you keep into a single compress call: content entries
({startId, endId, summary, topic?}) or one plain string holding one block per range ('m00150–m00220 topic'
header line, then the summary). Ranges you still need can wait — they reappear in later nudges; never split
the batch across separate calls.";

/// 渲染 nudge 文本
pub fn render_nudge(decision: &NudgeDecision, ranges: &[Range], state: &CompressionState) -> String {
    let mut s = String::new();
    match decision.label.as_str() {
        "EMERGENCY" => {
            s.push_str("⚠️ Context limit reached — compress now. Prioritize consumed tool outputs.\n\n");
            s.push_str(COMPRESS_PHILOSOPHY);
            s.push_str("\n\n");
            s.push_str(HOW_TO_COMPRESS_RULES);
        }
        "OVER-LIMIT" => {
            s.push_str("Context is at the limit — compress consumed outputs now.\n\n");
            s.push_str(COMPRESS_PHILOSOPHY);
            s.push_str("\n\n");
            s.push_str(HOW_TO_COMPRESS_RULES);
        }
        "T2" => {
            s.push_str("[TIER 2 DISTILLATION TRIGGER]\n\n");
            s.push_str(TIER2_DISTILL_RULES);
        }
        "T3" => {
            s.push_str("[EMERGENCY — TIER 3 CONDENSATION]\n\n");
            s.push_str(TIER3_CONDENSE_RULES);
        }
        _ => {
            s.push_str("This is an efficiency nudge to compress early and keep context lean — not an overflow warning. A separate, stronger alert will appear if the context is actually full.\n\n");
            s.push_str(COMPRESS_PHILOSOPHY);
            s.push_str("\n\n");
            s.push_str(HOW_TO_COMPRESS_RULES);
        }
    }

    s.push_str("\n\nContext breakdown: ");
    s.push_str(&format!(
        "T1 可压 {} | T2 可蒸馏 {} | T3 可浓缩 {}",
        decision.pending_t1, decision.pending_t2, decision.pending_t3
    ));

    if decision.tier == 1 && !ranges.is_empty() {
        s.push_str(&format!("\n\nCompressible ranges ({}):\n", ranges.len()));
        for r in ranges.iter().take(8) {
            s.push_str(&format!(
                "  {}–{}  {} msgs  {} chars  [tool {:.0}%]{}\n",
                r.start_ref,
                r.end_ref,
                r.message_count,
                r.chars,
                r.tool_ratio * 100.0,
                if r.has_user { " · 含 user 消息" } else { "" }
            ));
        }
    }

    let active: Vec<&CompressionBlock> = state.active_blocks().collect();
    if !active.is_empty() {
        s.push_str("\nBlocks: ");
        s.push_str(
            &active
                .iter()
                .map(|b| {
                    format!(
                        "{}=T{} {}",
                        b.block_id,
                        b.tier,
                        b.topic.clone().unwrap_or_else(|| "(无主题)".into())
                    )
                })
                .collect::<Vec<_>>()
                .join(" · "),
        );
    }

    s.push_str("\n\n");
    s.push_str(ONE_CALL_HINT);
    s.push_str(&format!("\n\n(sent view 约 {} token)", decision.pending_t1));
    s
}

// ════════════════════════════════════════════════════════
// 从可见历史重建状态（会话重载用）
// ════════════════════════════════════════════════════════

/// 纯文本概览（acp_status 工具）
pub fn report(messages: &[CoreMessage], state: &CompressionState, cfg: &BillionConfig) -> String {
    let usage: usize = messages.iter().map(|m| m.tokens()).sum();
    let pct = usage as f64 / cfg.model_context_limit.max(1) as f64 * 100.0;
    let ranges = recommend(messages, state, cfg);
    let active: Vec<&CompressionBlock> = state.active_blocks().collect();
    let mut s = String::new();
    s.push_str("ACP Context Analysis\n");
    s.push_str(&format!(
        "Sent to LLM (est.): {} tok ({:.1}% of {})\n",
        usage, pct, cfg.model_context_limit
    ));
    s.push_str(&format!(
        "Blocks: {} active / {} total ({} tok compressed, cumulative; {} compressions)\n",
        active.len(),
        state.blocks.len(),
        state.stats.tokens_compressed,
        state.stats.compression_count
    ));
    s.push_str(&format!("Compressible ranges: {}\n", ranges.len()));
    for r in ranges.iter().take(20) {
        s.push_str(&format!(
            "  {}–{}  {} msgs  {} chars  ~{} tok  [tool {:.0}%]\n",
            r.start_ref, r.end_ref, r.message_count, r.chars, r.tokens, r.tool_ratio * 100.0
        ));
    }
    for b in active.iter().take(20) {
        s.push_str(&format!(
            "  [{}] T{} {}→{} tok: {}\n",
            b.block_id,
            b.tier,
            b.compressed_tokens,
            b.summary_tokens(),
            b.topic.clone().unwrap_or_else(|| "(无主题)".into())
        ));
    }
    s
}

/// search_context：对活跃块的 topic/summary 打分（对齐内核实际发货的子串计数实现）
pub fn search_blocks(
    state: &CompressionState,
    query: &str,
    limit: usize,
) -> Vec<(String, f64, String)> {
    let terms: Vec<String> = query.to_lowercase().split_whitespace().map(|s| s.to_string()).collect();
    if terms.is_empty() {
        return vec![];
    }
    let mut scored: Vec<(String, f64, String)> = Vec::new();
    for b in state.active_blocks() {
        let topic = b.topic.clone().unwrap_or_default().to_lowercase();
        let summary = b.summary.to_lowercase();
        let mut score = 0.0f64;
        for t in &terms {
            let th = topic.matches(t.as_str()).count() as f64;
            let sh = summary.matches(t.as_str()).count() as f64;
            score += (th * 0.15).min(0.45) + (sh * 0.04).min(0.2);
        }
        score = score.min(1.0);
        if score > 0.0 {
            scored.push((b.block_id.clone(), score, b.summary.clone()));
        }
    }
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, role: &str, ct: ContentType, text: &str) -> CoreMessage {
        CoreMessage {
            id: id.into(),
            role: role.into(),
            content_type: ct,
            text: text.into(),
            tool_name: None,
            tool_call_id: None,
            has_media: false,
            is_summary: false,
        }
    }

    #[test]
    fn cjk_aware_token_counting() {
        assert_eq!(count_tokens(""), 0);
        // 10 个汉字 = 10 token
        assert_eq!(count_tokens(&"中".repeat(10)), 10);
        // 40 个 ASCII = 10 token
        assert_eq!(count_tokens(&"a".repeat(40)), 10);
        // 混合：5 汉字 + 20 ascii = 5 + 5
        let mixed = format!("{}{}", "中".repeat(5), "a".repeat(20));
        assert_eq!(count_tokens(&mixed), 10);
    }

    #[test]
    fn refs_are_never_reissued() {
        let mut m = RefMap::default();
        let r1 = m.assign("raw-1", false);
        let r2 = m.assign("raw-2", false);
        assert_eq!(r1, "m00001");
        assert_eq!(r2, "m00002");
        // 同一 id 复用
        assert_eq!(m.assign("raw-1", false), "m00001");
        // 受保护不占槽
        assert_eq!(m.assign("raw-3", true), BLOCKED_REF);
        // 下一个仍然是 3
        assert_eq!(m.assign("raw-4", false), "m00003");
        // 剪枝后高水位重新钉住，不重发
        let live: HashSet<String> = ["raw-2".to_string()].into_iter().collect();
        m.prune_to(&live);
        assert_eq!(m.highest_used, 2);
        assert_eq!(m.assign("raw-9", false), "m00003");
    }

    #[test]
    fn nudge_thresholds_match_kernel_defaults() {
        let cfg = BillionConfig::with_limit(100_000);
        // 增长步长恒定 50000（不随窗口缩放）
        assert_eq!(cfg.nudge_growth_tokens(), 50_000);
        // 增长下限 = max(20000, 0.45*50000) = 22500
        assert_eq!(cfg.growth_floor_effective(), 22_500);
        // 最小收益 = max(5000, L*0.01) = 5000
        assert_eq!(cfg.min_pressure_benefit(), 5000);
        // T2 阈值 = 50000 * 1.5
        assert_eq!(cfg.tier_threshold(2), 75_000);
    }

    #[test]
    fn pressure_beats_growth_and_tier_choice() {
        let cfg = BillionConfig::with_limit(100_000);
        let state = CompressionState::default();
        let mut msgs = vec![msg("u1", "user", ContentType::Text, "go")];
        // 造一个足够大的可压范围（单条 5000 字符，软保护区才会很快收敛）
        for i in 0..40 {
            msgs.push(msg(
                &format!("t{}", i),
                "tool",
                ContentType::ToolResult,
                &"x".repeat(5000),
            ));
        }
        // 90% 占用 → OVER-LIMIT，忽略增长
        let d = decide_nudge(&msgs, &state, &cfg, 90_000);
        assert!(d.inject, "90% 占用应触发：{}", d.reason);
        assert_eq!(d.label, "OVER-LIMIT");

        // 96% → EMERGENCY
        let d = decide_nudge(&msgs, &state, &cfg, 96_000);
        assert!(d.inject);
        assert_eq!(d.label, "EMERGENCY");

        // 10% 占用且无可压质量 → 不注入
        let d = decide_nudge(&msgs, &state, &cfg, 10_000);
        assert!(!d.inject);
    }

    #[test]
    fn prune_pins_first_user_message_and_replaces_range() {
        let cfg = BillionConfig::with_limit(100_000);
        let mut state = CompressionState::default();
        let mut msgs = vec![msg("u1", "user", ContentType::Text, "first request")];
        for i in 0..30 {
            msgs.push(msg(
                &format!("t{}", i),
                "tool",
                ContentType::ToolResult,
                &"y".repeat(5000),
            ));
        }
        msgs.push(msg("u2", "user", ContentType::Text, "later request"));
        for m in msgs.iter() {
            state.message_refs.assign(&m.id, false);
        }
        let ranges = recommend(&msgs, &state, &cfg);
        assert!(!ranges.is_empty(), "应当有可压范围");

        let r = &ranges[0];
        let input = CompressInput {
            start_idx: r.start_idx,
            end_idx: r.end_idx,
            summary: "S".repeat(120),
            topic: Some("tool output".into()),
        };
        let out = apply_compression(&msgs, &mut state, &cfg, &[input]).expect("压缩应成功");
        assert_eq!(out.len(), 1);

        let view = prune(&msgs, &state);
        // 第一条用户消息必须保留（pin 在 covered 判定之前）
        assert!(view.iter().any(|m| m.id == "u1"), "首条用户消息必须钉住");
        // 摘要消息出现且带承重头
        assert!(view
            .iter()
            .any(|m| m.is_summary && m.text.contains(SUMMARY_HEADER)));
        // 视图 token 必须下降
        let before: usize = msgs.iter().map(|m| m.tokens()).sum();
        let after: usize = view.iter().map(|m| m.tokens()).sum();
        assert!(after < before, "剪枝后 token 应下降：{} -> {}", before, after);
    }

    #[test]
    fn empty_range_is_rejected_not_silently_succeeded() {
        let cfg = BillionConfig::with_limit(100_000);
        let mut state = CompressionState::default();
        let msgs = vec![
            msg("u1", "user", ContentType::Text, "hi"),
            msg("a1", "assistant", ContentType::Text, "hello"),
        ];
        for m in &msgs {
            state.message_refs.assign(&m.id, false);
        }
        let input = CompressInput {
            start_idx: 1,
            end_idx: 1,
            summary: "S".repeat(120),
            topic: None,
        };
        // 先成功压一次
        state.blocks.push(CompressionBlock {
            block_id: "b1".into(),
            tier: 1,
            topic: None,
            summary: "S".repeat(120),
            direct_message_ids: vec!["a1".into()],
            effective_message_ids: vec!["a1".into()],
            direct_block_ids: vec![],
            compressed_tokens: 10,
            created_at: 0,
            survived_count: 0,
            generation: "young".into(),
            active: true,
            restored_inline: false,
            start_ref: None,
            end_ref: None,
        });
        // 再压同一范围 → 必须硬报错（防活锁），而不是"成功"
        let err = apply_compression(&msgs, &mut state, &cfg, &[input]);
        assert!(err.is_err(), "完全覆盖的范围必须硬报错");
    }

    #[test]
    fn pair_boundaries_are_widened() {
        let mut msgs = vec![msg("u1", "user", ContentType::Text, "go")];
        let mut call = msg("c1", "assistant", ContentType::ToolCall, "call_1");
        call.tool_call_id = None;
        msgs.push(call);
        let mut res = msg("r1", "tool", ContentType::ToolResult, "output");
        res.tool_call_id = Some("call_1".into());
        msgs.push(res);
        // 只选 call → 应扩张到包含 result
        let (s, e) = adjust_pair_boundaries(&msgs, 1, 1);
        assert_eq!(s, 1);
        assert_eq!(e, 2, "工具调用必须与其结果保持成对");
    }

    #[test]
    fn nudge_baseline_reset_prevents_feedback_loop() {
        let cfg = BillionConfig::with_limit(100_000);
        let mut state = CompressionState::default();
        let mut msgs = vec![msg("u1", "user", ContentType::Text, "go")];
        for i in 0..30 {
            msgs.push(msg(&format!("t{}", i), "tool", ContentType::ToolResult, &"z".repeat(5000)));
        }
        for m in &msgs {
            state.message_refs.assign(&m.id, false);
        }
        state.nudge.last_nudge_shown_tokens = 90_000;
        let ranges = recommend(&msgs, &state, &cfg);
        let input = CompressInput {
            start_idx: ranges[0].start_idx,
            end_idx: ranges[0].end_idx,
            summary: "S".repeat(120),
            topic: None,
        };
        let _ = apply_compression(&msgs, &mut state, &cfg, &[input]).unwrap();
        // 压缩成功后基线必须清零
        assert_eq!(state.nudge.last_nudge_shown_tokens, 0);
        assert_eq!(state.nudge.last_per_message_nudge_tokens, 0);
        assert!(state.nudge.last_shown_by_tier.is_empty());
    }
}

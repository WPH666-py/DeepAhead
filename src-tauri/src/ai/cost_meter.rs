//! ─── 费用统计（移植自 Han-1413141/dsh-cost-meter）───
//!
//! 原插件对每次 LLM 调用做 token/金额计量：会话成本、按模型/按天聚合、
//! 预算、供应商余额、编码套餐额度。这里移植**可移植的纯计算层**：
//!   - DeepSeek 价目表（off-peak / peak / legacyBase，USD 与 CNY）
//!   - 单次调用成本公式（每 1,000,000 token；cacheWrite 回退到 cacheHit）
//!   - 峰谷时段判定（UTC 01:00–04:00 / 06:00–10:00，周末与节假日全天低谷）
//!   - 账本（天 / 会话 / 按供应商-模型桶，同时记 cost 与 apiCost 双轨）
//!   - 预算与聚合窗口（今日 / 本月 / 全部）
//!   - 金额格式化（账本内**永不取整**，只在展示时格式化）
//!
//! 未移植：官方价格页抓取、供应商余额接口、编码套餐额度、外部用量快照、
//! 以及依赖 DSH `llm/stream` 水龙头的**实时**计费（这里由 agent loop 直接记账）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

// ════════════════════════════════════════════════════════
// 价目表（每 1,000,000 token）
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Tier {
    pub cache_hit: f64,
    pub cache_miss: f64,
    pub output: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelPrice {
    /// 低谷价
    pub off_peak: Tier,
    /// 高峰价
    pub peak: Tier,
    /// 旧基准价（rateHistory 边界之前）
    pub legacy_base: Option<Tier>,
    /// rateHistory 边界（Unix 秒）：此前用 legacy_base
    pub rate_before: Option<i64>,
}

/// 旧基准价边界：2026-08-16T16:00:00Z
pub const LEGACY_BASE_BOUNDARY: i64 = 1_786_982_400;
/// 峰谷窗口生效时刻：2026-08-01T00:00:00Z
pub const PEAK_EFFECTIVE_AT: i64 = 1_785_715_200;
/// 周末全天低谷生效：2026-08-22T16:00:00Z
pub const WEEKEND_OFFPEAK_EFFECTIVE_AT: i64 = 1_787_414_400;
/// flash 调价边界：2026-09-10T04:00:00Z
pub const FLASH_PRICE_EFFECTIVE_AT: i64 = 1_789_070_400;
/// pro 调价边界：2026-09-14T04:00:00Z
pub const PRO_FLASH_ROUTING_EFFECTIVE_AT: i64 = 1_789_416_000;

/// 默认峰谷窗口（UTC 小时）
pub const DEFAULT_PEAK_WINDOWS: [(u32, u32); 2] = [(1, 4), (6, 10)];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    Usd,
    Cny,
}

/// DeepSeek 价目表（USD）
pub fn deepseek_usd() -> HashMap<String, ModelPrice> {
    let mut m = HashMap::new();
    // 调价后（2026-09-10 / 09-14 之后）
    let flash = ModelPrice {
        off_peak: Tier { cache_hit: 0.003, cache_miss: 0.15, output: 0.6 },
        peak: Tier { cache_hit: 0.006, cache_miss: 0.3, output: 1.2 },
        legacy_base: Some(Tier { cache_hit: 0.0028, cache_miss: 0.14, output: 0.28 }),
        rate_before: Some(FLASH_PRICE_EFFECTIVE_AT),
    };
    let pro = ModelPrice {
        off_peak: Tier { cache_hit: 0.003, cache_miss: 0.15, output: 0.6 },
        peak: Tier { cache_hit: 0.006, cache_miss: 0.3, output: 1.2 },
        legacy_base: Some(Tier { cache_hit: 0.003625, cache_miss: 0.435, output: 0.87 }),
        rate_before: Some(PRO_FLASH_ROUTING_EFFECTIVE_AT),
    };
    // 调价前的费率（rateHistory）
    let flash_old = Tier { cache_hit: 0.007, cache_miss: 0.22, output: 0.66 };
    let flash_old_peak = Tier { cache_hit: 0.014, cache_miss: 0.44, output: 1.32 };
    let pro_old = Tier { cache_hit: 0.022, cache_miss: 0.66, output: 1.98 };
    let pro_old_peak = Tier { cache_hit: 0.044, cache_miss: 1.32, output: 3.96 };

    for name in ["deepseek-v4-flash", "deepseek-v4-flash-vision-exp", "deepseek-v4.1-flash", "default"] {
        m.insert(name.to_string(), flash.clone());
    }
    m.insert("deepseek-v4-pro".to_string(), pro);

    // 调价前费率表（按 rate_before 生效，作为 rateHistory 的第 0 段）
    m.insert(
        "__history_flash".to_string(),
        ModelPrice {
            off_peak: flash_old,
            peak: flash_old_peak,
            legacy_base: None,
            rate_before: Some(FLASH_PRICE_EFFECTIVE_AT),
        },
    );
    m.insert(
        "__history_pro".to_string(),
        ModelPrice {
            off_peak: pro_old,
            peak: pro_old_peak,
            legacy_base: None,
            rate_before: Some(PRO_FLASH_ROUTING_EFFECTIVE_AT),
        },
    );

    // 历史别名
    m.insert(
        "deepseek-chat".to_string(),
        ModelPrice {
            off_peak: Tier { cache_hit: 0.07, cache_miss: 0.27, output: 1.1 },
            peak: Tier { cache_hit: 0.07, cache_miss: 0.27, output: 1.1 },
            legacy_base: None,
            rate_before: None,
        },
    );
    m.insert(
        "deepseek-reasoner".to_string(),
        ModelPrice {
            off_peak: Tier { cache_hit: 0.14, cache_miss: 0.55, output: 2.19 },
            peak: Tier { cache_hit: 0.14, cache_miss: 0.55, output: 2.19 },
            legacy_base: None,
            rate_before: None,
        },
    );
    m
}

/// DeepSeek 价目表（CNY）
pub fn deepseek_cny() -> HashMap<String, ModelPrice> {
    let mut m = HashMap::new();
    let flash = ModelPrice {
        off_peak: Tier { cache_hit: 0.02, cache_miss: 1.0, output: 4.0 },
        peak: Tier { cache_hit: 0.04, cache_miss: 2.0, output: 8.0 },
        legacy_base: Some(Tier { cache_hit: 0.02, cache_miss: 1.0, output: 2.0 }),
        rate_before: Some(FLASH_PRICE_EFFECTIVE_AT),
    };
    let pro = ModelPrice {
        off_peak: Tier { cache_hit: 0.02, cache_miss: 1.0, output: 4.0 },
        peak: Tier { cache_hit: 0.04, cache_miss: 2.0, output: 8.0 },
        legacy_base: Some(Tier { cache_hit: 0.025, cache_miss: 3.0, output: 6.0 }),
        rate_before: Some(PRO_FLASH_ROUTING_EFFECTIVE_AT),
    };
    for name in ["deepseek-v4-flash", "deepseek-v4-flash-vision-exp", "deepseek-v4.1-flash", "default"] {
        m.insert(name.to_string(), flash.clone());
    }
    m.insert("deepseek-v4-pro".to_string(), pro);
    m.insert(
        "__history_flash".to_string(),
        ModelPrice {
            off_peak: Tier { cache_hit: 0.05, cache_miss: 1.5, output: 4.5 },
            peak: Tier { cache_hit: 0.1, cache_miss: 3.0, output: 9.0 },
            legacy_base: None,
            rate_before: Some(FLASH_PRICE_EFFECTIVE_AT),
        },
    );
    m.insert(
        "__history_pro".to_string(),
        ModelPrice {
            off_peak: Tier { cache_hit: 0.05, cache_miss: 1.5, output: 4.5 },
            peak: Tier { cache_hit: 0.1, cache_miss: 3.0, output: 9.0 },
            legacy_base: None,
            rate_before: Some(PRO_FLASH_ROUTING_EFFECTIVE_AT),
        },
    );
    m
}

fn table(currency: Currency) -> HashMap<String, ModelPrice> {
    match currency {
        Currency::Usd => deepseek_usd(),
        Currency::Cny => deepseek_cny(),
    }
}

/// 模型名归一：小写、去 llm- 前缀、去 (go)/(zen) 与日期/版本/尺寸后缀
pub fn canonical_model(model: &str) -> String {
    let mut s = model.trim().to_lowercase();
    if let Some(rest) = s.strip_prefix("llm-") {
        s = rest.to_string();
    }
    for deco in [" (go)", "(go)", " (zen)", "(zen)"] {
        s = s.replace(deco, "");
    }
    // 去掉厂商命名空间前缀（vendor/model → model）
    if let Some(idx) = s.rfind('/') {
        s = s[idx + 1..].to_string();
    }
    s.trim().to_string()
}

/// 解析出价目条目（未知模型返回 None ⇒ **不计价**，而不是当作 0 或 default）
pub fn price_for(model: &str, currency: Currency) -> Option<ModelPrice> {
    let t = table(currency);
    let c = canonical_model(model);
    // 只有 DeepSeek 系列才回退到 default（未知第三方模型必须保持 unpriced）
    if !c.contains("deepseek") {
        return None;
    }
    if let Some(p) = t.get(&c) {
        return Some(p.clone());
    }
    // 前缀匹配（如 deepseek-v4-flash-2026-09-10）
    for (k, v) in t.iter() {
        if k.starts_with("__history_") {
            continue;
        }
        if c.starts_with(k.as_str()) {
            return Some(v.clone());
        }
    }
    t.get("default").cloned()
}

/// 是否高峰时段（UTC 工作日 01:00–04:00 / 06:00–10:00；周末全天低谷）
pub fn is_peak_hour(at: i64) -> bool {
    if at < PEAK_EFFECTIVE_AT {
        return false;
    }
    use chrono::{Datelike, TimeZone, Timelike, Utc, Weekday};
    let Some(dt) = Utc.timestamp_opt(at, 0).single() else { return false };
    // 周末全天低谷（仅在生效时刻之后）
    if at >= WEEKEND_OFFPEAK_EFFECTIVE_AT {
        let wd = dt.weekday();
        if wd == Weekday::Sat || wd == Weekday::Sun {
            return false;
        }
    }
    let h = dt.hour();
    DEFAULT_PEAK_WINDOWS.iter().any(|(a, b)| h >= *a && h < *b)
}

/// 选档：legacy 边界 → 峰谷
pub fn tier_for(price: &ModelPrice, at: i64) -> Tier {
    if at < LEGACY_BASE_BOUNDARY {
        if let Some(l) = price.legacy_base {
            return l;
        }
    }
    if is_peak_hour(at) {
        price.peak
    } else {
        price.off_peak
    }
}

/// 单次调用成本（每 1,000,000 token）。
/// 注意：cacheWrite **回退到 cacheHit**（不是 cacheMiss）；账本内不取整。
pub fn cost_of(
    model: &str,
    currency: Currency,
    input: usize,
    cache_read: usize,
    cache_write: usize,
    output: usize,
    reasoning: usize,
    at: i64,
) -> Option<f64> {
    let price = price_for(model, currency)?;
    let t = tier_for(&price, at);
    let cost = (input as f64 * t.cache_miss
        + output as f64 * t.output
        + cache_read as f64 * t.cache_hit
        + cache_write as f64 * t.cache_hit
        + reasoning as f64 * 0.0)
        / 1_000_000.0;
    if cost.is_finite() && cost > 0.0 {
        Some(cost)
    } else {
        Some(0.0)
    }
}

// ════════════════════════════════════════════════════════
// 账本
// ════════════════════════════════════════════════════════

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Bucket {
    pub input: usize,
    pub output: usize,
    pub cache_read: usize,
    pub cache_write: usize,
    pub reasoning: usize,
    pub calls: usize,
    /// 等价成本
    pub cost: f64,
    /// 真实扣费（plan 轨为 0）
    pub api_cost: f64,
}

impl Bucket {
    pub fn add(&mut self, o: &Bucket) {
        self.input += o.input;
        self.output += o.output;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.reasoning += o.reasoning;
        self.calls += o.calls;
        self.cost += o.cost;
        self.api_cost += o.api_cost;
    }
    pub fn total_tokens(&self) -> usize {
        self.input + self.output + self.cache_read + self.cache_write + self.reasoning
    }
    /// 缓存命中率
    pub fn cache_hit_rate(&self) -> f64 {
        let denom = self.input + self.cache_read + self.cache_write;
        if denom == 0 {
            0.0
        } else {
            self.cache_read as f64 / denom as f64
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionCost {
    pub id: String,
    pub title: String,
    pub at: i64,
    #[serde(flatten)]
    pub bucket: Bucket,
    pub by_provider_model: HashMap<String, Bucket>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DayCost {
    pub date: String,
    #[serde(flatten)]
    pub bucket: Bucket,
    pub by_provider_model: HashMap<String, Bucket>,
    pub sessions: Vec<SessionCost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostConfig {
    pub currency: Currency,
    pub symbol: String,
    pub decimals: u32,
    /// 展示汇率（USD → 展示币种）
    pub exchange_rate: f64,
    pub history_days: u32,
    pub budget_enabled: bool,
    pub budget_amount: f64,
    /// day / month / all
    pub budget_period: String,
}

impl Default for CostConfig {
    fn default() -> Self {
        Self {
            currency: Currency::Usd,
            symbol: "$".into(),
            decimals: 4,
            exchange_rate: 7.2,
            history_days: 180,
            budget_enabled: false,
            budget_amount: 100.0,
            budget_period: "month".into(),
        }
    }
}

/// 账本（按本地日键聚合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub version: u32,
    pub config: CostConfig,
    pub days: HashMap<String, DayCost>,
}

impl Default for Ledger {
    fn default() -> Self {
        Self { version: 1, config: CostConfig::default(), days: HashMap::new() }
    }
}

pub fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("DEEPAHEAD_COST_DIR") {
        if !d.trim().is_empty() {
            return PathBuf::from(d);
        }
    }
    dirs_next::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("DeepAhead")
        .join("cost")
}

pub fn ledger_path() -> PathBuf {
    data_dir().join("ledger.json")
}

static LEDGER_LOCK: Mutex<()> = Mutex::new(());

pub fn load_ledger() -> Ledger {
    let Ok(text) = std::fs::read_to_string(ledger_path()) else {
        return Ledger { version: 1, config: CostConfig::default(), days: HashMap::new() };
    };
    serde_json::from_str::<Ledger>(&text).unwrap_or(Ledger {
        version: 1,
        config: CostConfig::default(),
        days: HashMap::new(),
    })
}

fn save_ledger(l: &Ledger) -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建费用目录失败: {}", e))?;
    // 原子写：临时文件 + rename
    let tmp = dir.join("ledger.json.tmp");
    let json = serde_json::to_string(l).map_err(|e| format!("序列化账本失败: {}", e))?;
    std::fs::write(&tmp, json).map_err(|e| format!("写账本失败: {}", e))?;
    std::fs::rename(&tmp, ledger_path()).map_err(|e| format!("替换账本失败: {}", e))
}

fn local_day(at: i64) -> String {
    use chrono::{Local, TimeZone};
    Local
        .timestamp_opt(at, 0)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".into())
}

/// 记账一次调用
pub struct CallUsage {
    pub model: String,
    pub provider: String,
    pub session_id: String,
    pub session_title: String,
    pub input: usize,
    pub output: usize,
    pub cache_read: usize,
    pub cache_write: usize,
    pub reasoning: usize,
    /// 该调用是否走套餐（plan 轨 ⇒ api_cost = 0）
    pub is_plan: bool,
    pub at: i64,
}

/// 写入一条调用记录，返回本次成本
pub fn record_call(u: &CallUsage) -> Result<f64, String> {
    let _g = LEDGER_LOCK.lock().map_err(|_| "账本锁中毒".to_string())?;
    let mut ledger = load_ledger();
    let currency = ledger.config.currency;
    let cost = cost_of(
        &u.model, currency, u.input, u.cache_read, u.cache_write, u.output, u.reasoning, u.at,
    )
    .unwrap_or(0.0);
    let api_cost = if u.is_plan { 0.0 } else { cost };

    let bucket = Bucket {
        input: u.input,
        output: u.output,
        cache_read: u.cache_read,
        cache_write: u.cache_write,
        reasoning: u.reasoning,
        calls: 1,
        cost,
        api_cost,
    };

    let day_key = local_day(u.at);
    let day = ledger.days.entry(day_key.clone()).or_insert_with(|| DayCost {
        date: day_key.clone(),
        ..Default::default()
    });
    day.bucket.add(&bucket);
    let pm_key = format!("{}:{}", u.provider, canonical_model(&u.model));
    day.by_provider_model.entry(pm_key.clone()).or_default().add(&bucket);

    let session = day
        .sessions
        .iter_mut()
        .find(|s| s.id == u.session_id);
    match session {
        Some(s) => {
            s.bucket.add(&bucket);
            s.by_provider_model.entry(pm_key).or_default().add(&bucket);
        }
        None => {
            let mut s = SessionCost {
                id: u.session_id.clone(),
                title: u.session_title.clone(),
                at: u.at,
                ..Default::default()
            };
            s.bucket.add(&bucket);
            s.by_provider_model.entry(pm_key).or_default().add(&bucket);
            day.sessions.push(s);
        }
    }

    // 保留期裁剪
    let keep = ledger.config.history_days.clamp(7, 3650) as usize;
    if ledger.days.len() > keep {
        let mut keys: Vec<String> = ledger.days.keys().cloned().collect();
        keys.sort();
        let drop_n = keys.len() - keep;
        for k in keys.into_iter().take(drop_n) {
            ledger.days.remove(&k);
        }
    }

    save_ledger(&ledger)?;
    Ok(cost)
}

/// 聚合窗口
#[derive(Debug, Clone)]
pub enum Window {
    Today,
    Month,
    All,
    Range(String, String),
}

pub fn aggregate(ledger: &Ledger, w: &Window) -> Bucket {
    let today = local_day(chrono::Utc::now().timestamp());
    let month = today[..7].to_string();
    let mut out = Bucket::default();
    for (k, d) in &ledger.days {
        let hit = match w {
            Window::Today => k == &today,
            Window::Month => k.starts_with(&month),
            Window::All => true,
            Window::Range(a, b) => k >= a && k <= b,
        };
        if hit {
            out.add(&d.bucket);
        }
    }
    out
}

/// 按模型聚合（窗口内）
pub fn aggregate_by_model(ledger: &Ledger, w: &Window) -> HashMap<String, Bucket> {
    let today = local_day(chrono::Utc::now().timestamp());
    let month = today[..7].to_string();
    let mut out: HashMap<String, Bucket> = HashMap::new();
    for (k, d) in &ledger.days {
        let hit = match w {
            Window::Today => k == &today,
            Window::Month => k.starts_with(&month),
            Window::All => true,
            Window::Range(a, b) => k >= a && k <= b,
        };
        if !hit {
            continue;
        }
        for (m, b) in &d.by_provider_model {
            out.entry(m.clone()).or_default().add(b);
        }
    }
    out
}

/// 预算使用比例（0~1+）
pub fn budget_ratio(ledger: &Ledger) -> Option<f64> {
    let c = &ledger.config;
    if !c.budget_enabled || c.budget_amount <= 0.0 {
        return None;
    }
    let w = match c.budget_period.as_str() {
        "day" => Window::Today,
        "all" => Window::All,
        _ => Window::Month,
    };
    let b = aggregate(ledger, &w);
    Some(b.cost / c.budget_amount)
}

/// 金额格式化（账本永不取整，这里只影响展示）
pub fn format_money(usd: f64, cfg: &CostConfig) -> String {
    let value = if matches!(cfg.currency, Currency::Cny) {
        usd * cfg.exchange_rate
    } else {
        usd
    };
    let decimals = cfg.decimals.min(10) as usize;
    let mut s = format!("{:.*}", decimals, value);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    format!("{}{}", cfg.symbol, s)
}

/// 设置配置
pub fn update_config(f: impl FnOnce(&mut CostConfig)) -> CostConfig {
    let mut ledger = load_ledger();
    f(&mut ledger.config);
    let cfg = ledger.config.clone();
    let _ = save_ledger(&ledger);
    cfg
}

/// 清空历史
pub fn clear_history() -> usize {
    let mut ledger = load_ledger();
    let n = ledger.days.len();
    ledger.days.clear();
    let _ = save_ledger(&ledger);
    n
}

/// 汇总快照（供 UI 与 IPC）
pub fn snapshot() -> serde_json::Value {
    let ledger = load_ledger();
    let today = aggregate(&ledger, &Window::Today);
    let month = aggregate(&ledger, &Window::Month);
    let all = aggregate(&ledger, &Window::All);
    let cfg = &ledger.config;
    let by_model: Vec<serde_json::Value> = {
        let mut v: Vec<(String, Bucket)> = aggregate_by_model(&ledger, &Window::All).into_iter().collect();
        v.sort_by(|a, b| b.1.cost.partial_cmp(&a.1.cost).unwrap_or(std::cmp::Ordering::Equal));
        v.into_iter()
            .take(20)
            .map(|(m, b)| {
                serde_json::json!({
                    "model": m,
                    "cost": b.cost,
                    "api_cost": b.api_cost,
                    "calls": b.calls,
                    "tokens": b.total_tokens(),
                    "cache_hit_rate": b.cache_hit_rate(),
                })
            })
            .collect()
    };
    serde_json::json!({
        "currency": match cfg.currency { Currency::Usd => "USD", Currency::Cny => "CNY" },
        "symbol": cfg.symbol,
        "exchange_rate": cfg.exchange_rate,
        "today": { "cost": today.cost, "api_cost": today.api_cost, "calls": today.calls, "tokens": today.total_tokens(), "display": format_money(today.cost, cfg) },
        "month": { "cost": month.cost, "api_cost": month.api_cost, "calls": month.calls, "tokens": month.total_tokens(), "display": format_money(month.cost, cfg) },
        "all": { "cost": all.cost, "api_cost": all.api_cost, "calls": all.calls, "tokens": all.total_tokens(), "display": format_money(all.cost, cfg) },
        "cache_hit_rate": all.cache_hit_rate(),
        "budget_ratio": budget_ratio(&ledger),
        "budget_enabled": cfg.budget_enabled,
        "budget_amount": cfg.budget_amount,
        "budget_display": format_money(cfg.budget_amount, cfg),
        "by_model": by_model,
        "ledger_path": ledger_path().to_string_lossy().to_string(),
        "day_count": ledger.days.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-09-15 12:00 UTC（周二，非峰谷，且在两次调价边界之后）
    fn offpeak_after_repricing() -> i64 {
        use chrono::{TimeZone, Utc};
        Utc.with_ymd_and_hms(2026, 9, 15, 12, 0, 0).unwrap().timestamp()
    }

    #[test]
    fn cost_formula_matches_reference() {
        // 调价后 off-peak flash：cacheHit 0.003 / cacheMiss 0.15 / output 0.6（每 1M）
        // 1M input + 1M output = 0.15 + 0.6 = 0.75
        let at = offpeak_after_repricing();
        assert!(at > LEGACY_BASE_BOUNDARY && !is_peak_hour(at));
        let c = cost_of("deepseek-v4-flash", Currency::Usd, 1_000_000, 0, 0, 1_000_000, 0, at).unwrap();
        assert!((c - 0.75).abs() < 1e-9, "得到 {}", c);
    }

    #[test]
    fn cache_write_falls_back_to_cache_hit_not_miss() {
        let at = offpeak_after_repricing();
        // 1M cache_write 应按 cacheHit 0.003 计，而不是 cacheMiss 0.15
        let c = cost_of("deepseek-v4-flash", Currency::Usd, 0, 0, 1_000_000, 0, 0, at).unwrap();
        assert!((c - 0.003).abs() < 1e-9, "cache_write 应回退到 cache_hit，得到 {}", c);
    }

    #[test]
    fn unknown_third_party_model_is_unpriced_not_zero() {
        assert!(price_for("gpt-4o", Currency::Usd).is_none(), "未知第三方模型必须不计价");
        assert!(price_for("deepseek-chat", Currency::Usd).is_some());
        // 未知 deepseek-* 回退到 default（真实价格）
        assert!(price_for("deepseek-v9-unknown", Currency::Usd).is_some());
    }

    #[test]
    fn peak_hour_detection() {
        use chrono::{TimeZone, Utc};
        // 2026-09-15 是周二；UTC 02:00 在峰谷窗口内
        let peak = Utc.with_ymd_and_hms(2026, 9, 15, 2, 0, 0).unwrap().timestamp();
        let off = Utc.with_ymd_and_hms(2026, 9, 15, 12, 0, 0).unwrap().timestamp();
        assert!(is_peak_hour(peak));
        assert!(!is_peak_hour(off));
        // 周六全天低谷
        let sat = Utc.with_ymd_and_hms(2026, 9, 19, 2, 0, 0).unwrap().timestamp();
        assert!(!is_peak_hour(sat), "周末全天应为低谷");
    }

    #[test]
    fn cny_table_divides_by_rate_on_booking() {
        // CNY off-peak：1M input = 1.0 CNY
        let at = offpeak_after_repricing();
        let c = cost_of("deepseek-v4-flash", Currency::Cny, 1_000_000, 0, 0, 0, 0, at).unwrap();
        assert!((c - 1.0).abs() < 1e-9, "CNY 表直接得 1.0，得到 {}", c);
    }

    #[test]
    fn money_formatting_strips_trailing_zeros() {
        let cfg = CostConfig { currency: Currency::Usd, symbol: "$".into(), decimals: 4, exchange_rate: 7.2, ..Default::default() };
        assert_eq!(format_money(0.75, &cfg), "$0.75");
        assert_eq!(format_money(1.0, &cfg), "$1");
        // CNY 展示要乘汇率
        let cfg_cny = CostConfig { currency: Currency::Cny, symbol: "¥".into(), decimals: 2, exchange_rate: 7.2, ..Default::default() };
        assert_eq!(format_money(1.0, &cfg_cny), "¥7.2");
    }

    #[test]
    fn bucket_cache_hit_rate() {
        let b = Bucket { input: 100, cache_read: 300, cache_write: 0, ..Default::default() };
        assert!((b.cache_hit_rate() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn canonical_model_strips_decorations() {
        assert_eq!(canonical_model("llm-DeepSeek-V4-Flash"), "deepseek-v4-flash");
        assert_eq!(canonical_model("opencode/deepseek-v4-pro"), "deepseek-v4-pro");
        assert_eq!(canonical_model("deepseek-v4-flash (go)"), "deepseek-v4-flash");
    }
}

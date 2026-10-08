use serde::{Deserialize, Serialize};

/// 默认上下文窗口（Token）：未经用户配置时使用
pub const DEFAULT_CONTEXT_LIMIT: usize = 128_000;

/// ─── 上下文压缩 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorConfig {
    /// 最大上下文 Token 数（= 模型上下文窗口）
    pub max_tokens: usize,
    /// 保留最近的 N 轮对话不压缩
    pub preserve_recent_turns: usize,
    /// 提示/触发压缩的占用比例阈值（0.85 = 占用达 85% 时触发）
    pub warn_ratio: f64,
    /// 注意衰减率（越早的消息权重越低）
    pub attention_decay_rate: f64,
    /// true = 自动压缩（占用超阈值即自动压缩）；false = 手动压缩（仅提示用户）
    pub auto_compress: bool,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            max_tokens: DEFAULT_CONTEXT_LIMIT,
            preserve_recent_turns: 4,
            warn_ratio: 0.85,
            attention_decay_rate: 0.6,
            auto_compress: false,
        }
    }
}

impl CompressorConfig {
    /// 由调用方给出的窗口大小构造（前端可传入自定义上下文上限）
    pub fn with_limit(max_tokens: usize, auto_compress: bool) -> Self {
        Self {
            max_tokens: max_tokens.max(1000),
            auto_compress,
            ..Self::default()
        }
    }

    /// 触发线（Token）
    pub fn threshold_tokens(&self) -> usize {
        (self.max_tokens as f64 * self.warn_ratio) as usize
    }
}

/// 压缩后的消息
#[derive(Debug, Clone)]
pub struct CompressedMessage {
    pub role: String,
    pub content: String,
    pub estimated_tokens: usize,
}

impl CompressedMessage {
    /// 由普通消息构造（自动估算 Token）
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        let content = content.into();
        let estimated_tokens = ContextCompressor::estimate_tokens(&content);
        Self { role: role.into(), content, estimated_tokens }
    }
}

pub struct ContextCompressor {
    config: CompressorConfig,
}

impl ContextCompressor {
    pub fn new(config: CompressorConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(CompressorConfig::default())
    }

    pub fn config(&self) -> &CompressorConfig {
        &self.config
    }

    /// 粗略估算 Token 数（中文/英文混合：~2.5 chars = 1 token for DeepSeek）
    pub fn estimate_tokens(text: &str) -> usize {
        let char_count = text.chars().count();
        if char_count == 0 {
            return 0;
        }
        // 中文字符约占 1.5 chars/token，英文约 4 chars/token
        // 混合取 2.5 作为平均值
        (char_count as f64 / 2.5).ceil() as usize
    }

    /// 计算一系列消息的总 Token 数
    pub fn total_tokens(&self, messages: &[CompressedMessage]) -> usize {
        messages.iter().map(|m| m.estimated_tokens).sum()
    }

    /// 当前占用比例（0.0 ~ 1.0）
    pub fn usage_ratio(&self, messages: &[CompressedMessage]) -> f64 {
        if self.config.max_tokens == 0 {
            return 0.0;
        }
        (self.total_tokens(messages) as f64 / self.config.max_tokens as f64).min(1.0)
    }

    /// 判断是否需要压缩（占用超过阈值比例）
    pub fn needs_compression(&self, messages: &[CompressedMessage]) -> bool {
        self.total_tokens(messages) > self.config.threshold_tokens()
    }

    /// 压缩消息列表
    /// - 保留最近的 preserve_recent_turns 轮对话不变
    /// - 对更早的消息进行摘要压缩
    /// - 系统消息尽量保留（包含关键指令）
    /// - 只压缩对话上下文，不清空对话
    pub fn compress(&self, messages: &[CompressedMessage]) -> Vec<CompressedMessage> {
        let total = messages.len();
        if total == 0 {
            return vec![];
        }

        let preserve_count = self.config.preserve_recent_turns * 2; // user + assistant per turn
        let preserve_count = preserve_count.min(total);

        let mut compressed = Vec::new();

        // 保留系统消息
        for msg in messages.iter() {
            if msg.role == "system" {
                compressed.push(msg.clone());
            }
        }

        // 压缩更早的消息为摘要（跳过系统消息）
        if total > preserve_count {
            let early_count = total - preserve_count;
            let mut early_messages = Vec::new();
            for msg in messages.iter().take(early_count) {
                if msg.role != "system" {
                    early_messages.push(msg);
                }
            }

            if !early_messages.is_empty() {
                let summary = self.summarize_messages(&early_messages);
                compressed.push(CompressedMessage {
                    role: "system".into(),
                    content: format!("[Conversation Summary — earlier {} turns compressed]\n{}", 
                        early_count / 2, summary),
                    estimated_tokens: Self::estimate_tokens(&format!("[Summary] {}", summary)),
                });
            }
        }

        // 保留最近的消息
        for msg in messages.iter().skip(total.saturating_sub(preserve_count)) {
            if msg.role != "system" || !compressed.iter().any(|m| m.content == msg.content) {
                compressed.push(msg.clone());
            }
        }

        compressed
    }

    /// 将一组消息压缩为摘要
    fn summarize_messages(&self, messages: &[&CompressedMessage]) -> String {
        if messages.is_empty() {
            return String::new();
        }

        let mut summary = String::from("Key points from earlier conversation:\n");

        for (i, msg) in messages.iter().enumerate() {
            let preview = truncate_chars(&msg.content, 200);
            let tag = match msg.role.as_str() {
                "user" => "User asked",
                "assistant" => "AI responded",
                _ => "System noted",
            };
            // 衰减：越早的消息权重越低
            let weight = self.config.attention_decay_rate.powi((messages.len() - i) as i32);
            if weight > 0.1 {
                summary.push_str(&format!("- {}: {}\n", tag, preview));
            }
        }

        summary
    }

    /// 更新配置
    pub fn update_config(&mut self, config: CompressorConfig) {
        self.config = config;
    }
}

/// 按字符数安全截断（避免在 UTF-8 多字节字符中间切分导致 panic）
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(pairs: &[(&str, &str)]) -> Vec<CompressedMessage> {
        pairs.iter().map(|(r, c)| CompressedMessage::new(*r, *c)).collect()
    }

    #[test]
    fn usage_ratio_and_threshold() {
        let c = ContextCompressor::new(CompressorConfig::with_limit(1000, true));
        assert_eq!(c.config().threshold_tokens(), 850);
        // 空上下文占用为 0
        assert_eq!(c.usage_ratio(&[]), 0.0);
        // 2500 chars ≈ 1000 tokens → 超过 1000 上限时被夹到 1.0
        let big = msgs(&[("user", &"a".repeat(2500))]);
        assert!(c.usage_ratio(&big) >= 0.99);
        assert!(c.needs_compression(&big));
        // 小上下文不触发压缩
        let small = msgs(&[("user", "hello")]);
        assert!(!c.needs_compression(&small));
    }

    #[test]
    fn compress_preserves_recent_turns_and_keeps_dialogue() {
        let c = ContextCompressor::new(CompressorConfig::with_limit(1000, true));
        let input = msgs(&[
            ("system", "sys instructions"),
            ("user", &"old question ".repeat(40)),
            ("assistant", &"old answer ".repeat(40)),
            ("user", &"older question ".repeat(40)),
            ("assistant", &"older answer ".repeat(40)),
            ("user", "recent q1"),
            ("assistant", "recent a1"),
            ("user", "recent q2"),
            ("assistant", "recent a2"),
            ("user", "recent q3"),
            ("assistant", "recent a3"),
            ("user", "recent q4"),
            ("assistant", "recent a4"),
        ]);
        // preserve_recent_turns = 4 → 保留最近 8 条
        let out = c.compress(&input);
        assert!(!out.is_empty());
        // 最近一轮对话必须原样保留（不清空对话）
        assert!(out.iter().any(|m| m.content == "recent q4"));
        assert!(out.iter().any(|m| m.content == "recent a4"));
        // 系统消息保留
        assert!(out.iter().any(|m| m.content == "sys instructions"));
        // 早期内容被摘要替代
        assert!(out.iter().any(|m| m.content.contains("Conversation Summary")));
        // 总 Token 必须下降
        assert!(
            c.total_tokens(&out) < c.total_tokens(&input),
            "压缩后 Token 应下降：{} -> {}",
            c.total_tokens(&input),
            c.total_tokens(&out)
        );
    }

    #[test]
    fn estimate_tokens_handles_multibyte() {
        assert_eq!(ContextCompressor::estimate_tokens(""), 0);
        // 25 个中文字符 → 10 tokens
        assert_eq!(ContextCompressor::estimate_tokens(&"中".repeat(25)), 10);
    }
}

use serde::{Deserialize, Serialize};

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

/// ─── 上下文压缩 ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressorConfig {
    /// 最大上下文 Token 数
    pub max_tokens: usize,
    /// 保留最近的 N 轮对话不压缩
    pub preserve_recent_turns: usize,
    /// 压缩阈值（达到 max_tokens 的百分之多少时触发压缩）
    pub compression_threshold: f64,
    /// 注意衰减率（越早的消息权重越低）
    pub attention_decay_rate: f64,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            max_tokens: 6000,
            preserve_recent_turns: 4,
            compression_threshold: 0.7,
            attention_decay_rate: 0.6,
        }
    }
}

/// 压缩后的消息
#[derive(Debug, Clone)]
pub struct CompressedMessage {
    pub role: String,
    pub content: String,
    pub estimated_tokens: usize,
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

    /// 判断是否需要压缩
    pub fn needs_compression(&self, messages: &[CompressedMessage]) -> bool {
        let threshold = (self.config.max_tokens as f64 * self.config.compression_threshold) as usize;
        self.total_tokens(messages) > threshold
    }

    /// 压缩消息列表
    /// - 保留最近的 preserve_recent_turns 轮对话不变
    /// - 对更早的消息进行摘要压缩
    /// - 系统消息尽量保留（包含关键指令）
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

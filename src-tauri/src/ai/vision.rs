use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use std::path::Path;

/// ─── 多模态视觉引擎 ───
///
/// 内置两类视觉 Provider：
/// - `modlens`：即插即用视觉引擎，把图片转译为结构化 JSON 证据（OCR / 版面 / 语义），用于截图理解与语义级看图问答。
/// - `deepseek-ocr`：以「上下文光学压缩」著称的文档 OCR，擅长长文档、公式、表格的结构化还原（输出带版面的 Markdown）。
///
/// 两套引擎都通过 OpenAI 兼容的 `chat/completions` 端点接收 `image_url`，把图片发给用户配置的视觉模型，再把结果转为文本喂给纯文本的 DeepSeek 主模型。

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionConfig {
    /// 视觉引擎：`modlens` 或 `deepseek-ocr`
    pub provider: String,
    pub api_key: String,
    /// OpenAI 兼容视觉端点，默认 https://api.openai.com/v1
    pub base_url: String,
    /// 视觉模型名，如 gpt-4o-mini / glm-4v-plus / deepseek-ocr
    pub model: String,
}

impl Default for VisionConfig {
    fn default() -> Self {
        Self {
            provider: "modlens".to_string(),
            api_key: String::new(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
        }
    }
}

/// 视觉识别的结构化结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResult {
    pub text: String,
    pub provider: String,
    pub image_path: String,
}

static VISION_CONFIG: OnceLock<Mutex<VisionConfig>> = OnceLock::new();

fn config_guard() -> tokio::sync::MutexGuard<'static, VisionConfig> {
    VISION_CONFIG
        .get_or_init(|| Mutex::new(VisionConfig::default()))
        .blocking_lock()
}

/// 更新视觉配置（provider / key / 端点 / 模型）
pub fn set_config(provider: String, api_key: String, base_url: Option<String>, model: Option<String>) {
    let mut cfg = config_guard();
    cfg.provider = provider;
    cfg.api_key = api_key;
    if let Some(url) = base_url {
        cfg.base_url = if url.ends_with('/') { url.trim_end_matches('/').to_string() } else { url };
    }
    if let Some(m) = model {
        cfg.model = m;
    }
}

/// 返回当前配置快照
pub fn get_config() -> VisionConfig {
    (*config_guard()).clone()
}

/// 是否已配置可用的视觉 API
pub fn is_configured() -> bool {
    !get_config().api_key.is_empty()
}

/// 视觉请求超时。
///
/// 这里是 **read_image 卡死的根因**：此前用 `reqwest::Client::new()` 建客户端，
/// 而 `reqwest` 的默认客户端**不带任何超时** —— 视觉端点若不返回（网络卡住、
/// 模型排队、代理吞连接），这个 await 会永久挂起，Agent 循环随之整轮卡死
/// （界面上表现为「思考中…」不动、日志停在「调用工具 read_image」）。
///
/// 取 180s：OCR / 长文档识图确实可能跑一两分钟，但绝不能无限等。
const VISION_TIMEOUT_SECS: u64 = 180;

/// 视觉引擎的 HTTP 客户端（带连接超时 + 整体超时）
fn vision_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .timeout(std::time::Duration::from_secs(VISION_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Vision HTTP client init failed: {}", e))
}

/// 识别一张图片，返回结构化文本证据
pub async fn analyze_image(image_path: &str, prompt: Option<&str>) -> Result<VisionResult, String> {
    let cfg = get_config();
    if cfg.api_key.is_empty() {
        return Err("Vision API Key not configured. Set it via 设置 → 视觉识别.".into());
    }
    if !cfg.base_url.starts_with("http") {
        return Err(format!("Vision Base URL 不合法：{}", cfg.base_url));
    }

    let bytes = std::fs::read(image_path)
        .map_err(|e| format!("Failed to read image {}: {}", image_path, e))?;
    if bytes.is_empty() {
        return Err(format!("Image is empty: {}", image_path));
    }

    let mime = guess_mime(image_path);
    let b64 = base64_encode(&bytes);
    let data_url = format!("data:{};base64,{}", mime, b64);

    let prompt_text = prompt
        .map(|p| p.to_string())
        .unwrap_or_else(|| default_prompt(&cfg.provider));

    let (system, body_format_hint) = system_and_hint(&cfg.provider);

    let body = serde_json::json!({
        "model": cfg.model,
        "max_tokens": 4096,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": system },
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": format!("{}\n\n{}", prompt_text, body_format_hint) },
                    { "type": "image_url", "image_url": { "url": data_url } }
                ]
            }
        ]
    });

    let client = vision_client()?;
    let url = format!("{}/chat/completions", cfg.base_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", cfg.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                format!(
                    "Vision request timed out after {}s（视觉端点无响应：{}）。\
                     请检查视觉 Base URL / 网络，或在「AI 配置」改用可用的视觉模型。",
                    VISION_TIMEOUT_SECS, url
                )
            } else {
                format!("Vision request failed ({}): {}", url, e)
            }
        })?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("Vision API error ({}): {}", status, err_body));
    }

    // 读取响应体同样要有兜底：客户端已设 timeout，这里再包一层，
    // 保证"读到一半不再有数据"的情况也不会挂住整个 Agent 循环。
    let raw = tokio::time::timeout(
        std::time::Duration::from_secs(VISION_TIMEOUT_SECS),
        resp.text(),
    )
    .await
    .map_err(|_| format!("Vision response body timed out after {}s", VISION_TIMEOUT_SECS))?
    .map_err(|e| format!("Vision read error: {}", e))?;

    let json: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| {
            let head: String = raw.chars().take(200).collect();
            format!("Vision parse error: {}（响应开头：{}）", e, head)
        })?;

    let text = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if text.is_empty() {
        return Err("Vision model returned empty result.".into());
    }

    Ok(VisionResult {
        text,
        provider: cfg.provider,
        image_path: image_path.to_string(),
    })
}

/// ModLens 风格的默认提问：走「结构化证据」路线
fn default_prompt(provider: &str) -> String {
    if provider == "deepseek-ocr" {
        "请对这张图片做高质量的文档 OCR：提取其中的全部文字、公式、表格，并尽量保留版面结构。表格用 Markdown 表格呈现，公式保留 LaTeX。".to_string()
    } else {
        "请识别这张图片并返回结构化 JSON 证据，包含：ocr（图中全部文字）、layout（版面/区域描述）、semantics(图片语义、场景、意图)。若图片是报错截图或 UI 设计稿，请在 semantics 中重点描述。".to_string()
    }
}

fn system_and_hint(provider: &str) -> (String, String) {
    if provider == "deepseek-ocr" {
        (
            "You are a precise document OCR engine. Return only extraction results with layout preserved."
                .to_string(),
            "输出 Markdown，保留标题层级与表格结构。".to_string(),
        )
    } else {
        (
            "You are ModLens, a vision engine that turns images into structured text evidence for a text-only LLM."
                .to_string(),
            "输出一个 JSON 对象：{ \"ocr\": string, \"layout\": string, \"semantics\": string }。".to_string(),
        )
    }
}

fn guess_mime(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    }
}

/// 极简 Base64 编码（避免引入额外依赖）
fn base64_encode(bytes: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(CHARS[(b0 >> 2) as usize] as char);
        out.push(CHARS[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        out.push(if chunk.len() > 1 { CHARS[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { CHARS[(b2 & 0x3f) as usize] as char } else { '=' });
    }
    out
}

/// 供 Agent 循环注册的 `read_image` 工具调用
pub async fn read_image(image_path: &str) -> Result<VisionResult, String> {
    analyze_image(image_path, None).await
}

/// 极简 Base64 解码（与上面的编码器配对）
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let table = |c: u8| -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    for chunk in bytes.chunks(4) {
        let vals: Vec<u8> = chunk.iter().map(|&c| table(c).unwrap_or(0)).collect();
        if vals.len() < 2 { break; }
        let b0 = (vals[0] << 2) | (vals[1] >> 4);
        out.push(b0);
        if chunk.len() > 2 && chunk[2] != b'=' {
            let b1 = (vals[1] << 4) | (vals[2] >> 2);
            out.push(b1);
            if chunk.len() > 3 && chunk[3] != b'=' {
                let b2 = (vals[2] << 6) | vals[3];
                out.push(b2);
            }
        }
    }
    if out.is_empty() { Err("Invalid base64 data".into()) } else { Ok(out) }
}

/// 把前端粘贴的图片（base64，可带 data: 前缀）保存为临时文件，返回路径
pub fn save_temp_image(data: &str, ext: &str) -> Result<String, String> {
    // 去掉可能的 "data:image/png;base64," 前缀
    let b64 = data.split(',').last().unwrap_or(data).trim();
    let bytes = base64_decode(b64)?;
    let dir = std::env::temp_dir().join("DeepAhead_paste");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create temp dir: {}", e))?;
    let ext = if ext.trim().is_empty() { "png" } else { ext.trim().trim_start_matches('.') };
    let name = format!("img_{}.{}", chrono::Utc::now().timestamp_millis(), ext);
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("Failed to write temp image: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}
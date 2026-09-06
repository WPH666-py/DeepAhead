use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

fn hide_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        cmd.creation_flags(CREATE_NO_WINDOW_FLAG);
    }
}

 /// ─── 定位捆绑的 Python 解释器 ───
/// 返回 python.exe 的路径，优先捆绑版，fallback 到系统 py 启动器
pub fn bundled_python() -> Result<PathBuf, String> {
    let mut exe_dir = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."));
    exe_dir.pop();

    let mut candidates: Vec<PathBuf> = Vec::new();

    // 开发模式: exe_dir 是 target/debug 或 target/release
    if let Some(file_name) = exe_dir.file_name().and_then(|n| n.to_str()) {
        if file_name == "debug" || file_name == "release" {
            // 从 target/debug 上溯到 src-tauri/python
            candidates.push(exe_dir.join("../../python/python.exe"));
            candidates.push(exe_dir.join("../../../python/python.exe"));
        }
    }

    // 生产/安装模式: python/ 在 exe 旁边
    candidates.push(exe_dir.join("python").join("python.exe"));

    for cand in &candidates {
        if cand.exists() {
            return Ok(cand.canonicalize().unwrap_or_else(|_| cand.clone()));
        }
    }

    // Fallback: 系统 py 启动器（开发机才有）
    Err("No bundled Python found (system 'py' may be used as fallback)".into())
}

/// 获取可用的 Python 解释器路径（优先捆绑版）
pub fn python_interpreter() -> (String, Vec<String>) {
    match bundled_python() {
        Ok(path) => (path.to_string_lossy().to_string(), vec![]),
        Err(_) => {
            // Fallback: 系统的 py 启动器
            ("py".into(), vec!["-3".into()])
        }
    }
}

/// ─── 文件类型判断 ───

const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"];
const OFFICE_EXTS: &[&str] = &["xlsx", "xls", "docx", "doc", "pptx", "ppt"];

fn file_ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

fn is_image(ext: &str) -> bool {
    IMAGE_EXTS.contains(&ext)
}

fn is_office(ext: &str) -> bool {
    OFFICE_EXTS.contains(&ext)
}

/// ─── 底层读取函数（纯 Rust，不 spawn 进程）───

/// 读取文本文件（直接用 Rust std::fs + encoding_rs，不调用 file_ops.exe）
fn read_text(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Cannot read {}: {}", path, e))?;

    // BOM 检测与去除
    let stripped = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..] // UTF-8 BOM
    } else {
        &bytes[..]
    };

    // 尝试 UTF-8，失败时尝试 GBK
    match String::from_utf8(stripped.to_vec()) {
        Ok(s) => Ok(s),
        Err(_) => {
            let (cow, _encoding, had_errors) = encoding_rs::GBK.decode(stripped);
            if had_errors {
                Ok(String::from_utf8_lossy(stripped).into_owned())
            } else {
                Ok(cow.into_owned())
            }
        }
    }
}

/// 读取 Office 文件（纯 Rust：calamine 读 Excel，zip+XML 读 Word/PPT）
pub(crate) fn read_binary(path: &str) -> Result<String, String> {
    let ext = file_ext(path);
    match ext.as_str() {
        "xlsx" | "xls" => read_excel_rust(path),
        "docx" | "doc" => read_docx_rust(path),
        "pptx" | "ppt" => read_pptx_rust(path),
        _ => Err(format!("Unsupported office format: {}", ext)),
    }
}

/// 纯 Rust 读取 Excel（xlsx/xls），输出 TSV 格式
fn read_excel_rust(path: &str) -> Result<String, String> {
    use calamine::{open_workbook_auto, Reader, Data};

    let mut workbook = open_workbook_auto(path)
        .map_err(|e| format!("Failed to open Excel file: {}", e))?;

    let mut result = String::new();
    let sheet_names = workbook.sheet_names().to_vec();

    for (idx, sheet_name) in sheet_names.iter().enumerate() {
        if idx > 0 {
            result.push('\n');
        }
        result.push_str(&format!("=== {} ===\n", sheet_name));

        if let Ok(range) = workbook.worksheet_range(sheet_name) {
            for row in range.rows() {
                let cells: Vec<String> = row.iter().map(|cell| match cell {
                    Data::Empty => String::new(),
                    Data::String(s) => s.clone(),
                    Data::Float(f) => {
                        // 去除无意义的小数尾零
                        let s = f.to_string();
                        if s.contains('.') {
                            s.trim_end_matches('0').trim_end_matches('.').to_string()
                        } else {
                            s
                        }
                    }
                    Data::Int(i) => i.to_string(),
                    Data::Bool(b) => b.to_string(),
                    Data::Error(e) => format!("#ERR:{}", e),
                    Data::DateTime(dt) => {
                        // ExcelDateTime 结构体，直接用 Display 格式化
                        format!("{}", dt)
                    }
                    Data::DurationIso(_) | Data::DateTimeIso(_) => String::new(),
                }).collect();
                result.push_str(&cells.join("\t"));
                result.push('\n');
            }
        }
    }
    Ok(result)
}

/// 纯 Rust 读取 Word 文档（docx 为 ZIP 包，提取 word/document.xml 中的文本）
fn read_docx_rust(path: &str) -> Result<String, String> {
    use std::io::Read;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open docx: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Cannot read docx (not a valid ZIP): {}", e))?;

    let mut doc = archive.by_name("word/document.xml")
        .map_err(|e| format!("word/document.xml not found in docx: {}", e))?;
    let mut xml = String::new();
    doc.read_to_string(&mut xml)
        .map_err(|e| format!("Cannot read document.xml: {}", e))?;

    Ok(extract_docx_text(&xml))
}

/// 从 docx 的 document.xml 中提取文本，段落之间换行
fn extract_docx_text(xml: &str) -> String {
    // 先按段落分割，再提取每个段落的文本
    let para_re = regex::Regex::new(r"<w:p[ >][\s\S]*?</w:p>").unwrap();
    let text_re = regex::Regex::new(r"<w:t[^>]*>([^<]*)</w:t>").unwrap();

    let mut paragraphs: Vec<String> = Vec::new();
    for para in para_re.find_iter(xml) {
        let para_text: String = text_re.captures_iter(para.as_str())
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str())
            .collect::<Vec<_>>()
            .join("");
        if !para_text.trim().is_empty() {
            paragraphs.push(para_text);
        }
    }
    paragraphs.join("\n")
}

/// 纯 Rust 读取 PowerPoint（pptx 为 ZIP 包，提取每页 slide XML 中的文本）
fn read_pptx_rust(path: &str) -> Result<String, String> {
    use std::io::Read;

    let file = std::fs::File::open(path)
        .map_err(|e| format!("Cannot open pptx: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Cannot read pptx (not a valid ZIP): {}", e))?;

    let text_re = regex::Regex::new(r"<a:t[^>]*>([^<]*)</a:t>").unwrap();
    let mut result = String::new();

    for i in 1.. {
        let slide_name = format!("ppt/slides/slide{}.xml", i);
        match archive.by_name(&slide_name) {
            Ok(mut slide) => {
                let mut xml = String::new();
                if slide.read_to_string(&mut xml).is_err() {
                    break;
                }
                let slide_text: String = text_re.captures_iter(&xml)
                    .filter_map(|cap| cap.get(1))
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if !slide_text.trim().is_empty() {
                    result.push_str(&format!("--- Slide {} ---\n{}\n", i, slide_text));
                }
            }
            Err(_) => break,
        }
    }

    if result.is_empty() {
        Err("No slides with text found in pptx".into())
    } else {
        Ok(result)
    }
}

fn read_pdf(path: &str) -> Result<String, String> {
    let candidates = ["py", "python", "python3"];
    let mut python: Option<&str> = None;
    let mut last_err = String::new();
    for &c in &candidates {
        let mut cmd = Command::new(c);
        hide_window(&mut cmd);
        match cmd.arg("--version").output() {
            Ok(o) if o.status.success() => {
                python = Some(c);
                break;
            }
            Ok(o) => {
                last_err = format!(
                    "{} --version 失败: {}",
                    c,
                    String::from_utf8_lossy(&o.stderr)
                )
            }
            Err(e) => last_err = format!("{} 启动失败: {}", c, e),
        }
    }
    let python = python.ok_or_else(|| {
        format!(
            "未找到可用的 Python 解释器 (py/python/python3)。{}",
            last_err
        )
    })?;

    let script = format!(
        r#"
import sys
try:
    import fitz
    doc = fitz.open(r'{}')
    text = []
    for page in doc:
        text.append(page.get_text())
    print('\n'.join(text))
except Exception as e:
    print('ERROR:', e, file=sys.stderr)
    sys.exit(1)
"#,
        path.replace("'", "''")
    );

    let mut cmd = Command::new(python);
    hide_window(&mut cmd);
    let output = cmd
        .args(["-c", &script])
        .output()
        .map_err(|e| format!("无法启动 Python ({}): {}", python, e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        Err(format!(
            "pymupdf 读取 PDF 失败。请安装: {} -m pip install pymupdf。stderr: {} stdout: {}",
            python, stderr, stdout
        ))
    }
}

/// ─── 文件格式分类 ───

fn format_label(ext: &str) -> String {
    match ext {
        "py" => "Python".into(),
        "js" => "JavaScript".into(),
        "ts" => "TypeScript".into(),
        "rs" => "Rust".into(),
        "go" => "Go".into(),
        "java" => "Java".into(),
        "c" | "h" => "C".into(),
        "cpp" | "hpp" | "cc" | "cxx" => "C++".into(),
        "vue" => "Vue".into(),
        "json" => "JSON".into(),
        "yaml" | "yml" => "YAML".into(),
        "toml" => "TOML".into(),
        "md" => "Markdown".into(),
        "txt" => "Text".into(),
        "csv" => "CSV".into(),
        "xlsx" | "xls" => "Excel".into(),
        "docx" | "doc" => "Word".into(),
        "pptx" | "ppt" => "PowerPoint".into(),
        "pdf" => "PDF".into(),
        "html" => "HTML".into(),
        "css" => "CSS".into(),
        "scss" | "sass" => "SCSS".into(),
        "sql" => "SQL".into(),
        "sh" | "bash" => "Shell".into(),
        "ps1" => "PowerShell".into(),
        "xml" => "XML".into(),
        _ => format!("{} File", ext.to_uppercase()),
    }
}

/// ─── 公共数据结构 ───

#[derive(Debug, Clone)]
pub struct ParsedFile {
    /// 文件路径
    pub path: String,
    /// 解析后的文本内容（图片文件为占位描述）
    pub content: String,
    /// 人类可读的文件格式名（"Rust" / "Excel" / "PDF" / "Image (PNG)"）
    pub format: String,
    /// 文件字节数
    pub size_bytes: u64,
    /// 是否为二进制文件
    pub is_binary: bool,
    /// 内容是否被截断
    pub truncated: bool,
    /// 读取是否成功
    pub success: bool,
    /// 错误信息（如果有）
    pub error: Option<String>,
}

impl ParsedFile {
    pub fn file_name(&self) -> &str {
        Path::new(&self.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.path)
    }
}

/// ─── 统一文件解析入口 ───

const MAX_CONTEXT_SIZE: usize = 80 * 1024; // 80KB per file in context

/// 解析任意文件为 AI 可用的文本上下文
///
/// - 图片：返回路径占位描述（不读取字节）
/// - PDF：通过 Python pymupdf 提取文本
/// - Office (xlsx/docx/pptx)：通过 C++ file_ops 提取文本
/// - 代码/文本：通过 C++ file_ops 读取（自动编码检测）
/// - 超过 80KB 的文件截断为首尾各 40KB
pub fn parse_file(path: &str) -> ParsedFile {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return ParsedFile {
                path: path.to_string(),
                content: format!("[无法读取文件: {}]", e),
                format: "Unknown".into(),
                size_bytes: 0,
                is_binary: false,
                truncated: false,
                success: false,
                error: Some(e.to_string()),
            };
        }
    };
    let size = metadata.len();
    let ext = file_ext(path);

    // ── 图片：仅注路径 ──
    if is_image(&ext) {
        return ParsedFile {
            path: path.to_string(),
            content: format!(
                "[图片文件: {} bytes, 格式: {}]\n(用户上传了此图片作为上下文参考。图片内容为二进制数据，AI 助手可通过文件路径引用该文件，但无法直接读取像素内容。)",
                size,
                ext.to_uppercase()
            ),
            format: format!("Image ({})", ext.to_uppercase()),
            size_bytes: size,
            is_binary: true,
            truncated: false,
            success: true,
            error: None,
        };
    }

    // ── 读取原始内容 ──
    let raw_content = if ext == "pdf" {
        match read_pdf(path) {
            Ok(c) => c,
            Err(e) => {
                return ParsedFile {
                    path: path.to_string(),
                    content: format!("[PDF 解析失败: {}]\n请安装 pymupdf: py -m pip install pymupdf", e),
                    format: "PDF".into(),
                    size_bytes: size,
                    is_binary: true,
                    truncated: false,
                    success: false,
                    error: Some(e),
                };
            }
        }
    } else if is_office(&ext) {
        match read_binary(path) {
            Ok(c) => c,
            Err(e) => {
                return ParsedFile {
                    path: path.to_string(),
                    content: format!("[Office 文件解析失败: {}]", e),
                    format: format_label(&ext),
                    size_bytes: size,
                    is_binary: true,
                    truncated: false,
                    success: false,
                    error: Some(e),
                };
            }
        }
    } else {
        match read_text(path) {
            Ok(c) => c,
            Err(e) => {
                return ParsedFile {
                    path: path.to_string(),
                    content: format!("[文件读取失败: {}]", e),
                    format: format_label(&ext),
                    size_bytes: size,
                    is_binary: false,
                    truncated: false,
                    success: false,
                    error: Some(e),
                };
            }
        }
    };

    // ── 截断大文件 ──
    let (content, truncated) = if raw_content.len() > MAX_CONTEXT_SIZE {
        let half = MAX_CONTEXT_SIZE / 2;
        let head: String = raw_content.chars().take(half).collect();
        let tail: String = raw_content
            .chars()
            .rev()
            .take(half)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        (
            format!(
                "{}\n\n... [内容已截断: 共 {} 字符, 仅显示首尾各 {} 字符] ...\n\n{}",
                head,
                raw_content.len(),
                half,
                tail
            ),
            true,
        )
    } else {
        (raw_content, false)
    };

    ParsedFile {
        path: path.to_string(),
        content,
        format: format_label(&ext),
        size_bytes: size,
        is_binary: ext == "pdf" || is_office(&ext),
        truncated,
        success: true,
        error: None,
    }
}

/// 批量解析文件列表
pub fn parse_files(paths: &[String]) -> Vec<ParsedFile> {
    paths.iter().map(|p| parse_file(p)).collect()
}

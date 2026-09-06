use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW_FLAG: u32 = 0x08000000;

fn hide_window(cmd: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW_FLAG);
    }
}

use crate::{DirListResult, FileEntry};

/// 在开发/生产环境中定位 file_ops 工具目录
fn file_ops_dir() -> PathBuf {
    let mut exe_dir = std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("."));
    exe_dir.pop();

    // 开发模式: target/debug/.. -> src-tauri/tools/file-ops
    if let Some(parent) = exe_dir.parent() {
        if parent.file_name().map(|n| n == "debug" || n == "release").unwrap_or(false) {
            let dev_tools = parent.parent()
                .map(|p| p.join("tools").join("file-ops"));
            if let Some(ref p) = dev_tools {
                if p.join("file_ops.exe").exists() {
                    return p.clone();
                }
            }
        }
    }

    // 生产模式: resources/tools/file-ops
    let res_tools = exe_dir.join("..").join("resources").join("tools").join("file-ops");
    if res_tools.join("file_ops.exe").exists() {
        return res_tools.canonicalize().unwrap_or(res_tools);
    }

    // 兜底：当前工作目录
    PathBuf::from("tools").join("file-ops")
}

fn file_ops_exe() -> PathBuf {
    file_ops_dir().join("file_ops.exe")
}

fn list_dir_recursive(path: &str, depth: usize) -> Result<Vec<FileEntry>, String> {
    if depth == 0 {
        return Ok(vec![]);
    }
    let mut entries = vec![];
    let dir = std::fs::read_dir(path).map_err(|e| format!("Cannot read dir: {}", e))?;
    for entry in dir {
        let entry = entry.map_err(|e| format!("Dir entry error: {}", e))?;
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let size = if is_dir {
            0
        } else {
            entry.metadata().map(|m| m.len()).unwrap_or(0)
        };
        let children = if is_dir && depth > 1 {
            Some(list_dir_recursive(&path, depth - 1)?)
        } else {
            None
        };
        entries.push(FileEntry {
            name,
            path,
            is_dir,
            size,
            children,
        });
    }
    // 排序：文件夹在前，文件在后，按名称排序
    entries.sort_by(|a, b| {
        let a_dir = if a.is_dir { 0 } else { 1 };
        let b_dir = if b.is_dir { 0 } else { 1 };
        a_dir.cmp(&b_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// 列出目录内容
#[tauri::command]
pub fn list_directory(path: String, depth: Option<usize>) -> Result<DirListResult, String> {
    let depth = depth.unwrap_or(2);
    let entries = list_dir_recursive(&path, depth)?;
    Ok(DirListResult { entries, path })
}

/// 读取文本文件（直接用 Rust std::fs，避免 spawn file_ops.exe 造成卡顿和黑窗口）
#[tauri::command]
pub fn read_file_content(path: String) -> Result<String, String> {
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Cannot read {}: {}", path, e))?;

    // BOM 检测与去除
    let stripped = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..] // UTF-8 BOM
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        // UTF-16 LE — 转换到 UTF-8
        return decode_utf16le(&bytes[2..]);
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        // UTF-16 BE — 转换到 UTF-8
        return decode_utf16be(&bytes[2..]);
    } else {
        &bytes[..]
    };

    // 尝试 UTF-8
    match String::from_utf8(stripped.to_vec()) {
        Ok(s) => Ok(s),
        Err(_) => {
            // UTF-8 失败，尝试 GBK
            let (cow, _encoding, had_errors) = encoding_rs::GBK.decode(stripped);
            if had_errors {
                // GBK 也失败，回退到 lossy UTF-8
                Ok(String::from_utf8_lossy(stripped).into_owned())
            } else {
                Ok(cow.into_owned())
            }
        }
    }
}

fn decode_utf16le(bytes: &[u8]) -> Result<String, String> {
    let u16s: Vec<u16> = bytes.chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&u16s).map_err(|e| format!("UTF-16 decode error: {:?}", e))
}

fn decode_utf16be(bytes: &[u8]) -> Result<String, String> {
    let u16s: Vec<u16> = bytes.chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&u16s).map_err(|e| format!("UTF-16 decode error: {:?}", e))
}

/// 写入文件（使用 C++ file_ops 工具）
#[tauri::command]
pub fn write_file_content(path: String, content: String) -> Result<String, String> {
    let exe = file_ops_exe();
    if !exe.exists() {
        return Err(format!("file_ops.exe not found at {}", exe.display()));
    }

    let mut cmd = Command::new(&exe);
    hide_window(&mut cmd);
    let mut child = cmd
        .args(["write", &path])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to run file_ops write: {}", e))?;

    if let Some(stdin) = child.stdin.take() {
        use std::io::Write;
        let mut stdin = stdin;
        stdin.write_all(content.as_bytes()).map_err(|e| format!("Write stdin error: {}", e))?;
    }

    let output = child.wait_with_output().map_err(|e| format!("Wait error: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!("Write error: {}", String::from_utf8_lossy(&output.stderr)))
    }
}

/// 删除文件（使用 C++ file_ops 工具）
#[tauri::command]
pub fn delete_file(path: String) -> Result<String, String> {
    let exe = file_ops_exe();
    let mut cmd = Command::new(&exe);
    hide_window(&mut cmd);
    let output = cmd
        .args(["delete", &path])
        .output()
        .map_err(|e| format!("Failed to run file_ops delete: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// 检测文件是否为二进制（使用 C++ file_ops 工具）
#[tauri::command]
pub fn is_binary_file(path: String) -> Result<String, String> {
    let exe = file_ops_exe();
    if !exe.exists() {
        return Ok("".to_string());
    }
    let mut cmd = Command::new(&exe);
    hide_window(&mut cmd);
    let output = cmd
        .args(["isbin", &path])
        .output()
        .map_err(|e| format!("Failed to run file_ops isbin: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Ok("".to_string())
    }
}

/// 使用纯 Rust 读取 Office 二进制文件（xlsx/docx/pptx）文本内容
#[tauri::command]
pub fn read_binary_text(path: String) -> Result<String, String> {
    crate::ai::file_parser::read_binary(&path)
}

/// PDF fallback：使用 Python pymupdf 读取（Windows 优先尝试 py 启动器）
fn read_pdf_with_pymupdf(path: &str) -> Result<String, String> {
    let candidates = ["py", "python", "python3"];
    let mut python: Option<&str> = None;
    let mut last_err = String::new();
    for &c in &candidates {
        let mut cmd = Command::new(c);
        hide_window(&mut cmd);
        match cmd.arg("--version").output() {
            Ok(o) if o.status.success() => { python = Some(c); break; }
            Ok(o) => last_err = format!("{} --version 失败: {}", c, String::from_utf8_lossy(&o.stderr)),
            Err(e) => last_err = format!("{} 启动失败: {}", c, e),
        }
    }
    let python = python.ok_or_else(|| format!("未找到可用的 Python 解释器 (py/python/python3)。{}", last_err))?;

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

/// 用系统默认程序打开文件
#[tauri::command]
pub fn open_file_with_default_app(path: String) -> Result<(), String> {
    let mut cmd = Command::new("cmd");
    hide_window(&mut cmd);
    cmd.args(["/C", "start", "", &path])
        .spawn()
        .map_err(|e| format!("无法打开文件: {}", e))?;
    Ok(())
}

/// 读取文件为字节数组（用于图片预览）
#[tauri::command]
pub fn read_file_bytes(path: String) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Cannot access file: {}", e))?;
    if metadata.len() > 20 * 1024 * 1024 {
        return Err("File too large (>20MB).".into());
    }
    std::fs::read(&path).map_err(|e| format!("Read error: {}", e))
}

/// 智能读取：文本/Office二进制走 C++ file_ops，PDF 走 Python，图片返回特殊标记
#[tauri::command]
pub fn smart_read_file(path: String) -> Result<String, String> {
    let ext = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 图片直接走二进制预览
    let image_exts = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"];
    if image_exts.contains(&ext.as_str()) {
        return Err("IMAGE_PREVIEW".into());
    }

    // PDF 直接走 Python pymupdf
    if ext == "pdf" {
        return read_pdf_with_pymupdf(&path);
    }

    // Office 二进制走 C++ file_ops binary
    let office_exts = ["xlsx", "xls", "docx", "doc", "pptx", "ppt"];
    if office_exts.contains(&ext.as_str()) {
        return read_binary_text(path);
    }

    // 其他文件统一走 C++ file_ops read（含 CSV/代码/文本等）
    read_file_content(path)
}

fn escape_md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ").replace('\r', "")
}

fn convert_tsv_to_markdown(text: &str) -> String {
    let mut result = String::new();
    let mut in_header = false;
    for line in text.lines() {
        if (line.starts_with("=== ") && line.ends_with(" ===")) || (line.starts_with("--- Slide ") && line.ends_with(" ---")) {
            result.push_str(line);
            result.push('\n');
            in_header = true;
            continue;
        }
        if line.trim().is_empty() {
            result.push('\n');
            continue;
        }
        let cells: Vec<String> = line.split('\t').map(escape_md_cell).collect();
        result.push_str("| ");
        result.push_str(&cells.join(" | "));
        result.push_str(" |\n");
        if in_header {
            result.push_str("|");
            for _ in 0..cells.len() {
                result.push_str(" --- |");
            }
            result.push('\n');
            in_header = false;
        }
    }
    result
}

/// 解析一行 CSV（支持双引号包裹字段、字段内逗号/转义引号）
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            cells.push(cur.clone());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur);
    cells
}

fn convert_csv_to_markdown(text: &str) -> String {
    let mut result = String::new();
    let mut first_line = true;
    for line in text.lines() {
        if line.trim().is_empty() {
            result.push('\n');
            continue;
        }
        let cells: Vec<String> = parse_csv_line(line).iter().map(|c| escape_md_cell(c.trim())).collect();
        result.push_str("| ");
        result.push_str(&cells.join(" | "));
        result.push_str(" |\n");
        if first_line {
            result.push_str("|");
            for _ in 0..cells.len() {
                result.push_str(" --- |");
            }
            result.push('\n');
            first_line = false;
        }
    }
    result
}

/// Excel 预览为 Markdown 表格（C++ file_ops binary 读取，前端用 preview.md 渲染）
#[tauri::command]
pub fn preview_excel_as_markdown(path: String, _sheet: Option<String>) -> Result<String, String> {
    let text = read_binary_text(path)?;
    Ok(convert_tsv_to_markdown(&text))
}

/// CSV 预览为 Markdown 表格（C++ file_ops read 读取，前端用 preview.md 渲染）
#[tauri::command]
pub fn preview_csv_as_markdown(path: String) -> Result<String, String> {
    let text = read_file_content(path)?;
    Ok(convert_csv_to_markdown(&text))
}

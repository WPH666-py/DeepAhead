//! ─── 自动更新（检测 Gitee 新版本 → 提示 → 下载 → 卸载旧版并重装）───
//!
//! 设计要点（Windows 自更新的固有约束，必须显式处理）：
//!
//! 1. **运行中的 exe 不能被替换**。所以不能在进程内"就地覆盖"，
//!    必须由**游离的辅助脚本**在本进程退出后执行安装。
//! 2. **卸载 + 重装**由辅助脚本串行完成：等待本进程 PID 消失 → 静默卸载旧版
//!    → 静默安装新版 → 重新拉起应用。
//! 3. **检测不依赖 token**：Gitee 公开仓库的 releases 接口可直接读取，
//!    因此终端用户无需配置任何凭据即可检查更新。
//! 4. **版本比较用语义化版本**（主.次.修订），预发布后缀不参与比较但会被记录。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 默认的更新源仓库（Gitee）
pub const DEFAULT_REPO: &str = "ph-wang_admin/DeepAhead";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateAsset {
    pub name: String,
    pub size: u64,
    pub download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateInfo {
    /// 当前版本
    pub current: String,
    /// Gitee 上最新版本（无则空）
    pub latest: String,
    /// 是否有更新
    pub has_update: bool,
    /// 发布说明（截断）
    pub notes: String,
    pub published_at: String,
    /// 安装包（优先选名字里含 setup / .exe 的附件）
    pub asset: Option<UpdateAsset>,
    /// 检测失败原因（非致命，界面可静默）
    pub error: Option<String>,
}

/// 语义化版本比较：返回 a 是否比 b 新。
/// 容忍 `v` 前缀与 `-rc.1` 之类的预发布后缀。
pub fn is_newer(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> (u64, u64, u64) {
        let core = v.trim().trim_start_matches('v').trim_start_matches('V');
        let core = core.split(['-', '+']).next().unwrap_or(core);
        let mut it = core.split('.').map(|s| {
            s.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u64>()
                .unwrap_or(0)
        });
        (
            it.next().unwrap_or(0),
            it.next().unwrap_or(0),
            it.next().unwrap_or(0),
        )
    }
    parse(a) > parse(b)
}

/// Gitee releases 接口的原始响应（只取需要的字段）
#[derive(Debug, Deserialize)]
struct GiteeRelease {
    #[serde(default)]
    tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    assets: Vec<GiteeAsset>,
}

#[derive(Debug, Deserialize)]
struct GiteeAsset {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    browser_download_url: String,
}

/// 检查 Gitee 上的最新发布（无需 token）
pub async fn check_update(repo: &str, current: &str) -> UpdateInfo {
    let mut info = UpdateInfo {
        current: current.to_string(),
        ..Default::default()
    };
    let url = format!("https://gitee.com/api/v5/repos/{}/releases", repo);
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .user_agent("DeepAhead-Updater")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            info.error = Some(format!("HTTP 客户端初始化失败: {}", e));
            return info;
        }
    };
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => {
            info.error = Some(format!("无法访问 Gitee：{}", e));
            return info;
        }
    };
    if !resp.status().is_success() {
        info.error = Some(format!("Gitee 返回 {}", resp.status()));
        return info;
    }
    // Gitee 的 releases 接口返回 `Content-Type: application/json`，**不带 charset**。
    // reqwest 的 `Response::json()` 在没有 charset 时会按 Latin-1/Windows-1252 解码，
    // 于是发布说明里的中文全变成乱码。这里必须自己按 UTF-8 解码。
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            info.error = Some(format!("读取发布列表失败: {}", e));
            return info;
        }
    };
    let releases: Vec<GiteeRelease> = match parse_releases(&body, content_type.as_deref()) {
        Ok(v) => v,
        Err(e) => {
            info.error = Some(e);
            return info;
        }
    };

    // 在所有发布里挑版本号最大的（不只信"最新"排序）
    let mut best: Option<GiteeRelease> = None;
    for r in releases {
        if r.tag_name.trim().is_empty() {
            continue;
        }
        match &best {
            None => best = Some(r),
            Some(b) => {
                if is_newer(&r.tag_name, &b.tag_name) {
                    best = Some(r);
                }
            }
        }
    }
    let Some(best) = best else {
        info.error = Some("Gitee 上暂无发布".into());
        return info;
    };

    info.latest = best.tag_name.clone();
    info.published_at = best.created_at.clone();
    /*
     * 更新说明的取值顺序（v0.5.6 起）：
     *   1. 内置说明：docs/releases/v<版本>.md 在编译期打进程序，最可靠；
     *   2. 远端正文：仅当该版本没有内置说明、且**通过乱码校验**时才用；
     *   3. 兜底文案：版本号 + 升级方式，绝不给用户看乱码或半截文本。
     */
    let remote_notes = sanitize_notes(&best.body);
    info.notes = match crate::release_notes::builtin_notes(&best.tag_name) {
        Some(local) => clean_builtin_notes(local),
        None => {
            if remote_notes.is_empty() {
                let name = sanitize_notes(&best.name);
                if name.is_empty() {
                    crate::release_notes::fallback_notes(&info.latest, &info.current)
                } else {
                    name
                }
            } else {
                remote_notes
            }
        }
    };
    info.notes = info.notes.chars().take(4000).collect();

    // 选安装包：优先 .exe 且名字含 setup
    let pick = best
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".exe") && a.name.to_lowercase().contains("setup"))
        .or_else(|| best.assets.iter().find(|a| a.name.to_lowercase().ends_with(".exe")));
    info.asset = pick.map(|a| UpdateAsset {
        name: a.name.clone(),
        size: a.size,
        download_url: a.browser_download_url.clone(),
    });

    info.has_update = is_newer(&best.tag_name, current);
    if info.has_update && info.asset.is_none() {
        info.error = Some("检测到新版本，但该发布下没有可用的安装包附件".into());
    }
    info
}

/// 下载安装包到临时目录，返回本地路径。
/// 通过 `on_progress(已下载, 总大小)` 回调上报进度。
///
/// **可取消**：前端点了「取消下载 / 关闭弹框」后 [cancel_download] 会置位，
/// 这里在 200ms 内停止读取并**删除半成品文件**（不留 96MB 垃圾在临时目录）。
pub async fn download_installer(
    url: &str,
    file_name: &str,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("DeepAhead_update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败: {}", e))?;
    let dest = dir.join(file_name);

    // 本次下载开始：清掉取消位
    reset_cancel();

    // 清理上一次下载残留的安装包，避免每次更新都在临时目录里堆一个 100 MB 文件
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_lowercase();
            let is_installer = name.ends_with(".exe");
            let is_legacy_helper = name.ends_with(".cmd");
            if (is_installer || is_legacy_helper) && p != dest {
                let _ = std::fs::remove_file(&p);
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .user_agent("DeepAhead-Updater")
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {}", e))?;
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载失败: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败：HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&dest).map_err(|e| format!("创建文件失败: {}", e))?;
    use std::io::Write;
    let mut downloaded: u64 = 0;
    loop {
        // 取消位优先：用 select 保证"点了取消立刻停"，而不是等下一个数据块
        let chunk = tokio::select! {
            biased;
            _ = wait_cancel() => {
                drop(file);
                let _ = std::fs::remove_file(&dest);
                return Err(CANCELLED.into());
            }
            c = resp.chunk() => c.map_err(|e| format!("读取下载流失败: {}", e))?,
        };
        let Some(chunk) = chunk else { break };
        file.write_all(&chunk).map_err(|e| format!("写入失败: {}", e))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush().map_err(|e| format!("落盘失败: {}", e))?;

    // 基本校验：安装包不应该太小
    if downloaded < 1024 * 1024 {
        let _ = std::fs::remove_file(&dest);
        return Err(format!("下载内容异常（仅 {} 字节），已丢弃", downloaded));
    }
    Ok(dest)
}

/// 下载被用户取消时返回的错误串（前端据此静默复位，不弹"失败"）
pub const CANCELLED: &str = "下载已取消";

static CANCEL_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 复位取消位（每次开始下载前调用）
pub fn reset_cancel() {
    CANCEL_FLAG.store(false, std::sync::atomic::Ordering::SeqCst);
}

/// 请求取消当前下载
pub fn cancel_download() {
    CANCEL_FLAG.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// 是否已请求取消
pub fn is_cancelled() -> bool {
    CANCEL_FLAG.load(std::sync::atomic::Ordering::SeqCst)
}

/// 取消位一旦置位就立即返回（用于 select!）
async fn wait_cancel() {
    while !is_cancelled() {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// VBScript 字符串字面量转义（内部的双引号要写成两个）
fn vbs_str(s: &str) -> String {
    s.replace('"', "\"\"")
}

/// 发布列表的 JSON 解码。
///
/// **必须按 UTF-8 显式解码**：Gitee 返回 `Content-Type: application/json`（不带 charset），
/// 而 `reqwest::Response::json()` 在缺 charset 时退化为 Latin-1，中文会整段变乱码。
/// 这里只信任 `charset=` 明确声明的编码，其余一律按 UTF-8 处理。
fn parse_releases(bytes: &[u8], content_type: Option<&str>) -> Result<Vec<GiteeRelease>, String> {
    let declared = content_type
        .and_then(|ct| {
            ct.split(';')
                .map(|p| p.trim())
                .find(|p| p.to_lowercase().starts_with("charset="))
                .map(|p| p[8..].trim().trim_matches('"').to_lowercase())
        })
        .unwrap_or_else(|| "utf-8".to_string());

    let text = match declared.as_str() {
        "utf-8" | "utf8" => String::from_utf8(bytes.to_vec())
            .map_err(|e| format!("解析发布列表失败（响应不是合法 UTF-8）: {}", e))?,
        // Gitee 历史上只出现过 UTF-8；其它编码声明按 UTF-8 兜底而不是拒绝
        _ => String::from_utf8_lossy(bytes).into_owned(),
    };
    serde_json::from_str::<Vec<GiteeRelease>>(&text)
        .map_err(|e| format!("解析发布列表失败: {}", e))
}

/// 清洗发布说明：去掉会撑乱弹框的不可见控制字符。
/// **不做**乱码猜测式改写——一旦发现典型乱码就把说明交还给"版本号 + 下载按钮"，
/// 因为把乱码"猜回来"同样是错的。
fn sanitize_notes(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    let cleaned = collapse_blank_lines(&cleaned);
    if looks_like_mojibake(&cleaned) {
        return String::new();
    }
    cleaned
}

/// 内置说明（docs/releases/v*.md）的清洗：去掉开头那行标题。
/// 弹框顶部已经有「当前 0.5.5 → v0.5.6」的版本跃迁，正文不必再重复一遍标题。
fn clean_builtin_notes(raw: &str) -> String {
    let cleaned = sanitize_notes(raw);
    let mut lines: Vec<&str> = cleaned.lines().collect();
    while let Some(first) = lines.first() {
        let t = first.trim();
        if t.is_empty() || t.starts_with('#') {
            lines.remove(0);
        } else {
            break;
        }
    }
    // 文档里常用的 --- 分隔线在弹框里没有意义，去掉首尾的
    while let Some(last) = lines.last() {
        let t = last.trim();
        if t.is_empty() || t == "---" {
            lines.pop();
        } else {
            break;
        }
    }
    lines.join("\n").trim().to_string()
}

/// 折叠连续空行（发布说明里常有 3 个以上空行，弹框里很难看）
fn collapse_blank_lines(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut blanks = 0usize;
    for line in s.lines() {
        if line.trim().is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push(line.trim_end());
    }
    out.join("\n").trim().to_string()
}

/// 典型 UTF-8 被按单字节编码误解的"乱码字符"（如「锟斤拷」「烫烫烫」「Ã」「â」）
const MOJIBAKE_MARKERS: [&str; 10] = [
    "锟斤拷", "烫烫烫", "屯屯屯", "Ã", "Â", "â€", "ã€", "å", "æ", "é",
];

/// 是否明显是乱码（中英混排的正常说明不会命中）
fn looks_like_mojibake(s: &str) -> bool {
    if s.trim().is_empty() {
        return false;
    }
    // 「锟斤拷」是 UTF-8→GBK 误解码的产物：**重复出现**才算乱码，
    // 因为"锟斤拷"三个字本身作为正常词几乎不可能连用，但为了不误伤单个出现，
    // 这里要求重复。
    if s.matches("锟斤拷").count() >= 2 || s.matches("烫烫烫").count() >= 2 {
        return true;
    }
    // 命中 2 类典型乱码标记即判定
    let hits = MOJIBAKE_MARKERS.iter().filter(|m| s.contains(**m)).count();
    if hits >= 2 {
        return true;
    }
    // 或者：典型"高位拉丁字母"占比异常（真正的乱码里它们是主角）
    let total = s.chars().count();
    if total < 12 {
        return false;
    }
    let weird = s
        .chars()
        .filter(|c| matches!(*c, 'Ã' | 'Â' | 'â' | 'ã' | 'å' | 'æ' | 'ç' | 'è' | 'é' | 'ð'))
        .count();
    weird >= 3 && weird * 6 > total
}

/// 更新/卸载辅助脚本所在目录
pub fn helper_dir() -> PathBuf {
    std::env::temp_dir().join("DeepAhead_update")
}

/// 辅助脚本的日志（无控制台，出问题只能靠它排查）
pub fn helper_log_path() -> PathBuf {
    helper_dir().join("update.log")
}

/// 用 wscript.exe 启动一个**无窗口**的 VBScript。
///
/// 为什么不用批处理：
/// - 之前用 `cmd /C script.cmd` + `DETACHED_PROCESS|CREATE_NO_WINDOW`，
///   但批处理里的 `tasklist | find`、`timeout`、`start` 会在 Windows 11 上
///   弹出 Windows Terminal 黑窗（用户实测到标题为 `find "292"` 的窗口）。
///   而且 `DETACHED_PROCESS` 与 `CREATE_NO_WINDOW` 语义冲突，隐藏并不可靠。
/// - `wscript.exe` 是 GUI 子系统程序，**从不分配控制台**；配合 `//B`（批处理模式）
///   连脚本错误弹窗都不会有。内部所有 Run 都用窗口样式 0（隐藏）。
fn spawn_hidden_vbs(script_name: &str, script: &str) -> Result<PathBuf, String> {
    let dir = helper_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建辅助目录失败: {}", e))?;
    let path = dir.join(script_name);
    // VBScript 需要 UTF-16LE 才能正确读中文路径；这里用 UTF-16LE + BOM 写入
    let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
    for unit in script.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&path, bytes).map_err(|e| format!("写入辅助脚本失败: {}", e))?;

    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new("wscript.exe")
        // //B = 批处理模式（无 UI），//Nologo = 不显示版本信息
        .args(["//B", "//Nologo", &path.to_string_lossy()])
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map_err(|e| format!("启动辅助脚本失败: {}", e))?;
    Ok(path)
}

/// VBScript 公共头：等待指定 PID 退出（用 WMI，无控制台）
fn vbs_header(pid: u32, log: &str) -> String {
    format!(
        "Option Explicit\r\n\
Dim sh, fso, pid, logPath\r\n\
Set sh = CreateObject(\"WScript.Shell\")\r\n\
Set fso = CreateObject(\"Scripting.FileSystemObject\")\r\n\
pid = {pid}\r\n\
logPath = \"{log}\"\r\n\
\r\n\
Sub LogLine(msg)\r\n\
  Dim f\r\n\
  On Error Resume Next\r\n\
  Set f = fso.OpenTextFile(logPath, 8, True)\r\n\
  f.WriteLine Now & \"  \" & msg\r\n\
  f.Close\r\n\
  On Error GoTo 0\r\n\
End Sub\r\n\
\r\n\
Function ProcRunning(p)\r\n\
  Dim col\r\n\
  ProcRunning = False\r\n\
  On Error Resume Next\r\n\
  Set col = GetObject(\"winmgmts:\\.\\root\\cimv2\").ExecQuery(\"SELECT ProcessId FROM Win32_Process WHERE ProcessId=\" & p)\r\n\
  If Err.Number = 0 Then\r\n\
    If col.Count > 0 Then ProcRunning = True\r\n\
  End If\r\n\
  On Error GoTo 0\r\n\
End Function\r\n\
\r\n\
Dim waited\r\n\
waited = 0\r\n\
Do While ProcRunning(pid) And waited < 600\r\n\
  WScript.Sleep 1000\r\n\
  waited = waited + 1\r\n\
Loop\r\n\
LogLine \"app exited after \" & waited & \"s\"\r\n",
        pid = pid,
        log = vbs_str(log)
    )
}

/// 生成更新用的 VBScript 正文（纯函数，便于测试）
pub fn build_update_script(
    pid: u32,
    installer: &str,
    uninstaller: &str,
    appexe: &str,
    log: &str,
) -> String {
    format!(
        "{header}\
\r\n\
If fso.FileExists(\"{uninstaller}\") Then\r\n\
  LogLine \"running uninstaller (silent, hidden)\"\r\n\
  On Error Resume Next\r\n\
  sh.Run \"\"\"\" & \"{uninstaller}\" & \"\"\" /S\", 0, True\r\n\
  LogLine \"uninstaller returned\"\r\n\
  On Error GoTo 0\r\n\
Else\r\n\
  LogLine \"no uninstaller at {uninstaller}\"\r\n\
End If\r\n\
\r\n\
LogLine \"running installer (silent, hidden)\"\r\n\
On Error Resume Next\r\n\
sh.Run \"\"\"\" & \"{installer}\" & \"\"\" /S\", 0, True\r\n\
LogLine \"installer returned\"\r\n\
On Error GoTo 0\r\n\
\r\n\
If fso.FileExists(\"{appexe}\") Then\r\n\
  LogLine \"relaunching app\"\r\n\
  sh.Run \"\"\"\" & \"{appexe}\" & \"\"\"\", 1, False\r\n\
Else\r\n\
  LogLine \"app exe missing after install: {appexe}\"\r\n\
End If\r\n\
\r\n\
LogLine \"done; self-deleting helper\"\r\n\
On Error Resume Next\r\n\
fso.DeleteFile WScript.ScriptFullName, True\r\n\
On Error GoTo 0\r\n",
        header = vbs_header(pid, log),
        uninstaller = vbs_str(uninstaller),
        installer = vbs_str(installer),
        appexe = vbs_str(appexe),
    )
}

/// 生成卸载用的 VBScript 正文（纯函数，便于测试）
pub fn build_uninstall_script(pid: u32, uninstaller: &str, log: &str) -> String {
    format!(
        "{header}\
\r\n\
LogLine \"running uninstaller (silent, hidden)\"\r\n\
On Error Resume Next\r\n\
sh.Run \"\"\"\" & \"{uninstaller}\" & \"\"\" /S\", 0, True\r\n\
LogLine \"uninstaller returned\"\r\n\
On Error GoTo 0\r\n\
\r\n\
LogLine \"done; self-deleting helper\"\r\n\
On Error Resume Next\r\n\
fso.DeleteFile WScript.ScriptFullName, True\r\n\
On Error GoTo 0\r\n",
        header = vbs_header(pid, log),
        uninstaller = vbs_str(uninstaller),
    )
}

/// 生成并启动"游离"更新脚本：等待本进程退出 → 静默卸载 → 静默安装 → 重启。
///
/// 为什么必须交给独立进程：
/// - 运行中的 exe 被占用，安装程序无法覆盖它；
/// - 卸载更不能在进程内做（会删掉自己的文件）。
///
/// 全程**无窗口**：wscript.exe + 所有 Run 用窗口样式 0，日志写文件而不是控制台。
pub fn launch_update_installer(installer: &std::path::Path) -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {}", e))?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "无法获取安装目录".to_string())?
        .to_path_buf();
    let uninstaller = install_dir.join("uninstall.exe");
    let log = helper_log_path();

    let script = build_update_script(
        std::process::id(),
        &installer.to_string_lossy(),
        &uninstaller.to_string_lossy(),
        &current_exe.to_string_lossy(),
        &log.to_string_lossy(),
    );
    spawn_hidden_vbs("deepahead-update.vbs", &script)
}

/// 一键静默卸载：等待本进程退出 → 静默运行卸载器 → 自删除辅助脚本。
/// 全程无窗口。返回辅助脚本路径。
pub fn launch_uninstaller() -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {}", e))?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "无法获取安装目录".to_string())?
        .to_path_buf();
    let uninstaller = install_dir.join("uninstall.exe");
    if !uninstaller.exists() {
        return Err(format!(
            "未找到卸载程序：{}\n（可能是免安装/开发模式运行）",
            uninstaller.to_string_lossy()
        ));
    }

    let log = helper_log_path();
    let script = build_uninstall_script(
        std::process::id(),
        &uninstaller.to_string_lossy(),
        &log.to_string_lossy(),
    );
    spawn_hidden_vbs("deepahead-uninstall.vbs", &script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison() {
        assert!(is_newer("v0.4.2", "0.4.1"));
        assert!(is_newer("0.5.0", "0.4.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("v0.4.1", "0.4.1"));
        assert!(!is_newer("0.4.0", "0.4.1"));
        // 预发布后缀不参与比较
        assert!(is_newer("v0.4.2-rc.1", "0.4.1"));
        assert!(!is_newer("v0.4.1-rc.1", "0.4.1"));
        // 容错
        assert!(is_newer("v0.4.10", "0.4.9"));
        assert!(!is_newer("", "0.4.1"));
    }

    #[test]
    fn parse_handles_missing_parts() {
        assert!(is_newer("v1", "0.9.9"));
        assert!(!is_newer("v1.0", "1.0.0"));
    }

    /// **回归**：Gitee 返回 `Content-Type: application/json`（不带 charset）时，
    /// 发布说明里的中文必须原样保留 —— 曾经因为走 `Response::json()` 按 Latin-1
    /// 解码，弹框里整段更新内容变成乱码。
    ///
    /// 注意：这里用普通字符串（`\n` 是 JSON 需要的转义），**不要**换成原始字符串——
    /// 发布说明里的 `## 标题` 会提前闭合 `"##` 定界符。
    #[test]
    fn releases_json_without_charset_keeps_chinese() {
        let json = "[{\"tag_name\":\"v0.5.6\",\"name\":\"DeepAhead v0.5.6\",\
                    \"body\":\"修复\\n更新内容乱码问题，按钮样式重做\",\
                    \"created_at\":\"2026-10-09T21:22:57+08:00\",\"assets\":[\
                    {\"name\":\"DeepAhead_0.5.6_x64-setup.exe\",\"size\":101052529,\
                    \"browser_download_url\":\"https://gitee.com/x/y/releases/download/v0.5.6/a.exe\"}]}]";
        let parsed = parse_releases(json.as_bytes(), Some("application/json"))
            .expect("不带 charset 的 application/json 也必须能解析");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tag_name, "v0.5.6");
        assert!(
            parsed[0].body.contains("更新内容乱码问题"),
            "中文必须原样保留，实际为：{}",
            parsed[0].body
        );
        assert!(parsed[0].body.contains('\n'), "JSON 的 \\n 应解码为真实换行");
        assert!(parsed[0].assets[0].name.contains("0.5.6"));
    }

    /// 显式声明 charset 时按声明处理；带 charset 的 UTF-8 同样不能坏
    #[test]
    fn releases_json_with_charset_also_ok() {
        let json = "[{\"tag_name\":\"v0.5.6\",\"name\":\"n\",\"body\":\"中文说明\",\
                    \"created_at\":\"\",\"assets\":[]}]";
        let a = parse_releases(json.as_bytes(), Some("application/json; charset=utf-8")).unwrap();
        assert_eq!(a[0].body, "中文说明");
        let b = parse_releases(json.as_bytes(), None).unwrap();
        assert_eq!(b[0].body, "中文说明");
    }

    /// 发布说明清洗：控制字符与非 UTF-8 字节不得原样进弹框
    #[test]
    fn notes_are_sanitized() {
        // 控制字符被剔除，空行折叠
        let cleaned = sanitize_notes("标题\u{0}\u{7}\n\n\n\n正文\u{1b}[31m");
        assert!(!cleaned.contains('\u{0}') && !cleaned.contains('\u{1b}'));
        assert!(!cleaned.contains("\n\n\n"), "连续空行应被折叠: {:?}", cleaned);
        assert!(cleaned.contains("正文"));

        // 典型乱码 → 直接置空（宁可少显示，也不显示乱码）
        assert_eq!(sanitize_notes("æ´æ°åå®¹ä¹±ç Ã©Ã¥"), "");
        assert_eq!(sanitize_notes("锟斤拷锟斤拷锟斤拷"), "");

        // 正常中英混排不受影响
        let normal = "## DeepAhead v0.5.6\n修复 Gitee 中文乱码 + 按钮样式（96 MB）";
        assert_eq!(sanitize_notes(normal), normal);
    }

    /// 下载取消位：置位后可被读到，复位后清除
    #[test]
    fn cancel_flag_roundtrip() {
        reset_cancel();
        assert!(!is_cancelled());
        cancel_download();
        assert!(is_cancelled());
        reset_cancel();
        assert!(!is_cancelled());
    }

    /// 临时诊断：打印内置说明清洗后的实际展示内容（发布前肉眼确认）。
    /// 运行：`cargo test --lib -- --ignored live_gitee_notes --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_gitee_notes_have_no_mojibake() {
        let info = check_update(DEFAULT_REPO, "0.0.1").await;
        println!("---- latest={} error={:?} ----", info.latest, info.error);
        println!("---- notes ({} chars) ----", info.notes.chars().count());
        println!("{}", info.notes);
        println!("---- end ----");
        assert!(info.error.is_none(), "检查更新报错：{:?}", info.error);
        assert!(
            !looks_like_mojibake(&info.notes),
            "更新说明仍是乱码：{}",
            info.notes
        );
    }

    /// 内置发布说明：弹框里的"更新内容"始终可用（不依赖 Gitee 正文健康度）
    #[test]
    fn builtin_notes_exist_and_are_clean() {
        // 每个已发布的版本都应该有内置说明（docs/releases/v*.md）
        for v in ["0.5.4", "0.5.5", "v0.4.1"] {
            let notes = crate::release_notes::builtin_notes(v);
            assert!(notes.is_some(), "版本 {} 应有内置发布说明", v);
            let cleaned = clean_builtin_notes(notes.unwrap());
            assert!(!cleaned.is_empty(), "{} 的说明清洗后不应为空", v);
            assert!(!cleaned.starts_with('#'), "标题行应被去掉: {}", cleaned);
            assert!(!looks_like_mojibake(&cleaned), "内置说明不应判定为乱码");
        }
        assert!(crate::release_notes::builtin_notes("9.9.9").is_none());
    }

    /// 兜底文案：远端与内置都缺时，至少给出可读的版本信息
    #[test]
    fn fallback_notes_are_usable() {
        let f = crate::release_notes::fallback_notes("v0.5.6", "0.5.5");
        assert!(f.contains("v0.5.6") && f.contains("0.5.5"));
        assert!(!looks_like_mojibake(&f));
    }

    /// 真实网络测试：验证 Rust 侧的 Gitee 检测链路确实能跑通
    /// （不放进常规测试，避免离线环境失败）。
    /// 运行：`cargo test --lib -- --ignored live_gitee`
    #[tokio::test]
    #[ignore]
    async fn live_gitee_check_detects_published_release() {
        // 用一个很旧的"当前版本"，必然应当检测到更新
        let info = check_update(DEFAULT_REPO, "0.0.1").await;
        println!("current={} latest={} has_update={} error={:?}",
            info.current, info.latest, info.has_update, info.error);
        assert!(info.error.is_none(), "检查更新报错：{:?}", info.error);
        assert!(!info.latest.is_empty(), "未能从 Gitee 读到任何发布");
        assert!(info.has_update, "0.0.1 应当检测到更新，实际 latest={}", info.latest);
        let asset = info.asset.as_ref().expect("应当找到安装包附件");
        println!("asset={} size={} url={}", asset.name, asset.size, asset.download_url);
        assert!(asset.name.to_lowercase().ends_with(".exe"));
        assert!(asset.download_url.starts_with("https://gitee.com/"));
    }

    /// 真实网络测试：安装包下载可达（只取前 256 KiB 验证，不落整包）
    #[tokio::test]
    #[ignore]
    async fn live_gitee_installer_is_downloadable() {
        let info = check_update(DEFAULT_REPO, "0.0.1").await;
        let asset = info.asset.expect("需要安装包");
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent("DeepAhead-Updater")
            .build()
            .unwrap();
        let resp = client
            .get(&asset.download_url)
            .header("Range", "bytes=0-262143")
            .send()
            .await
            .expect("下载请求失败");
        println!("status={}", resp.status());
        assert!(
            resp.status().is_success(),
            "下载应成功，实际 {}",
            resp.status()
        );
        let bytes = resp.bytes().await.expect("读取响应体失败");
        println!("received {} bytes", bytes.len());
        assert!(bytes.len() > 1024, "收到的内容过少");
        assert_eq!(&bytes[0..2], b"MZ", "安装包应以 MZ 开头");
    }

    /// 辅助脚本不得包含任何会弹控制台的构造。
    /// 这条测试是为用户实测到的黑窗 bug（标题为 `find "292"`）加的回归防线。
    #[test]
    fn helper_scripts_never_allocate_a_console() {
        let s = build_update_script(
            1234,
            r"C:\tmp\setup.exe",
            r"C:\app\uninstall.exe",
            r"C:\app\DeepAhead.exe",
            r"C:\tmp\update.log",
        );
        // 这些都是上一版批处理里弹黑窗的元凶
        for banned in ["cmd /C", "cmd.exe", "start \"\"", "tasklist", "timeout /t", "find \"", "@echo off"] {
            assert!(
                !s.contains(banned),
                "辅助脚本不应再出现 `{}`（会弹控制台）",
                banned
            );
        }
        // 必须用 WMI 无控制台地等待进程退出
        assert!(s.contains("Win32_Process"), "应当用 WMI 查询进程");
        // 所有操作必须以窗口样式 0（隐藏）运行
        assert!(s.contains(", 0, True"), "卸载/安装必须以隐藏窗口运行");
        // 日志落文件（因为已经没有控制台可看）
        assert!(s.contains("OpenTextFile"), "应当把过程写进日志文件");

        let u = build_uninstall_script(1234, r"C:\app\uninstall.exe", r"C:\tmp\update.log");
        assert!(u.contains(", 0, True"));
        assert!(!u.contains("cmd.exe"));
        assert!(!u.contains("tasklist"));
    }

    /// 真机验证辅助脚本：用同一份 header 跑 cscript，并通过**日志文件**确认执行成功。
    ///
    /// 注意：`cscript //B` 会把 `WScript.Echo` 的输出一并吞掉（实测即使最小 ANSI 脚本
    /// 也拿不到 stdout），所以断言必须走日志文件 —— 这也正是生产脚本用的通道。
    /// 用一个不存在的 PID，等待循环立即退出；正文只写日志，不触碰真实卸载/安装。
    #[test]
    fn vbscript_syntax_is_valid_on_this_machine() {
        let dir = helper_dir();
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("syntax-probe.log");
        let _ = std::fs::remove_file(&log);

        // 与生产完全相同的 header（UTF-16LE 写入、WMI 等待、Sub/Function 定义）
        let probe = format!(
            "{}LogLine \"probe-ran\"\r\n",
            vbs_header(999_999, &log.to_string_lossy())
        );
        let path = dir.join("syntax-probe.vbs");
        let mut bytes: Vec<u8> = vec![0xFF, 0xFE];
        for unit in probe.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&path, bytes).expect("写入探针脚本");

        let out = std::process::Command::new("cscript.exe")
            .args(["//B", "//Nologo", &path.to_string_lossy()])
            .output()
            .expect("无法运行 cscript.exe");
        println!("cscript exit={:?}", out.status.code());
        assert!(out.status.success(), "cscript 退出码非 0：{:?}", out.status.code());

        let written = std::fs::read_to_string(&log).unwrap_or_default();
        println!("probe log = {}", written.trim());
        assert!(
            written.contains("app exited after"),
            "header 的等待循环未按预期执行，日志：{}",
            written
        );
        assert!(
            written.contains("probe-ran"),
            "脚本正文未执行到 LogLine，日志：{}",
            written
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&log);
    }
}

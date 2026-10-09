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
    let releases: Vec<GiteeRelease> = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            info.error = Some(format!("解析发布列表失败: {}", e));
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
    info.notes = best.body.chars().take(4000).collect();
    if info.notes.is_empty() {
        info.notes = best.name.clone();
    }

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
pub async fn download_installer(
    url: &str,
    file_name: &str,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("DeepAhead_update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建更新目录失败: {}", e))?;
    let dest = dir.join(file_name);

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
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("读取下载流失败: {}", e))? {
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

/// VBScript 字符串字面量转义（内部的双引号要写成两个）
fn vbs_str(s: &str) -> String {
    s.replace('"', "\"\"")
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

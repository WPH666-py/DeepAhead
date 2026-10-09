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

/// 生成并启动"游离"更新脚本：等待本进程退出 → 静默卸载 → 静默安装 → 重启。
///
/// 为什么必须这样做：
/// - 运行中的 exe 被占用，安装程序无法覆盖它；
/// - 卸载同样不能在进程内做（会把自己的文件删掉）；
/// - 因此必须交给一个**独立于本进程**的脚本，在本进程退出后串行执行。
pub fn launch_update_installer(installer: &std::path::Path) -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {}", e))?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "无法获取安装目录".to_string())?
        .to_path_buf();
    let uninstaller = install_dir.join("uninstall.exe");
    let pid = std::process::id();

    let script_path = std::env::temp_dir().join("DeepAhead_update").join("deepahead-update.cmd");
    if let Some(dir) = script_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }

    // 注意：批处理里的路径都加引号；%~f0 用于自删除
    let script = format!(
        "@echo off\r\n\
chcp 65001 >nul\r\n\
setlocal\r\n\
set \"PID={pid}\"\r\n\
:waitloop\r\n\
tasklist /FI \"PID eq %PID%\" 2>nul | find \"%PID%\" >nul\r\n\
if not errorlevel 1 (\r\n\
  timeout /t 2 /nobreak >nul\r\n\
  goto waitloop\r\n\
)\r\n\
if exist \"{uninstaller}\" (\r\n\
  start \"\" /wait \"{uninstaller}\" /S\r\n\
)\r\n\
start \"\" /wait \"{installer}\" /S\r\n\
if exist \"{appexe}\" (\r\n\
  start \"\" \"{appexe}\"\r\n\
)\r\n\
del \"%~f0\"\r\n",
        pid = pid,
        uninstaller = uninstaller.to_string_lossy(),
        installer = installer.to_string_lossy(),
        appexe = current_exe.to_string_lossy(),
    );
    std::fs::write(&script_path, script).map_err(|e| format!("写入更新脚本失败: {}", e))?;

    // 用 cmd /C 启动，且完全脱离父进程（这样应用退出后它继续跑）
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("cmd")
        .args(["/C", &script_path.to_string_lossy()])
        .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("启动更新脚本失败: {}", e))?;

    Ok(script_path)
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
}

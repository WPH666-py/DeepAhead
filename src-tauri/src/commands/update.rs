//! 自动更新相关的 IPC 命令

use tauri::{AppHandle, Emitter};
use crate::update;

/// 当前应用版本（来自 Cargo 包版本，与 tauri.conf.json 一致）
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// 检查 Gitee 上是否有新版本（公开仓库，无需凭据）
#[tauri::command]
pub async fn check_update(repo: Option<String>, current: Option<String>) -> serde_json::Value {
    let repo = repo
        .filter(|r| !r.trim().is_empty())
        .unwrap_or_else(|| update::DEFAULT_REPO.to_string());
    let current = current
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let info = update::check_update(&repo, &current).await;
    serde_json::to_value(info).unwrap_or(serde_json::json!({
        "current": current, "has_update": false, "error": "序列化失败"
    }))
}

/// 下载安装包并上报进度（事件 `update-download-progress`）
#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    url: String,
    file_name: String,
) -> Result<String, String> {
    let app_for_progress = app.clone();
    let path = update::download_installer(&url, &file_name, move |done, total| {
        let _ = app_for_progress.emit(
            "update-download-progress",
            serde_json::json!({ "downloaded": done, "total": total }),
        );
    })
    .await?;
    Ok(path.to_string_lossy().to_string())
}

/// 取消正在进行的安装包下载。
/// 前端在用户点「取消下载」或关闭更新弹框时调用；半成品文件会被删除。
#[tauri::command]
pub fn cancel_update() -> bool {
    update::cancel_download();
    true
}

/// 启动更新：写入游离脚本 → 本进程退出后静默卸载旧版并安装新版
#[tauri::command]
pub fn install_update(installer_path: String) -> Result<String, String> {
    let p = std::path::PathBuf::from(&installer_path);
    if !p.exists() {
        return Err(format!("安装包不存在：{}", installer_path));
    }
    let script = update::launch_update_installer(&p)?;
    Ok(script.to_string_lossy().to_string())
}

/// 一键静默卸载：本进程退出后静默运行卸载器（无窗口）。
/// 返回辅助脚本路径；调用方随后应立即退出应用。
#[tauri::command]
pub fn uninstall_now() -> Result<String, String> {
    let script = update::launch_uninstaller()?;
    Ok(script.to_string_lossy().to_string())
}

/// 更新/卸载辅助脚本的日志路径（供界面显示，便于排查"没反应"）
#[tauri::command]
pub fn update_helper_log() -> serde_json::Value {
    let path = update::helper_log_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::json!({
        "path": path.to_string_lossy().to_string(),
        "exists": path.exists(),
        "text": text,
    })
}

/// 退出应用（供"立即更新"在启动辅助脚本后调用）
#[tauri::command]
pub fn quit_for_update(app: AppHandle) {
    app.exit(0);
}

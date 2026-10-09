//! ─── 内置发布说明 ───
//!
//! `docs/releases/v*.md` 在编译期被打包进程序（见 `build.rs`）。
//! 更新弹框优先展示这里的内容，Gitee 的 release 正文只在**通过校验**时
//! 作为补充 —— 远端文本被写坏（编码/截断）也不会让用户看到乱码。
//!
//! 版本解析顺序：
//!   1. `builtin_notes(latest)`：内置说明（最可靠，随包发布）
//!   2. 远端正文：仅当版本未被内置、且 `looks_like_mojibake` 判定健康时使用
//!   3. 兜底：`DeepAhead vX.Y.Z 发布，当前版本 A → B，可直接下载安装包升级。`

include!(concat!(env!("OUT_DIR"), "/release_notes.rs"));

/// 兜底说明（远端与内置都不可用时使用）
pub fn fallback_notes(latest: &str, current: &str) -> String {
    format!(
        "DeepAhead {} 已发布。\n\n当前版本：{}\n最新版本：{}\n\n\
         可直接下载安装包升级：退出应用后会自动静默卸载旧版并安装新版。\n\
         完整更新说明见仓库 docs/releases/ 目录。",
        latest, current, latest
    )
}

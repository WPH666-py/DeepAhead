//! 构建脚本：
//!   1. `tauri_build::build()`（Tauri 资源/权限生成）
//!   2. 把 `docs/releases/v*.md` 生成成 Rust 常量模块（内置发布说明）
//!
//! 为什么要在编译期内置发布说明：
//! Gitee 的 release 正文可能被第三方工具或编码问题写坏（v0.5.5 就发生过），
//! 而"更新内容"正是用户决定升不升级的依据 —— 不能让它取决于远端文本的健康度。
//! 本地文档随安装包一起发布，远端正文只在**校验通过**时才追加为补充。

use std::path::PathBuf;

fn main() {
    tauri_build::build();

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let releases_dir = manifest_dir.join("..").join("docs").join("releases");
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("release_notes.rs");

    // 收集 v*.md（按文件名排序，最新在后）
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    if let Ok(read) = std::fs::read_dir(&releases_dir) {
        for e in read.flatten() {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with('v') || !name.ends_with(".md") {
                continue;
            }
            let version = name
                .trim_start_matches('v')
                .trim_end_matches(".md")
                .to_string();
            if version.is_empty() {
                continue;
            }
            entries.push((version, path));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut code = String::new();
    // 注意：这个文件是 include! 进来的，**不能**用 //! 内部文档注释
    code.push_str("// 由 build.rs 自动生成：内置发布说明（不要手改）\n\n");
    code.push_str("/// 内置发布说明表：(版本, 正文)。版本不含前缀 v。\n");
    code.push_str("pub const RELEASE_NOTES: &[(&str, &str)] = &[\n");
    for (version, path) in &entries {
        // 文档变化时重跑构建脚本
        println!("cargo:rerun-if-changed={}", path.display());
        code.push_str(&format!(
            "    ({:?}, include_str!({:?})),\n",
            version,
            path.to_string_lossy().to_string()
        ));
    }
    code.push_str("];\n\n");
    code.push_str(
        "/// 取某个版本的内置发布说明（兼容 v0.5.6 / 0.5.6 两种写法）。\n\
         pub fn builtin_notes(version: &str) -> Option<&'static str> {\n\
         \x20   let v = version.trim().trim_start_matches(['v', 'V']);\n\
         \x20   RELEASE_NOTES.iter().find(|(k, _)| *k == v).map(|(_, t)| *t)\n\
         }\n",
    );

    std::fs::write(&dest, code).expect("写入 release_notes.rs 失败");
    println!("cargo:rerun-if-changed={}", releases_dir.display());
}

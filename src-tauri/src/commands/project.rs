use std::path::PathBuf;
use std::fs;

/// 创建新项目
#[tauri::command]
pub fn create_project(name: String, path: String) -> Result<String, String> {
    let project_path = PathBuf::from(&path).join(&name);
    fs::create_dir_all(&project_path)
        .map_err(|e| format!("Failed to create project: {}", e))?;

    // 创建基本目录结构
    let dirs = ["src", "docs", "tests"];
    for dir in &dirs {
        fs::create_dir_all(project_path.join(dir))
            .map_err(|e| format!("Failed to create directory {}: {}", dir, e))?;
    }

    // 创建 README.md
    let readme = format!("# {}\n\nCreated with DeepAhead\n", name);
    fs::write(project_path.join("README.md"), readme)
        .map_err(|e| format!("Failed to write README: {}", e))?;

    Ok(project_path.display().to_string())
}

/// 打开已有项目（验证路径存在）
#[tauri::command]
pub fn open_project(path: String) -> Result<String, String> {
    let project_path = PathBuf::from(&path);
    if !project_path.exists() {
        return Err(format!("Project path does not exist: {}", path));
    }
    if !project_path.is_dir() {
        return Err(format!("Path is not a directory: {}", path));
    }
    Ok(project_path.display().to_string())
}

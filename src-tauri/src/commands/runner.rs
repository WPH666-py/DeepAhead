use std::path::Path;
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

/// 解析 shebang 或扩展名，构造实际执行命令
fn resolve_run_command(file_path: &str, runtime: Option<String>) -> Result<(String, Vec<String>), String> {
    let path = Path::new(file_path);
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let parent = path
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".")
        .to_string();
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(file_path)
        .to_string();

    // 如果用户指定了运行时，优先使用
    if let Some(rt) = runtime {
        let rt = rt.to_lowercase();
        // 支持 "python"、"py"、"python 3.11.9" 等格式
        if rt.starts_with("python") || rt == "py" {
            let program = if cfg!(target_os = "windows") { "py" } else { "python3" };
            return Ok((program.into(), vec![file_path.into()]));
        }
        if rt.starts_with("node") || rt.starts_with("nodejs") {
            return Ok(("node".into(), vec![file_path.into()]));
        }
        if rt.starts_with("java") {
            return Ok(("java".into(), vec![file_path.into()]));
        }
        if rt.starts_with("go") {
            return Ok(("go".into(), vec!["run".into(), file_path.into()]));
        }
        if rt.starts_with("rust") || rt.starts_with("rs") {
            return Ok(("rustc".into(), vec![file_path.into(), "-o".into(), "temp_runner".into()]));
        }
        if rt.starts_with("gcc") || rt.starts_with("c") || rt.starts_with("cpp") {
            return Ok(("gcc".into(), vec![file_path.into(), "-o".into(), "temp_runner".into()]));
        }
    }

    let runner_name = format!("{}_runner", parent.replace("\\", "_").replace("/", "_"));
    match ext.as_str() {
        "py" => {
            let program = if cfg!(target_os = "windows") { "py" } else { "python3" };
            Ok((program.into(), vec![file_path.into()]))
        }
        "js" | "mjs" | "cjs" => Ok(("node".into(), vec![file_path.into()])),
        "ts" => Ok(("ts-node".into(), vec![file_path.into()])),
        "jsx" | "tsx" => Ok(("tsx".into(), vec![file_path.into()])),
        "java" => {
            // 先 javac 编译，再 java 运行
            let mut compile_cmd = Command::new("javac");
            hide_window(&mut compile_cmd);
            let compile = compile_cmd.args([&file_path]).current_dir(&parent).output();
            match compile {
                Ok(o) if o.status.success() => {
                    let class_name = Path::new(&file_name).file_stem().unwrap().to_string_lossy().to_string();
                    Ok(("java".into(), vec!["-cp".into(), parent.clone(), class_name]))
                }
                Ok(o) => Err(format!("javac 编译失败:\n{}", String::from_utf8_lossy(&o.stderr))),
                Err(e) => Err(format!("未找到 javac: {}", e)),
            }
        }
        "go" => Ok(("go".into(), vec!["run".into(), file_path.into()])),
        "rs" => {
            let out = format!("{}.exe", runner_name);
            let mut compile_cmd = Command::new("rustc");
            hide_window(&mut compile_cmd);
            let compile = compile_cmd.args([&file_path, "-o", &out]).current_dir(&parent).output();
            match compile {
                Ok(o) if o.status.success() => Ok((format!("{}/{}", parent, out), vec![])),
                Ok(o) => Err(format!("rustc 编译失败:\n{}", String::from_utf8_lossy(&o.stderr))),
                Err(e) => Err(format!("未找到 rustc: {}", e)),
            }
        }
        "c" => {
            let out = format!("{}.exe", runner_name);
            let mut compile_cmd = Command::new("gcc");
            hide_window(&mut compile_cmd);
            let compile = compile_cmd.args([&file_path, "-o", &out]).current_dir(&parent).output();
            match compile {
                Ok(o) if o.status.success() => Ok((format!("{}/{}", parent, out), vec![])),
                Ok(o) => Err(format!("gcc 编译失败:\n{}", String::from_utf8_lossy(&o.stderr))),
                Err(e) => Err(format!("未找到 gcc: {}", e)),
            }
        }
        "cpp" | "cc" | "cxx" => {
            let out = format!("{}.exe", runner_name);
            let mut compile_cmd = Command::new("g++");
            hide_window(&mut compile_cmd);
            let compile = compile_cmd.args([&file_path, "-o", &out]).current_dir(&parent).output();
            match compile {
                Ok(o) if o.status.success() => Ok((format!("{}/{}", parent, out), vec![])),
                Ok(o) => Err(format!("g++ 编译失败:\n{}", String::from_utf8_lossy(&o.stderr))),
                Err(e) => Err(format!("未找到 g++: {}", e)),
            }
        }
        "cs" => {
            // 优先 dotnet run，其次 csc 编译
            let mut dotnet_cmd = Command::new("dotnet");
            hide_window(&mut dotnet_cmd);
            if dotnet_cmd.arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
                Ok(("dotnet".into(), vec!["run".into(), "--project".into(), parent.clone()]))
            } else {
                let out = format!("{}.exe", runner_name);
                let mut compile_cmd = Command::new("csc");
                hide_window(&mut compile_cmd);
                let compile = compile_cmd.args([&file_path, format!("/out:{}", out).as_str()]).current_dir(&parent).output();
                match compile {
                    Ok(o) if o.status.success() => Ok((format!("{}/{}", parent, out), vec![])),
                    Ok(o) => Err(format!("csc 编译失败:\n{}", String::from_utf8_lossy(&o.stderr))),
                    Err(e) => Err(format!("未找到 dotnet 或 csc: {}", e)),
                }
            }
        }
        "php" => Ok(("php".into(), vec![file_path.into()])),
        "sql" => {
            // SQL 文件尝试用 sqlite3 执行
            Ok(("sqlite3".into(), vec!["-header".into(), "-column".into(), file_path.into()]))
        }
        "m" => {
            // MATLAB 脚本
            Ok(("matlab".into(), vec!["-batch".into(), format!("run('{}')", file_path.replace("\\", "/"))]))
        }
        "sh" => Ok(("sh".into(), vec![file_path.into()])),
        "bat" | "cmd" => Ok(("cmd".into(), vec!["/C".into(), file_path.into()])),
        "html" | "htm" | "vue" => {
            // HTML/Vue 不在这里运行，由前端打开浏览器
            Err("HTML/Vue files should be opened in browser".into())
        }
        "rb" => Ok(("ruby".into(), vec![file_path.into()])),
        _ => {
            // 尝试从 shebang 推断
            if let Ok(content) = std::fs::read_to_string(file_path) {
                let first_line = content.lines().next().unwrap_or("");
                if first_line.starts_with("#!") {
                    let interpreter = first_line
                        .trim_start_matches("#!")
                        .trim()
                        .split_whitespace()
                        .next()
                        .unwrap_or("sh")
                        .to_string();
                    return Ok((interpreter, vec![file_path.into()]));
                }
            }
            Err(format!("不支持的文件类型: .{}", ext))
        }
    }
}

/// 在项目目录中执行某个文件（根据扩展名自动选择解释器）
#[tauri::command]
pub async fn run_file(path: String, runtime: Option<String>) -> Result<String, String> {
    let (program, args) = resolve_run_command(&path, runtime)?;
    let work_dir = Path::new(&path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");

    let mut cmd = Command::new(&program);
    hide_window(&mut cmd);
    cmd.args(&args).current_dir(work_dir);
    // Python 在 Windows 上默认用 GBK 输出，强制 UTF-8 避免乱码
    if program == "py" || program == "python" || program == "python3" {
        cmd.env("PYTHONIOENCODING", "utf-8");
    }
    let output = cmd.output().map_err(|e| format!("无法启动 {}: {}", program, e))?;

    let mut result = String::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&stderr);
    }

    // 如果 stdout 和 stderr 都为空，可能是程序未找到或 MS Store 占位符
    if result.is_empty() {
        if !output.status.success() {
            let code = output.status.code().unwrap_or(-1);
            result = format!("[进程退出码 {}，无输出。请检查 {} 是否正确安装]", code, program);
        } else {
            result = format!("[进程正常退出但无输出。请检查 {} 是否正确安装，Windows 上 'python' 可能指向 Microsoft Store 占位程序，请改用 'py' 或完整路径]", program);
        }
    }

    Ok(result)
}

/// 增强版运行时检测，扫描 PATH 以及 C/D/E 盘常见安装目录
#[tauri::command]
pub fn detect_runtimes_enhanced() -> Vec<serde_json::Value> {
    let runtime_defs: Vec<(&str, Vec<&str>, Vec<&str>, &str)> = vec![
        (
            "python",
            vec!["python", "python3"],
            vec![
                "C:\\Python*",
                "C:\\Users\\*\\AppData\\Local\\Programs\\Python\\Python*",
                "C:\\Program Files\\Python*",
                "D:\\Python*",
                "E:\\Python*",
            ],
            "--version",
        ),
        (
            "node",
            vec!["node", "nodejs"],
            vec![
                "C:\\Program Files\\nodejs",
                "C:\\ProgramData\\nvm",
                "D:\\nodejs",
                "E:\\nodejs",
                "C:\\Users\\*\\AppData\\Roaming\\nvm",
            ],
            "--version",
        ),
        (
            "npm",
            vec!["npm"],
            vec!["C:\\Program Files\\nodejs"],
            "--version",
        ),
        (
            "java",
            vec!["java"],
            vec![
                "C:\\Program Files\\Java\\*",
                "C:\\Program Files (x86)\\Java\\*",
                "D:\\Java\\*",
                "E:\\Java\\*",
            ],
            "-version 2>&1",
        ),
        (
            "go",
            vec!["go"],
            vec![
                "C:\\Program Files\\Go",
                "C:\\Go",
                "D:\\Go",
                "E:\\Go",
            ],
            "version",
        ),
        (
            "rust",
            vec!["rustc"],
            vec![
                "C:\\Users\\*\\.cargo\\bin",
                "C:\\Users\\*\\.rustup\\toolchains\\*\\bin",
            ],
            "--version",
        ),
        (
            "gcc",
            vec!["gcc"],
            vec![
                "C:\\Program Files\\mingw-w64\\*\\mingw64\\bin",
                "C:\\mingw64\\bin",
                "C:\\TDM-GCC-64\\bin",
                "D:\\mingw64\\bin",
                "E:\\mingw64\\bin",
            ],
            "--version",
        ),
        (
            "git",
            vec!["git"],
            vec![
                "C:\\Program Files\\Git\\bin",
                "C:\\Program Files\\Git\\cmd",
                "D:\\Git\\bin",
                "E:\\Git\\bin",
            ],
            "--version",
        ),
        (
            "docker",
            vec!["docker"],
            vec![
                "C:\\Program Files\\Docker\\Docker\\resources\\bin",
            ],
            "--version",
        ),
        (
            "php",
            vec!["php"],
            vec![
                "C:\\php",
                "C:\\Program Files\\PHP",
                "D:\\php",
                "E:\\php",
            ],
            "--version",
        ),
        (
            "dotnet",
            vec!["dotnet"],
            vec![
                "C:\\Program Files\\dotnet",
                "D:\\dotnet",
                "E:\\dotnet",
            ],
            "--version",
        ),
    ];

    runtime_defs
        .into_iter()
        .map(|(name, bin_names, scan_patterns, version_arg)| {
            // 1. 先尝试 PATH 中直接调用
            for bin in &bin_names {
                let version = run_version_cmd(bin, version_arg);
                if let Some(v) = version {
                    return serde_json::json!({
                        "name": name,
                        "version": v,
                        "available": true,
                        "path": bin,
                    });
                }
            }

            // 2. 扫描 C/D/E 盘常见目录
            for pattern in &scan_patterns {
                if let Ok(paths) = glob::glob(pattern) {
                    for entry in paths.flatten() {
                        if entry.is_dir() {
                            for bin in &bin_names {
                                let candidate = entry.join(format!("{}.exe", bin));
                                if candidate.exists() {
                                    let candidate_str = candidate.to_string_lossy().to_string();
                                    let version = run_version_cmd(&candidate_str, version_arg);
                                    if let Some(v) = version {
                                        return serde_json::json!({
                                            "name": name,
                                            "version": v,
                                            "available": true,
                                            "path": candidate_str,
                                        });
                                    }
                                }
                            }
                        } else if entry.is_file() {
                            let candidate_str = entry.to_string_lossy().to_string();
                            let version = run_version_cmd(&candidate_str, version_arg);
                            if let Some(v) = version {
                                return serde_json::json!({
                                    "name": name,
                                    "version": v,
                                    "available": true,
                                    "path": candidate_str,
                                });
                            }
                        }
                    }
                }
            }

            serde_json::json!({
                "name": name,
                "version": null,
                "available": false,
                "path": null,
            })
        })
        .collect()
}

fn run_version_cmd(cmd: &str, args: &str) -> Option<String> {
    // java -version 输出到 stderr
    let output = if cfg!(windows) {
        let mut c = Command::new("cmd");
        hide_window(&mut c);
        c.args(["/C", cmd, args]).output().ok()
    } else {
        let parts: Vec<&str> = args.split_whitespace().collect();
        Command::new(cmd).args(parts).output().ok()
    }?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout, stderr);
    let line = combined.lines().next().unwrap_or("").trim();
    if !line.is_empty() { Some(line.to_string()) } else { None }
}

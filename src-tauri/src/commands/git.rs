use std::process::Command;
use serde::{Deserialize, Serialize};

/// Git 状态结果
#[derive(Debug, Serialize, Deserialize)]
pub struct GitStatus {
    pub branch: String,
    pub changes: Vec<String>,
    pub staged: Vec<String>,
    pub untracked: Vec<String>,
    pub ahead: usize,
    pub behind: usize,
    pub clean: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitLogEntry {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitDiffResult {
    pub files: Vec<String>,
    pub diff: String,
}

/// Git 状态
#[tauri::command]
pub fn git_status(path: String) -> Result<GitStatus, String> {
    let output = run_git(&path, &["status", "--porcelain", "-b"]);
    let lines: Vec<&str> = output.lines().collect();
    let mut status = GitStatus {
        branch: String::new(),
        changes: vec![],
        staged: vec![],
        untracked: vec![],
        ahead: 0,
        behind: 0,
        clean: true,
    };

    for line in &lines {
        if line.starts_with("## ") {
            let branch_info = &line[3..];
            status.branch = branch_info.split("...").next().unwrap_or(branch_info).to_string();
            if branch_info.contains("ahead") {
                status.ahead = parse_num(branch_info, "ahead ");
            }
            if branch_info.contains("behind") {
                status.behind = parse_num(branch_info, "behind ");
            }
        } else if line.len() >= 2 {
            let flag = &line[..2];
            let file = line[3..].trim().to_string();
            status.clean = false;
            match flag {
                "??" => status.untracked.push(file),
                "M " | "A " | "D " | "R " => status.staged.push(file),
                " M" | " D" => status.changes.push(file),
                "MM" | "AM" => {
                    status.staged.push(file.clone());
                    status.changes.push(file);
                }
                _ => status.changes.push(file),
            }
        }
    }

    Ok(status)
}

/// Git Diff
#[tauri::command]
pub fn git_diff(path: String, staged: Option<bool>) -> Result<GitDiffResult, String> {
    let mut args = vec!["diff"];
    if staged.unwrap_or(false) {
        args.push("--staged");
    }
    args.push("--name-only");
    let files_output = run_git(&path, &args);
    let files: Vec<String> = files_output.lines().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect();

    let mut diff_args = vec!["diff"];
    if staged.unwrap_or(false) { diff_args.push("--staged"); }
    let diff = run_git(&path, &diff_args);

    Ok(GitDiffResult { files, diff })
}

/// Git Log
#[tauri::command]
pub fn git_log(path: String, count: Option<usize>) -> Result<Vec<GitLogEntry>, String> {
    let n = count.unwrap_or(20);
    let format = "--pretty=format:%H||%an||%ad||%s";
    let output = run_git(&path, &["log", format, &format!("-{}", n), "--date=short"]);
    let entries: Vec<GitLogEntry> = output
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split("||").collect();
            if parts.len() >= 4 {
                Some(GitLogEntry {
                    hash: parts[0].to_string(),
                    author: parts[1].to_string(),
                    date: parts[2].to_string(),
                    message: parts[3..].join("||"),
                })
            } else {
                None
            }
        })
        .collect();
    Ok(entries)
}

/// Git Branch
#[tauri::command]
pub fn git_branches(path: String) -> Result<Vec<String>, String> {
    let output = run_git(&path, &["branch", "--list"]);
    let branches: Vec<String> = output
        .lines()
        .map(|l| l.trim_start_matches("* ").trim().to_string())
        .collect();
    Ok(branches)
}

/// Git Clone
#[tauri::command]
pub async fn git_clone(url: String, target: String) -> Result<String, String> {
    let output = Command::new("git")
        .args(["clone", &url, &target])
        .output()
        .map_err(|e| format!("Failed to clone: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        Err(format!("Clone failed: {}", stderr))
    }
}

/// Git 推送：add + commit + push
#[tauri::command]
pub async fn git_push(
    path: String,
    username: String,
    token: String,
    repo: String,
    branch: String,
    message: String,
) -> Result<String, String> {
    // 配置 git 用户信息
    let _ = run_git_status_only(&path, &["config", "user.email", "deep-ide@example.com"]);
    let _ = run_git_status_only(&path, &["config", "user.name", &username]);

    // 添加所有变更
    let _ = run_git_status_only(&path, &["add", "."]);

    // 提交
    let commit_output = Command::new("git")
        .args(["-C", &path, "commit", "-m", &message])
        .output()
        .map_err(|e| format!("Commit failed: {}", e))?;
    if !commit_output.status.success() {
        let err = String::from_utf8_lossy(&commit_output.stderr).to_string();
        // 如果没有变更要提交，也继续 push
        if !err.contains("nothing to commit") && !err.contains("nothing added") {
            return Err(format!("Commit failed: {}", err));
        }
    }

    // 设置远程仓库
    let remote_url = format!("https://{}:{}@github.com/{}", username, token, repo);
    let remote_out = Command::new("git")
        .args(["-C", &path, "remote", "set-url", "origin", &remote_url])
        .output()
        .map_err(|e| format!("Set remote failed: {}", e))?;
    if !remote_out.status.success() {
        // 如果 remote 不存在则添加
        let add_remote = Command::new("git")
            .args(["-C", &path, "remote", "add", "origin", &remote_url])
            .output()
            .map_err(|e| format!("Add remote failed: {}", e))?;
        if !add_remote.status.success() {
            return Err(format!("Remote config failed: {}", String::from_utf8_lossy(&add_remote.stderr)));
        }
    }

    // push
    let push_output = Command::new("git")
        .args(["-C", &path, "push", "-u", "origin", &branch])
        .output()
        .map_err(|e| format!("Push failed: {}", e))?;
    if push_output.status.success() {
        Ok(format!("Push to {}/{} succeeded", repo, branch))
    } else {
        Err(format!("Push failed: {}", String::from_utf8_lossy(&push_output.stderr)))
    }
}

fn run_git_status_only(path: &str, args: &[&str]) -> String {
    Command::new("git")
        .args([&["-C", path], args].concat())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn run_git(path: &str, args: &[&str]) -> String {
    Command::new("git")
        .args([&["-C", path], args].concat())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default()
}

fn parse_num(s: &str, prefix: &str) -> usize {
    s.split(prefix)
        .nth(1)
        .and_then(|p| p.split(',').next())
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

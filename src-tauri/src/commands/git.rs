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

/// 提交图节点：对应上游 dsh-git-graph 的 GraphCommit
#[derive(Debug, Serialize, Deserialize)]
pub struct GitGraphCommit {
    pub oid: String,
    pub short: String,
    /// 父提交（merge 时有多个）——computeLanes 的分支/合并判定依据
    pub parents: Vec<String>,
    pub author: String,
    /// 作者时间（Unix 秒，%at）——前端据此计算相对时间，避免格式化后二次解析
    pub author_time: i64,
    pub subject: String,
    /// 解析后的引用（已去掉 HEAD -> / tag: 前缀）
    pub refs: Vec<String>,
}

/// 提交图视图（含分页信息）
#[derive(Debug, Serialize, Deserialize)]
pub struct GitGraphView {
    pub branch: String,
    pub commits: Vec<GitGraphCommit>,
    pub has_more: bool,
}

/// 解析 `%D` 装饰串（对齐上游 parseDecoration）：
/// 按 ", " 切分，丢弃裸 "HEAD"，去掉 "HEAD -> " 与 "tag: " 前缀，去空白与空项。
fn parse_decoration(d: &str) -> Vec<String> {
    d.split(", ")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && *s != "HEAD")
        .map(|s| {
            let s = s.strip_prefix("HEAD -> ").unwrap_or(s);
            let s = s.strip_prefix("tag: ").unwrap_or(s);
            s.trim().to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// 读取提交图数据。
/// 与上游 dsh-git-graph 一致：
/// - `--topo-order --parents`（拓扑序是泳道算法的前提）
/// - 记录用 \x1e 分隔、字段用 \x00 分隔，避免提交信息中的字符冲突
/// - 多取一条（limit+1）来判断 hasMore，而不是再跑一次 rev-list --count
#[tauri::command]
pub fn git_log_graph(
    path: String,
    count: Option<usize>,
    all: Option<bool>,
) -> Result<GitGraphView, String> {
    let limit = count.unwrap_or(200).clamp(1, 5000);
    let branch = run_git(&path, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string();

    // %x00 字段分隔，%x1e 记录分隔
    let format = "--format=%H%x00%P%x00%an%x00%at%x00%D%x00%s%x1e";
    let mut args: Vec<String> = vec!["log".into(), format.into()];
    if all.unwrap_or(true) {
        args.push("--branches".into());
        args.push("--tags".into());
        args.push("--remotes".into());
    }
    args.push("--topo-order".into());
    args.push("--parents".into());
    args.push(format!("--max-count={}", limit + 1));
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    let output = run_git(&path, &arg_refs);

    let mut commits: Vec<GitGraphCommit> = Vec::new();
    for record in output.split('\u{1e}') {
        // git 的 tformat 会在记录分隔符后附加一个换行，必须剥掉，否则 oid 被污染
        let record = record.strip_prefix('\n').unwrap_or(record);
        let record = record.trim_end_matches(['\n', '\r']);
        if record.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = record.split('\u{0}').collect();
        if f.len() < 6 {
            continue;
        }
        let oid = f[0].trim().to_string();
        if oid.is_empty() {
            continue;
        }
        let parents: Vec<String> = f[1].split_whitespace().map(|p| p.to_string()).collect();
        commits.push(GitGraphCommit {
            short: oid.chars().take(7).collect(),
            oid,
            parents,
            author: f[2].to_string(),
            author_time: f[3].trim().parse::<i64>().unwrap_or(0),
            refs: parse_decoration(f[4]),
            subject: f[5].to_string(),
        });
    }

    let has_more = commits.len() > limit;
    if has_more {
        commits.truncate(limit);
    }

    Ok(GitGraphView { branch, commits, has_more })
}

/// 单个提交的改动详情（「历史提交记录」里点开某条提交时使用）
#[tauri::command]
pub fn git_commit_detail(path: String, hash: String) -> Result<serde_json::Value, String> {
    let stat = run_git(&path, &["show", "--stat", "--oneline", &hash]);
    let files_out = run_git(&path, &["show", "--name-only", "--pretty=format:", &hash]);
    let files: Vec<String> = files_out
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let patch = run_git(&path, &["show", "--pretty=format:", "--patch", &hash]);
    // 限制 patch 体积，避免超大提交把前端卡死
    let patch = if patch.len() > 200_000 {
        format!("{}\n... [patch truncated, {} chars total]", &patch[..200_000], patch.len())
    } else {
        patch
    };
    Ok(serde_json::json!({
        "hash": hash,
        "stat": stat,
        "files": files,
        "patch": patch,
    }))
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

use std::path::PathBuf;
use std::fs;
use serde::{Deserialize, Serialize};
use chrono::Utc;

use crate::ai::Message;

/// 会话文件
#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub agent: String,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
    pub total_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionIndex {
    pub sessions: Vec<SessionMeta>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub agent: String,
    pub message_count: usize,
    pub updated_at: String,
}

fn get_sessions_dir() -> PathBuf {
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".deep-ide").join("sessions")
}

fn ensure_sessions_dir() -> Result<PathBuf, String> {
    let dir = get_sessions_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create sessions dir: {}", e))?;
    Ok(dir)
}

/// 保存会话
#[tauri::command]
pub fn save_session(
    id: String,
    name: String,
    mode: String,
    agent: String,
    messages: Vec<Message>,
    total_tokens: u32,
) -> Result<String, String> {
    let dir = ensure_sessions_dir()?;
    let now = Utc::now().format("%Y-%m-%d %H:%M").to_string();

    let session = Session {
        id: id.clone(),
        name: name.clone(),
        mode,
        agent,
        messages,
        total_tokens,
        created_at: now.clone(),
        updated_at: now,
    };

    let json = serde_json::to_string_pretty(&session)
        .map_err(|e| format!("Serialize error: {}", e))?;

    let file_path = dir.join(format!("{}.json", id));
    fs::write(&file_path, json).map_err(|e| format!("Write error: {}", e))?;

    Ok(format!("Session '{}' saved", name))
}

/// 加载会话
#[tauri::command]
pub fn load_session(id: String) -> Result<Session, String> {
    let dir = get_sessions_dir();
    let file_path = dir.join(format!("{}.json", id));

    let json = fs::read_to_string(&file_path)
        .map_err(|e| format!("Session not found: {}", e))?;

    let session: Session = serde_json::from_str(&json)
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(session)
}

/// 列出所有会话
#[tauri::command]
pub fn list_sessions() -> Result<Vec<SessionMeta>, String> {
    let dir = get_sessions_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }

    let mut sessions = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(json) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<Session>(&json) {
                        sessions.push(SessionMeta {
                            id: session.id,
                            name: session.name,
                            mode: session.mode,
                            agent: session.agent,
                            message_count: session.messages.len(),
                            updated_at: session.updated_at,
                        });
                    }
                }
            }
        }
    }

    sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(sessions)
}

/// 删除会话
#[tauri::command]
pub fn delete_session(id: String) -> Result<String, String> {
    let dir = get_sessions_dir();
    let file_path = dir.join(format!("{}.json", id));
    fs::remove_file(&file_path).map_err(|e| format!("Delete error: {}", e))?;
    Ok(format!("Session '{}' deleted", id))
}

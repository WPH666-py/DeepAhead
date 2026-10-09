use serde::{Deserialize, Serialize};

pub mod commands;
pub mod ai;
pub mod cli;
pub mod update;
/// 内置发布说明（由 build.rs 从 docs/releases/v*.md 生成）
pub mod release_notes;

pub use ai::{ApprovalGate, ApprovalMode, DeepSeekClient, UndoStore};

/// 文件条目（用于文件树）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub children: Option<Vec<FileEntry>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DirListResult {
    pub entries: Vec<FileEntry>,
    pub path: String,
}

/// AI 模式（DeepAhead 仅支持四种）
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AIMode {
    #[serde(rename = "dsh")]
    DSH,
    #[serde(rename = "dsk")]
    DSK,
    #[serde(rename = "dsa")]
    DSA,
    #[serde(rename = "dsf")]
    DSF,
}

impl AIMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            AIMode::DSH => "dsh",
            AIMode::DSK => "dsk",
            AIMode::DSA => "dsa",
            AIMode::DSF => "dsf",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "dsh" => Some(AIMode::DSH),
            "dsk" => Some(AIMode::DSK),
            "dsa" => Some(AIMode::DSA),
            "dsf" => Some(AIMode::DSF),
            _ => None,
        }
    }
}

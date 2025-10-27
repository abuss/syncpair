use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletionInfo {
    pub path: String,
    pub deleted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientState {
    pub files: HashMap<String, FileInfo>,
    pub deleted_files: HashMap<String, DateTime<Utc>>, // Files deleted with timestamp
    pub last_sync: DateTime<Utc>,
}

// Configuration types for multi-directory support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    pub server: String,
    pub directories: Vec<DirectoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryConfig {
    pub name: String,
    pub local_path: PathBuf,
    #[serde(default)]
    pub settings: DirectorySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySettings {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_sync_interval")]
    pub sync_interval_seconds: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub shared: bool,
}

impl Default for DirectorySettings {
    fn default() -> Self {
        Self {
            description: None,
            sync_interval_seconds: default_sync_interval(),
            enabled: default_true(),
            ignore_patterns: Vec::new(),
            shared: false,
        }
    }
}

fn default_sync_interval() -> u64 {
    30
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadRequest {
    pub file_info: FileInfo,
    pub content: Vec<u8>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub files: HashMap<String, FileInfo>,
    pub deleted_files: HashMap<String, DateTime<Utc>>, // Files deleted with timestamp
    pub last_sync: DateTime<Utc>,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub path: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub files_to_upload: Vec<String>,
    pub files_to_download: Vec<FileInfo>,
    pub files_to_delete: Vec<String>,
    pub conflicts: Vec<FileConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConflict {
    pub path: String,
    pub client_file: FileInfo,
    pub server_file: FileInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub path: String,
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub file_info: Option<FileInfo>,
    pub content: Option<Vec<u8>>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChangeEvent {
    pub path: String,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
}

pub mod error {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum SyncError {
        #[error("IO error: {0}")]
        Io(#[from] std::io::Error),

        #[error("Network error: {0}")]
        Network(#[from] reqwest::Error),

        #[error("Serialization error: {0}")]
        Serialization(#[from] serde_json::Error),

        #[error("File not found: {0}")]
        FileNotFound(String),

        #[error("Hash mismatch: {0}")]
        HashMismatch(String),

        #[error("Watch error: {0}")]
        Watch(String),
    }
}

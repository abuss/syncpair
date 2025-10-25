use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientState {
    pub files: HashMap<String, FileInfo>,
    pub deleted_files: Vec<String>,  // Files deleted since last sync
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadRequest {
    pub file_info: FileInfo,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub files: HashMap<String, FileInfo>,
    pub deleted_files: Vec<String>,  // Files deleted since last sync
    pub last_sync: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub path: String,
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
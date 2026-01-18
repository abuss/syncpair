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
    #[serde(default)]
    pub default: Option<DefaultSettings>,
    pub directories: Vec<DirectoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultSettings {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub sync_interval_seconds: Option<u64>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub shared: Option<bool>,
}

impl Default for DefaultSettings {
    fn default() -> Self {
        Self {
            description: None,
            sync_interval_seconds: None,
            enabled: None,
            ignore_patterns: Vec::new(),
            shared: None,
        }
    }
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
    #[serde(default)]
    pub sync_interval_seconds: Option<u64>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    #[serde(default)]
    pub shared: Option<bool>,
}

impl Default for DirectorySettings {
    fn default() -> Self {
        Self {
            description: None,
            sync_interval_seconds: None,
            enabled: None,
            ignore_patterns: Vec::new(),
            shared: None,
        }
    }
}

impl DirectorySettings {
    /// Merge this DirectorySettings with defaults, where defaults provide fallback values
    pub fn merge_with_defaults(self, defaults: &DefaultSettings) -> Self {
        Self {
            // Apply default description only if current is None
            description: self.description.or_else(|| defaults.description.clone()),

            // Apply default sync_interval only if current is None
            sync_interval_seconds: self
                .sync_interval_seconds
                .or(defaults.sync_interval_seconds),

            // Apply default enabled only if current is None
            enabled: self.enabled.or(defaults.enabled),

            // Merge ignore patterns: defaults first, then directory-specific
            ignore_patterns: if defaults.ignore_patterns.is_empty() {
                self.ignore_patterns
            } else {
                let mut merged = defaults.ignore_patterns.clone();
                merged.extend(self.ignore_patterns);
                // Remove duplicates while preserving order
                let mut unique_patterns = Vec::new();
                for pattern in merged {
                    if !unique_patterns.contains(&pattern) {
                        unique_patterns.push(pattern);
                    }
                }
                unique_patterns
            },

            // Apply default shared only if current is None
            shared: self.shared.or(defaults.shared),
        }
    }

    /// Get the effective values with struct defaults applied
    pub fn effective_values(&self) -> EffectiveDirectorySettings {
        EffectiveDirectorySettings {
            description: self.description.clone(),
            sync_interval_seconds: self
                .sync_interval_seconds
                .unwrap_or(default_sync_interval()),
            enabled: self.enabled.unwrap_or(default_true()),
            ignore_patterns: self.ignore_patterns.clone(),
            shared: self.shared.unwrap_or(false),
        }
    }
}

/// DirectorySettings with all Option fields resolved to concrete values
#[derive(Debug, Clone)]
pub struct EffectiveDirectorySettings {
    pub description: Option<String>,
    pub sync_interval_seconds: u64,
    pub enabled: bool,
    pub ignore_patterns: Vec<String>,
    pub shared: bool,
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

// Delta Sync Types

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockMsg {
    pub index: u64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaInitRequest {
    pub file_info: FileInfo,
    pub block_hashes: Vec<BlockMsg>,
    pub block_size: u64,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub directory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaInitResponse {
    pub missing_block_indices: Vec<u64>,
    pub should_full_upload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockUploadRequest {
    pub path: String,
    pub directory: String,
    pub index: u64,
    pub content: Vec<u8>,
    #[serde(default)]
    pub client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockUploadResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeltaCompleteRequest {
    pub path: String,
    pub directory: Option<String>,
    pub client_id: Option<String>,
    pub expected_hash: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeltaCompleteResponse {
    pub success: bool,
    pub message: String,
}

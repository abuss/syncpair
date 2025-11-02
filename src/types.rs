use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// File information with hash for change detection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileInfo {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
}

/// Client state for tracking local files and sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientState {
    pub files: HashMap<String, FileInfo>,
    pub deleted_files: HashMap<String, DateTime<Utc>>,
    pub last_sync: DateTime<Utc>,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            files: HashMap::new(),
            deleted_files: HashMap::new(),
            last_sync: DateTime::from_timestamp(0, 0).unwrap_or_else(Utc::now),
        }
    }
}

/// Request sent from client to server for synchronization
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncRequest {
    pub files: HashMap<String, FileInfo>,
    pub deleted_files: HashMap<String, DateTime<Utc>>,
    pub last_sync: DateTime<Utc>,
    pub client_id: Option<String>,
    pub directory: Option<String>,
}

/// Response sent from server to client with sync instructions
#[derive(Debug, Serialize, Deserialize)]
pub struct SyncResponse {
    pub files_to_upload: Vec<String>,
    pub files_to_download: Vec<FileInfo>,
    pub files_to_delete: Vec<String>,
    pub conflicts: Vec<FileConflict>,
}

/// Information about a file conflict
#[derive(Debug, Serialize, Deserialize)]
pub struct FileConflict {
    pub path: String,
    pub client_modified: DateTime<Utc>,
    pub server_modified: DateTime<Utc>,
    pub resolution: ConflictResolution,
}

/// How a conflict was resolved
#[derive(Debug, Serialize, Deserialize)]
pub enum ConflictResolution {
    ClientWins,
    ServerWins,
}

/// Request to upload a file to the server
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadRequest {
    pub path: String,
    pub hash: String,
    pub size: u64,
    pub modified: DateTime<Utc>,
    pub content: Vec<u8>,
    pub client_id: Option<String>,
    pub directory: Option<String>,
}

/// Response from server after file upload
#[derive(Debug, Serialize, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: Option<String>,
}

/// Request to download a file from the server
#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub path: String,
    pub client_id: Option<String>,
    pub directory: Option<String>,
}

/// Response from server with file content
#[derive(Debug, Serialize, Deserialize)]
pub struct DownloadResponse {
    pub success: bool,
    pub file_info: Option<FileInfo>,
    pub content: Option<Vec<u8>>,
    pub message: Option<String>,
}

/// Request to delete a file from the server
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub path: String,
    pub client_id: Option<String>,
    pub directory: Option<String>,
}

/// Response from server after file deletion
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub success: bool,
    pub message: Option<String>,
}

/// Configuration for a single directory in multi-directory setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryConfig {
    pub name: String,
    pub local_path: String,
    pub settings: DirectorySettings,
}

/// Settings for a directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySettings {
    pub description: Option<String>,
    pub shared: Option<bool>,
    pub sync_interval_seconds: Option<u64>,
    pub enabled: Option<bool>,
    pub ignore_patterns: Option<Vec<String>>,
}

impl Default for DirectorySettings {
    fn default() -> Self {
        Self {
            description: None,
            shared: Some(false),
            sync_interval_seconds: Some(30),
            enabled: Some(true),
            ignore_patterns: Some(vec![]),
        }
    }
}

/// Default configuration applied to all directories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultConfig {
    pub description: Option<String>,
    pub shared: Option<bool>,
    pub sync_interval_seconds: Option<u64>,
    pub enabled: Option<bool>,
    pub ignore_patterns: Option<Vec<String>>,
}

/// Complete client configuration loaded from YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_id: String,
    pub server: String,
    pub default: Option<DefaultConfig>,
    pub directories: Vec<DirectoryConfig>,
}

impl DirectorySettings {
    /// Merge with default configuration, with directory settings taking precedence
    pub fn merge_with_defaults(&mut self, defaults: &DefaultConfig) {
        if self.description.is_none() {
            self.description = defaults.description.clone();
        }
        if self.shared.is_none() {
            self.shared = defaults.shared;
        }
        if self.sync_interval_seconds.is_none() {
            self.sync_interval_seconds = defaults.sync_interval_seconds;
        }
        if self.enabled.is_none() {
            self.enabled = defaults.enabled;
        }
        
        // Merge ignore patterns (defaults + directory-specific)
        if let Some(default_patterns) = &defaults.ignore_patterns {
            let mut merged_patterns = default_patterns.clone();
            if let Some(dir_patterns) = &self.ignore_patterns {
                merged_patterns.extend(dir_patterns.clone());
            }
            // Remove duplicates
            merged_patterns.sort();
            merged_patterns.dedup();
            self.ignore_patterns = Some(merged_patterns);
        }
    }
    
    /// Get the effective sync interval, defaulting to 30 seconds
    pub fn effective_sync_interval(&self) -> u64 {
        self.sync_interval_seconds.unwrap_or(30)
    }
    
    /// Get whether the directory is shared, defaulting to false
    pub fn is_shared(&self) -> bool {
        self.shared.unwrap_or(false)
    }
    
    /// Get whether the directory is enabled, defaulting to true
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
    
    /// Get the ignore patterns, defaulting to empty vector
    /// Always includes built-in patterns that should never be synced
    pub fn get_ignore_patterns(&self) -> Vec<String> {
        let mut patterns = self.ignore_patterns.as_ref().unwrap_or(&vec![]).clone();
        
        // Add built-in ignore patterns that should always be excluded
        let builtin_patterns = vec![
            ".syncpair_state.db".to_string(),
        ];
        
        patterns.extend(builtin_patterns);
        
        // Remove duplicates and sort for consistency
        patterns.sort();
        patterns.dedup();
        
        patterns
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_ignore_patterns() {
        // Test that .syncpair_state.db is always included in ignore patterns
        let settings = DirectorySettings::default();
        let patterns = settings.get_ignore_patterns();
        assert!(patterns.contains(&".syncpair_state.db".to_string()));
    }

    #[test]
    fn test_ignore_patterns_with_user_defined() {
        // Test that built-in patterns are added to user-defined patterns
        let settings = DirectorySettings {
            ignore_patterns: Some(vec!["*.tmp".to_string(), "*.log".to_string()]),
            ..Default::default()
        };
        
        let patterns = settings.get_ignore_patterns();
        assert!(patterns.contains(&".syncpair_state.db".to_string()));
        assert!(patterns.contains(&"*.tmp".to_string()));
        assert!(patterns.contains(&"*.log".to_string()));
    }

    #[test]
    fn test_ignore_patterns_no_duplicates() {
        // Test that duplicates are removed
        let settings = DirectorySettings {
            ignore_patterns: Some(vec![".syncpair_state.db".to_string(), "*.tmp".to_string()]),
            ..Default::default()
        };
        
        let patterns = settings.get_ignore_patterns();
        let syncpair_count = patterns.iter().filter(|p| *p == ".syncpair_state.db").count();
        assert_eq!(syncpair_count, 1, "Should have exactly one .syncpair_state.db pattern");
    }
}
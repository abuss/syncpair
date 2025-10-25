use crate::types::{ClientState, FileInfo};
use anyhow::Result;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn calculate_file_hash(path: &Path) -> Result<String> {
    let contents = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn get_file_info(path: &Path, relative_path: &str) -> Result<FileInfo> {
    let metadata = fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;

    Ok(FileInfo {
        path: relative_path.to_string(),
        hash,
        size: metadata.len(),
        modified: metadata.modified()?.into(),
    })
}

pub fn scan_directory(dir_path: &Path) -> Result<Vec<FileInfo>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            // Skip hidden files (files starting with .)
            if let Some(file_name) = entry.path().file_name() {
                if file_name.to_string_lossy().starts_with('.') {
                    continue;
                }
            }

            // Get relative path from the base directory
            let relative_path = entry.path().strip_prefix(dir_path)?;
            let relative_path_str = relative_path.to_string_lossy().to_string();

            let file_info = get_file_info(entry.path(), &relative_path_str)?;
            files.push(file_info);
        }
    }

    Ok(files)
}

pub fn load_client_state(state_path: &Path) -> Result<ClientState> {
    if !state_path.exists() {
        return Ok(ClientState {
            files: std::collections::HashMap::new(),
            deleted_files: HashMap::new(),
            last_sync: Utc::now(),
        });
    }

    let content = fs::read_to_string(state_path)?;
    let state: ClientState = serde_json::from_str(&content).unwrap_or_else(|_| {
        // If deserialization fails (e.g., due to missing fields), create a new state
        ClientState {
            files: std::collections::HashMap::new(),
            deleted_files: HashMap::new(),
            last_sync: Utc::now(),
        }
    });

    // Ensure deleted_files field exists (for backward compatibility)
    if state.deleted_files.is_empty() {
        // This is fine - it could be a legitimately empty list or missing from old format
    }

    Ok(state)
}

pub fn save_client_state(state: &ClientState, state_path: &Path) -> Result<()> {
    let content = serde_json::to_string_pretty(state)?;
    fs::write(state_path, content)?;
    Ok(())
}

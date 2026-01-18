use crate::types::{ClientState, FileInfo, BlockMsg};
use anyhow::Result;
use chrono::{DateTime, Utc};
use duckdb::{Connection, params};
use glob::Pattern;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::path::Path;
use std::io::{Read, Seek, SeekFrom, Write};
use walkdir::WalkDir;
use tracing::{warn, debug};

pub fn calculate_file_hash(path: &Path) -> Result<String> {
    let contents = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&contents);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn calculate_block_hashes(path: &Path, block_size: u64) -> Result<Vec<BlockMsg>> {
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let len = metadata.len();
    let mut block_hashes = Vec::new();
    let mut buffer = vec![0u8; block_size as usize];

    // Calculate number of blocks
    let num_blocks = (len + block_size - 1) / block_size;

    for i in 0..num_blocks {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        
        // Hash the actual bytes read (slice of buffer)
        let mut hasher = Sha256::new();
        hasher.update(&buffer[0..bytes_read]);
        let hash = format!("{:x}", hasher.finalize());

        block_hashes.push(BlockMsg {
            index: i,
            hash,
        });
    }

    Ok(block_hashes)
}

pub fn patch_file(path: &Path, offset: u64, content: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .read(true) // Open for reading too, just in case, though write/create is usually enough specifically for writing
        .open(path)?;
        
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(content)?;
    Ok(())
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
    scan_directory_with_patterns(dir_path, &[])
}

pub fn scan_directory_with_patterns(dir_path: &Path, exclude_patterns: &[String]) -> Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    
    // Compile exclude patterns once for efficiency
    let compiled_patterns: Result<Vec<Pattern>, _> = exclude_patterns
        .iter()
        .map(|pattern| Pattern::new(pattern))
        .collect();
    
    let compiled_patterns = match compiled_patterns {
        Ok(patterns) => patterns,
        Err(e) => {
            warn!("Invalid exclude pattern: {}", e);
            Vec::new() // Continue with no patterns if any are invalid
        }
    };

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
            
            // Check if file matches any exclude pattern
            let should_exclude = compiled_patterns.iter().any(|pattern| {
                // Check the full relative path for pattern matches
                if pattern.matches(&relative_path_str) {
                    return true;
                }
                
                // Check if any component of the path matches the pattern
                // This handles cases like "node_modules" matching "node_modules/package.json"
                if relative_path.components().any(|component| {
                    pattern.matches(component.as_os_str().to_string_lossy().as_ref())
                }) {
                    return true;
                }
                
                // Check just the filename
                pattern.matches(entry.path().file_name().unwrap_or_default().to_string_lossy().as_ref())
            });
            
            if should_exclude {
                debug!("Excluding file due to pattern match: {}", relative_path_str);
                continue;
            }

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

// DuckDB-based state management functions
pub fn init_state_database(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    
    // Create tables if they don't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sync_state (
            last_sync TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_states (
            file_path TEXT PRIMARY KEY,
            file_hash TEXT NOT NULL,
            file_size BIGINT NOT NULL,
            modified_at TEXT NOT NULL
        )",
        [],
    )?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS deleted_files (
            file_path TEXT PRIMARY KEY,
            deleted_at TEXT NOT NULL
        )",
        [],
    )?;
    
    // Initialize sync_state if empty
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sync_state",
        [],
        |row| row.get(0)
    )?;
    
    if count == 0 {
        conn.execute(
            "INSERT INTO sync_state (last_sync) VALUES (?)",
            params![Utc::now().to_rfc3339()],
        )?;
    }
    
    Ok(conn)
}

pub fn load_client_state_db(db_path: &Path) -> Result<ClientState> {
    let conn = init_state_database(db_path)?;
    
    // Load last sync time
    let last_sync_str: String = conn.query_row(
        "SELECT last_sync FROM sync_state LIMIT 1",
        [],
        |row| row.get(0)
    )?;
    let last_sync = DateTime::parse_from_rfc3339(&last_sync_str)?.with_timezone(&Utc);
    
    // Load files
    let mut files = HashMap::new();
    let mut stmt = conn.prepare("SELECT file_path, file_hash, file_size, modified_at FROM file_states")?;
    let file_iter = stmt.query_map([], |row| {
        let modified_str: String = row.get(3)?;
        let modified = match DateTime::parse_from_rfc3339(&modified_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return Err(duckdb::Error::InvalidColumnIndex(3)),
        };
        
        Ok(FileInfo {
            path: row.get(0)?,
            hash: row.get(1)?,
            size: row.get(2)?,
            modified,
        })
    })?;
    
    for file_result in file_iter {
        let file_info = file_result?;
        files.insert(file_info.path.clone(), file_info);
    }
    
    // Load deleted files
    let mut deleted_files = HashMap::new();
    let mut stmt = conn.prepare("SELECT file_path, deleted_at FROM deleted_files")?;
    let deleted_iter = stmt.query_map([], |row| {
        let deleted_at_str: String = row.get(1)?;
        let deleted_at = match DateTime::parse_from_rfc3339(&deleted_at_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(_) => return Err(duckdb::Error::InvalidColumnIndex(1)),
        };
        
        Ok((row.get::<_, String>(0)?, deleted_at))
    })?;
    
    for deleted_result in deleted_iter {
        let (path, deleted_at) = deleted_result?;
        deleted_files.insert(path, deleted_at);
    }
    
    Ok(ClientState {
        files,
        deleted_files,
        last_sync,
    })
}

pub fn save_client_state_db(state: &ClientState, db_path: &Path) -> Result<()> {
    let conn = init_state_database(db_path)?;
    
    // Begin transaction for consistency
    let tx = conn.unchecked_transaction()?;
    
    // Update last sync time
    tx.execute(
        "UPDATE sync_state SET last_sync = ?",
        params![state.last_sync.to_rfc3339()],
    )?;
    
    // Clear existing file states
    tx.execute("DELETE FROM file_states", [])?;
    
    // Insert current file states
    for file_info in state.files.values() {
        tx.execute(
            "INSERT INTO file_states (file_path, file_hash, file_size, modified_at) VALUES (?, ?, ?, ?)",
            params![file_info.path, file_info.hash, file_info.size, file_info.modified.to_rfc3339()],
        )?;
    }
    
    // Clear existing deleted files
    tx.execute("DELETE FROM deleted_files", [])?;
    
    // Insert current deleted files
    for (path, deleted_at) in &state.deleted_files {
        tx.execute(
            "INSERT INTO deleted_files (file_path, deleted_at) VALUES (?, ?)",
            params![path, deleted_at.to_rfc3339()],
        )?;
    }
    
    // Commit transaction
    tx.commit()?;
    
    Ok(())
}

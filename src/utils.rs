use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::types::{ClientState, FileInfo};

/// Calculate SHA-256 hash of file contents
pub fn calculate_file_hash(path: &Path) -> Result<String> {
    let content = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Get file metadata and hash
pub fn get_file_info(path: &Path, relative_path: &str) -> Result<FileInfo> {
    let metadata = fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;
    
    let modified = metadata
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();
    
    Ok(FileInfo {
        path: relative_path.to_string(),
        hash,
        size: metadata.len(),
        modified: DateTime::from_timestamp(modified as i64, 0)
            .unwrap_or_else(Utc::now),
    })
}

/// Scan directory and return file information, respecting ignore patterns
pub fn scan_directory(
    dir_path: &Path, 
    ignore_patterns: &[String]
) -> Result<HashMap<String, FileInfo>> {
    let mut files = HashMap::new();
    
    if !dir_path.exists() {
        tracing::debug!("Directory does not exist: {}", dir_path.display());
        return Ok(files);
    }
    
    for entry in WalkDir::new(dir_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let file_path = entry.path();
        let relative_path = file_path
            .strip_prefix(dir_path)?
            .to_string_lossy()
            .replace('\\', "/"); // Normalize path separators
        
        // Check if file matches any ignore pattern
        if should_ignore_file(&relative_path, ignore_patterns) {
            tracing::debug!("Ignoring file: {}", relative_path);
            continue;
        }
        
        match get_file_info(file_path, &relative_path) {
            Ok(file_info) => {
                files.insert(relative_path.clone(), file_info);
            }
            Err(e) => {
                tracing::warn!("Failed to get info for file {}: {}", relative_path, e);
            }
        }
    }
    
    Ok(files)
}

/// Check if a file should be ignored based on gitignore-style patterns
pub fn should_ignore_file(relative_path: &str, ignore_patterns: &[String]) -> bool {
    for pattern in ignore_patterns {
        if matches_gitignore_pattern(pattern, relative_path) {
            return true;
        }
    }
    false
}

/// Match a single gitignore-style pattern against a file path
fn matches_gitignore_pattern(pattern: &str, file_path: &str) -> bool {
    // Handle empty patterns
    if pattern.is_empty() {
        return false;
    }
    
    // Handle patterns with invalid glob syntax by falling back to basic matching
    let glob_result = glob::Pattern::new(pattern);
    let glob_pattern = match glob_result {
        Ok(pattern) => pattern,
        Err(_) => {
            tracing::warn!("Invalid ignore pattern: {}", pattern);
            return false;
        }
    };
    
    // For gitignore-style behavior, we need to handle different pattern types
    
    // 1. If pattern ends with '/', it only matches directories (not implemented for files)
    if pattern.ends_with('/') {
        // For files, directory patterns don't match - this would need directory context
        return false;
    }
    
    // 2. If pattern starts with '**/], it can match at any depth
    if pattern.starts_with("**/") {
        return glob_pattern.matches(file_path);
    }
    
    // 3. If pattern contains '**' anywhere, use full glob matching
    if pattern.contains("**") {
        return glob_pattern.matches(file_path);
    }
    
    // 4. If pattern contains '/' anywhere, it's a path-based pattern
    // But we need to be careful about the depth matching
    if pattern.contains('/') {
        // For gitignore, path patterns should match exactly at the specified depth
        // unless they contain wildcards that allow for deeper matching
        
        // Count the number of '/' in pattern and file_path
        let pattern_depth = pattern.matches('/').count();
        let file_depth = file_path.matches('/').count();
        
        // If the pattern doesn't end with a wildcard and has fewer path segments
        // than the file, it shouldn't match (unless it contains **)
        if !pattern.ends_with('*') && pattern_depth < file_depth {
            return false;
        }
        
        return glob_pattern.matches(file_path);
    }
    
    // 5. For simple patterns (like *.tmp), only match if the file is at the root level
    // This is the key gitignore behavior: simple patterns don't match nested files
    if file_path.contains('/') {
        // File is in a subdirectory, simple patterns should not match
        return false;
    }
    
    // File is at root level, match against the filename
    glob_pattern.matches(file_path)
}

/// Initialize SQLite database for state storage
pub fn init_database(db_path: &Path) -> Result<Connection> {
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
            file_size INTEGER NOT NULL,
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
        |row| row.get(0),
    )?;
    
    if count == 0 {
        conn.execute(
            "INSERT INTO sync_state (last_sync) VALUES (?)",
            [&Utc::now().to_rfc3339()],
        )?;
    }
    
    Ok(conn)
}

/// Load client state from SQLite database
pub fn load_client_state(db_path: &Path) -> Result<ClientState> {
    if !db_path.exists() {
        return Ok(ClientState::default());
    }
    
    let conn = init_database(db_path)?;
    
    // Load last sync time
    let last_sync: String = conn.query_row(
        "SELECT last_sync FROM sync_state ORDER BY rowid DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).unwrap_or_else(|_| Utc::now().to_rfc3339());
    
    let last_sync = DateTime::parse_from_rfc3339(&last_sync)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    
    // Load file states
    let mut stmt = conn.prepare(
        "SELECT file_path, file_hash, file_size, modified_at FROM file_states"
    )?;
    
    let file_rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let hash: String = row.get(1)?;
        let size: i64 = row.get(2)?;
        let modified_str: String = row.get(3)?;
        
        let modified = DateTime::parse_from_rfc3339(&modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        
        Ok((path.clone(), FileInfo {
            path,
            hash,
            size: size as u64,
            modified,
        }))
    })?;
    
    let mut files = HashMap::new();
    for row_result in file_rows {
        if let Ok((path, file_info)) = row_result {
            files.insert(path, file_info);
        }
    }
    
    // Load deleted files
    let mut stmt = conn.prepare(
        "SELECT file_path, deleted_at FROM deleted_files"
    )?;
    
    let deleted_rows = stmt.query_map([], |row| {
        let path: String = row.get(0)?;
        let deleted_str: String = row.get(1)?;
        
        let deleted_at = DateTime::parse_from_rfc3339(&deleted_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        
        Ok((path, deleted_at))
    })?;
    
    let mut deleted_files = HashMap::new();
    for row_result in deleted_rows {
        if let Ok((path, deleted_at)) = row_result {
            deleted_files.insert(path, deleted_at);
        }
    }
    
    Ok(ClientState {
        files,
        deleted_files,
        last_sync,
    })
}

/// Save client state to SQLite database
pub fn save_client_state(db_path: &Path, state: &ClientState) -> Result<()> {
    let mut conn = init_database(db_path)?;
    
    // Use transaction for atomic updates
    let tx = conn.transaction()?;
    
    // Update sync time
    tx.execute(
        "UPDATE sync_state SET last_sync = ?",
        [&state.last_sync.to_rfc3339()],
    )?;
    
    // Clear and update file states
    tx.execute("DELETE FROM file_states", [])?;
    
    {
        let mut stmt = tx.prepare(
            "INSERT INTO file_states (file_path, file_hash, file_size, modified_at) VALUES (?, ?, ?, ?)"
        )?;
        
        for (path, file_info) in &state.files {
            stmt.execute([
                path,
                &file_info.hash,
                &file_info.size.to_string(),
                &file_info.modified.to_rfc3339(),
            ])?;
        }
    } // stmt is dropped here
    
    // Clear and update deleted files
    tx.execute("DELETE FROM deleted_files", [])?;
    
    {
        let mut stmt = tx.prepare(
            "INSERT INTO deleted_files (file_path, deleted_at) VALUES (?, ?)"
        )?;
        
        for (path, deleted_at) in &state.deleted_files {
            stmt.execute([
                path,
                &deleted_at.to_rfc3339(),
            ])?;
        }
    } // stmt is dropped here
    
    tx.commit()?;
    Ok(())
}

/// Ensure directory exists, create if necessary
pub fn ensure_directory_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
        tracing::info!("Created directory: {}", path.display());
    }
    Ok(())
}

/// Write file content to disk atomically
pub fn write_file_atomic(path: &Path, content: &[u8]) -> Result<()> {
    let temp_path = path.with_extension("tmp");
    
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        ensure_directory_exists(parent)?;
    }
    
    // Write to temporary file first
    fs::write(&temp_path, content)?;
    
    // Atomically move to final location
    fs::rename(&temp_path, path)?;
    
    Ok(())
}

/// Get relative path from base directory
pub fn get_relative_path(full_path: &Path, base_path: &Path) -> Result<String> {
    let relative = full_path.strip_prefix(base_path)?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

/// Convert relative path to absolute path within base directory
pub fn resolve_path(base_path: &Path, relative_path: &str) -> PathBuf {
    let normalized = relative_path.replace('/', std::path::MAIN_SEPARATOR_STR);
    base_path.join(normalized)
}

/// Clean up old entries from deleted files based on age
pub fn cleanup_deleted_entries(
    deleted_files: &mut HashMap<String, DateTime<Utc>>,
    max_age_days: i64,
) {
    let cutoff = Utc::now() - chrono::Duration::days(max_age_days);
    deleted_files.retain(|_, deleted_at| *deleted_at > cutoff);
}

/// Expand tilde (~) in path to home directory
pub fn expand_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Some(home_dir) = dirs::home_dir() {
            home_dir.join(&path[2..])
        } else {
            PathBuf::from(path)
        }
    } else if path == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(path))
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_calculate_file_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let hash = calculate_file_hash(&file_path).unwrap();
        assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[test]
    fn test_should_ignore_file() {
        let patterns = vec!["*.tmp".to_string(), "node_modules/**".to_string()];
        
        assert!(should_ignore_file("test.tmp", &patterns));
        assert!(should_ignore_file("node_modules/package.json", &patterns));
        assert!(!should_ignore_file("test.txt", &patterns));
        assert!(!should_ignore_file("src/main.rs", &patterns));
    }

    #[test]
    fn test_directory_exclude_patterns() {
        // Test the problematic pattern
        let patterns_wrong = vec!["temp/".to_string()];
        let patterns_correct = vec!["temp/**".to_string()];
        
        // The wrong pattern "temp/" only matches a directory named exactly "temp/"
        assert!(!should_ignore_file("temp/temp_file.txt", &patterns_wrong));
        
        // The correct pattern "temp/**" matches files within the temp directory
        assert!(should_ignore_file("temp/temp_file.txt", &patterns_correct));
        assert!(should_ignore_file("temp/subdir/file.txt", &patterns_correct));
    }

    #[test]
    fn test_nested_directory_exclude_patterns() {
        // Test comprehensive nested directory patterns with corrected expectations
        let patterns = vec![
            "node_modules/**".to_string(),
            "target/**".to_string(),
            ".git/**".to_string(),
            "*.tmp".to_string(),      // This should only match top-level .tmp files
            "**/*.log".to_string(),   // This should match .log files at any level
        ];
        
        // Test files that should be ignored
        let should_ignore = vec![
            "node_modules/package.json",
            "node_modules/some-package/dist/index.js",
            "node_modules/deep/nested/path/file.js",
            "target/debug/app",
            "target/release/optimized/binary",
            ".git/config",
            ".git/objects/abc123",
            ".git/refs/heads/main",
            "build.tmp",              // Top-level .tmp file should be ignored
            "app.log",               // Will be caught by **/*.log
            "logs/debug.log",        // Will be caught by **/*.log
        ];
        
        for path in should_ignore {
            assert!(should_ignore_file(path, &patterns), 
                "Expected '{}' to be ignored with patterns: {:?}", path, patterns);
        }
        
        // Test files that should NOT be ignored  
        let should_include = vec![
            "src/main.rs",
            "Cargo.toml",
            "README.md",
            "docs/guide.md",
            "tests/integration_test.rs",
            "src/components/header.rs",
            "public/index.html",
            "temp/file.tmp",         // Nested .tmp file should NOT be ignored by *.tmp
        ];
        
        for path in should_include {
            assert!(!should_ignore_file(path, &patterns),
                "Expected '{}' to NOT be ignored with patterns: {:?}", path, patterns);
        }
    }

    #[test]
    fn test_scan_directory_with_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path();
        
        // Create a complex directory structure
        fs::create_dir_all(base_path.join("src/components")).unwrap();
        fs::create_dir_all(base_path.join("node_modules/package")).unwrap();
        fs::create_dir_all(base_path.join("target/debug")).unwrap();
        fs::create_dir_all(base_path.join("logs")).unwrap();
        fs::create_dir_all(base_path.join("temp")).unwrap();
        
        // Create various files
        fs::write(base_path.join("README.md"), "readme").unwrap();
        fs::write(base_path.join("app.tmp"), "root tmp file").unwrap();
        fs::write(base_path.join("src/main.rs"), "main code").unwrap();
        fs::write(base_path.join("src/components/header.rs"), "header component").unwrap();
        fs::write(base_path.join("node_modules/package.json"), "package config").unwrap();
        fs::write(base_path.join("node_modules/package/index.js"), "package code").unwrap();
        fs::write(base_path.join("target/debug/app"), "binary").unwrap();
        fs::write(base_path.join("logs/app.log"), "log content").unwrap();
        fs::write(base_path.join("temp/cache.tmp"), "nested tmp file").unwrap();
        fs::write(base_path.join("debug.log"), "root log file").unwrap();
        
        // Test with gitignore-style patterns
        let ignore_patterns = vec![
            "node_modules/**".to_string(),
            "target/**".to_string(),
            "*.tmp".to_string(),          // Should only match root-level .tmp files
            "**/*.log".to_string(),       // Should match .log files at any level
        ];
        
        let files = scan_directory(base_path, &ignore_patterns).unwrap();
        
        // Files that should be included
        let expected_included = vec![
            "README.md",
            "src/main.rs",
            "src/components/header.rs",
            "temp/cache.tmp",  // This should be included because *.tmp only matches root level
        ];
        
        for file in expected_included {
            assert!(files.contains_key(file), 
                "Expected '{}' to be included in scan results", file);
        }
        
        // Files that should be excluded
        let expected_excluded = vec![
            "app.tmp",                    // Excluded by *.tmp
            "node_modules/package.json",  // Excluded by node_modules/**
            "node_modules/package/index.js", // Excluded by node_modules/**
            "target/debug/app",          // Excluded by target/**
            "logs/app.log",              // Excluded by **/*.log
            "debug.log",                 // Excluded by **/*.log
        ];
        
        for file in expected_excluded {
            assert!(!files.contains_key(file), 
                "Expected '{}' to be excluded from scan results", file);
        }
        
        println!("Scan results:");
        for (file, _) in &files {
            println!("  Included: {}", file);
        }
    }

    #[test]
    fn test_get_relative_path() {
        let base = Path::new("/home/user/documents");
        let full = Path::new("/home/user/documents/folder/file.txt");
        
        let relative = get_relative_path(full, base).unwrap();
        assert_eq!(relative, "folder/file.txt");
    }

    #[test]
    fn test_expand_path() {
        // Test tilde expansion with slash
        let path = "~/Documents/test.txt";
        let expanded = expand_path(path);
        assert!(expanded.to_string_lossy().contains("Documents/test.txt"));
        assert!(!expanded.to_string_lossy().starts_with("~/"));

        // Test tilde only
        let path = "~";
        let expanded = expand_path(path);
        assert!(!expanded.to_string_lossy().starts_with("~"));

        // Test regular path (no tilde)
        let path = "/home/user/test.txt";
        let expanded = expand_path(path);
        assert_eq!(expanded, PathBuf::from(path));

        // Test relative path (no tilde)
        let path = "./test.txt";
        let expanded = expand_path(path);
        assert_eq!(expanded, PathBuf::from(path));
    }
}
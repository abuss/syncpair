use anyhow::Result;
use notify::{Watcher, RecursiveMode, Result as NotifyResult, Event, EventKind};
use reqwest::Client;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::time::{sleep, interval, MissedTickBehavior};
use tokio::sync::broadcast;

use crate::types::{FileInfo, UploadRequest, UploadResponse, SyncRequest, SyncResponse, DownloadResponse, DeleteRequest, DeleteResponse};
use crate::utils::{scan_directory, get_file_info, load_client_state, save_client_state, calculate_file_hash};

pub struct SimpleClient {
    server_url: String,
    watch_dir: PathBuf,
    state_file: PathBuf,
    http_client: Client,
    sync_interval: Duration,
}

impl SimpleClient {
    pub fn new(server_url: String, watch_dir: PathBuf) -> Self {
        let state_file = watch_dir.join(".syncit_state.json");
        
        Self {
            server_url,
            watch_dir,
            state_file,
            http_client: Client::new(),
            sync_interval: Duration::from_secs(30), // Default: sync every 30 seconds
        }
    }

    pub fn with_sync_interval(mut self, interval: Duration) -> Self {
        self.sync_interval = interval;
        self
    }

    pub async fn initial_sync(&self) -> Result<()> {
        println!("Starting bidirectional sync...");
        
        let current_files = scan_directory(&self.watch_dir)?;
        let mut state = load_client_state(&self.state_file)?;
        
        // Build client file map
        let mut client_files = std::collections::HashMap::new();
        for file_info in current_files {
            client_files.insert(file_info.path.clone(), file_info);
        }
        
        // Detect files that were deleted since last sync
        let mut newly_deleted_files = Vec::new();
        for (old_path, _) in &state.files {
            if !client_files.contains_key(old_path) {
                println!("🗑️  Detected deletion: {}", old_path);
                newly_deleted_files.push(old_path.clone());
            }
        }
        
        // Add newly deleted files to the deleted files list
        state.deleted_files.extend(newly_deleted_files);
        
        // Send sync request to server
        let sync_request = SyncRequest {
            files: client_files.clone(),
            deleted_files: state.deleted_files.clone(),
            last_sync: state.last_sync,
        };
        
        let url = format!("{}/sync", self.server_url);
        let sync_response: SyncResponse = self.http_client
            .post(&url)
            .json(&sync_request)
            .send()
            .await?
            .json()
            .await?;
        
        // Handle conflicts first
        for conflict in &sync_response.conflicts {
            println!("⚠️  Conflict detected for file: {}", conflict.path);
            println!("   Client: modified {}, hash {}", conflict.client_file.modified, &conflict.client_file.hash[..8]);
            println!("   Server: modified {}, hash {}", conflict.server_file.modified, &conflict.server_file.hash[..8]);
            
            // For now, use a simple strategy: newer file wins, client wins on tie
            if conflict.server_file.modified > conflict.client_file.modified {
                println!("   → Downloading server version (newer)");
                if let Err(e) = self.download_file(&conflict.path).await {
                    println!("   ✗ Failed to download {}: {}", conflict.path, e);
                    println!("   → Keeping client version instead");
                }
            } else {
                println!("   → Uploading client version (newer or same time)");
                if let Some(file_info) = client_files.get(&conflict.path) {
                    if let Err(e) = self.upload_file(file_info).await {
                        println!("   ✗ Failed to upload {}: {}", conflict.path, e);
                    }
                }
            }
        }
        
        // Upload files that need to be uploaded
        for file_path in &sync_response.files_to_upload {
            if let Some(file_info) = client_files.get(file_path) {
                println!("↑ Uploading: {}", file_path);
                if let Err(e) = self.upload_file(file_info).await {
                    println!("✗ Failed to upload {}: {}", file_path, e);
                }
            }
        }
        
        // Download files that need to be downloaded
        for file_info in &sync_response.files_to_download {
            println!("↓ Downloading: {}", file_info.path);
            if let Err(e) = self.download_file(&file_info.path).await {
                println!("✗ Failed to download {}: {}", file_info.path, e);
                println!("  → File may have been deleted from server or is inaccessible");
            }
        }
        
        // Delete files that need to be deleted
        for file_path in &sync_response.files_to_delete {
            println!("🗑️  Deleting: {}", file_path);
            if let Err(e) = self.delete_file(file_path).await {
                println!("✗ Failed to delete {}: {}", file_path, e);
            }
        }
        
        // Update state with all current files (re-scan after downloads)
        let final_files = scan_directory(&self.watch_dir)?;
        state.files.clear();
        for file_info in final_files {
            state.files.insert(file_info.path.clone(), file_info);
        }
        
        // Clear deleted files list after successful sync
        state.deleted_files.clear();
        
        state.last_sync = chrono::Utc::now();
        save_client_state(&state, &self.state_file)?;
        
        println!("✓ Bidirectional sync completed");
        Ok(())
    }

    pub async fn start_watching(&self) -> Result<()> {
        self.start_watching_with_shutdown(None).await
    }

    pub async fn start_watching_with_shutdown(&self, mut shutdown_rx: Option<broadcast::Receiver<()>>) -> Result<()> {
        println!("Starting file system watcher for: {}", self.watch_dir.display());
        println!("Periodic sync interval: {} seconds", self.sync_interval.as_secs());
        
        // Perform initial sync
        self.initial_sync().await?;
        
        // Set up file system watcher
        let (tx, rx) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;
        
        watcher.watch(&self.watch_dir, RecursiveMode::Recursive)?;
        
        // Convert the std::sync::mpsc::Receiver to tokio-compatible
        let (async_tx, mut async_rx) = tokio::sync::mpsc::unbounded_channel();
        
        // Spawn a task to bridge std::sync::mpsc to tokio::sync::mpsc
        tokio::task::spawn_blocking(move || {
            while let Ok(event) = rx.recv() {
                if async_tx.send(event).is_err() {
                    break; // Channel closed
                }
            }
        });
        
        // Set up periodic sync timer
        let mut sync_timer = interval(self.sync_interval);
        sync_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        
        // Process file change events with proper shutdown handling and periodic sync
        loop {
            tokio::select! {
                // Check for shutdown signal
                shutdown_result = async {
                    if let Some(ref mut shutdown_rx) = shutdown_rx {
                        shutdown_rx.recv().await
                    } else {
                        // If no shutdown receiver, wait indefinitely
                        std::future::pending::<Result<(), broadcast::error::RecvError>>().await
                    }
                } => {
                    match shutdown_result {
                        Ok(_) => {
                            println!("Received shutdown signal, stopping file watcher...");
                            break;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            println!("Shutdown channel closed, stopping file watcher...");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            println!("Shutdown signal lagged, stopping file watcher...");
                            break;
                        }
                    }
                }
                
                // Periodic sync trigger
                _ = sync_timer.tick() => {
                    println!("🔄 Performing periodic sync...");
                    if let Err(e) = self.initial_sync().await {
                        eprintln!("Error during periodic sync: {}", e);
                    }
                }
                
                // Check for file system events
                event = async_rx.recv() => {
                    match event {
                        Some(event) => {
                            if let Err(e) = self.handle_file_event(event).await {
                                eprintln!("Error handling file event: {}", e);
                            }
                        }
                        None => {
                            eprintln!("File watcher channel closed");
                            break;
                        }
                    }
                }
            }
        }
        
        // Save final state before stopping
        if let Err(e) = self.save_final_state().await {
            eprintln!("Warning: Failed to save final client state: {}", e);
        }
        
        println!("File watcher stopped successfully");
        Ok(())
    }

    async fn handle_file_event(&self, event: Event) -> Result<()> {
        match event.kind {
            EventKind::Create(_) | EventKind::Modify(_) => {
                for path in event.paths {
                    if path.is_file() && self.should_sync_file(&path) {
                        // Add a small delay to avoid partial file writes
                        sleep(Duration::from_millis(100)).await;
                        
                        if let Err(e) = self.handle_file_change(&path).await {
                            eprintln!("Error syncing file {}: {}", path.display(), e);
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if self.should_sync_file(&path) {
                        if let Err(e) = self.handle_file_deletion(&path).await {
                            eprintln!("Error handling file deletion {}: {}", path.display(), e);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_file_change(&self, file_path: &std::path::Path) -> Result<()> {
        if let Ok(relative_path) = file_path.strip_prefix(&self.watch_dir) {
            let relative_path_str = relative_path.to_string_lossy().to_string();
            
            let file_info = get_file_info(file_path, &relative_path_str)?;
            
            // Check if file actually changed
            let mut state = load_client_state(&self.state_file)?;
            let should_upload = match state.files.get(&file_info.path) {
                Some(existing) => existing.hash != file_info.hash,
                None => true,
            };
            
            if should_upload {
                println!("Detected change in: {}", relative_path_str);
                self.upload_file(&file_info).await?;
                
                state.files.insert(file_info.path.clone(), file_info);
                state.last_sync = chrono::Utc::now();
                save_client_state(&state, &self.state_file)?;
            }
        }
        
        Ok(())
    }

    async fn handle_file_deletion(&self, file_path: &std::path::Path) -> Result<()> {
        if let Ok(relative_path) = file_path.strip_prefix(&self.watch_dir) {
            let relative_path_str = relative_path.to_string_lossy().to_string();
            
            println!("Detected deletion: {}", relative_path_str);
            
            // Load current state
            let mut state = load_client_state(&self.state_file)?;
            
            // Check if this file was in our tracked files
            if state.files.contains_key(&relative_path_str) {
                // Remove from tracked files and add to deleted list
                state.files.remove(&relative_path_str);
                
                // Add to deleted files list if not already there
                if !state.deleted_files.contains(&relative_path_str) {
                    state.deleted_files.push(relative_path_str.clone());
                }
                
                // Send delete request to server immediately
                if let Err(e) = self.send_delete_request(&relative_path_str).await {
                    eprintln!("Failed to send delete request for {}: {}", relative_path_str, e);
                } else {
                    println!("✓ Deletion synced to server: {}", relative_path_str);
                }
                
                state.last_sync = chrono::Utc::now();
                save_client_state(&state, &self.state_file)?;
            }
        }
        
        Ok(())
    }

    async fn upload_file(&self, file_info: &FileInfo) -> Result<()> {
        let file_path = self.watch_dir.join(&file_info.path);
        let content = std::fs::read(&file_path)?;
        
        // Verify hash before upload
        let actual_hash = calculate_file_hash(&file_path)?;
        if actual_hash != file_info.hash {
            return Err(anyhow::anyhow!("Hash mismatch for file: {}", file_info.path));
        }
        
        let upload_request = UploadRequest {
            file_info: file_info.clone(),
            content,
        };
        
        let url = format!("{}/upload", self.server_url);
        let response: UploadResponse = self.http_client
            .post(&url)
            .json(&upload_request)
            .send()
            .await?
            .json()
            .await?;
        
        if response.success {
            println!("✓ Uploaded: {}", file_info.path);
        } else {
            return Err(anyhow::anyhow!("Upload failed: {}", response.message));
        }
        
        Ok(())
    }

    async fn download_file(&self, file_path: &str) -> Result<()> {
        let url = format!("{}/download/{}", self.server_url, urlencoding::encode(file_path));
        let response: DownloadResponse = self.http_client
            .get(&url)
            .send()
            .await?
            .json()
            .await?;
        
        if response.success {
            if let (Some(file_info), Some(content)) = (response.file_info, response.content) {
                let local_path = self.watch_dir.join(&file_info.path);
                
                // Create parent directories if needed
                if let Some(parent) = local_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                
                // Write the file
                std::fs::write(&local_path, &content)?;
                
                // Verify the hash
                let actual_hash = calculate_file_hash(&local_path)?;
                if actual_hash != file_info.hash {
                    std::fs::remove_file(&local_path)?;
                    return Err(anyhow::anyhow!("Hash mismatch for downloaded file: {}", file_info.path));
                }
                
                println!("✓ Downloaded: {}", file_info.path);
            } else {
                return Err(anyhow::anyhow!("Download response missing file info or content"));
            }
        } else {
            return Err(anyhow::anyhow!("Download failed: {}", response.message));
        }
        
        Ok(())
    }

    async fn delete_file(&self, file_path: &str) -> Result<()> {
        let local_path = self.watch_dir.join(file_path);
        
        if local_path.exists() {
            if local_path.is_file() {
                std::fs::remove_file(&local_path)?;
                println!("✓ Deleted: {}", file_path);
            } else if local_path.is_dir() {
                std::fs::remove_dir_all(&local_path)?;
                println!("✓ Deleted directory: {}", file_path);
            }
        } else {
            // File already doesn't exist, which is fine
            println!("ℹ️  File already deleted: {}", file_path);
        }
        
        Ok(())
    }

    async fn send_delete_request(&self, file_path: &str) -> Result<()> {
        let delete_request = DeleteRequest {
            path: file_path.to_string(),
        };
        
        let url = format!("{}/delete", self.server_url);
        let response: DeleteResponse = self.http_client
            .post(&url)
            .json(&delete_request)
            .send()
            .await?
            .json()
            .await?;
        
        if response.success {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Delete request failed: {}", response.message))
        }
    }

    fn should_sync_file(&self, path: &std::path::Path) -> bool {
        // Skip hidden files and the state file
        if let Some(file_name) = path.file_name() {
            let name = file_name.to_string_lossy();
            !name.starts_with('.') && name != ".syncit_state.json"
        } else {
            false
        }
    }

    async fn save_final_state(&self) -> Result<()> {
        // Perform one final scan to ensure state is up to date
        let current_files = scan_directory(&self.watch_dir)?;
        let mut state = load_client_state(&self.state_file)?;
        
        // Update state with current files
        for file_info in current_files {
            state.files.insert(file_info.path.clone(), file_info);
        }
        
        state.last_sync = chrono::Utc::now();
        save_client_state(&state, &self.state_file)?;
        println!("Final client state saved");
        Ok(())
    }
}


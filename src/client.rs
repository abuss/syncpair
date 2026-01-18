use anyhow::Result;
use futures::stream::{self, StreamExt};
use notify::{Event, EventKind, RecursiveMode, Result as NotifyResult, Watcher};
use reqwest::Client;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::{interval, sleep, MissedTickBehavior};
use tracing::{debug, error, info, warn};

use crate::types::{
    BlockUploadRequest, BlockUploadResponse, DeleteRequest, DeleteResponse, DeltaCompleteRequest,
    DeltaCompleteResponse, DeltaInitRequest, DeltaInitResponse, DownloadResponse, FileInfo,
    SyncRequest, SyncResponse, UploadRequest, UploadResponse,
};
use crate::utils::{
    calculate_block_hashes, calculate_file_hash, get_file_info, load_client_state_db,
    save_client_state_db, scan_directory_with_patterns,
};

const DELTA_SYNC_THRESHOLD: u64 = 1024 * 1024; // 1 MB
const BLOCK_SIZE: u64 = 1024 * 1024; // 1 MB

#[derive(Clone)]
pub struct SimpleClient {
    server_url: String,
    watch_dir: PathBuf,
    state_db: PathBuf,
    http_client: Client,
    sync_interval: Duration,
    client_id: Option<String>,
    directory: Option<String>,
    exclude_patterns: Vec<String>,
}

impl SimpleClient {
    pub fn new(server_url: String, watch_dir: PathBuf) -> Self {
        let state_db = watch_dir.join(".syncpair_state.db");

        // Create HTTP client with timeouts
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            server_url,
            watch_dir,
            state_db,
            http_client,
            sync_interval: Duration::from_secs(30), // Default: sync every 30 seconds
            client_id: None,
            directory: None,
            exclude_patterns: Vec::new(),
        }
    }

    pub fn with_sync_interval(mut self, interval: Duration) -> Self {
        self.sync_interval = interval;
        self
    }

    pub fn with_client_id(mut self, client_id: String) -> Self {
        self.client_id = Some(client_id);
        self
    }

    pub fn with_directory(mut self, directory: String) -> Self {
        self.directory = Some(directory);
        self
    }

    pub fn with_exclude_patterns(mut self, patterns: Vec<String>) -> Self {
        self.exclude_patterns = patterns;
        self
    }

    pub async fn initial_sync(&self) -> Result<()> {
        if self.directory.is_none() {
            return Err(anyhow::anyhow!(
                "Directory must be specified for client operations"
            ));
        }

        info!("Starting bidirectional sync...");

        let current_files = scan_directory_with_patterns(&self.watch_dir, &self.exclude_patterns)?;
        let mut state = load_client_state_db(&self.state_db)?;

        // Build client file map
        let mut client_files = std::collections::HashMap::new();
        for file_info in current_files {
            client_files.insert(file_info.path.clone(), file_info);
        }

        // Detect files that were deleted since last sync
        let mut newly_deleted_files = std::collections::HashMap::new();
        for old_path in state.files.keys() {
            if !client_files.contains_key(old_path) {
                info!("🗑️  Detected deletion: {}", old_path);
                let deletion_time = chrono::Utc::now();
                newly_deleted_files.insert(old_path.clone(), deletion_time);
            }
        }

        // Add newly deleted files to the deleted files map
        state.deleted_files.extend(newly_deleted_files);

        // Send sync request to server
        let sync_request = SyncRequest {
            files: client_files.clone(),
            deleted_files: state.deleted_files.clone(),
            last_sync: state.last_sync,
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        let url = format!("{}/sync", self.server_url);
        let sync_response: SyncResponse = self
            .http_client
            .post(&url)
            .json(&sync_request)
            .send()
            .await?
            .json()
            .await?;

        // Handle conflicts first
        for conflict in &sync_response.conflicts {
            warn!("⚠️  Conflict detected for file: {}", conflict.path);
            warn!(
                "   Client: modified {}, hash {}",
                conflict.client_file.modified,
                &conflict.client_file.hash[..8]
            );
            warn!(
                "   Server: modified {}, hash {}",
                conflict.server_file.modified,
                &conflict.server_file.hash[..8]
            );

            // For now, use a simple strategy: newer file wins, client wins on tie
            if conflict.server_file.modified > conflict.client_file.modified {
                info!("   → Downloading server version (newer)");
                if let Err(e) = self.download_file(&conflict.path).await {
                    error!("   ✗ Failed to download {}: {}", conflict.path, e);
                    warn!("   → Keeping client version instead");
                }
            } else {
                info!("   → Uploading client version (newer or same time)");
                if let Some(file_info) = client_files.get(&conflict.path) {
                    if let Err(e) = self.upload_file(file_info).await {
                        error!("   ✗ Failed to upload {}: {}", conflict.path, e);
                    }
                }
            }
        }

        // Define concurrency limit
        const CONCURRENCY_LIMIT: usize = 10;

        if !sync_response.files_to_upload.is_empty() {
            info!(
                "Processing {} uploads...",
                sync_response.files_to_upload.len()
            );
            // Clone the vector to own the data for the stream
            let files_to_upload = sync_response.files_to_upload.clone();
            let upload_tasks = stream::iter(files_to_upload.into_iter())
                .map(|file_path| {
                    // Clone file_info if found to move into async block
                    let file_info = client_files.get(&file_path).cloned(); // file_info needs to be Clone
                    let client = self.clone(); // Clone client for shared state

                    async move {
                        if let Some(file_info) = file_info {
                            debug!("↑ Uploading: {}", file_path);
                            if let Err(e) = client.upload_file(&file_info).await {
                                error!("✗ Failed to upload {}: {}", file_path, e);
                            }
                        }
                    }
                })
                .buffer_unordered(CONCURRENCY_LIMIT);

            upload_tasks.collect::<Vec<()>>().await;
        }

        // Process downloads in parallel
        if !sync_response.files_to_download.is_empty() {
            info!(
                "Processing {} downloads...",
                sync_response.files_to_download.len()
            );
            // Clone the vector to own the data for the stream
            let files_to_download = sync_response.files_to_download.clone();
            let download_tasks = stream::iter(files_to_download.into_iter())
                .map(|file_info| {
                    let client = self.clone();
                    async move {
                        debug!("↓ Downloading: {}", file_info.path);
                        if let Err(e) = client.download_file(&file_info.path).await {
                            error!("✗ Failed to download {}: {}", file_info.path, e);
                            warn!("  → File may have been deleted from server or is inaccessible");
                        }
                    }
                })
                .buffer_unordered(CONCURRENCY_LIMIT);

            download_tasks.collect::<Vec<()>>().await;
        }

        // Process deletions in parallel
        if !sync_response.files_to_delete.is_empty() {
            info!(
                "Processing {} deletions...",
                sync_response.files_to_delete.len()
            );
            // Clone the vector to own the data for the stream
            let files_to_delete = sync_response.files_to_delete.clone();
            let delete_tasks = stream::iter(files_to_delete.into_iter())
                .map(|file_path| {
                    let client = self.clone();
                    async move {
                        debug!("🗑️  Deleting: {}", file_path);
                        if let Err(e) = client.delete_file(&file_path).await {
                            error!("✗ Failed to delete {}: {}", file_path, e);
                        }
                    }
                })
                .buffer_unordered(CONCURRENCY_LIMIT);

            delete_tasks.collect::<Vec<()>>().await;
        }

        // Update state with all current files (re-scan after downloads)
        let final_files = scan_directory_with_patterns(&self.watch_dir, &self.exclude_patterns)?;
        state.files.clear();
        for file_info in final_files {
            state.files.insert(file_info.path.clone(), file_info);
        }

        // Only clear old deleted files (older than 24 hours) to ensure proper sync across clients
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(24);
        state
            .deleted_files
            .retain(|_, deletion_time| *deletion_time > cutoff_time);

        state.last_sync = chrono::Utc::now();
        save_client_state_db(&state, &self.state_db)?;

        info!("✓ Bidirectional sync completed");
        Ok(())
    }

    async fn initial_sync_with_retries(&self) -> Result<()> {
        let max_retries = 5;
        let mut retry_delay = std::time::Duration::from_secs(1);

        for attempt in 1..=max_retries {
            match self.initial_sync().await {
                Ok(()) => {
                    return Ok(());
                }
                Err(e) => {
                    if attempt == max_retries {
                        return Err(anyhow::anyhow!(
                            "Failed to connect to server after {} attempts. Last error: {}",
                            max_retries,
                            e
                        ));
                    }

                    warn!(
                        "⚠️  Failed to connect to server (attempt {}/{}): {}",
                        attempt, max_retries, e
                    );
                    info!("🔄 Retrying in {} seconds...", retry_delay.as_secs());

                    tokio::time::sleep(retry_delay).await;

                    // Exponential backoff: double the delay, max 30 seconds
                    retry_delay =
                        std::cmp::min(retry_delay * 2, std::time::Duration::from_secs(30));
                }
            }
        }

        unreachable!()
    }

    pub async fn start_watching(&self) -> Result<()> {
        self.start_watching_with_shutdown(None).await
    }

    pub async fn start_watching_with_shutdown(
        &self,
        mut shutdown_rx: Option<broadcast::Receiver<()>>,
    ) -> Result<()> {
        info!(
            "Starting file system watcher for: {}",
            self.watch_dir.display()
        );
        info!(
            "Periodic sync interval: {} seconds",
            self.sync_interval.as_secs()
        );

        // Perform initial sync with retries
        self.initial_sync_with_retries().await?;

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
                            info!("Received shutdown signal, stopping file watcher...");
                            break;
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Shutdown channel closed, stopping file watcher...");
                            break;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            warn!("Shutdown signal lagged, stopping file watcher...");
                            break;
                        }
                    }
                }

                // Periodic sync trigger
                _ = sync_timer.tick() => {
                    debug!("🔄 Performing periodic sync...");
                    if let Err(e) = self.initial_sync().await {
                        error!("Error during periodic sync: {}", e);
                    }
                }

                // Check for file system events
                event = async_rx.recv() => {
                    match event {
                        Some(event) => {
                            if let Err(e) = self.handle_file_event(event).await {
                                error!("Error handling file event: {}", e);
                            }
                        }
                        None => {
                            error!("File watcher channel closed");
                            break;
                        }
                    }
                }
            }
        }

        // Save final state before stopping
        if let Err(e) = self.save_final_state().await {
            warn!("Warning: Failed to save final client state: {}", e);
        }

        info!("File watcher stopped successfully");
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
                            error!("Error syncing file {}: {}", path.display(), e);
                        }
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if self.should_sync_file(&path) {
                        if let Err(e) = self.handle_file_deletion(&path).await {
                            error!("Error handling file deletion {}: {}", path.display(), e);
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
            let mut state = load_client_state_db(&self.state_db)?;
            let should_upload = match state.files.get(&file_info.path) {
                Some(existing) => existing.hash != file_info.hash,
                None => true,
            };

            if should_upload {
                debug!("Detected change in: {}", relative_path_str);
                self.upload_file(&file_info).await?;

                state.files.insert(file_info.path.clone(), file_info);
                state.last_sync = chrono::Utc::now();
                save_client_state_db(&state, &self.state_db)?;
            }
        }

        Ok(())
    }

    async fn handle_file_deletion(&self, file_path: &std::path::Path) -> Result<()> {
        if let Ok(relative_path) = file_path.strip_prefix(&self.watch_dir) {
            let relative_path_str = relative_path.to_string_lossy().to_string();

            debug!("Detected deletion: {}", relative_path_str);

            // Load current state
            let mut state = load_client_state_db(&self.state_db)?;

            // Check if this file was in our tracked files
            if state.files.contains_key(&relative_path_str) {
                let deletion_time = chrono::Utc::now();

                // Remove from tracked files and add to deleted list with timestamp
                state.files.remove(&relative_path_str);
                state
                    .deleted_files
                    .insert(relative_path_str.clone(), deletion_time);

                // Send delete request to server immediately
                if let Err(e) = self.send_delete_request(&relative_path_str).await {
                    error!(
                        "Failed to send delete request for {}: {}",
                        relative_path_str, e
                    );
                } else {
                    debug!("✓ Deletion synced to server: {}", relative_path_str);
                }

                state.last_sync = chrono::Utc::now();
                save_client_state_db(&state, &self.state_db)?;
            }
        }

        Ok(())
    }

    async fn upload_file(&self, file_info: &FileInfo) -> Result<()> {
        let file_path = self.watch_dir.join(&file_info.path);

        // Verify hash before upload
        let actual_hash = calculate_file_hash(&file_path)?;
        if actual_hash != file_info.hash {
            return Err(anyhow::anyhow!(
                "Hash mismatch for file: {}",
                file_info.path
            ));
        }

        // Check for Delta Sync
        if file_info.size > DELTA_SYNC_THRESHOLD {
            match self.upload_file_delta(file_info, &file_path).await {
                Ok(true) => return Ok(()), // Delta sync succeeded
                Ok(false) => debug!("Delta sync recommended full upload for {}", file_info.path), // Fallback
                Err(e) => {
                    error!(
                        "Delta sync failed for {}, falling back to full upload: {}",
                        file_info.path, e
                    );
                    // Fallback
                }
            }
        }

        // Full Upload Fallback
        let content = std::fs::read(&file_path)?;

        let upload_request = UploadRequest {
            file_info: file_info.clone(),
            content,
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        let url = format!("{}/upload", self.server_url);
        let response: UploadResponse = self
            .http_client
            .post(&url)
            .json(&upload_request)
            .send()
            .await?
            .json()
            .await?;

        if response.success {
            debug!("✓ Uploaded (Full): {}", file_info.path);
        } else {
            return Err(anyhow::anyhow!("Upload failed: {}", response.message));
        }

        Ok(())
    }

    async fn upload_file_delta(
        &self,
        file_info: &FileInfo,
        file_path: &std::path::Path,
    ) -> Result<bool> {
        debug!("Attempting delta sync for: {}", file_info.path);

        let block_hashes = calculate_block_hashes(file_path, BLOCK_SIZE)?;

        let init_req = DeltaInitRequest {
            file_info: file_info.clone(),
            block_hashes,
            block_size: BLOCK_SIZE,
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        let url = format!("{}/delta/init", self.server_url);
        let init_res: DeltaInitResponse = self
            .http_client
            .post(&url)
            .json(&init_req)
            .send()
            .await?
            .json()
            .await?;

        if init_res.should_full_upload {
            return Ok(false);
        }

        if init_res.missing_block_indices.is_empty() {
            debug!("✓ No blocks need uploading for {}", file_info.path);
            return Ok(true);
        }

        debug!(
            "Uploading {} missing blocks for {}",
            init_res.missing_block_indices.len(),
            file_info.path
        );

        let mut file = std::fs::File::open(file_path)?;

        for index in init_res.missing_block_indices {
            let offset = index * BLOCK_SIZE;
            file.seek(SeekFrom::Start(offset))?;

            let mut buffer = vec![0u8; BLOCK_SIZE as usize];
            let bytes_read = file.read(&mut buffer)?;

            // Truncate buffer to actual bytes read
            buffer.truncate(bytes_read);

            let upload_req = BlockUploadRequest {
                path: file_info.path.clone(),
                directory: self.directory.clone().unwrap_or_default(), // Should ensure directory is set
                index,
                content: buffer,
                client_id: self.client_id.clone(),
            };

            let url = format!("{}/delta/upload", self.server_url);
            let res: BlockUploadResponse = self
                .http_client
                .post(&url)
                .json(&upload_req)
                .send()
                .await?
                .json()
                .await?;

            if !res.success {
                return Err(anyhow::anyhow!(
                    "Block {} upload failed: {}",
                    index,
                    res.message
                ));
            }
        }

        debug!("✓ Delta upload (blocks) complete: {}", file_info.path);

        // Finalize delta sync
        let complete_req = DeltaCompleteRequest {
            path: file_info.path.clone(),
            directory: self.directory.clone(),
            client_id: self.client_id.clone(),
            expected_hash: file_info.hash.clone(),
        };

        let url = format!("{}/delta/complete", self.server_url);
        let res: DeltaCompleteResponse = self
            .http_client
            .post(&url)
            .json(&complete_req)
            .send()
            .await?
            .json()
            .await?;

        if !res.success {
            return Err(anyhow::anyhow!("Delta completion failed: {}", res.message));
        }

        Ok(true)
    }

    async fn download_file(&self, file_path: &str) -> Result<()> {
        let directory = self
            .directory
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Directory must be specified for client operations"))?;
        let directory_param = format!("?directory={}", urlencoding::encode(directory));
        let url = format!(
            "{}/download/{}{}",
            self.server_url,
            urlencoding::encode(file_path),
            directory_param
        );
        let response: DownloadResponse = self.http_client.get(&url).send().await?.json().await?;

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
                    return Err(anyhow::anyhow!(
                        "Hash mismatch for downloaded file: {}",
                        file_info.path
                    ));
                }

                debug!("✓ Downloaded: {}", file_info.path);
            } else {
                return Err(anyhow::anyhow!(
                    "Download response missing file info or content"
                ));
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
                debug!("✓ Deleted: {}", file_path);
            } else if local_path.is_dir() {
                std::fs::remove_dir_all(&local_path)?;
                debug!("✓ Deleted directory: {}", file_path);
            }
        } else {
            // File already doesn't exist, which is fine
            debug!("ℹ️  File already deleted: {}", file_path);
        }

        Ok(())
    }

    async fn send_delete_request(&self, file_path: &str) -> Result<()> {
        let delete_request = DeleteRequest {
            path: file_path.to_string(),
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        let url = format!("{}/delete", self.server_url);
        let response: DeleteResponse = self
            .http_client
            .post(&url)
            .json(&delete_request)
            .send()
            .await?
            .json()
            .await?;

        if response.success {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Delete request failed: {}",
                response.message
            ))
        }
    }

    fn should_sync_file(&self, path: &std::path::Path) -> bool {
        // Skip hidden files and the state database file
        if let Some(file_name) = path.file_name() {
            let name = file_name.to_string_lossy();
            !name.starts_with('.') && name != ".syncpair_state.db"
        } else {
            false
        }
    }

    async fn save_final_state(&self) -> Result<()> {
        // Perform one final scan to ensure state is up to date
        let current_files = scan_directory_with_patterns(&self.watch_dir, &self.exclude_patterns)?;
        let mut state = load_client_state_db(&self.state_db)?;

        // Update state with current files
        for file_info in current_files {
            state.files.insert(file_info.path.clone(), file_info);
        }

        state.last_sync = chrono::Utc::now();
        save_client_state_db(&state, &self.state_db)?;
        debug!("Final client state saved");
        Ok(())
    }
}

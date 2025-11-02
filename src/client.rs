use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use chrono::Utc;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use reqwest::Client as HttpClient;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{interval, sleep};

use crate::types::{
    ClientConfig, ClientState, DirectorySettings, DownloadRequest, SyncRequest, UploadRequest,
};
use crate::utils::{
    cleanup_deleted_entries, ensure_directory_exists, expand_path, get_relative_path, load_client_state,
    resolve_path, save_client_state, scan_directory, should_ignore_file, write_file_atomic,
};

/// Simple client for synchronizing a single directory
pub struct SyncClient {
    server_url: String,
    watch_dir: PathBuf,
    sync_interval: Duration,
    client_id: Option<String>,
    directory: Option<String>,
    exclude_patterns: Vec<String>,
    http_client: HttpClient,
    state: Arc<RwLock<ClientState>>,
}

impl SyncClient {
    /// Create a new SyncClient
    pub fn new(
        server_url: String,
        watch_dir: PathBuf,
        sync_interval: Duration,
        client_id: Option<String>,
        directory: Option<String>,
        exclude_patterns: Vec<String>,
    ) -> Result<Self> {
        ensure_directory_exists(&watch_dir)?;

        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        // Merge user-provided patterns with built-in patterns
        let settings = DirectorySettings {
            ignore_patterns: Some(exclude_patterns),
            ..Default::default()
        };
        let merged_exclude_patterns = settings.get_ignore_patterns();

        // Load existing state or create new
        let state_db_path = watch_dir.join(".syncpair_state.db");
        let state = load_client_state(&state_db_path)?;

        Ok(Self {
            server_url,
            watch_dir,
            sync_interval,
            client_id,
            directory,
            exclude_patterns: merged_exclude_patterns,
            http_client,
            state: Arc::new(RwLock::new(state)),
        })
    }

    /// Start the client with file watching and periodic sync
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting SyncClient");
        tracing::info!("Server: {}", self.server_url);
        tracing::info!("Watch directory: {}", self.watch_dir.display());
        tracing::info!("Sync interval: {:?}", self.sync_interval);

        // Initial sync
        if let Err(e) = self.sync().await {
            tracing::error!("Initial sync failed: {}", e);
        }

        // Start file watcher
        let (tx, mut rx) = mpsc::channel(100);
        let watch_dir = self.watch_dir.clone();
        let _exclude_patterns = self.exclude_patterns.clone();

        // Setup file watcher in a separate task
        let watcher_tx = tx.clone();
        tokio::task::spawn_blocking(move || {
            let mut watcher: RecommendedWatcher = Watcher::new(
                move |result: Result<Event, notify::Error>| {
                    if let Ok(event) = result {
                        if let Err(e) = watcher_tx.blocking_send(event) {
                            tracing::error!("Failed to send file event: {}", e);
                        }
                    }
                },
                notify::Config::default(),
            )
            .expect("Failed to create file watcher");

            watcher
                .watch(&watch_dir, RecursiveMode::Recursive)
                .expect("Failed to start watching directory");

            // Keep the watcher alive
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        });

        // Setup periodic sync timer
        let sync_interval = self.sync_interval;
        let sync_client = self.clone();
        tokio::spawn(async move {
            let mut timer = interval(sync_interval);
            let mut consecutive_failures = 0;
            
            loop {
                timer.tick().await;
                match sync_client.sync().await {
                    Ok(()) => {
                        consecutive_failures = 0;
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        let error_string = e.to_string().to_lowercase();
                        
                        if error_string.contains("server unavailable") {
                            // Only log server unavailable message occasionally to avoid spam
                            if consecutive_failures == 1 || consecutive_failures % 10 == 0 {
                                tracing::info!("Server unavailable, will continue retrying (attempt {})", consecutive_failures);
                            }
                        } else {
                            tracing::error!("Periodic sync failed: {}", e);
                            // Only retry with backoff for non-connection errors
                            if consecutive_failures <= 3 {
                                sync_client.retry_sync().await;
                            }
                        }
                    }
                }
            }
        });

        // Handle file system events
        while let Some(event) = rx.recv().await {
            if let Err(e) = self.handle_file_event(event).await {
                tracing::error!("Failed to handle file event: {}", e);
            }
        }

        Ok(())
    }

    /// Handle a file system event
    async fn handle_file_event(&self, event: Event) -> Result<()> {
        tracing::debug!("File event: {:?}", event);

        // Debounce - wait a bit for file operations to complete
        sleep(Duration::from_millis(100)).await;

        for path in event.paths {
            if let Ok(relative_path) = get_relative_path(&path, &self.watch_dir) {
                // Skip if should be ignored
                if should_ignore_file(&relative_path, &self.exclude_patterns) {
                    continue;
                }

                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) => {
                        if path.is_file() {
                            tracing::debug!("File changed: {}", relative_path);
                            // Trigger sync after a short delay
                            let client = self.clone();
                            tokio::spawn(async move {
                                sleep(Duration::from_millis(500)).await;
                                if let Err(e) = client.sync().await {
                                    let error_string = e.to_string().to_lowercase();
                                    if !error_string.contains("server unavailable") {
                                        tracing::error!("Event-triggered sync failed: {}", e);
                                    }
                                }
                            });
                        }
                    }
                    EventKind::Remove(_) => {
                        tracing::debug!("File deleted: {}", relative_path);
                        // Mark file as deleted in state
                        let mut state = self.state.write().await;
                        state.deleted_files.insert(relative_path.clone(), Utc::now());
                        state.files.remove(&relative_path);

                        // Save state and trigger sync
                        let state_db_path = self.watch_dir.join(".syncpair_state.db");
                        if let Err(e) = save_client_state(&state_db_path, &*state) {
                            tracing::error!("Failed to save state after deletion: {}", e);
                        }
                        drop(state);

                        // Trigger sync
                        let client = self.clone();
                        tokio::spawn(async move {
                            if let Err(e) = client.sync().await {
                                let error_string = e.to_string().to_lowercase();
                                if !error_string.contains("server unavailable") {
                                    tracing::error!("Delete-triggered sync failed: {}", e);
                                }
                            }
                        });
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Perform bidirectional synchronization with the server
    pub async fn sync(&self) -> Result<()> {
        tracing::debug!("Starting sync");

        // Scan local directory for current files
        let current_files = scan_directory(&self.watch_dir, &self.exclude_patterns)?;

        // Get current state
        let mut state = self.state.write().await;

        // Update state with current files and detect deletions
        for (path, file_info) in &current_files {
            state.files.insert(path.clone(), file_info.clone());
        }

        // Detect locally deleted files
        let mut to_remove = Vec::new();
        for path in state.files.keys() {
            if !current_files.contains_key(path) && !state.deleted_files.contains_key(path) {
                to_remove.push(path.clone());
            }
        }

        for path in to_remove {
            state.files.remove(&path);
            state.deleted_files.insert(path, Utc::now());
        }

        // Create sync request
        let sync_request = SyncRequest {
            files: state.files.clone(),
            deleted_files: state.deleted_files.clone(),
            last_sync: state.last_sync,
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        drop(state); // Release lock before network call

        // Send sync request to server
        let sync_url = format!("{}/sync", self.server_url);
        let response = match self
            .http_client
            .post(&sync_url)
            .json(&sync_request)
            .send()
            .await {
                Ok(resp) => resp,
                Err(e) => {
                    let error_string = e.to_string().to_lowercase();
                    if error_string.contains("connection") || 
                       error_string.contains("network") ||
                       error_string.contains("refused") ||
                       error_string.contains("timeout") ||
                       error_string.contains("unreachable") ||
                       error_string.contains("sending request") ||
                       error_string.contains("connect") {
                        tracing::debug!("Server unavailable, sync will be retried later");
                        return Err(anyhow!("Server unavailable"));
                    } else {
                        return Err(anyhow!("Sync request failed: {}", e));
                    }
                }
            };

        if !response.status().is_success() {
            return Err(anyhow!("Sync request failed: {}", response.status()));
        }

        let sync_response: crate::types::SyncResponse = response.json().await?;

        tracing::info!(
            "Sync response: {} to upload, {} to download, {} to delete, {} conflicts",
            sync_response.files_to_upload.len(),
            sync_response.files_to_download.len(),
            sync_response.files_to_delete.len(),
            sync_response.conflicts.len()
        );

        // Log conflicts
        for conflict in &sync_response.conflicts {
            tracing::warn!(
                "⚠️  Conflict detected for file: {} (resolution: {:?})",
                conflict.path,
                conflict.resolution
            );
        }

        // Upload files to server
        let mut connection_error = false;
        let mut successful_uploads = 0;
        
        for file_path in &sync_response.files_to_upload {
            // If we already detected a connection error, don't attempt more uploads
            if connection_error {
                tracing::debug!("Skipping upload due to server unavailability: {}", file_path);
                continue;
            }
            
            match self.upload_file(file_path).await {
                Ok(()) => {
                    successful_uploads += 1;
                    tracing::debug!("✓ Uploaded: {}", file_path);
                }
                Err(e) => {
                    // Check if this is a connection error (server unavailable)
                    let error_string = e.to_string().to_lowercase();
                    if error_string.contains("connection") || 
                       error_string.contains("network") ||
                       error_string.contains("refused") ||
                       error_string.contains("timeout") ||
                       error_string.contains("unreachable") ||
                       error_string.contains("sending request") ||
                       error_string.contains("connect") {
                        connection_error = true;
                        let remaining_files = sync_response.files_to_upload.len() - successful_uploads;
                        tracing::warn!("Server became unavailable during upload. {} files queued for retry when server is available", remaining_files);
                        break; // Stop attempting further uploads
                    } else {
                        tracing::error!("Failed to upload {}: {}", file_path, e);
                    }
                }
            }
        }

        // Download files from server
        let mut download_connection_error = false;
        let mut successful_downloads = 0;
        
        for file_info in &sync_response.files_to_download {
            // If we already detected a connection error, don't attempt more downloads
            if download_connection_error {
                tracing::debug!("Skipping download due to server unavailability: {}", file_info.path);
                continue;
            }
            
            match self.download_file(&file_info.path).await {
                Ok(()) => {
                    successful_downloads += 1;
                    tracing::debug!("✓ Downloaded: {}", file_info.path);
                }
                Err(e) => {
                    let error_string = e.to_string().to_lowercase();
                    if error_string.contains("connection") || 
                       error_string.contains("network") ||
                       error_string.contains("refused") ||
                       error_string.contains("timeout") ||
                       error_string.contains("unreachable") ||
                       error_string.contains("sending request") ||
                       error_string.contains("connect") {
                        download_connection_error = true;
                        let remaining_files = sync_response.files_to_download.len() - successful_downloads;
                        tracing::warn!("Server became unavailable during download. {} files queued for retry when server is available", remaining_files);
                        break; // Stop attempting further downloads
                    } else {
                        tracing::error!("Failed to download {}: {}", file_info.path, e);
                    }
                }
            }
        }

        // Delete files locally
        for file_path in &sync_response.files_to_delete {
            if let Err(e) = self.delete_file_local(file_path).await {
                tracing::error!("Failed to delete {}: {}", file_path, e);
            } else {
                tracing::debug!("✓ Deleted: {}", file_path);
            }
        }

        // Update local state only if all operations completed successfully
        let mut state = self.state.write().await;
        
        // Only update last_sync if no connection errors occurred
        let all_operations_successful = !connection_error && !download_connection_error;
        if all_operations_successful {
            state.last_sync = Utc::now();
            tracing::debug!("Sync completed successfully");
        }

        // Clean up old deletion entries
        cleanup_deleted_entries(&mut state.deleted_files, 30);

        // Save updated state
        let state_db_path = self.watch_dir.join(".syncpair_state.db");
        save_client_state(&state_db_path, &*state)?;

        // Provide comprehensive sync status
        if connection_error || download_connection_error {
            let total_pending = (sync_response.files_to_upload.len() - successful_uploads) + 
                              (sync_response.files_to_download.len() - successful_downloads);
            if total_pending > 0 {
                tracing::info!("Sync partially completed: {} operations pending retry when server is available", total_pending);
                // Return error to indicate incomplete sync, but not "Server unavailable" to avoid retry loop
                return Err(anyhow!("Sync incomplete: {} operations pending", total_pending));
            } else {
                tracing::debug!("Sync completed successfully");
            }
        }
        
        Ok(())
    }

    /// Upload a file to the server
    async fn upload_file(&self, file_path: &str) -> Result<()> {
        let full_path = resolve_path(&self.watch_dir, file_path);

        if !full_path.exists() {
            return Err(anyhow!("File does not exist: {}", full_path.display()));
        }

        let content = std::fs::read(&full_path)?;
        let file_info = crate::utils::get_file_info(&full_path, file_path)?;

        let upload_request = UploadRequest {
            path: file_path.to_string(),
            hash: file_info.hash,
            size: file_info.size,
            modified: file_info.modified,
            content,
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        let upload_url = format!("{}/upload", self.server_url);
        let response = self
            .http_client
            .post(&upload_url)
            .json(&upload_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Upload failed: {}", response.status()));
        }

        Ok(())
    }

    /// Download a file from the server
    async fn download_file(&self, file_path: &str) -> Result<()> {
        let download_request = DownloadRequest {
            path: file_path.to_string(),
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
        };

        let download_url = format!("{}/download", self.server_url);
        let response = self
            .http_client
            .post(&download_url)
            .json(&download_request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("Download failed: {}", response.status()));
        }

        let download_response: crate::types::DownloadResponse = response.json().await?;

        if !download_response.success {
            return Err(anyhow!(
                "Download failed: {}",
                download_response.message.unwrap_or_default()
            ));
        }

        if let (Some(file_info), Some(content)) = (download_response.file_info, download_response.content) {
            let full_path = resolve_path(&self.watch_dir, file_path);
            write_file_atomic(&full_path, &content)?;

            // Update local state
            let mut state = self.state.write().await;
            state.files.insert(file_path.to_string(), file_info);
            state.deleted_files.remove(file_path);
        }

        Ok(())
    }

    /// Delete a file locally
    async fn delete_file_local(&self, file_path: &str) -> Result<()> {
        let full_path = resolve_path(&self.watch_dir, file_path);

        if full_path.exists() {
            std::fs::remove_file(&full_path)?;
        }

        // Update local state
        let mut state = self.state.write().await;
        state.files.remove(file_path);
        state.deleted_files.insert(file_path.to_string(), Utc::now());

        Ok(())
    }

    /// Force resynchronization by clearing local state
    pub async fn force_resync(&self) -> Result<()> {
        tracing::info!("Clearing local state for force resync");
        
        let state_db_path = self.watch_dir.join(".syncpair_state.db");
        
        // Create a fresh state with default values
        let fresh_state = ClientState::default();
        
        // Save the fresh state, overwriting any existing state
        save_client_state(&state_db_path, &fresh_state)?;
        
        // Update the in-memory state as well
        let mut state = self.state.write().await;
        *state = fresh_state;
        
        tracing::info!("Local state cleared for directory: {}", self.watch_dir.display());
        Ok(())
    }

    /// Retry sync with exponential backoff
    async fn retry_sync(&self) {
        let max_attempts = 5;
        let mut delay = Duration::from_secs(1);

        for attempt in 1..=max_attempts {
            tracing::info!("Retry attempt {} of {}", attempt, max_attempts);
            
            match self.sync().await {
                Ok(()) => {
                    tracing::info!("Sync retry successful");
                    return;
                }
                Err(e) => {
                    tracing::error!("Sync retry {} failed: {}", attempt, e);
                    if attempt < max_attempts {
                        sleep(delay).await;
                        delay = std::cmp::min(delay * 2, Duration::from_secs(30));
                    }
                }
            }
        }

        tracing::error!("All sync retry attempts failed");
    }
}

impl Clone for SyncClient {
    fn clone(&self) -> Self {
        Self {
            server_url: self.server_url.clone(),
            watch_dir: self.watch_dir.clone(),
            sync_interval: self.sync_interval,
            client_id: self.client_id.clone(),
            directory: self.directory.clone(),
            exclude_patterns: self.exclude_patterns.clone(),
            http_client: self.http_client.clone(),
            state: self.state.clone(),
        }
    }
}

/// Multi-directory client that manages multiple SyncClient instances
pub struct MultiDirectoryClient {
    clients: Vec<SyncClient>,
}

impl MultiDirectoryClient {
    /// Create a new MultiDirectoryClient from configuration
    pub fn new(mut config: ClientConfig) -> Result<Self> {
        // Apply default settings to each directory
        if let Some(defaults) = &config.default {
            for dir_config in &mut config.directories {
                dir_config.settings.merge_with_defaults(defaults);
            }
        }

        let mut clients = Vec::new();

        for dir_config in &config.directories {
            if !dir_config.settings.is_enabled() {
                tracing::info!("Skipping disabled directory: {}", dir_config.name);
                continue;
            }

            let local_path = expand_path(&dir_config.local_path);
            let sync_interval = Duration::from_secs(dir_config.settings.effective_sync_interval());
            let ignore_patterns = dir_config.settings.get_ignore_patterns();

            let client_id = if dir_config.settings.is_shared() {
                None // Shared directories don't use client_id in path
            } else {
                Some(config.client_id.clone())
            };

            let client = SyncClient::new(
                config.server.clone(),
                local_path,
                sync_interval,
                client_id,
                Some(dir_config.name.clone()),
                ignore_patterns,
            )?;

            clients.push(client);
        }

        Ok(Self { clients })
    }

    /// Start all clients concurrently
    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting MultiDirectoryClient with {} directories", self.clients.len());

        let mut handles = Vec::new();

        for (i, client) in self.clients.iter().enumerate() {
            let client = client.clone();
            let handle = tokio::spawn(async move {
                if let Err(e) = client.start().await {
                    tracing::error!("Client {} failed: {}", i, e);
                }
            });
            handles.push(handle);
        }

        // Wait for all clients to complete (they shouldn't under normal circumstances)
        for handle in handles {
            if let Err(e) = handle.await {
                tracing::error!("Client task panicked: {}", e);
            }
        }

        Ok(())
    }

    /// Force resynchronization by clearing local state for all clients
    pub async fn force_resync(&mut self) -> Result<()> {
        tracing::info!("Force resync: clearing local state for all {} directories", self.clients.len());
        
        for (i, client) in self.clients.iter().enumerate() {
            if let Err(e) = client.force_resync().await {
                tracing::error!("Failed to clear state for directory {}: {}", i, e);
                return Err(e);
            }
        }
        
        tracing::info!("Force resync: local state cleared for all directories");
        Ok(())
    }

    /// Load configuration from YAML file
    pub fn from_config_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: ClientConfig = serde_yaml::from_str(&content)?;
        Self::new(config)
    }
}
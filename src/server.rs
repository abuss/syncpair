use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use warp::Filter;
use tracing::{info, warn, error, debug};

use crate::types::{FileInfo, UploadRequest, UploadResponse, SyncRequest, SyncResponse, FileConflict, DownloadResponse, DeleteRequest, DeleteResponse};
use crate::utils::{calculate_file_hash, load_client_state_db, save_client_state_db, init_state_database};
use crate::types::ClientState;

#[derive(Clone)]
pub struct SimpleServer {
    base_storage_dir: PathBuf,
    // Directory-based shared storage: directory_name -> (files, deleted_files)
    directory_storage: Arc<Mutex<HashMap<String, (HashMap<String, FileInfo>, HashMap<String, chrono::DateTime<chrono::Utc>>)>>>,
}

impl SimpleServer {
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&storage_dir)?;
        
        let mut directory_storage = HashMap::new();
        
        // Load existing directory states from subdirectories
        if let Ok(entries) = std::fs::read_dir(&storage_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    if let Some(directory_name) = entry.file_name().to_str() {
                        let dir_path = entry.path();
                        let state_db = dir_path.join("server_state.db");
                        if state_db.exists() {
                            match load_client_state_db(&state_db) {
                                Ok(state) => {
                                    directory_storage.insert(directory_name.to_string(), (state.files, state.deleted_files));
                                    info!("Loaded state for directory: {}", directory_name);
                                }
                                Err(e) => {
                                    warn!("Failed to load state for directory {}: {}", directory_name, e);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // If no directory storage exists, create default directory for backward compatibility
        if directory_storage.is_empty() {
            let state_db = storage_dir.join("server_state.db");
            let (files, deleted_files) = if state_db.exists() {
                let state = load_client_state_db(&state_db)?;
                (state.files, state.deleted_files)
            } else {
                (HashMap::new(), HashMap::new())
            };
            directory_storage.insert("default".to_string(), (files, deleted_files));
        }
        
        Ok(Self {
            base_storage_dir: storage_dir,
            directory_storage: Arc::new(Mutex::new(directory_storage)),
        })
    }

    pub async fn start(&self, port: u16) -> Result<()> {
        let server = self.clone();
        let server_for_sync = self.clone();
        let server_for_download = self.clone();
        let server_for_delete = self.clone();
        
        let upload_route = warp::path("upload")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |upload_req: UploadRequest| {
                let server = server.clone();
                async move {
                    match server.handle_upload(upload_req).await {
                        Ok(response) => Ok::<_, warp::Rejection>(warp::reply::json(&response)),
                        Err(e) => {
                            let error_response = UploadResponse {
                                success: false,
                                message: format!("Upload failed: {}", e),
                            };
                            Ok(warp::reply::json(&error_response))
                        }
                    }
                }
            });

        let sync_route = warp::path("sync")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |sync_req: SyncRequest| {
                let server = server_for_sync.clone();
                async move {
                    match server.handle_sync(sync_req).await {
                        Ok(response) => Ok::<_, warp::Rejection>(warp::reply::json(&response)),
                        Err(e) => {
                            let error_response = SyncResponse {
                                files_to_upload: vec![],
                                files_to_download: vec![],
                                files_to_delete: vec![],
                                conflicts: vec![],
                            };
                            error!("Sync error: {}", e);
                            Ok(warp::reply::json(&error_response))
                        }
                    }
                }
            });

        let download_route = warp::path!("download" / String)
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and_then(move |file_path: String, query: HashMap<String, String>| {
                let server = server_for_download.clone();
                async move {
                    let directory_name = query.get("directory").cloned().unwrap_or("default".to_string());
                    match server.handle_download(file_path, directory_name).await {
                        Ok(response) => Ok::<_, warp::Rejection>(warp::reply::json(&response)),
                        Err(e) => {
                            let error_response = DownloadResponse {
                                success: false,
                                file_info: None,
                                content: None,
                                message: format!("Download failed: {}", e),
                            };
                            Ok(warp::reply::json(&error_response))
                        }
                    }
                }
            });

        let delete_route = warp::path("delete")
            .and(warp::post())
            .and(warp::body::json())
            .and_then(move |delete_req: DeleteRequest| {
                let server = server_for_delete.clone();
                async move {
                    match server.handle_delete(delete_req).await {
                        Ok(response) => Ok::<_, warp::Rejection>(warp::reply::json(&response)),
                        Err(e) => {
                            let error_response = DeleteResponse {
                                success: false,
                                message: format!("Delete failed: {}", e),
                            };
                            Ok(warp::reply::json(&error_response))
                        }
                    }
                }
            });

        let routes = upload_route
            .or(sync_route)
            .or(download_route)
            .or(delete_route)
            .with(warp::cors().allow_any_origin().allow_methods(vec!["GET", "POST", "DELETE"]).allow_headers(vec!["content-type"]));

        info!("Server starting on http://0.0.0.0:{}", port);
        
        // Create a graceful shutdown future
        let shutdown = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
            info!("\nShutdown signal received, stopping server...");
        };

        // Start server with graceful shutdown
        let (_, server_future) = warp::serve(routes)
            .bind_with_graceful_shutdown(([0, 0, 0, 0], port), shutdown);
        
        server_future.await;
        
        // Save final state before shutdown
        if let Err(e) = self.save_all_states() {
            warn!("Warning: Failed to save server state during shutdown: {}", e);
        }
        
        info!("Server stopped successfully!");
        Ok(())
    }



    fn get_directory_storage_dir(&self, directory_name: &str) -> PathBuf {
        self.base_storage_dir.join(directory_name)
    }

    fn ensure_directory_exists(&self, directory_name: &str) -> Result<()> {
        let dir_path = self.get_directory_storage_dir(directory_name);
        std::fs::create_dir_all(&dir_path)?;
        
        // Initialize directory storage if it doesn't exist
        let mut directory_storage = self.directory_storage.lock().unwrap();
        if !directory_storage.contains_key(directory_name) {
            directory_storage.insert(directory_name.to_string(), (HashMap::new(), HashMap::new()));
            info!("Created new shared directory: {}", directory_name);
        }
        
        Ok(())
    }

    async fn handle_upload(&self, upload_req: UploadRequest) -> Result<UploadResponse> {
        let directory_name = upload_req.directory.unwrap_or_else(|| "default".to_string());
        self.ensure_directory_exists(&directory_name)?;
        
        let directory_storage_dir = self.get_directory_storage_dir(&directory_name);
        let file_path = directory_storage_dir.join(&upload_req.file_info.path);
        
        // Create parent directories if they don't exist
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Write file content
        std::fs::write(&file_path, &upload_req.content)?;
        
        // Verify file hash
        let calculated_hash = calculate_file_hash(&file_path)?;
        if calculated_hash != upload_req.file_info.hash {
            return Ok(UploadResponse {
                success: false,
                message: format!("Hash mismatch: expected {}, got {}", upload_req.file_info.hash, calculated_hash),
            });
        }
        
        // Update directory state
        let state_modified = {
            let mut directory_storage = self.directory_storage.lock().unwrap();
            let (directory_files, _) = directory_storage.get_mut(&directory_name).unwrap();
            
            let old_file = directory_files.get(&upload_req.file_info.path);
            let is_new_or_changed = old_file.map_or(true, |old| old.hash != upload_req.file_info.hash);
            
            if is_new_or_changed {
                directory_files.insert(upload_req.file_info.path.clone(), upload_req.file_info.clone());
                true
            } else {
                false
            }
        };
        
        if state_modified {
            self.atomic_save_directory_state(&directory_name)?;
            info!("📁 Uploaded to directory '{}': {}", directory_name, upload_req.file_info.path);
        }
        
        Ok(UploadResponse {
            success: true,
            message: format!("File uploaded successfully to directory '{}'", directory_name),
        })
    }

    async fn handle_sync(&self, sync_req: SyncRequest) -> Result<SyncResponse> {
        let directory_name = sync_req.directory.unwrap_or_else(|| "default".to_string());
        
        // Create directory on filesystem first
        let directory_storage_dir = self.get_directory_storage_dir(&directory_name);
        std::fs::create_dir_all(&directory_storage_dir)?;
        
        // Use a single, scoped lock to ensure atomicity and avoid deadlock
        let (files_to_upload, files_to_download, files_to_delete, conflicts) = {
            let mut directory_storage = self.directory_storage.lock().unwrap();
            
            // Initialize directory storage if it doesn't exist (within the same lock)
            if !directory_storage.contains_key(&directory_name) {
                directory_storage.insert(directory_name.clone(), (HashMap::new(), HashMap::new()));
                info!("Created new shared directory: {}", directory_name);
            }
            
            let (directory_files, directory_deleted_files) = directory_storage.get_mut(&directory_name).unwrap();
            
            // Clean up old deletion records (older than 7 days) to prevent unlimited growth
            let cutoff_time = chrono::Utc::now() - chrono::Duration::days(7);
            directory_deleted_files.retain(|_, deletion_time| *deletion_time > cutoff_time);
            
            let client_files = sync_req.files;
            let client_deleted_files = sync_req.deleted_files;
            
            let mut files_to_upload = Vec::new();
            let mut files_to_download = Vec::new();
            let mut files_to_delete = Vec::new();
            let mut conflicts = Vec::new();
            let mut state_modified = false;

            // Handle files that the client has deleted with timestamp comparison
            for (deleted_path, deletion_time) in &client_deleted_files {
                if let Some(directory_file) = directory_files.get(deleted_path) {
                    // Compare deletion time with directory file modification time
                    if *deletion_time > directory_file.modified {
                        info!("📁 Client deleted file from directory '{}' (newer than server): {}", directory_name, deleted_path);
                        
                        // Delete the file from directory storage
                        let directory_file_path = directory_storage_dir.join(deleted_path);
                        if directory_file_path.exists() {
                            if let Err(e) = std::fs::remove_file(&directory_file_path) {
                                error!("Failed to delete {} from directory '{}' storage: {}", deleted_path, directory_name, e);
                                continue;
                            } else {
                                debug!("✓ Deleted from directory '{}' storage: {}", directory_name, deleted_path);
                            }
                        }
                        
                        // Remove from directory state and add to deleted files with timestamp
                        directory_files.remove(deleted_path);
                        directory_deleted_files.insert(deleted_path.clone(), *deletion_time);
                        state_modified = true;
                    } else {
                        warn!("⚠️  Client deletion ignored in directory '{}': server file {} is newer", directory_name, deleted_path);
                        // Directory file is newer, client should get the update
                        files_to_download.push(directory_file.clone());
                    }
                } else if let Some(directory_deletion_time) = directory_deleted_files.get(deleted_path) {
                    // File was already deleted in directory, update timestamp if client deletion is newer
                    if *deletion_time > *directory_deletion_time {
                        directory_deleted_files.insert(deleted_path.clone(), *deletion_time);
                        state_modified = true;
                    }
                } else {
                    // File doesn't exist in directory, just record the deletion
                    directory_deleted_files.insert(deleted_path.clone(), *deletion_time);
                    state_modified = true;
                }
            }

            // Handle files that the directory has deleted (client should delete them)
            let mut deletions_to_remove = Vec::new();
            for (deleted_path, deletion_time) in directory_deleted_files.iter() {
                if let Some(client_file) = client_files.get(deleted_path) {
                    // Compare deletion time with client file modification time
                    if *deletion_time > client_file.modified {
                        files_to_delete.push(deleted_path.clone());
                        debug!("📁 Client should delete (directory deleted it): {}", deleted_path);
                    } else {
                        warn!("⚠️  Directory '{}' deletion ignored: client file {} is newer", directory_name, deleted_path);
                        // Client file is newer, should be uploaded
                        files_to_upload.push(deleted_path.clone());
                        // Mark for removal from deleted files since client has newer version
                        deletions_to_remove.push(deleted_path.clone());
                        state_modified = true;
                    }
                }
            }
            
            // Remove obsolete deletions
            for path in deletions_to_remove {
                directory_deleted_files.remove(&path);
            }

            // Compare client files with directory files
            for (file_path, client_file) in &client_files {
                // Skip if file was deleted
                if directory_deleted_files.contains_key(file_path) {
                    continue;
                }
                
                if let Some(directory_file) = directory_files.get(file_path) {
                    // File exists in both client and directory - check for conflicts
                    if client_file.hash != directory_file.hash {
                        if client_file.modified > directory_file.modified {
                            // Client file is newer
                            files_to_upload.push(file_path.clone());
                            debug!("📁 Client file is newer in directory '{}': {}", directory_name, file_path);
                        } else if directory_file.modified > client_file.modified {
                            // Directory file is newer
                            files_to_download.push(directory_file.clone());
                            debug!("📁 Directory '{}' file is newer: {}", directory_name, file_path);
                        } else {
                            // Same timestamp but different content - conflict
                            conflicts.push(FileConflict {
                                path: file_path.clone(),
                                client_file: client_file.clone(),
                                server_file: directory_file.clone(),
                            });
                            files_to_download.push(directory_file.clone());
                            warn!("⚠️  Conflict in directory '{}' resolved (server wins): {}", directory_name, file_path);
                        }
                    }
                } else {
                    // File exists only on client
                    files_to_upload.push(file_path.clone());
                    debug!("📁 New file from client for directory '{}': {}", directory_name, file_path);
                }
            }

            // Check for files that exist only in directory (client should download them)
            for (file_path, directory_file) in directory_files.iter() {
                if !client_files.contains_key(file_path) && !client_deleted_files.contains_key(file_path) {
                    files_to_download.push(directory_file.clone());
                    debug!("📁 New file from directory '{}' for client: {}", directory_name, file_path);
                }
            }

            // Save state if modified (while still holding the lock)
            if state_modified {
                if let Err(e) = self.save_directory_state_with_lock(&directory_name, directory_files, directory_deleted_files) {
                    error!("Failed to save directory state for '{}': {}", directory_name, e);
                }
            }

            (files_to_upload, files_to_download, files_to_delete, conflicts)
        };

        info!("📁 Sync completed for directory '{}': {} to upload, {} to download, {} to delete, {} conflicts", 
              directory_name, files_to_upload.len(), files_to_download.len(), files_to_delete.len(), conflicts.len());

        Ok(SyncResponse {
            files_to_upload,
            files_to_download,
            files_to_delete,
            conflicts,
        })
    }

    async fn handle_download(&self, file_path: String, directory_name: String) -> Result<DownloadResponse> {
        // URL decode the file path since the client URL-encodes it
        let decoded_file_path = urlencoding::decode(&file_path)
            .map_err(|e| anyhow::anyhow!("Failed to decode file path '{}': {}", file_path, e))?
            .into_owned();
            
        let directory_storage_dir = self.get_directory_storage_dir(&directory_name);
        let full_file_path = directory_storage_dir.join(&decoded_file_path);
        
        if !full_file_path.exists() {
            return Ok(DownloadResponse {
                success: false,
                file_info: None,
                content: None,
                message: format!("File not found in directory '{}': {}", directory_name, decoded_file_path),
            });
        }
        
        let content = std::fs::read(&full_file_path)?;
        let file_hash = calculate_file_hash(&full_file_path)?;
        let metadata = std::fs::metadata(&full_file_path)?;
        let modified = metadata.modified()?.into();
        
        let file_info = FileInfo {
            path: decoded_file_path.clone(),
            hash: file_hash,
            size: metadata.len(),
            modified,
        };
        
        info!("📁 Downloaded from directory '{}': {}", directory_name, decoded_file_path);
        
        Ok(DownloadResponse {
            success: true,
            file_info: Some(file_info),
            content: Some(content),
            message: format!("File downloaded successfully from directory '{}'", directory_name),
        })
    }

    async fn handle_delete(&self, delete_req: DeleteRequest) -> Result<DeleteResponse> {
        let directory_name = delete_req.directory.unwrap_or_else(|| "default".to_string());
        self.ensure_directory_exists(&directory_name)?;
        
        let directory_storage_dir = self.get_directory_storage_dir(&directory_name);
        let file_path = directory_storage_dir.join(&delete_req.path);
        
        // Delete the physical file if it exists
        if file_path.exists() {
            std::fs::remove_file(&file_path)?;
        }
        
        // Update directory state
        let state_modified = {
            let mut directory_storage = self.directory_storage.lock().unwrap();
            let (directory_files, directory_deleted_files) = directory_storage.get_mut(&directory_name).unwrap();
            
            let was_present = directory_files.remove(&delete_req.path).is_some();
            directory_deleted_files.insert(delete_req.path.clone(), chrono::Utc::now());
            
            was_present
        };
        
        if state_modified {
            self.atomic_save_directory_state(&directory_name)?;
            info!("📁 Deleted from directory '{}': {}", directory_name, delete_req.path);
        }
        
        Ok(DeleteResponse {
            success: true,
            message: format!("File deleted successfully from directory '{}'", directory_name),
        })
    }

    fn atomic_save_directory_state(&self, directory_name: &str) -> Result<()> {
        let directory_storage_dir = self.get_directory_storage_dir(directory_name);
        let state_db = directory_storage_dir.join("server_state.db");
        
        // Initialize the database if it doesn't exist
        if !state_db.exists() {
            init_state_database(&state_db)?;
        }
        
        let directory_storage = self.directory_storage.lock().unwrap();
        if let Some((directory_files, directory_deleted_files)) = directory_storage.get(directory_name) {
            let state = ClientState {
                files: directory_files.clone(),
                deleted_files: directory_deleted_files.clone(),
                last_sync: chrono::Utc::now(),
            };
            
            save_client_state_db(&state, &state_db)?;
        }
        
        Ok(())
    }

    fn save_directory_state_with_lock(&self, directory_name: &str, directory_files: &HashMap<String, FileInfo>, directory_deleted_files: &HashMap<String, chrono::DateTime<chrono::Utc>>) -> Result<()> {
        let directory_storage_dir = self.get_directory_storage_dir(directory_name);
        let state_db = directory_storage_dir.join("server_state.db");
        
        // Initialize the database if it doesn't exist
        if !state_db.exists() {
            init_state_database(&state_db)?;
        }
        
        let state = ClientState {
            files: directory_files.clone(),
            deleted_files: directory_deleted_files.clone(),
            last_sync: chrono::Utc::now(),
        };
        
        save_client_state_db(&state, &state_db)?;
        Ok(())
    }

    // Helper methods for directory storage management
    fn save_all_states(&self) -> Result<()> {
        let directory_storage = self.directory_storage.lock().unwrap();
        for (directory_name, (directory_files, directory_deleted_files)) in directory_storage.iter() {
            if let Err(e) = self.save_directory_state_with_lock(directory_name, directory_files, directory_deleted_files) {
                warn!("Failed to save state for directory {}: {}", directory_name, e);
            }
        }
        Ok(())
    }
}
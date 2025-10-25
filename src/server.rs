use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use warp::Filter;
use tracing::{info, warn, error, debug};

use crate::types::{FileInfo, UploadRequest, UploadResponse, SyncRequest, SyncResponse, FileConflict, DownloadResponse, DeleteRequest, DeleteResponse};
use crate::utils::{calculate_file_hash, load_client_state};

#[derive(Clone)]
pub struct SimpleServer {
    storage_dir: PathBuf,
    files: Arc<Mutex<HashMap<String, FileInfo>>>,
    deleted_files: Arc<Mutex<HashMap<String, chrono::DateTime<chrono::Utc>>>>,
}

impl SimpleServer {
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&storage_dir)?;
        
        // Load existing files from state if it exists
        let state_file = storage_dir.join("server_state.json");
        let (files, deleted_files) = if state_file.exists() {
            let state = load_client_state(&state_file)?;
            (state.files, state.deleted_files)
        } else {
            (HashMap::new(), HashMap::new())
        };
        
        Ok(Self {
            storage_dir,
            files: Arc::new(Mutex::new(files)),
            deleted_files: Arc::new(Mutex::new(deleted_files)),
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
            .and_then(move |file_path: String| {
                let server = server_for_download.clone();
                async move {
                    match server.handle_download(file_path).await {
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
            .with(warp::cors()
                .allow_any_origin()
                .allow_headers(vec!["content-type"])
                .allow_methods(vec!["POST", "GET"]));

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
        if let Err(e) = self.save_state() {
            warn!("Warning: Failed to save server state during shutdown: {}", e);
        }
        
        info!("Server stopped successfully!");
        Ok(())
    }

    async fn handle_upload(&self, upload_req: UploadRequest) -> Result<UploadResponse> {
        let file_path = self.storage_dir.join(&upload_req.file_info.path);
        
        // Create parent directories if needed
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Write the file
        std::fs::write(&file_path, &upload_req.content)?;
        
        // Verify the hash
        let actual_hash = calculate_file_hash(&file_path)?;
        if actual_hash != upload_req.file_info.hash {
            std::fs::remove_file(&file_path)?;
            return Ok(UploadResponse {
                success: false,
                message: format!("Hash mismatch for file: {}", upload_req.file_info.path),
            });
        }

        // Atomically update server state and save
        {
            let mut files = self.files.lock().unwrap();
            files.insert(upload_req.file_info.path.clone(), upload_req.file_info.clone());
            
            // Create state snapshot for saving
            let deleted_files = self.deleted_files.lock().unwrap();
            let state = crate::types::ClientState {
                files: files.clone(),
                deleted_files: deleted_files.clone(),
                last_sync: chrono::Utc::now(),
            };
            
            // Release locks before I/O
            drop(files);
            drop(deleted_files);
            
            // Atomic state save
            self.atomic_save_state(&state)?;
        }
        
        debug!("File uploaded: {}", upload_req.file_info.path);
        Ok(UploadResponse {
            success: true,
            message: format!("File {} uploaded successfully", upload_req.file_info.path),
        })
    }

    async fn handle_sync(&self, sync_req: SyncRequest) -> Result<SyncResponse> {
        // Use a single, scoped lock to ensure atomicity
        let (files_to_upload, files_to_download, files_to_delete, conflicts, state_modified) = {
            let mut server_files = self.files.lock().unwrap();
            let mut server_deleted_files = self.deleted_files.lock().unwrap();
            
            let client_files = sync_req.files;
            let client_deleted_files = sync_req.deleted_files;
            
            let mut files_to_upload = Vec::new();
            let mut files_to_download = Vec::new();
            let mut files_to_delete = Vec::new();
            let mut conflicts = Vec::new();
            let mut state_modified = false;

            // Handle files that the client has deleted with timestamp comparison
            for (deleted_path, deletion_time) in &client_deleted_files {
                if let Some(server_file) = server_files.get(deleted_path) {
                    // Compare deletion time with server file modification time
                    if *deletion_time > server_file.modified {
                        info!("📤 Client deleted file (newer than server): {}", deleted_path);
                        
                        // Delete the file from server storage
                        let server_file_path = self.storage_dir.join(deleted_path);
                        if server_file_path.exists() {
                            if let Err(e) = std::fs::remove_file(&server_file_path) {
                                error!("Failed to delete {} from server storage: {}", deleted_path, e);
                                continue;
                            } else {
                                debug!("✓ Deleted from server storage: {}", deleted_path);
                            }
                        }
                        
                        // Remove from server state and add to deleted files with timestamp
                        server_files.remove(deleted_path);
                        server_deleted_files.insert(deleted_path.clone(), *deletion_time);
                        state_modified = true;
                    } else {
                        warn!("⚠️  Client deletion ignored: server file {} is newer", deleted_path);
                        // Server file is newer, client should get the update
                        files_to_download.push(server_file.clone());
                    }
                } else if let Some(server_deletion_time) = server_deleted_files.get(deleted_path) {
                    // File was already deleted on server, update timestamp if client deletion is newer
                    if *deletion_time > *server_deletion_time {
                        server_deleted_files.insert(deleted_path.clone(), *deletion_time);
                        state_modified = true;
                    }
                } else {
                    // File doesn't exist on server and wasn't deleted before, record the deletion
                    server_deleted_files.insert(deleted_path.clone(), *deletion_time);
                    state_modified = true;
                }
            }

            // Send files that were deleted by other clients to this client (if deletion is newer)
            let mut deletions_to_remove = Vec::new();
            for (deleted_path, server_deletion_time) in &*server_deleted_files {
                if let Some(client_file) = client_files.get(deleted_path) {
                    // Only tell client to delete if server deletion is newer than client file
                    if *server_deletion_time > client_file.modified {
                        debug!("📤 Sending deletion to client: {} (deleted at {})", deleted_path, server_deletion_time);
                        files_to_delete.push(deleted_path.clone());
                    } else {
                        info!("ℹ️  Client file {} is newer than deletion, ignoring server deletion", deleted_path);
                        // Client file is newer than deletion, so deletion should be ignored
                        // Mark for removal from server deleted files and restore file on server
                        deletions_to_remove.push(deleted_path.clone());
                        files_to_upload.push(deleted_path.clone());
                        state_modified = true;
                    }
                }
            }
            
            // Remove obsolete deletions
            for path in deletions_to_remove {
                server_deleted_files.remove(&path);
            }

            // Create snapshot for iteration to avoid borrow checker issues
            let server_files_snapshot: Vec<(String, FileInfo)> = server_files.iter()
                .map(|(k, v)| (k.clone(), v.clone())).collect();

            // Check files that exist on server but not on client
            for (path, server_file) in &server_files_snapshot {
                if !client_files.contains_key(path) {
                    // Check if client deleted this file
                    if let Some(client_deletion_time) = client_deleted_files.get(path) {
                        // Client deleted it, check timestamps
                        if *client_deletion_time > server_file.modified {
                            // Client deletion is newer, delete from server (handled above)
                            continue;
                        } else {
                            // Server file is newer, client should get it
                            files_to_download.push(server_file.clone());
                        }
                    } else {
                        // Verify the file actually exists on server disk before offering download
                        let server_file_path = self.storage_dir.join(path);
                        if server_file_path.exists() {
                            // Client never had this file or lost it, offer download
                            files_to_download.push(server_file.clone());
                        } else {
                            warn!("⚠️  Stale entry found in server state: {} (file missing from disk)", path);
                            // File was deleted on server side, add to deleted files with current time
                            let now = chrono::Utc::now();
                            server_deleted_files.insert(path.clone(), now);
                            files_to_delete.push(path.clone());
                            state_modified = true;
                        }
                    }
                }
            }

            // Check files that exist on client but not on server
            for (path, client_file) in &client_files {
                if !server_files.contains_key(path) {
                    // Check if this file was deleted on server
                    if let Some(server_deletion_time) = server_deleted_files.get(path) {
                        // File was deleted on server, check timestamps
                        if client_file.modified > *server_deletion_time {
                            // Client file is newer than deletion, client should upload
                            files_to_upload.push(path.clone());
                            // Remove from deleted files since we're restoring it
                            server_deleted_files.remove(path);
                            state_modified = true;
                        } else {
                            // Deletion is newer, client should delete
                            files_to_delete.push(path.clone());
                        }
                    } else {
                        // File doesn't exist on server and wasn't deleted, upload it
                        files_to_upload.push(path.clone());
                    }
                } else {
                    let server_file = &server_files[path];
                    
                    // Check if files are different
                    if client_file.hash != server_file.hash {
                        // Check which file is newer for conflict resolution
                        if client_file.modified > server_file.modified {
                            files_to_upload.push(path.clone());
                        } else if server_file.modified > client_file.modified {
                            files_to_download.push(server_file.clone());
                        } else {
                            // Same timestamp but different hash - this is a conflict
                            conflicts.push(FileConflict {
                                path: path.clone(),
                                client_file: client_file.clone(),
                                server_file: server_file.clone(),
                            });
                        }
                    }
                }
            }

            // Clean up stale entries from server state (files that no longer exist on disk)
            let paths_to_check: Vec<String> = server_files.keys().cloned().collect();
            for path in paths_to_check {
                let server_file_path = self.storage_dir.join(&path);
                if !server_file_path.exists() {
                    debug!("🗑️  Removing stale entry from server state: {}", path);
                    server_files.remove(&path);
                    let now = chrono::Utc::now();
                    server_deleted_files.insert(path, now);
                    state_modified = true;
                }
            }

            // Return all results while still holding the lock
            (files_to_upload, files_to_download, files_to_delete, conflicts, state_modified)
        }; // Lock is released here

        // Save state only if modifications were made
        if state_modified {
            // Re-acquire locks only for saving
            let state = {
                let server_files = self.files.lock().unwrap();
                let server_deleted_files = self.deleted_files.lock().unwrap();
                crate::types::ClientState {
                    files: server_files.clone(),
                    deleted_files: server_deleted_files.clone(),
                    last_sync: chrono::Utc::now(),
                }
            };
            
            self.atomic_save_state(&state)?;
        }

        Ok(SyncResponse {
            files_to_upload,
            files_to_download,
            files_to_delete,
            conflicts,
        })
    }

    async fn handle_download(&self, encoded_file_path: String) -> Result<DownloadResponse> {
        // URL decode the file path
        let file_path = urlencoding::decode(&encoded_file_path)
            .map_err(|e| anyhow::anyhow!("Failed to decode file path '{}': {}", encoded_file_path, e))?
            .to_string();
        
        let server_files = self.files.lock().unwrap();
        
        if let Some(file_info) = server_files.get(&file_path) {
            let full_path = self.storage_dir.join(&file_path);
            
            if full_path.exists() {
                let content = std::fs::read(&full_path)?;
                
                // Verify the file hasn't changed
                let current_hash = calculate_file_hash(&full_path)?;
                if current_hash != file_info.hash {
                    return Ok(DownloadResponse {
                        success: false,
                        file_info: None,
                        content: None,
                        message: format!("File {} has been modified since last sync", file_path),
                    });
                }
                
                Ok(DownloadResponse {
                    success: true,
                    file_info: Some(file_info.clone()),
                    content: Some(content),
                    message: format!("File {} downloaded successfully", file_path),
                })
            } else {
                Ok(DownloadResponse {
                    success: false,
                    file_info: None,
                    content: None,
                    message: format!("File {} not found on disk", file_path),
                })
            }
        } else {
            Ok(DownloadResponse {
                success: false,
                file_info: None,
                content: None,
                message: format!("File {} not found in server state", file_path),
            })
        }
    }

    async fn handle_delete(&self, delete_req: DeleteRequest) -> Result<DeleteResponse> {
        let file_path = &delete_req.path;
        let full_path = self.storage_dir.join(file_path);
        let deletion_time = chrono::Utc::now();
        
        // Delete the file from disk if it exists
        if full_path.exists() {
            std::fs::remove_file(&full_path)?;
            debug!("🗑️  Deleted file from server storage: {}", file_path);
        }
        
        // Atomically remove from server state and save
        {
            let mut files = self.files.lock().unwrap();
            let mut deleted_files = self.deleted_files.lock().unwrap();
            
            files.remove(file_path);
            
            // Add to deleted files with current timestamp
            deleted_files.insert(file_path.clone(), deletion_time);
            
            // Create state snapshot for saving
            let state = crate::types::ClientState {
                files: files.clone(),
                deleted_files: deleted_files.clone(),
                last_sync: chrono::Utc::now(),
            };
            
            // Release locks before I/O
            drop(files);
            drop(deleted_files);
            
            // Atomic state save
            self.atomic_save_state(&state)?;
        }
        
        Ok(DeleteResponse {
            success: true,
            message: format!("File {} deleted successfully", file_path),
        })
    }

    fn save_state(&self) -> Result<()> {
        let state = {
            let files = self.files.lock().unwrap();
            let deleted_files = self.deleted_files.lock().unwrap();
            crate::types::ClientState {
                files: files.clone(),
                deleted_files: deleted_files.clone(),
                last_sync: chrono::Utc::now(),
            }
        };
        
        self.atomic_save_state(&state)
    }

    fn atomic_save_state(&self, state: &crate::types::ClientState) -> Result<()> {
        // Perform atomic file write using temp file + rename
        let state_file = self.storage_dir.join("server_state.json");
        let temp_file = state_file.with_extension("json.tmp");
        
        // Write to temporary file first
        let content = serde_json::to_string_pretty(state)?;
        std::fs::write(&temp_file, content)?;
        
        // Atomic rename (on most filesystems this is atomic)
        std::fs::rename(&temp_file, &state_file)?;
        
        Ok(())
    }
}
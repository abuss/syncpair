use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use tokio::sync::RwLock;
use warp::{Filter, Rejection, Reply};

use crate::types::{
    ConflictResolution, DeleteRequest, DeleteResponse, DownloadRequest, DownloadResponse,
    FileConflict, FileInfo, SyncRequest, SyncResponse, UploadRequest, UploadResponse,
};
use crate::utils::{
    ensure_directory_exists, get_file_info, load_client_state, resolve_path,
    save_client_state, write_file_atomic,
};

/// SyncPair server for handling file synchronization
pub struct SyncServer {
    storage_path: PathBuf,
    directory_storage: Arc<RwLock<HashMap<String, Arc<RwLock<()>>>>>, // Per-directory locks
}

impl SyncServer {
    /// Create a new SyncServer with the specified storage directory
    pub fn new(storage_path: PathBuf) -> Result<Self> {
        ensure_directory_exists(&storage_path)?;
        
        Ok(Self {
            storage_path,
            directory_storage: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Start the HTTP server on the specified port
    pub async fn start(&self, port: u16) -> Result<()> {
        tracing::info!("Starting SyncPair server on port {}", port);
        tracing::info!("Storage directory: {}", self.storage_path.display());

        let server = Arc::new(self.clone());

        // CORS headers for all responses
        let cors = warp::cors()
            .allow_any_origin()
            .allow_headers(vec!["content-type"])
            .allow_methods(vec!["GET", "POST", "DELETE", "PUT"]);

        // Health check endpoint
        let health = warp::path("health")
            .and(warp::get())
            .map(|| warp::reply::with_status("OK", warp::http::StatusCode::OK));

        // Sync endpoint - negotiate what files need to be synchronized
        let sync = warp::path("sync")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_server(server.clone()))
            .and_then(handle_sync);

        // Upload endpoint - receive files from clients
        let upload = warp::path("upload")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_server(server.clone()))
            .and_then(handle_upload);

        // Download endpoint - send files to clients
        let download = warp::path("download")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_server(server.clone()))
            .and_then(handle_download);

        // Delete endpoint - remove files from server
        let delete = warp::path("delete")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_server(server.clone()))
            .and_then(handle_delete);

        let routes = health
            .or(sync)
            .or(upload)
            .or(download)
            .or(delete)
            .with(cors)
            .recover(handle_rejection);

        tracing::info!("Server ready to accept connections");
        warp::serve(routes).run(([0, 0, 0, 0], port)).await;

        Ok(())
    }

    /// Get the storage directory path for a specific client/directory combination
    fn get_directory_path(&self, client_id: Option<&str>, directory: Option<&str>) -> PathBuf {
        match (client_id, directory) {
            (Some(client), Some(dir)) => {
                // Check if this should be a shared directory
                // For now, assume shared if client_id is empty or matches special pattern
                if client.is_empty() || directory.unwrap_or("").starts_with("shared_") {
                    self.storage_path.join(dir)
                } else {
                    self.storage_path.join(format!("{}:{}", client, dir))
                }
            }
            (Some(client), None) => self.storage_path.join(format!("{}:default", client)),
            (None, Some(dir)) => self.storage_path.join(dir),
            (None, None) => self.storage_path.join("default"),
        }
    }

    /// Get or create a lock for a specific directory
    async fn get_directory_lock(&self, directory_key: &str) -> Arc<RwLock<()>> {
        let mut storage = self.directory_storage.write().await;
        storage
            .entry(directory_key.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }

    /// Handle sync request and determine what files need to be synchronized
    async fn handle_sync_request(&self, request: SyncRequest) -> Result<SyncResponse> {
        let directory_path = self.get_directory_path(request.client_id.as_deref(), request.directory.as_deref());
        let directory_key = directory_path.to_string_lossy().to_string();
        let lock = self.get_directory_lock(&directory_key).await;
        let _lock = lock.write().await;

        ensure_directory_exists(&directory_path)?;

        // Load server state for this directory
        let state_db_path = directory_path.join("server_state.db");
        let mut server_state = load_client_state(&state_db_path)?;

        let mut files_to_upload = Vec::new();
        let mut files_to_download = Vec::new();
        let mut files_to_delete = Vec::new();
        let mut conflicts = Vec::new();

        // Process client files
        for (path, client_file) in &request.files {
            match server_state.files.get(path) {
                Some(server_file) => {
                    // File exists on both sides
                    if client_file.hash != server_file.hash {
                        // Conflict - determine winner based on timestamp
                        if client_file.modified > server_file.modified {
                            conflicts.push(FileConflict {
                                path: path.clone(),
                                client_modified: client_file.modified,
                                server_modified: server_file.modified,
                                resolution: ConflictResolution::ClientWins,
                            });
                            files_to_upload.push(path.clone());
                        } else {
                            conflicts.push(FileConflict {
                                path: path.clone(),
                                client_modified: client_file.modified,
                                server_modified: server_file.modified,
                                resolution: ConflictResolution::ServerWins,
                            });
                            files_to_download.push(server_file.clone());
                        }
                    }
                    // If hashes match, no action needed
                }
                None => {
                    // File only exists on client
                    // Check if it was deleted on server after client's last sync
                    if let Some(deleted_time) = server_state.deleted_files.get(path) {
                        if *deleted_time > request.last_sync {
                            // File was deleted on server after client's last sync
                            files_to_delete.push(path.clone());
                        } else {
                            // Client created file after deletion, upload it
                            files_to_upload.push(path.clone());
                        }
                    } else {
                        // New file on client, upload it
                        files_to_upload.push(path.clone());
                    }
                }
            }
        }

        // Check for files that exist on server but not on client
        let mut files_to_remove = Vec::new();
        for (path, server_file) in &server_state.files {
            if !request.files.contains_key(path) {
                // File exists on server but not on client
                // Check if client deleted it
                if let Some(deleted_time) = request.deleted_files.get(path) {
                    if *deleted_time > server_file.modified {
                        // Client deleted file after it was modified on server
                        // Mark for deletion from server
                        files_to_remove.push((path.clone(), *deleted_time));
                        
                        // Remove physical file
                        let file_path = resolve_path(&directory_path, path);
                        if file_path.exists() {
                            if let Err(e) = std::fs::remove_file(&file_path) {
                                tracing::warn!("Failed to delete file {}: {}", file_path.display(), e);
                            }
                        }
                    } else {
                        // Server file is newer, download it
                        files_to_download.push(server_file.clone());
                    }
                } else {
                    // Client doesn't have file, download it
                    files_to_download.push(server_file.clone());
                }
            }
        }

        // Remove marked files from server state
        for (path, deleted_time) in files_to_remove {
            server_state.files.remove(&path);
            server_state.deleted_files.insert(path, deleted_time);
        }

        // Save updated server state
        save_client_state(&state_db_path, &server_state)?;

        tracing::info!(
            "Sync request processed: {} to upload, {} to download, {} to delete, {} conflicts",
            files_to_upload.len(),
            files_to_download.len(),
            files_to_delete.len(),
            conflicts.len()
        );

        Ok(SyncResponse {
            files_to_upload,
            files_to_download,
            files_to_delete,
            conflicts,
        })
    }

    /// Handle file upload from client
    async fn handle_upload_request(&self, request: UploadRequest) -> Result<UploadResponse> {
        let directory_path = self.get_directory_path(request.client_id.as_deref(), request.directory.as_deref());
        let directory_key = directory_path.to_string_lossy().to_string();
        let lock = self.get_directory_lock(&directory_key).await;
        let _lock = lock.write().await;

        ensure_directory_exists(&directory_path)?;

        // Write file to disk
        let file_path = resolve_path(&directory_path, &request.path);
        write_file_atomic(&file_path, &request.content)?;

        // Update server state
        let state_db_path = directory_path.join("server_state.db");
        let mut server_state = load_client_state(&state_db_path)?;

        let file_info = FileInfo {
            path: request.path.clone(),
            hash: request.hash,
            size: request.size,
            modified: request.modified,
        };

        server_state.files.insert(request.path.clone(), file_info);
        server_state.last_sync = Utc::now();

        save_client_state(&state_db_path, &server_state)?;

        tracing::info!("File uploaded: {}", request.path);

        Ok(UploadResponse {
            success: true,
            message: Some("File uploaded successfully".to_string()),
        })
    }

    /// Handle file download request from client
    async fn handle_download_request(&self, request: DownloadRequest) -> Result<DownloadResponse> {
        let directory_path = self.get_directory_path(request.client_id.as_deref(), request.directory.as_deref());
        let directory_key = directory_path.to_string_lossy().to_string();
        let lock = self.get_directory_lock(&directory_key).await;
        let _lock = lock.read().await;

        let file_path = resolve_path(&directory_path, &request.path);

        if !file_path.exists() {
            return Ok(DownloadResponse {
                success: false,
                file_info: None,
                content: None,
                message: Some("File not found".to_string()),
            });
        }

        // Read file content
        let content = std::fs::read(&file_path)?;

        // Get file info
        let file_info = get_file_info(&file_path, &request.path)?;

        tracing::info!("File downloaded: {}", request.path);

        Ok(DownloadResponse {
            success: true,
            file_info: Some(file_info),
            content: Some(content),
            message: None,
        })
    }

    /// Handle file deletion request from client
    async fn handle_delete_request(&self, request: DeleteRequest) -> Result<DeleteResponse> {
        let directory_path = self.get_directory_path(request.client_id.as_deref(), request.directory.as_deref());
        let directory_key = directory_path.to_string_lossy().to_string();
        let lock = self.get_directory_lock(&directory_key).await;
        let _lock = lock.write().await;

        let file_path = resolve_path(&directory_path, &request.path);

        // Remove file from disk if it exists
        if file_path.exists() {
            std::fs::remove_file(&file_path)?;
        }

        // Update server state
        let state_db_path = directory_path.join("server_state.db");
        let mut server_state = load_client_state(&state_db_path)?;

        server_state.files.remove(&request.path);
        server_state.deleted_files.insert(request.path.clone(), Utc::now());
        server_state.last_sync = Utc::now();

        save_client_state(&state_db_path, &server_state)?;

        tracing::info!("File deleted: {}", request.path);

        Ok(DeleteResponse {
            success: true,
            message: Some("File deleted successfully".to_string()),
        })
    }
}

impl Clone for SyncServer {
    fn clone(&self) -> Self {
        Self {
            storage_path: self.storage_path.clone(),
            directory_storage: self.directory_storage.clone(),
        }
    }
}

// Warp filter helpers

fn with_server(
    server: Arc<SyncServer>,
) -> impl Filter<Extract = (Arc<SyncServer>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || server.clone())
}

// Handler functions

async fn handle_sync(
    request: SyncRequest,
    server: Arc<SyncServer>,
) -> Result<impl Reply, Rejection> {
    match server.handle_sync_request(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            tracing::error!("Sync error: {}", e);
            Err(warp::reject::custom(ServerError(e.to_string())))
        }
    }
}

async fn handle_upload(
    request: UploadRequest,
    server: Arc<SyncServer>,
) -> Result<impl Reply, Rejection> {
    match server.handle_upload_request(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            tracing::error!("Upload error: {}", e);
            Err(warp::reject::custom(ServerError(e.to_string())))
        }
    }
}

async fn handle_download(
    request: DownloadRequest,
    server: Arc<SyncServer>,
) -> Result<impl Reply, Rejection> {
    match server.handle_download_request(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            tracing::error!("Download error: {}", e);
            Err(warp::reject::custom(ServerError(e.to_string())))
        }
    }
}

async fn handle_delete(
    request: DeleteRequest,
    server: Arc<SyncServer>,
) -> Result<impl Reply, Rejection> {
    match server.handle_delete_request(request).await {
        Ok(response) => Ok(warp::reply::json(&response)),
        Err(e) => {
            tracing::error!("Delete error: {}", e);
            Err(warp::reject::custom(ServerError(e.to_string())))
        }
    }
}

// Error handling

#[derive(Debug)]
struct ServerError(String);

impl warp::reject::Reject for ServerError {}

async fn handle_rejection(err: Rejection) -> Result<impl Reply, std::convert::Infallible> {
    if let Some(ServerError(message)) = err.find::<ServerError>() {
        Ok(warp::reply::with_status(
            format!("Server error: {}", message),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ))
    } else if err.is_not_found() {
        Ok(warp::reply::with_status(
            "Not found".to_string(),
            warp::http::StatusCode::NOT_FOUND,
        ))
    } else {
        tracing::error!("Unhandled rejection: {:?}", err);
        Ok(warp::reply::with_status(
            "Internal server error".to_string(),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR,
        ))
    }
}
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::client::SimpleClient;
use crate::types::{ClientConfig, DirectoryConfig};

pub struct MultiDirectoryClient {
    pub config: ClientConfig,
    clients: HashMap<String, SimpleClient>,
}

impl MultiDirectoryClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let mut clients = HashMap::new();

        for dir_config in &config.directories {
            // Apply default settings to directory settings
            let settings = if let Some(ref defaults) = config.default {
                dir_config.settings.clone().merge_with_defaults(defaults)
            } else {
                dir_config.settings.clone()
            };
            let effective = settings.effective_values();
            if !effective.enabled {
                info!("Skipping disabled directory: {}", dir_config.name);
                continue;
            }

            // Expand the home directory if present
            let local_path = Self::expand_path(&dir_config.local_path)?;

            // Create the directory if it doesn't exist
            std::fs::create_dir_all(&local_path)?;

            // Use appropriate directory naming based on whether directory is shared
            let directory_name = if effective.shared {
                // Shared directories don't get client_id prefix
                dir_config.name.clone()
            } else {
                // Client-specific directories get client_id prefix for isolation
                format!("{}:{}", config.client_id, dir_config.name)
            };

            let client = SimpleClient::new(config.server.clone(), local_path)
                .with_sync_interval(Duration::from_secs(effective.sync_interval_seconds))
                .with_client_id(format!("{}:{}", config.client_id, dir_config.name))
                .with_directory(directory_name)
                .with_exclude_patterns(effective.ignore_patterns.clone());

            clients.insert(dir_config.name.clone(), client);

            info!(
                "Configured directory '{}': {}",
                dir_config.name,
                dir_config.local_path.display()
            );
        }

        if clients.is_empty() {
            return Err(anyhow::anyhow!(
                "No enabled directories found in configuration"
            ));
        }

        info!(
            "MultiDirectoryClient initialized with {} directories",
            clients.len()
        );

        Ok(Self { config, clients })
    }

    pub fn from_config_file(config_path: &PathBuf) -> Result<Self> {
        let config_content = std::fs::read_to_string(config_path)?;
        let config: ClientConfig = serde_yaml::from_str(&config_content)?;
        Self::new(config)
    }

    pub async fn start_watching(&self) -> Result<()> {
        self.start_watching_with_shutdown(None).await
    }

    pub async fn start_watching_with_shutdown(
        &self,
        shutdown_rx: Option<broadcast::Receiver<()>>,
    ) -> Result<()> {
        info!(
            "Starting multi-directory synchronization for client: {}",
            self.config.client_id
        );

        let mut tasks: Vec<JoinHandle<Result<()>>> = Vec::new();
        let mut shutdown_channels = Vec::new();

        // Create shutdown channels for each client
        for _ in 0..self.clients.len() {
            let (tx, _rx) = broadcast::channel(1);
            if let Some(ref main_rx) = shutdown_rx {
                let mut main_rx_clone = main_rx.resubscribe();
                let tx_clone = tx.clone();

                // Forward shutdown signals to individual clients
                tokio::spawn(async move {
                    if main_rx_clone.recv().await.is_ok() {
                        let _ = tx_clone.send(());
                    }
                });
            }
            shutdown_channels.push(tx);
        }

        // Start each client in its own task
        for ((dir_name, client), shutdown_tx) in self.clients.iter().zip(shutdown_channels.iter()) {
            let client_clone = client.clone();
            let dir_name_clone = dir_name.clone();
            let shutdown_rx = shutdown_tx.subscribe();

            let task = tokio::spawn(async move {
                info!("Starting file watcher for directory: {}", dir_name_clone);

                match client_clone
                    .start_watching_with_shutdown(Some(shutdown_rx))
                    .await
                {
                    Ok(_) => {
                        info!(
                            "File watcher stopped successfully for directory: {}",
                            dir_name_clone
                        );
                        Ok(())
                    }
                    Err(e) => {
                        error!(
                            "Error in file watcher for directory {}: {}",
                            dir_name_clone, e
                        );
                        Err(e)
                    }
                }
            });

            tasks.push(task);
        }

        // Wait for shutdown signal if provided
        if let Some(mut shutdown_rx) = shutdown_rx {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    info!("Received shutdown signal, stopping all directory watchers...");

                    // Send shutdown to all clients
                    for shutdown_tx in shutdown_channels {
                        let _ = shutdown_tx.send(());
                    }
                }
                // If no shutdown signal provided, this will never complete
                _ = std::future::pending::<()>() => {}
            }
        }

        // Wait for all tasks to complete
        let mut had_errors = false;
        for (task, dir_name) in tasks.into_iter().zip(self.clients.keys()) {
            match task.await {
                Ok(Ok(_)) => {
                    debug!("Directory watcher completed successfully: {}", dir_name);
                }
                Ok(Err(e)) => {
                    error!("Directory watcher error for {}: {}", dir_name, e);
                    had_errors = true;
                }
                Err(e) => {
                    error!("Task join error for directory {}: {}", dir_name, e);
                    had_errors = true;
                }
            }
        }

        if had_errors {
            warn!("Some directory watchers encountered errors during shutdown");
        }

        info!("All directory watchers stopped");
        Ok(())
    }

    pub fn get_client_id(&self) -> &str {
        &self.config.client_id
    }

    pub fn get_server_url(&self) -> &str {
        &self.config.server
    }

    pub fn get_directory_configs(&self) -> &Vec<DirectoryConfig> {
        &self.config.directories
    }

    pub fn get_enabled_directories(&self) -> Vec<&DirectoryConfig> {
        self.config
            .directories
            .iter()
            .filter(|dir| {
                // Apply defaults to check if directory is enabled
                let settings = if let Some(ref defaults) = self.config.default {
                    dir.settings.clone().merge_with_defaults(defaults)
                } else {
                    dir.settings.clone()
                };
                settings.effective_values().enabled
            })
            .collect()
    }

    fn expand_path(path: &Path) -> Result<PathBuf> {
        let path_str = path.to_string_lossy();

        if path_str.starts_with("~/") {
            if let Some(home_dir) = dirs::home_dir() {
                let expanded = home_dir.join(path_str.strip_prefix("~/").unwrap());
                Ok(expanded)
            } else {
                Err(anyhow::anyhow!("Could not determine home directory"))
            }
        } else {
            Ok(path.to_path_buf())
        }
    }
}

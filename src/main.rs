use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;

use syncit::client::SimpleClient;
use syncit::server::SimpleServer;

#[derive(Parser)]
#[command(author, version, about = "A simple file synchronization tool", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server to receive file uploads
    Server {
        #[arg(short, long, default_value = "8080", help = "Port to run the server on")]
        port: u16,
        #[arg(short, long, default_value = "./server_storage", help = "Directory to store uploaded files")]
        storage_dir: PathBuf,
    },
    /// Start the client to watch and sync files
    Client {
        #[arg(short, long, default_value = "http://localhost:8080", help = "Server URL to sync to")]
        server: String,
        #[arg(short, long, default_value = "./sync_dir", help = "Directory to watch for file changes")]
        dir: PathBuf,
        #[arg(short = 'i', long, default_value = "30", help = "Sync interval in seconds")]
        sync_interval: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.command {
        Commands::Server { port, storage_dir } => {
            println!("Starting syncit server on port {} with storage directory: {}", 
                     port, storage_dir.display());
            
            let server = SimpleServer::new(storage_dir)?;
            server.start(port).await?;
        }
        Commands::Client { server, dir, sync_interval } => {
            println!("Starting syncit client, syncing {} to {} (sync every {}s)", 
                     dir.display(), server, sync_interval);
            
            let client = SimpleClient::new(server, dir)
                .with_sync_interval(Duration::from_secs(sync_interval));
            
            println!("Starting watch mode. Press Ctrl+C to stop.");
            
            // Create shutdown channel
            let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
            
            // Start watching in a separate task
            let watch_task = tokio::spawn(async move {
                if let Err(e) = client.start_watching_with_shutdown(Some(shutdown_rx)).await {
                    eprintln!("Watch error: {}", e);
                }
            });
            
            // Wait for Ctrl+C
            signal::ctrl_c().await?;
            println!("\nShutdown signal received, stopping...");
            
            // Send shutdown signal to client
            let _ = shutdown_tx.send(());
            
            // Wait for the watch task to complete gracefully
            if let Err(e) = watch_task.await {
                eprintln!("Error during client shutdown: {}", e);
            }
        }
    }

    println!("Syncit stopped successfully!");
    Ok(())
}
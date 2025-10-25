use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing::{Level, info, error};
use std::fs::OpenOptions;

use syncit::client::SimpleClient;
use syncit::server::SimpleServer;

#[derive(Parser)]
#[command(author, version, about = "A simple file synchronization tool", long_about = None)]
struct Args {
    /// Log level: error, warn, info, debug, trace
    #[arg(short = 'v', long, default_value = "info", help = "Set log level")]
    log_level: String,
    
    /// Write logs to file instead of stdout
    #[arg(short = 'l', long, help = "Write logs to file")]
    log_file: Option<PathBuf>,
    
    /// Quiet mode - suppress all output except errors
    #[arg(short = 'q', long, help = "Quiet mode - only show errors")]
    quiet: bool,
    
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

fn init_logging(log_level: &str, log_file: Option<&PathBuf>, quiet: bool) -> Result<(), Box<dyn std::error::Error>> {
    let level = if quiet {
        Level::ERROR
    } else {
        match log_level.to_lowercase().as_str() {
            "error" => Level::ERROR,
            "warn" => Level::WARN,
            "info" => Level::INFO,
            "debug" => Level::DEBUG,
            "trace" => Level::TRACE,
            _ => Level::INFO,
        }
    };

    let filter = EnvFilter::from_default_env()
        .add_directive(level.into());

    if let Some(log_file_path) = log_file {
        // Log to file
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file_path)?;
        
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(file).with_ansi(false))
            .init();
    } else {
        // Log to stdout/stderr
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(std::io::stderr))
            .init();
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize logging
    init_logging(&args.log_level, args.log_file.as_ref(), args.quiet)?;

    match args.command {
        Commands::Server { port, storage_dir } => {
            info!("Starting syncit server on port {} with storage directory: {}", 
                     port, storage_dir.display());
            
            let server = SimpleServer::new(storage_dir)?;
            server.start(port).await?;
        }
        Commands::Client { server, dir, sync_interval } => {
            info!("Starting syncit client, syncing {} to {} (sync every {}s)", 
                     dir.display(), server, sync_interval);
            
            let client = SimpleClient::new(server, dir)
                .with_sync_interval(Duration::from_secs(sync_interval));
            
            info!("Starting watch mode. Press Ctrl+C to stop.");
            
            // Create shutdown channel
            let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
            
            // Start watching in a separate task
            let watch_task = tokio::spawn(async move {
                if let Err(e) = client.start_watching_with_shutdown(Some(shutdown_rx)).await {
                    error!("Watch error: {}", e);
                }
            });
            
            // Wait for Ctrl+C
            signal::ctrl_c().await?;
            info!("Shutdown signal received, stopping...");
            
            // Send shutdown signal to client
            let _ = shutdown_tx.send(());
            
            // Wait for the watch task to complete gracefully
            if let Err(e) = watch_task.await {
                error!("Error during client shutdown: {}", e);
            }
        }
    }

    info!("Syncit stopped successfully!");
    Ok(())
}
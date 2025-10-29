use clap::{Parser, Subcommand};
use std::fs::OpenOptions;
use std::path::PathBuf;

use tokio::signal;
use tokio::sync::broadcast;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use syncpair::multi_client::MultiDirectoryClient;
use syncpair::server::SimpleServer;

#[derive(Parser)]
#[command(author, version, about = "A bidirectional file synchronization tool", long_about = None)]
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
        #[arg(
            short,
            long,
            default_value = "8080",
            help = "Port to run the server on"
        )]
        port: u16,
        #[arg(short, long, help = "Directory to store uploaded files")]
        storage_dir: PathBuf,
    },
    /// Start the client using a YAML configuration file for multi-directory sync
    Client {
        #[arg(short, long, help = "Path to the YAML configuration file")]
        file: PathBuf,
    },
}

fn init_logging(
    log_level: &str,
    log_file: Option<&PathBuf>,
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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

    let filter = EnvFilter::from_default_env().add_directive(level.into());

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
            info!(
                "Starting syncpair server on port {} with storage directory: {}",
                port,
                storage_dir.display()
            );

            let server = SimpleServer::new(storage_dir)?;
            server.start(port).await?;
        }
        Commands::Client { file } => {
            info!(
                "Starting syncpair multi-directory client using config file: {}",
                file.display()
            );

            // Load configuration and create multi-directory client
            let multi_client = MultiDirectoryClient::from_config_file(&file)?;

            info!(
                "Multi-directory client configured for: {}",
                multi_client.get_client_id()
            );
            info!("Server: {}", multi_client.get_server_url());

            let enabled_dirs = multi_client.get_enabled_directories();
            info!("Enabled directories:");
            for dir_config in &enabled_dirs {
                info!(
                    "  - {} ({})",
                    dir_config.name,
                    dir_config.local_path.display()
                );
                if let Some(ref desc) = dir_config.settings.description {
                    info!("    Description: {}", desc);
                }
                info!(
                    "    Sync interval: {}s",
                    dir_config.settings.sync_interval_seconds
                );
            }

            info!("Starting watch mode. Press Ctrl+C to stop.");

            // Create shutdown channel
            let (shutdown_tx, shutdown_rx) = broadcast::channel(1);

            // Start watching in a separate task
            let watch_task = tokio::spawn(async move {
                if let Err(e) = multi_client
                    .start_watching_with_shutdown(Some(shutdown_rx))
                    .await
                {
                    error!("Multi-directory watch error: {}", e);
                }
            });

            // Wait for Ctrl+C
            signal::ctrl_c().await?;
            info!("Shutdown signal received, stopping...");

            // Send shutdown signal to multi-client
            let _ = shutdown_tx.send(());

            // Wait for the watch task to complete gracefully
            if let Err(e) = watch_task.await {
                error!("Error during multi-client shutdown: {}", e);
            }
        }
    }

    info!("SyncPair stopped successfully!");
    Ok(())
}

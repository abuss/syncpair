use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use syncpair::{MultiDirectoryClient, SyncServer};

#[derive(Parser)]
#[command(name = "syncpair")]
#[command(about = "Collaborative file synchronization tool")]
#[command(version = "0.1.0")]
struct Cli {
    /// Set log level: error, warn, info, debug, trace
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Write logs to file instead of stdout
    #[arg(long)]
    log_file: Option<PathBuf>,

    /// Quiet mode - only show errors
    #[arg(long)]
    quiet: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the server to receive file uploads
    Server {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Storage directory for files
        #[arg(short, long, default_value = "./server_storage")]
        storage_dir: PathBuf,
    },
    /// Start multi-directory client using YAML configuration
    Client {
        /// YAML configuration file path
        #[arg(short, long)]
        file: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging
    setup_logging(&cli)?;

    // Log startup information
    tracing::info!("SyncPair v{} starting", env!("CARGO_PKG_VERSION"));

    match &cli.command {
        Commands::Server { port, storage_dir } => {
            tracing::info!("Starting server mode");
            
            let server = SyncServer::new(storage_dir.clone())?;
            server.start(*port).await?;
        }
        Commands::Client { file } => {
            tracing::info!("Starting client mode with config: {}", file.display());
            
            let client = MultiDirectoryClient::from_config_file(file)?;
            client.start().await?;
        }
    }

    Ok(())
}

fn setup_logging(cli: &Cli) -> Result<()> {
    let log_level = if cli.quiet {
        "error"
    } else {
        &cli.log_level
    };

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            format!("syncpair={},warn", log_level).into()
        });

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_file(false)
        .with_line_number(false);

    if let Some(log_file) = &cli.log_file {
        // Log to file
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer.with_writer(file))
            .init();

        // Also log startup to stderr if not quiet
        if !cli.quiet {
            eprintln!("Logging to file: {}", log_file.display());
        }
    } else {
        // Log to stdout/stderr
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    }

    Ok(())
}
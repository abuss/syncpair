use std::path::PathBuf;
use tempfile::TempDir;
use tracing_subscriber::EnvFilter;

/// Initialize test logging for better debugging
pub fn init_test_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("debug"))
        )
        .with_test_writer()
        .try_init();
}

/// Create a temporary directory for testing
pub fn create_temp_dir() -> TempDir {
    TempDir::new().expect("Failed to create temporary directory")
}

/// Create a test file with specified content
pub fn create_test_file(dir: &PathBuf, name: &str, content: &str) -> PathBuf {
    let file_path = dir.join(name);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create parent directories");
    }
    std::fs::write(&file_path, content).expect("Failed to write test file");
    file_path
}

/// Wait for a condition with timeout
pub async fn wait_for_condition<F>(mut condition: F, timeout_secs: u64) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    
    while start.elapsed() < timeout {
        if condition() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    
    false
}
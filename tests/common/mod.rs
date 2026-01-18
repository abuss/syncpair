use anyhow::Result;
use std::fs;
use std::path::Path;
use tempfile::TempDir;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Create a temporary directory for testing
pub fn create_temp_dir() -> Result<TempDir> {
    Ok(tempfile::tempdir()?)
}

/// Initialize logging for tests
pub fn init_test_logging() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with(fmt::layer().with_test_writer())
        .try_init();
}

/// Create test directory with sample files for exclude pattern testing
pub fn create_exclude_pattern_test_dir(base_path: &Path) -> Result<()> {
    let test_dir = base_path.join("test_exclude_demo");
    fs::create_dir_all(&test_dir)?;

    // Create normal files
    fs::write(test_dir.join("normal_file.txt"), "normal content")?;
    fs::write(test_dir.join("README.md"), "readme content")?;

    // Create files that should be excluded
    fs::write(test_dir.join("temp_file.tmp"), "temp content")?;
    fs::write(test_dir.join("build.log"), "log content")?;
    fs::write(test_dir.join("backup_file.bak"), "backup content")?;

    // Create directories that should be excluded
    let node_modules = test_dir.join("node_modules");
    fs::create_dir_all(&node_modules)?;
    fs::write(node_modules.join("package.json"), r#"{"name": "test"}"#)?;

    let temp_dir = test_dir.join("temp");
    fs::create_dir_all(&temp_dir)?;
    fs::write(temp_dir.join("cache.dat"), "cache data")?;

    let git_dir = test_dir.join(".git");
    fs::create_dir_all(&git_dir)?;
    fs::write(git_dir.join("config"), "git config")?;

    Ok(())
}

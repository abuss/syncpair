use std::path::{Path, PathBuf};
use std::fs;
use tempfile::TempDir;
use anyhow::Result;

/// Get the path to test fixtures directory
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Get the path to test configs directory
pub fn configs_dir() -> PathBuf {
    fixtures_dir().join("configs")
}

/// Get the path to test directories
pub fn directories_dir() -> PathBuf {
    fixtures_dir().join("directories")
}

/// Create a temporary directory for testing
pub fn create_temp_dir() -> Result<TempDir> {
    Ok(tempfile::tempdir()?)
}

/// Initialize logging for tests
pub fn init_test_logging() {
    let _ = env_logger::builder()
        .is_test(true)
        .filter_level(log::LevelFilter::Debug)
        .try_init();
}

/// Copy a directory recursively
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    
    Ok(())
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
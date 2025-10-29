use syncpair::utils::scan_directory_with_patterns;
use anyhow::Result;

#[path = "common/mod.rs"]
mod common;

#[tokio::test]
async fn test_exclude_patterns_functionality() -> Result<()> {
    common::init_test_logging();
    
    // Create temporary test directory with sample files
    let temp_dir = common::create_temp_dir()?;
    let test_dir = temp_dir.path().join("test_exclude_demo");
    common::create_exclude_pattern_test_dir(temp_dir.path())?;
    
    let exclude_patterns = vec![
        "*.tmp".to_string(),
        "*.log".to_string(),
        "node_modules".to_string(),
        "*.bak".to_string(),
        "temp/*".to_string(),
        ".git".to_string(),
        "*.DS_Store".to_string(),
    ];
    
    println!("🔍 Testing exclude patterns functionality...");
    println!("📁 Directory: {}", test_dir.display());
    println!("🚫 Exclude patterns: {:?}", exclude_patterns);
    
    // Scan without exclude patterns
    let all_files = syncpair::utils::scan_directory(&test_dir)?;
    println!("📄 All files found (without patterns): {}", all_files.len());
    for file in &all_files {
        println!("  - {}", file.path);
    }
    
    // Scan with exclude patterns
    let filtered_files = scan_directory_with_patterns(&test_dir, &exclude_patterns)?;
    println!("✅ Files after applying exclude patterns: {}", filtered_files.len());
    for file in &filtered_files {
        println!("  - {}", file.path);
    }
    
    // Verify that exclude patterns work correctly
    let excluded_count = all_files.len() - filtered_files.len();
    println!("🎯 Result: {} files excluded", excluded_count);
    
    // Assertions to verify functionality
    assert!(excluded_count > 0, "Exclude patterns should filter out some files");
    
    // Verify specific files are excluded
    let filtered_paths: Vec<&String> = filtered_files.iter().map(|f| &f.path).collect();
    assert!(!filtered_paths.iter().any(|p| p.ends_with(".tmp")), ".tmp files should be excluded");
    assert!(!filtered_paths.iter().any(|p| p.ends_with(".log")), ".log files should be excluded");
    assert!(!filtered_paths.iter().any(|p| p.ends_with(".bak")), ".bak files should be excluded");
    assert!(!filtered_paths.iter().any(|p| p.contains("node_modules")), "node_modules should be excluded");
    assert!(!filtered_paths.iter().any(|p| p.contains(".git")), ".git directory should be excluded");
    assert!(!filtered_paths.iter().any(|p| p.contains("temp/")), "temp/ directory should be excluded");
    
    // Verify normal files are included
    assert!(filtered_paths.iter().any(|p| p.contains("normal_file.txt")), "normal files should be included");
    assert!(filtered_paths.iter().any(|p| p.contains("README.md")), "README.md should be included");
    
    println!("✅ All exclude pattern tests passed!");
    
    Ok(())
}

#[tokio::test]
async fn test_exclude_patterns_edge_cases() -> Result<()> {
    common::init_test_logging();
    
    let temp_dir = common::create_temp_dir()?;
    let test_dir = temp_dir.path().join("edge_case_test");
    std::fs::create_dir_all(&test_dir)?;
    
    // Create files with various patterns
    std::fs::write(test_dir.join("file.TMP"), "uppercase tmp")?; // Should NOT be excluded (case sensitive)
    std::fs::write(test_dir.join("document.tmp"), "ends with tmp")?; // Should be excluded (.tmp extension)
    std::fs::write(test_dir.join("temporary.txt"), "similar name")?; // Should NOT be excluded
    
    let exclude_patterns = vec!["*.tmp".to_string()];
    
    let _all_files = syncpair::utils::scan_directory(&test_dir)?;
    let filtered_files = scan_directory_with_patterns(&test_dir, &exclude_patterns)?;
    
    println!("Edge case test results:");
    for file in &filtered_files {
        println!("  Included: {}", file.path);
    }
    
    let filtered_paths: Vec<&String> = filtered_files.iter().map(|f| &f.path).collect();
    
    // Verify case sensitivity and exact matching
    assert!(filtered_paths.iter().any(|p| p.contains("file.TMP")), "file.TMP should be included (case sensitive)");
    assert!(filtered_paths.iter().any(|p| p.contains("temporary.txt")), "temporary.txt should be included");
    assert!(!filtered_paths.iter().any(|p| p.contains("document.tmp")), "document.tmp should be excluded");
    
    println!("✅ Edge case tests passed!");
    
    Ok(())
}
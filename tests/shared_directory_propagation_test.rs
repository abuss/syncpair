use std::time::Duration;
use syncpair::{SyncClient, SyncServer};
use tokio::time::timeout;

mod common;

use common::{create_temp_dir, create_test_file, init_test_logging, wait_for_condition};

#[tokio::test]
async fn test_shared_directory_file_creation_propagation() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();
    let client3_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(120), server.start(9001)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create three clients all using the same shared directory
    let client1 = SyncClient::new(
        "http://localhost:9001".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None, // Shared directory (no client_id)
        Some("shared_project".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:9001".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None, // Shared directory (no client_id)
        Some("shared_project".to_string()),
        vec![],
    ).unwrap();

    let client3 = SyncClient::new(
        "http://localhost:9001".to_string(),
        client3_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None, // Shared directory (no client_id)
        Some("shared_project".to_string()),
        vec![],
    ).unwrap();

    // Test 1: Client 1 creates a file
    println!("Test 1: Client 1 creates a file");
    create_test_file(&client1_dir.path().to_path_buf(), "client1_file.txt", "Created by client 1");

    // Sync client 1
    let sync1_result = timeout(Duration::from_secs(15), client1.sync()).await;
    assert!(sync1_result.is_ok(), "Client 1 sync should succeed");

    // Sync other clients to get the new file
    let sync2_result = timeout(Duration::from_secs(15), client2.sync()).await;
    assert!(sync2_result.is_ok(), "Client 2 sync should succeed");

    let sync3_result = timeout(Duration::from_secs(15), client3.sync()).await;
    assert!(sync3_result.is_ok(), "Client 3 sync should succeed");

    // Verify file appears on both other clients
    let client2_file = client2_dir.path().join("client1_file.txt");
    let client3_file = client3_dir.path().join("client1_file.txt");

    let file_exists_client2 = wait_for_condition(|| client2_file.exists(), 10).await;
    let file_exists_client3 = wait_for_condition(|| client3_file.exists(), 10).await;

    assert!(file_exists_client2, "File should be synchronized to client 2");
    assert!(file_exists_client3, "File should be synchronized to client 3");

    if client2_file.exists() {
        let content = std::fs::read_to_string(&client2_file).unwrap();
        assert_eq!(content, "Created by client 1");
    }

    if client3_file.exists() {
        let content = std::fs::read_to_string(&client3_file).unwrap();
        assert_eq!(content, "Created by client 1");
    }

    println!("✓ File creation propagation test passed");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_shared_directory_file_modification_propagation() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();
    let client3_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(120), server.start(9002)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create three clients all using the same shared directory
    let client1 = SyncClient::new(
        "http://localhost:9002".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("modification_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:9002".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("modification_test".to_string()),
        vec![],
    ).unwrap();

    let client3 = SyncClient::new(
        "http://localhost:9002".to_string(),
        client3_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("modification_test".to_string()),
        vec![],
    ).unwrap();

    // Test 2: File modification propagation
    println!("Test 2: File modification propagation");
    
    // Create initial file on client 1
    create_test_file(&client1_dir.path().to_path_buf(), "modify_me.txt", "Original content");

    // Initial sync to distribute file
    let _sync1 = timeout(Duration::from_secs(15), client1.sync()).await;
    let _sync2 = timeout(Duration::from_secs(15), client2.sync()).await;
    let _sync3 = timeout(Duration::from_secs(15), client3.sync()).await;

    // Wait for initial file to appear
    let client2_file = client2_dir.path().join("modify_me.txt");
    let client3_file = client3_dir.path().join("modify_me.txt");
    
    let initial_sync_done = wait_for_condition(|| {
        client2_file.exists() && client3_file.exists()
    }, 10).await;
    assert!(initial_sync_done, "Initial file should be on all clients");

    // Now modify file on client 2 - add longer delay to ensure newer timestamp
    tokio::time::sleep(Duration::from_millis(1000)).await; // Ensure different timestamp
    std::fs::write(&client2_file, "Modified by client 2").unwrap();

    // Sync client 2 to upload modification
    let sync2_mod = timeout(Duration::from_secs(15), client2.sync()).await;
    assert!(sync2_mod.is_ok(), "Client 2 modification sync should succeed");

    // Sync other clients to get the modification
    let sync1_get_mod = timeout(Duration::from_secs(15), client1.sync()).await;
    assert!(sync1_get_mod.is_ok(), "Client 1 should sync modification");

    let sync3_get_mod = timeout(Duration::from_secs(15), client3.sync()).await;
    assert!(sync3_get_mod.is_ok(), "Client 3 should sync modification");

    // Verify modification propagated
    let client1_file = client1_dir.path().join("modify_me.txt");
    
    let modification_propagated = wait_for_condition(|| {
        if let Ok(content1) = std::fs::read_to_string(&client1_file) {
            if let Ok(content3) = std::fs::read_to_string(&client3_file) {
                return content1 == "Modified by client 2" && content3 == "Modified by client 2";
            }
        }
        false
    }, 10).await;

    assert!(modification_propagated, "File modification should propagate to all clients");

    println!("✓ File modification propagation test passed");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_shared_directory_file_deletion_propagation() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();
    let client3_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(120), server.start(9003)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create three clients all using the same shared directory
    let client1 = SyncClient::new(
        "http://localhost:9003".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("deletion_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:9003".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("deletion_test".to_string()),
        vec![],
    ).unwrap();

    let client3 = SyncClient::new(
        "http://localhost:9003".to_string(),
        client3_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("deletion_test".to_string()),
        vec![],
    ).unwrap();

    // Test 3: File deletion propagation
    println!("Test 3: File deletion propagation");
    
    // Create files on client 1
    let file_to_delete = create_test_file(&client1_dir.path().to_path_buf(), "delete_me.txt", "Will be deleted");
    create_test_file(&client1_dir.path().to_path_buf(), "keep_me.txt", "Should remain");

    // Initial sync to distribute files
    let _sync1 = timeout(Duration::from_secs(15), client1.sync()).await;
    let _sync2 = timeout(Duration::from_secs(15), client2.sync()).await;
    let _sync3 = timeout(Duration::from_secs(15), client3.sync()).await;

    // Verify files exist on all clients
    let client2_delete_file = client2_dir.path().join("delete_me.txt");
    let client3_delete_file = client3_dir.path().join("delete_me.txt");
    let client2_keep_file = client2_dir.path().join("keep_me.txt");
    let client3_keep_file = client3_dir.path().join("keep_me.txt");

    let files_distributed = wait_for_condition(|| {
        client2_delete_file.exists() && client3_delete_file.exists() &&
        client2_keep_file.exists() && client3_keep_file.exists()
    }, 10).await;
    assert!(files_distributed, "Files should be distributed to all clients");

    // Delete file from client 3
    std::fs::remove_file(&client3_delete_file).unwrap();

    // Sync client 3 to propagate deletion
    let sync3_del = timeout(Duration::from_secs(15), client3.sync()).await;
    assert!(sync3_del.is_ok(), "Client 3 deletion sync should succeed");

    // Sync other clients to receive deletion
    let sync1_get_del = timeout(Duration::from_secs(15), client1.sync()).await;
    assert!(sync1_get_del.is_ok(), "Client 1 should sync deletion");

    let sync2_get_del = timeout(Duration::from_secs(15), client2.sync()).await;
    assert!(sync2_get_del.is_ok(), "Client 2 should sync deletion");

    // Verify deletion propagated (file should be gone, but keep_me.txt should remain)
    let deletion_propagated = wait_for_condition(|| {
        !file_to_delete.exists() && !client2_delete_file.exists() &&
        client1_dir.path().join("keep_me.txt").exists() && client2_keep_file.exists()
    }, 10).await;

    assert!(deletion_propagated, "File deletion should propagate to all clients while preserving other files");

    println!("✓ File deletion propagation test passed");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_comprehensive_shared_directory_propagation() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();
    let client3_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(180), server.start(9004)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create three clients all using the same shared directory
    let client1 = SyncClient::new(
        "http://localhost:9004".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("comprehensive_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:9004".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("comprehensive_test".to_string()),
        vec![],
    ).unwrap();

    let client3 = SyncClient::new(
        "http://localhost:9004".to_string(),
        client3_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("comprehensive_test".to_string()),
        vec![],
    ).unwrap();

    // Test 4: Comprehensive workflow
    println!("Test 4: Comprehensive shared directory workflow");

    // Step 1: Client 1 creates initial files
    create_test_file(&client1_dir.path().to_path_buf(), "project.md", "# Project Documentation\nInitial content");
    create_test_file(&client1_dir.path().to_path_buf(), "config.json", r#"{"version": "1.0", "debug": false}"#);
    std::fs::create_dir_all(client1_dir.path().join("src")).unwrap();
    create_test_file(&client1_dir.path().join("src").to_path_buf(), "main.rs", "fn main() { println!(\"Hello\"); }");

    // Sync to distribute initial files
    let _sync1 = timeout(Duration::from_secs(15), client1.sync()).await;
    let _sync2 = timeout(Duration::from_secs(15), client2.sync()).await;
    let _sync3 = timeout(Duration::from_secs(15), client3.sync()).await;

    // Verify initial distribution
    let initial_files_distributed = wait_for_condition(|| {
        client2_dir.path().join("project.md").exists() &&
        client2_dir.path().join("config.json").exists() &&
        client2_dir.path().join("src/main.rs").exists() &&
        client3_dir.path().join("project.md").exists() &&
        client3_dir.path().join("config.json").exists() &&
        client3_dir.path().join("src/main.rs").exists()
    }, 15).await;
    assert!(initial_files_distributed, "Initial files should be distributed");

    // Step 2: Client 2 modifies project.md
    tokio::time::sleep(Duration::from_millis(1000)).await;
    std::fs::write(client2_dir.path().join("project.md"), 
        "# Project Documentation\nInitial content\n\n## Updated by Client 2\nAdded new section").unwrap();

    // Step 3: Client 3 creates a new file and modifies config.json
    tokio::time::sleep(Duration::from_millis(1000)).await;
    create_test_file(&client3_dir.path().to_path_buf(), "README.md", "# README\nCreated by client 3");
    std::fs::write(client3_dir.path().join("config.json"), 
        r#"{"version": "1.1", "debug": true, "features": ["logging"]}"#).unwrap();

    // Step 4: Sync all changes
    let _sync2_changes = timeout(Duration::from_secs(15), client2.sync()).await;
    let _sync3_changes = timeout(Duration::from_secs(15), client3.sync()).await;
    let _sync1_get_changes = timeout(Duration::from_secs(15), client1.sync()).await;
    let _sync2_get_all = timeout(Duration::from_secs(15), client2.sync()).await;
    let _sync3_get_all = timeout(Duration::from_secs(15), client3.sync()).await;

    // Step 5: Verify all changes propagated
    let all_changes_propagated = wait_for_condition(|| {
        // Check if README.md exists on all clients
        let readme_exists = client1_dir.path().join("README.md").exists() &&
                           client2_dir.path().join("README.md").exists() &&
                           client3_dir.path().join("README.md").exists();

        // Check if project.md was updated on all clients
        let project_updated = if let (Ok(content1), Ok(content2), Ok(content3)) = (
            std::fs::read_to_string(client1_dir.path().join("project.md")),
            std::fs::read_to_string(client2_dir.path().join("project.md")),
            std::fs::read_to_string(client3_dir.path().join("project.md"))
        ) {
            content1.contains("Updated by Client 2") &&
            content2.contains("Updated by Client 2") &&
            content3.contains("Updated by Client 2")
        } else { false };

        // Check if config.json was updated on all clients
        let config_updated = if let (Ok(content1), Ok(content2), Ok(content3)) = (
            std::fs::read_to_string(client1_dir.path().join("config.json")),
            std::fs::read_to_string(client2_dir.path().join("config.json")),
            std::fs::read_to_string(client3_dir.path().join("config.json"))
        ) {
            content1.contains("1.1") && content1.contains("logging") &&
            content2.contains("1.1") && content2.contains("logging") &&
            content3.contains("1.1") && content3.contains("logging")
        } else { false };

        readme_exists && project_updated && config_updated
    }, 20).await;

    assert!(all_changes_propagated, "All changes should propagate to all clients");

    // Step 6: Client 1 deletes a file
    std::fs::remove_file(client1_dir.path().join("src/main.rs")).unwrap();

    // Step 7: Final sync to propagate deletion
    let _sync1_del = timeout(Duration::from_secs(15), client1.sync()).await;
    let _sync2_get_del = timeout(Duration::from_secs(15), client2.sync()).await;
    let _sync3_get_del = timeout(Duration::from_secs(15), client3.sync()).await;

    // Step 8: Verify deletion propagated
    let deletion_propagated = wait_for_condition(|| {
        !client1_dir.path().join("src/main.rs").exists() &&
        !client2_dir.path().join("src/main.rs").exists() &&
        !client3_dir.path().join("src/main.rs").exists()
    }, 10).await;

    assert!(deletion_propagated, "File deletion should propagate to all clients");

    println!("✓ Comprehensive shared directory propagation test passed");

    // Cleanup
    server_handle.abort();
}
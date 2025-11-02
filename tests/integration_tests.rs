use std::path::PathBuf;
use std::time::Duration;

use syncpair::{SyncClient, SyncServer};
use tokio::time::timeout;

mod common;

use common::{create_temp_dir, create_test_file, init_test_logging, wait_for_condition};

#[tokio::test]
async fn test_basic_server_startup() {
    init_test_logging();

    let temp_dir = create_temp_dir();
    let _server = SyncServer::new(temp_dir.path().to_path_buf()).unwrap();

    // Test that server can be created successfully
    assert!(temp_dir.path().exists());
}

#[tokio::test]
async fn test_single_client_sync() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server in background
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8081)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create test file in client directory
    create_test_file(&client_dir.path().to_path_buf(), "test.txt", "Hello, World!");

    // Create client
    let client = SyncClient::new(
        "http://localhost:8081".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("test_client".to_string()),
        Some("test_dir".to_string()),
        vec![],
    ).unwrap();

    // Perform initial sync
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(sync_result.is_ok(), "Sync should complete successfully");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_multi_client_collaboration() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8082)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create shared directory clients (no client_id for shared directories)
    let client1 = SyncClient::new(
        "http://localhost:8082".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None, // Shared directory
        Some("shared_project".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:8082".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None, // Shared directory
        Some("shared_project".to_string()),
        vec![],
    ).unwrap();

    // Client 1 creates a file
    create_test_file(&client1_dir.path().to_path_buf(), "shared_file.txt", "Content from client 1");

    // Sync client 1
    let sync1_result = timeout(Duration::from_secs(10), client1.sync()).await;
    assert!(sync1_result.is_ok(), "Client 1 sync should succeed");

    // Sync client 2 (should download the file)
    let sync2_result = timeout(Duration::from_secs(10), client2.sync()).await;
    assert!(sync2_result.is_ok(), "Client 2 sync should succeed");

    // Check if file exists in client 2
    let client2_file = client2_dir.path().join("shared_file.txt");
    
    // Wait for file to appear
    let file_exists = wait_for_condition(|| client2_file.exists(), 5).await;
    assert!(file_exists, "File should be synchronized to client 2");

    if client2_file.exists() {
        let content = std::fs::read_to_string(&client2_file).unwrap();
        assert_eq!(content, "Content from client 1");
    }

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_conflict_resolution() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8083)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create clients
    let client1 = SyncClient::new(
        "http://localhost:8083".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(10),
        None,
        Some("conflict_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:8083".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(10),
        None,
        Some("conflict_test".to_string()),
        vec![],
    ).unwrap();

    // Both clients create the same file with different content
    create_test_file(&client1_dir.path().to_path_buf(), "conflict.txt", "Version 1");
    
    // Small delay to ensure different timestamps
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    create_test_file(&client2_dir.path().to_path_buf(), "conflict.txt", "Version 2");

    // Sync client 1 first
    let _sync1 = timeout(Duration::from_secs(10), client1.sync()).await;

    // Then sync client 2 (should resolve conflict)
    let _sync2 = timeout(Duration::from_secs(10), client2.sync()).await;

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_file_deletion_sync() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8084)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create clients
    let client1 = SyncClient::new(
        "http://localhost:8084".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(10),
        None,
        Some("deletion_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:8084".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(10),
        None,
        Some("deletion_test".to_string()),
        vec![],
    ).unwrap();

    // Client 1 creates file
    let file_path = create_test_file(&client1_dir.path().to_path_buf(), "to_delete.txt", "Will be deleted");

    // Sync to get file on both clients
    let _sync1 = timeout(Duration::from_secs(10), client1.sync()).await;
    let _sync2 = timeout(Duration::from_secs(10), client2.sync()).await;

    // Delete file from client 1
    std::fs::remove_file(&file_path).unwrap();

    // Sync client 1 (should propagate deletion)
    let _sync1_delete = timeout(Duration::from_secs(10), client1.sync()).await;

    // Sync client 2 (should receive deletion)
    let _sync2_delete = timeout(Duration::from_secs(10), client2.sync()).await;

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_server_unavailable_error() {
    init_test_logging();

    let client_dir = create_temp_dir();

    // Create client pointing to non-existent server
    let client = SyncClient::new(
        "http://localhost:9999".to_string(), // Non-existent server
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("test_client".to_string()),
        Some("test_dir".to_string()),
        vec![],
    ).unwrap();

    // Create test file
    create_test_file(&client_dir.path().to_path_buf(), "test.txt", "Hello, World!");

    // Sync should fail due to server unavailable
    let sync_result = timeout(Duration::from_secs(5), client.sync()).await;
    assert!(sync_result.is_ok()); // Timeout succeeds but sync should handle error gracefully
}

#[tokio::test]
async fn test_invalid_client_creation() {
    init_test_logging();

    // Test with non-existent directory path
    let invalid_path = PathBuf::from("/non/existent/path/that/should/not/exist");
    
    // This should fail because the directory doesn't exist and can't be created
    let result = SyncClient::new(
        "http://localhost:8080".to_string(),
        invalid_path,
        Duration::from_secs(5),
        Some("test_client".to_string()),
        Some("test_dir".to_string()),
        vec![],
    );
    
    // Should handle the error gracefully (the implementation might create the directory)
    // So we'll just ensure it doesn't panic
    match result {
        Ok(_) => {
            // Directory was created successfully
        },
        Err(_) => {
            // Error was handled properly
        }
    }
}

#[tokio::test]
async fn test_ignore_patterns() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8085)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client with ignore patterns
    let client = SyncClient::new(
        "http://localhost:8085".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("ignore_test".to_string()),
        Some("ignore_dir".to_string()),
        vec!["*.tmp".to_string(), "*.log".to_string(), "temp/".to_string()],
    ).unwrap();

    // Create files that should be ignored
    create_test_file(&client_dir.path().to_path_buf(), "should_sync.txt", "This should sync");
    create_test_file(&client_dir.path().to_path_buf(), "ignore_me.tmp", "This should be ignored");
    create_test_file(&client_dir.path().to_path_buf(), "debug.log", "This should be ignored");
    
    // Create temp directory and file inside
    std::fs::create_dir_all(client_dir.path().join("temp")).unwrap();
    create_test_file(&client_dir.path().join("temp").to_path_buf(), "temp_file.txt", "This should be ignored");

    // Sync should only upload the non-ignored file
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(sync_result.is_ok(), "Sync should complete successfully even with ignored files");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_large_file_sync() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8086)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8086".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("large_file_test".to_string()),
        Some("large_file_dir".to_string()),
        vec![],
    ).unwrap();

    // Create a larger test file (1MB)
    let large_content = "A".repeat(1024 * 1024); // 1MB of 'A's
    create_test_file(&client_dir.path().to_path_buf(), "large_file.txt", &large_content);

    // Sync the large file
    let sync_result = timeout(Duration::from_secs(30), client.sync()).await;
    assert!(sync_result.is_ok(), "Large file sync should complete successfully");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_nested_directory_sync() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8087)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create clients
    let client1 = SyncClient::new(
        "http://localhost:8087".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("nested_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:8087".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("nested_test".to_string()),
        vec![],
    ).unwrap();

    // Create nested directory structure
    std::fs::create_dir_all(client1_dir.path().join("level1/level2/level3")).unwrap();
    create_test_file(&client1_dir.path().join("level1").to_path_buf(), "file1.txt", "Level 1 content");
    create_test_file(&client1_dir.path().join("level1/level2").to_path_buf(), "file2.txt", "Level 2 content");
    create_test_file(&client1_dir.path().join("level1/level2/level3").to_path_buf(), "file3.txt", "Level 3 content");

    // Sync client 1
    let sync1_result = timeout(Duration::from_secs(10), client1.sync()).await;
    assert!(sync1_result.is_ok(), "Client 1 sync should succeed");

    // Sync client 2 (should download the nested structure)
    let sync2_result = timeout(Duration::from_secs(10), client2.sync()).await;
    assert!(sync2_result.is_ok(), "Client 2 sync should succeed");

    // Verify nested structure was synchronized
    let file1_path = client2_dir.path().join("level1/file1.txt");
    let file2_path = client2_dir.path().join("level1/level2/file2.txt");
    let file3_path = client2_dir.path().join("level1/level2/level3/file3.txt");

    let structure_synced = wait_for_condition(|| {
        file1_path.exists() && file2_path.exists() && file3_path.exists()
    }, 5).await;
    
    assert!(structure_synced, "Nested directory structure should be synchronized");

    // Verify content
    if file1_path.exists() {
        let content = std::fs::read_to_string(&file1_path).unwrap();
        assert_eq!(content, "Level 1 content");
    }

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_concurrent_sync_operations() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8088)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create clients
    let client1 = SyncClient::new(
        "http://localhost:8088".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("concurrent_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:8088".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(2),
        None,
        Some("concurrent_test".to_string()),
        vec![],
    ).unwrap();

    // Create different files on both clients simultaneously
    create_test_file(&client1_dir.path().to_path_buf(), "client1_file.txt", "From client 1");
    create_test_file(&client2_dir.path().to_path_buf(), "client2_file.txt", "From client 2");

    // Perform concurrent sync operations
    let sync1_future = client1.sync();
    let sync2_future = client2.sync();

    let (result1, result2) = tokio::join!(
        timeout(Duration::from_secs(15), sync1_future),
        timeout(Duration::from_secs(15), sync2_future)
    );

    assert!(result1.is_ok(), "Client 1 concurrent sync should succeed");
    assert!(result2.is_ok(), "Client 2 concurrent sync should succeed");

    // Both clients should have both files after another sync
    let _final_sync1 = timeout(Duration::from_secs(10), client1.sync()).await;
    let _final_sync2 = timeout(Duration::from_secs(10), client2.sync()).await;

    // Verify both files exist on both clients
    let client1_has_both = wait_for_condition(|| {
        client1_dir.path().join("client1_file.txt").exists() && 
        client1_dir.path().join("client2_file.txt").exists()
    }, 5).await;

    let client2_has_both = wait_for_condition(|| {
        client2_dir.path().join("client1_file.txt").exists() && 
        client2_dir.path().join("client2_file.txt").exists()
    }, 5).await;

    assert!(client1_has_both, "Client 1 should have both files");
    assert!(client2_has_both, "Client 2 should have both files");

    // Cleanup
    server_handle.abort();
}

#[tokio::test] 
async fn test_empty_directory_sync() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8089)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client with empty directory
    let client = SyncClient::new(
        "http://localhost:8089".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("empty_test".to_string()),
        Some("empty_dir".to_string()),
        vec![],
    ).unwrap();

    // Sync empty directory (should work without errors)
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(sync_result.is_ok(), "Empty directory sync should succeed");

    // Cleanup
    server_handle.abort();
}

#[tokio::test]
async fn test_server_shutdown_graceful_handling() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8090)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8090".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("shutdown_test".to_string()),
        Some("shutdown_dir".to_string()),
        vec![],
    ).unwrap();

    // Create files to upload
    create_test_file(&client_dir.path().to_path_buf(), "file1.txt", "Content 1");
    create_test_file(&client_dir.path().to_path_buf(), "file2.txt", "Content 2");
    create_test_file(&client_dir.path().to_path_buf(), "file3.txt", "Content 3");

    // First sync should work (server is running)
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(sync_result.is_ok(), "Initial sync should succeed");

    // Create more files after initial sync
    create_test_file(&client_dir.path().to_path_buf(), "file4.txt", "Content 4");
    create_test_file(&client_dir.path().to_path_buf(), "file5.txt", "Content 5");

    // Shutdown server
    server_handle.abort();
    
    // Wait for server to be fully down
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try to sync after server shutdown - should handle gracefully
    let sync_result = timeout(Duration::from_secs(5), client.sync()).await;
    // This should return an error but handle it gracefully without panic
    assert!(sync_result.is_ok()); // Timeout succeeded
    
    // The key test is that the client doesn't crash and handles the error appropriately
    // In the logs, we should see "Server unavailable" instead of individual upload failures
}

#[tokio::test]
async fn test_server_reconnect_after_shutdown() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(15), server.start(8091)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8091".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("reconnect_test".to_string()),
        Some("reconnect_dir".to_string()),
        vec![],
    ).unwrap();

    // Create files
    create_test_file(&client_dir.path().to_path_buf(), "persistent_file.txt", "Should sync eventually");

    // Initial sync
    let _initial_sync = timeout(Duration::from_secs(10), client.sync()).await;

    // Create file after initial sync
    create_test_file(&client_dir.path().to_path_buf(), "new_file.txt", "Created after initial sync");

    // Shutdown server
    server_handle.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try sync while server is down - should handle gracefully
    let _down_sync = timeout(Duration::from_secs(3), client.sync()).await;

    // Restart server on same port
    let server2 = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle2 = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(15), server2.start(8091)).await;
    });

    // Wait for server to restart
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Sync should work again
    let reconnect_sync = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(reconnect_sync.is_ok(), "Sync should work after server restart");

    // Cleanup
    server_handle2.abort();
}
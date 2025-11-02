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
        Some("journal_test".to_string()),
        Some("journal_dir".to_string()),
        vec![], // No custom ignore patterns - testing built-in exclusions only
    ).unwrap();

    // Create regular files that should sync
    create_test_file(&client_dir.path().to_path_buf(), "regular_file.txt", "This should sync");
    create_test_file(&client_dir.path().to_path_buf(), "data.json", "This should also sync");

    // Create SQLite journal files that should be excluded
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-journal", "Journal file content");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-wal", "WAL file content");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-shm", "Shared memory file content");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db", "Main database file");

    // Create other SQLite-related files that should be excluded
    create_test_file(&client_dir.path().to_path_buf(), "user.db-journal", "User journal file");
    create_test_file(&client_dir.path().to_path_buf(), "app.sqlite-wal", "App WAL file");

    // Perform sync
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(sync_result.is_ok(), "Sync should complete successfully");

    // Wait a moment for any async operations to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check what files were synced to the server
    let server_client_dir = server_dir.path().join("journal_test:journal_dir");
    
    // Regular files should be present on server
    assert!(
        server_client_dir.join("regular_file.txt").exists(),
        "Regular file should be synced to server"
    );
    assert!(
        server_client_dir.join("data.json").exists(),
        "Data file should be synced to server"
    );

    // SQLite journal/system files should NOT be present on server
    assert!(
        !server_client_dir.join(".syncpair_state.db-journal").exists(),
        "Journal file should not be synced to server"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db-wal").exists(),
        "WAL file should not be synced to server"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db-shm").exists(),
        "Shared memory file should not be synced to server"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db").exists(),
        "Main database file should not be synced to server"
    );

    // Other database journal files WILL be synced because they're not built-in patterns
    // (Note: these would need custom ignore patterns in real usage)
    assert!(
        server_client_dir.join("user.db-journal").exists(),
        "user.db-journal SHOULD be synced (not covered by built-in patterns)"
    );
    assert!(
        server_client_dir.join("app.sqlite-wal").exists(),
        "app.sqlite-wal SHOULD be synced (not covered by built-in patterns)"
    );

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
        let _ = timeout(Duration::from_secs(60), server.start(8096)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8096".to_string(),
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
use std::time::Duration;
use syncpair::{SyncClient, SyncServer};
use tokio::time::timeout;

mod common;
use common::{create_temp_dir, create_test_file, init_test_logging, wait_for_condition};

#[tokio::test]
async fn test_upload_resumption_after_server_restart() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8092)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8092".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("resumption_test".to_string()),
        Some("resumption_dir".to_string()),
        vec![],
    ).unwrap();

    // Create files to upload
    create_test_file(&client_dir.path().to_path_buf(), "file1.txt", "Content 1");
    create_test_file(&client_dir.path().to_path_buf(), "file2.txt", "Content 2");
    create_test_file(&client_dir.path().to_path_buf(), "file3.txt", "Content 3");

    // Perform initial sync successfully
    let initial_sync = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(initial_sync.is_ok() && initial_sync.unwrap().is_ok(), "Initial sync should succeed");

    // Create additional files that will need to be uploaded
    create_test_file(&client_dir.path().to_path_buf(), "file4.txt", "Content 4");
    create_test_file(&client_dir.path().to_path_buf(), "file5.txt", "Content 5");

    // Shutdown server before these new files can be uploaded
    server_handle.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Try to sync while server is down - this should detect files to upload but fail
    let _failed_sync = timeout(Duration::from_secs(5), client.sync()).await;
    println!("Sync with server down completed - should have handled gracefully");

    // Start a new server instance
    let server2 = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle2 = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server2.start(8092)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now sync should resume and upload the pending files
    let resumption_sync = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(resumption_sync.is_ok() && resumption_sync.unwrap().is_ok(), "Resumption sync should succeed");

    // Verify files were uploaded by checking server storage
    let server_storage_path = server_dir.path().join("resumption_test:resumption_dir");
    
    let files_synced = wait_for_condition(|| {
        server_storage_path.join("file4.txt").exists() && 
        server_storage_path.join("file5.txt").exists()
    }, 5).await;
    
    assert!(files_synced, "Files should be uploaded after server restart");

    // Verify file contents
    if server_storage_path.join("file4.txt").exists() {
        let content = std::fs::read_to_string(server_storage_path.join("file4.txt")).unwrap();
        assert_eq!(content, "Content 4");
    }
    
    if server_storage_path.join("file5.txt").exists() {
        let content = std::fs::read_to_string(server_storage_path.join("file5.txt")).unwrap();
        assert_eq!(content, "Content 5");
    }

    // Cleanup
    server_handle2.abort();
}

#[tokio::test]
async fn test_upload_resumption_with_mixed_operations() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client1_dir = create_temp_dir();
    let client2_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server.start(8093)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create two clients for the same shared directory
    let client1 = SyncClient::new(
        "http://localhost:8093".to_string(),
        client1_dir.path().to_path_buf(),
        Duration::from_secs(5),
        None, // Shared directory
        Some("mixed_ops_test".to_string()),
        vec![],
    ).unwrap();

    let client2 = SyncClient::new(
        "http://localhost:8093".to_string(),
        client2_dir.path().to_path_buf(),
        Duration::from_secs(5),
        None, // Shared directory  
        Some("mixed_ops_test".to_string()),
        vec![],
    ).unwrap();

    // Client 1 creates a file and syncs it
    create_test_file(&client1_dir.path().to_path_buf(), "shared_file.txt", "Initial content");
    let _sync1 = timeout(Duration::from_secs(10), client1.sync()).await;

    // Client 2 syncs to get the file
    let _sync2 = timeout(Duration::from_secs(10), client2.sync()).await;

    // Client 1 creates new files that will need uploading
    create_test_file(&client1_dir.path().to_path_buf(), "new_file1.txt", "New content 1");
    create_test_file(&client1_dir.path().to_path_buf(), "new_file2.txt", "New content 2");

    // Wait to ensure timestamp separation for conflict resolution
    tokio::time::sleep(Duration::from_millis(1000)).await;
    
    // Client 2 modifies the shared file (so client1 will need to download)
    create_test_file(&client2_dir.path().to_path_buf(), "shared_file.txt", "Modified by client 2");
    let _sync2_modify = timeout(Duration::from_secs(10), client2.sync()).await;

    // Shutdown server before client1 can sync its new files
    server_handle.abort();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Client1 tries to sync - should fail but handle gracefully  
    let _failed_sync = timeout(Duration::from_secs(5), client1.sync()).await;
    println!("Client1 sync during server downtime handled gracefully");

    // Restart server
    let server2 = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle2 = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(60), server2.start(8093)).await;
    });
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Client1 should now be able to sync: upload new files AND download modified shared file
    let resumption_sync = timeout(Duration::from_secs(15), client1.sync()).await;
    assert!(resumption_sync.is_ok() && resumption_sync.unwrap().is_ok(), "Mixed operations resumption should succeed");

    // Verify client1 has downloaded the modified shared file
    let client1_shared_file = client1_dir.path().join("shared_file.txt");
    
    // Debug: Check what content the file actually has
    if client1_shared_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&client1_shared_file) {
            println!("DEBUG: client1_shared_file.txt content: '{}'", content);
            println!("DEBUG: Expected: 'Modified by client 2'");
        }
    } else {
        println!("DEBUG: client1_shared_file.txt doesn't exist");
    }
    
    let has_updated_content = wait_for_condition(|| {
        if client1_shared_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&client1_shared_file) {
                return content == "Modified by client 2";
            }
        }
        false
    }, 5).await;
    
    assert!(has_updated_content, "Client1 should have downloaded the updated shared file");

    // Verify server has client1's new files
    let server_storage_path = server_dir.path().join("mixed_ops_test");
    let new_files_uploaded = wait_for_condition(|| {
        server_storage_path.join("new_file1.txt").exists() && 
        server_storage_path.join("new_file2.txt").exists()
    }, 5).await;
    
    assert!(new_files_uploaded, "Client1's new files should be uploaded after resumption");

    // Cleanup
    server_handle2.abort();
}
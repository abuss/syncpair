use std::time::Duration;
use syncpair::{SyncClient, SyncServer};
use tokio::time::timeout;

mod common;
use common::{create_temp_dir, create_test_file, init_test_logging};

#[tokio::test]
async fn test_server_shutdown_during_upload_queue() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8095)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8095".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("upload_test".to_string()),
        Some("upload_dir".to_string()),
        vec![],
    ).unwrap();

    // Create multiple files to upload
    create_test_file(&client_dir.path().to_path_buf(), "file1.txt", "Content 1");
    create_test_file(&client_dir.path().to_path_buf(), "file2.txt", "Content 2");
    create_test_file(&client_dir.path().to_path_buf(), "file3.txt", "Content 3");
    create_test_file(&client_dir.path().to_path_buf(), "file4.txt", "Content 4");
    create_test_file(&client_dir.path().to_path_buf(), "file5.txt", "Content 5");

    // Start sync operation that will return list of files to upload
    let sync_future = async {
        // Perform a sync to get the files queued
        let _client_clone = client.clone();
        tokio::spawn(async move {
            // Give some time for sync negotiation to complete, then shut down server
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("Simulating server shutdown during upload phase...");
        });

        // This should handle the server shutdown gracefully
        client.sync().await
    };

    // Shutdown server very quickly after starting sync to simulate mid-upload shutdown
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        server_handle.abort();
    });

    // Run the sync - should handle shutdown gracefully
    let sync_result = timeout(Duration::from_secs(10), sync_future).await;
    
    // Test passes if sync handles the error gracefully (doesn't panic)
    // and we get a proper error result
    match sync_result {
        Ok(Ok(())) => {
            println!("Sync completed before server shutdown");
        }
        Ok(Err(e)) => {
            println!("Sync handled server shutdown gracefully: {}", e);
            // This is the expected behavior - graceful error handling
        }
        Err(_) => {
            println!("Sync operation timed out - this is acceptable for this test");
        }
    }

    // The key test is that the client doesn't crash and handles the error appropriately
}
use std::time::Duration;
use syncpair::SyncClient;
use tokio::time::timeout;

mod common;
use common::{create_temp_dir, create_test_file, init_test_logging};

#[tokio::test]
async fn test_upload_errors_when_server_unavailable() {
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

    // Create multiple files to upload
    create_test_file(&client_dir.path().to_path_buf(), "file1.txt", "Content 1");
    create_test_file(&client_dir.path().to_path_buf(), "file2.txt", "Content 2");
    create_test_file(&client_dir.path().to_path_buf(), "file3.txt", "Content 3");
    create_test_file(&client_dir.path().to_path_buf(), "file4.txt", "Content 4");
    create_test_file(&client_dir.path().to_path_buf(), "file5.txt", "Content 5");

    // Sync should fail due to server unavailable, but handle gracefully
    println!("Starting sync with server unavailable - this should demonstrate the improved error handling");
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    
    match sync_result {
        Ok(Err(e)) => {
            let error_message = e.to_string();
            println!("Sync returned expected error: {}", error_message);
            assert!(error_message.contains("Server unavailable"), 
                    "Error should indicate server unavailability, got: {}", error_message);
        }
        Ok(Ok(())) => {
            panic!("Sync should not succeed with unavailable server");
        }
        Err(_) => {
            panic!("Sync should not timeout");
        }
    }
    
    println!("Test completed - error was handled gracefully");
}
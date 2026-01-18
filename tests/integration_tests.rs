use anyhow::Result;
use std::path::PathBuf;
use std::time::Duration;
use syncpair::client::SimpleClient;
use syncpair::server::SimpleServer;

use tokio::time::sleep;

#[path = "common/mod.rs"]
mod common;

// Test helpers
async fn setup_server(port: u16, storage_dir: PathBuf) -> Result<()> {
    // Spawn server in background
    tokio::spawn(async move {
        let server = SimpleServer::new(storage_dir).unwrap();
        if let Err(e) = server.start(port).await {
            // It's expected that the server might get cancelled or fail to bind if port is taken
            // but for tests we want to see why
            eprintln!("Server error: {}", e);
        }
    });

    // Give server a moment to start
    sleep(Duration::from_millis(100)).await;
    Ok(())
}

fn setup_client(server_url: String, watch_dir: PathBuf, dir_name: String) -> SimpleClient {
    SimpleClient::new(server_url, watch_dir)
        .with_directory(dir_name)
        .with_sync_interval(Duration::from_millis(100)) // Fast sync for tests
}



#[tokio::test]
async fn test_basic_sync() -> Result<()> {
    common::init_test_logging();
    
    // Setup environment
    let temp_dir = common::create_temp_dir()?;
    let server_dir = temp_dir.path().join("server_storage");
    let client_a_dir = temp_dir.path().join("client_a");
    let client_b_dir = temp_dir.path().join("client_b");
    
    std::fs::create_dir_all(&server_dir)?;
    std::fs::create_dir_all(&client_a_dir)?;
    std::fs::create_dir_all(&client_b_dir)?;

    // Start server
    let port = 9001; // Use different port than default to avoid conflicts
    setup_server(port, server_dir).await?;
    let server_url = format!("http://localhost:{}", port);
    let shared_dir = "test_project".to_string();

    // Create clients
    let client_a = setup_client(server_url.clone(), client_a_dir.clone(), shared_dir.clone())
        .with_client_id("client_a".to_string());
        
    let client_b = setup_client(server_url.clone(), client_b_dir.clone(), shared_dir.clone())
        .with_client_id("client_b".to_string());

    // START TEST SCENARIO
    
    // 1. Client A creates a file
    let file_name = "hello.txt";
    let content = "Hello, World!";
    std::fs::write(client_a_dir.join(file_name), content)?;
    
    // Perform syncs
    // A uploads
    client_a.initial_sync().await?;
    
    // B downloads
    client_b.initial_sync().await?;
    
    // Verify file exists on B
    let synced_file = client_b_dir.join(file_name);
    assert!(synced_file.exists(), "File should be synced to client B");
    assert_eq!(std::fs::read_to_string(synced_file)?, content, "Content should match");

    Ok(())
}

#[tokio::test]
async fn test_conflict_resolution() -> Result<()> {
    common::init_test_logging();
    
    let temp_dir = common::create_temp_dir()?;
    let server_dir = temp_dir.path().join("server_storage");
    let client_a_dir = temp_dir.path().join("client_a");
    let client_b_dir = temp_dir.path().join("client_b");
    
    std::fs::create_dir_all(&server_dir)?;
    std::fs::create_dir_all(&client_a_dir)?;
    std::fs::create_dir_all(&client_b_dir)?;

    let port = 9002;
    setup_server(port, server_dir).await?;
    let server_url = format!("http://localhost:{}", port);
    let shared_dir = "conflict_project".to_string();

    let client_a = setup_client(server_url.clone(), client_a_dir.clone(), shared_dir.clone())
        .with_client_id("client_a".to_string());
    let client_b = setup_client(server_url.clone(), client_b_dir.clone(), shared_dir.clone())
        .with_client_id("client_b".to_string());

    // 1. Create initial file
    let file_name = "conflict.txt";
    std::fs::write(client_a_dir.join(file_name), "Initial content")?;
    client_a.initial_sync().await?;
    client_b.initial_sync().await?;

    // 2. Create conflict
    // Modify on A (older timestamp)
    sleep(Duration::from_millis(100)).await;
    std::fs::write(client_a_dir.join(file_name), "Content from A")?;
    
    // Modify on B (newer timestamp)
    sleep(Duration::from_millis(1000)).await; // Ensure distinct timestamp
    std::fs::write(client_b_dir.join(file_name), "Content from B")?;
    
    // Sync A to server first (A wins temporarily)
    client_a.initial_sync().await?;
    
    // Sync B to server (B should win because it's newer)
    client_b.initial_sync().await?;
    
    // Sync A again (should download B's version)
    client_a.initial_sync().await?;

    // Verify both have B's content
    assert_eq!(std::fs::read_to_string(client_a_dir.join(file_name))?, "Content from B");
    assert_eq!(std::fs::read_to_string(client_b_dir.join(file_name))?, "Content from B");

    Ok(())
}

#[tokio::test]
async fn test_deletion_propagation() -> Result<()> {
    common::init_test_logging();
    
    let temp_dir = common::create_temp_dir()?;
    let server_dir = temp_dir.path().join("server_storage");
    let client_a_dir = temp_dir.path().join("client_a");
    let client_b_dir = temp_dir.path().join("client_b");
    
    std::fs::create_dir_all(&server_dir)?;
    std::fs::create_dir_all(&client_a_dir)?;
    std::fs::create_dir_all(&client_b_dir)?;

    let port = 9003;
    setup_server(port, server_dir).await?;
    let server_url = format!("http://localhost:{}", port);
    let shared_dir = "deletion_project".to_string();

    let client_a = setup_client(server_url.clone(), client_a_dir.clone(), shared_dir.clone())
        .with_client_id("client_a".to_string());
    let client_b = setup_client(server_url.clone(), client_b_dir.clone(), shared_dir.clone())
        .with_client_id("client_b".to_string());

    // 1. Create and sync file
    let file_name = "to_delete.txt";
    std::fs::write(client_a_dir.join(file_name), "Delete me")?;
    client_a.initial_sync().await?;
    client_b.initial_sync().await?;
    
    assert!(client_b_dir.join(file_name).exists());

    // 2. Delete on A
    std::fs::remove_file(client_a_dir.join(file_name))?;
    
    // Sync A (propagates delete to server)
    client_a.initial_sync().await?;
    
    // Sync B (propagates delete to B)
    client_b.initial_sync().await?;

    // Verify deletion
    assert!(!client_b_dir.join(file_name).exists(), "File should be deleted on Client B");

    Ok(())
}

#[tokio::test]
async fn test_delta_sync() -> Result<()> {
    common::init_test_logging();
    
    let temp_dir = common::create_temp_dir()?;
    let server_dir = temp_dir.path().join("server_storage");
    let client_a_dir = temp_dir.path().join("client_a");
    
    std::fs::create_dir_all(&server_dir)?;
    std::fs::create_dir_all(&client_a_dir)?;

    let port = 9004;
    setup_server(port, server_dir.clone()).await?;
    let server_url = format!("http://localhost:{}", port);
    let shared_dir = "delta_project".to_string();

    let client_a = setup_client(server_url.clone(), client_a_dir.clone(), shared_dir.clone())
        .with_client_id("client_a".to_string());

    // 1. Create large file (> 1MB)
    let file_name = "large_file.bin";
    // 2MB + 100 bytes
    let size = 2 * 1024 * 1024 + 100;
    let mut content = vec![0u8; size];
    // Fill with some data
    for i in 0..size {
        content[i] = (i % 256) as u8;
    }
    
    std::fs::write(client_a_dir.join(file_name), &content)?;
    
    // Initial sync (Full upload)
    client_a.initial_sync().await?;
    
    // Verify server has it
    let server_file = server_dir.join(shared_dir.clone()).join(file_name);
    assert!(server_file.exists());
    assert_eq!(server_file.metadata()?.len() as usize, size);
    
    // 2. Modify middle of second block (index 1)
    // Block size is 1MB. Middle of 2nd block is around 1.5MB offset.
    let modify_offset = 1024 * 1024 + 500_000;
    content[modify_offset] = 255; // Change a byte
    std::fs::write(client_a_dir.join(file_name), &content)?;
    
    // Delta Sync
    client_a.initial_sync().await?;
    
    // Verify server content
    let server_content = std::fs::read(server_file)?;
    assert_eq!(server_content, content, "Server content should match client content after delta sync");

    Ok(())
}

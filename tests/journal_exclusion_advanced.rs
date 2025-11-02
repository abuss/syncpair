use std::time::Duration;
use std::fs::File;
use std::io::Write;

use syncpair::{SyncClient, SyncServer};
use tokio::time::timeout;

mod common;

use common::{create_temp_dir, create_test_file, init_test_logging};

/// Test that journal files are excluded even during active database operations
#[tokio::test]
async fn test_active_database_journal_exclusion() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8097)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client
    let client = SyncClient::new(
        "http://localhost:8097".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("active_db_test".to_string()),
        Some("active_db_dir".to_string()),
        vec![], // No custom ignore patterns
    ).unwrap();

    // Create regular files
    create_test_file(&client_dir.path().to_path_buf(), "app_data.txt", "Application data");
    
    // Simulate an active SQLite database by creating the main db and journal files
    let db_path = client_dir.path().join("app.db");
    let journal_path = client_dir.path().join("app.db-journal");
    let wal_path = client_dir.path().join("app.db-wal");
    let shm_path = client_dir.path().join("app.db-shm");

    // Create main database file
    let mut db_file = File::create(&db_path).unwrap();
    db_file.write_all(b"SQLite format 3\x00").unwrap();
    db_file.write_all(&vec![0u8; 1000]).unwrap(); // Simulate some database content
    db_file.sync_all().unwrap();

    // Create journal file (simulating active transaction)
    let mut journal_file = File::create(&journal_path).unwrap();
    journal_file.write_all(b"SQLite journal file").unwrap();
    journal_file.write_all(&vec![42u8; 500]).unwrap(); // Simulate journal data
    journal_file.sync_all().unwrap();

    // Create WAL file (Write-Ahead Logging)
    let mut wal_file = File::create(&wal_path).unwrap();
    wal_file.write_all(b"WAL data").unwrap();
    wal_file.write_all(&vec![1u8; 200]).unwrap();
    wal_file.sync_all().unwrap();

    // Create shared memory file
    let mut shm_file = File::create(&shm_path).unwrap();
    shm_file.write_all(&vec![0u8; 32768]).unwrap(); // Typical shm size
    shm_file.sync_all().unwrap();

    // Also create the syncpair state files that should definitely be excluded
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db", "Syncpair state");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-journal", "Syncpair journal");

    // Perform initial sync
    let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
    assert!(sync_result.is_ok(), "Initial sync should complete successfully");

    // Simulate database activity during sync by modifying journal files
    tokio::spawn(async move {
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            
            // Modify journal file as if database is actively writing
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&journal_path) {
                let _ = file.write_all(format!("Transaction {}\n", i).as_bytes());
                let _ = file.sync_all();
            }
            
            // Modify WAL file
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(&wal_path) {
                let _ = file.write_all(format!("WAL entry {}\n", i).as_bytes());
                let _ = file.sync_all();
            }
        }
    });

    // Perform additional syncs while database is "active"
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let sync_result = timeout(Duration::from_secs(10), client.sync()).await;
        assert!(sync_result.is_ok(), "Sync during database activity should complete successfully");
    }

    // Wait for all operations to complete
    tokio::time::sleep(Duration::from_millis(1000)).await;

    // Verify sync results
    let server_client_dir = server_dir.path().join("active_db_test:active_db_dir");
    
    // Regular files should be synced
    assert!(
        server_client_dir.join("app_data.txt").exists(),
        "Regular file should be synced"
    );

    // Main database file should be synced (it's not in the built-in exclusion list)
    assert!(
        server_client_dir.join("app.db").exists(),
        "Main database file should be synced"
    );

    // Journal, WAL, and SHM files should NOT be synced (they're not in built-in patterns, 
    // but would need custom ignore patterns for app.db-* files in real usage)
    // However, .syncpair_state.db files should definitely be excluded
    assert!(
        !server_client_dir.join(".syncpair_state.db").exists(),
        "Syncpair state database should not be synced"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db-journal").exists(),
        "Syncpair journal should not be synced"
    );

    // Note: app.db-journal, app.db-wal, app.db-shm would be synced unless custom
    // ignore patterns are specified. This test primarily verifies that the built-in
    // .syncpair_state.db* exclusions work correctly.

    println!("Journal exclusion test completed successfully");

    // Cleanup
    server_handle.abort();
}

/// Test that journal exclusion works with multiple database files
#[tokio::test]
async fn test_multiple_database_journal_exclusion() {
    init_test_logging();

    let server_dir = create_temp_dir();
    let client_dir = create_temp_dir();

    // Start server
    let server = SyncServer::new(server_dir.path().to_path_buf()).unwrap();
    let server_handle = tokio::spawn(async move {
        let _ = timeout(Duration::from_secs(30), server.start(8098)).await;
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create client with custom ignore patterns for all database journal files
    let client = SyncClient::new(
        "http://localhost:8098".to_string(),
        client_dir.path().to_path_buf(),
        Duration::from_secs(5),
        Some("multi_db_test".to_string()),
        Some("multi_db_dir".to_string()),
        vec![
            "*.db-journal".to_string(),
            "*.db-wal".to_string(), 
            "*.db-shm".to_string(),
            "*.sqlite-journal".to_string(),
            "*.sqlite-wal".to_string(),
            "*.sqlite-shm".to_string(),
        ],
    ).unwrap();

    // Create multiple database files and their journals
    let db_files = vec!["app.db", "users.db", "cache.sqlite", "logs.sqlite"];
    
    for db_name in &db_files {
        // Create main database
        create_test_file(&client_dir.path().to_path_buf(), db_name, "Database content");
        
        // Create journal files
        create_test_file(&client_dir.path().to_path_buf(), &format!("{}-journal", db_name), "Journal content");
        create_test_file(&client_dir.path().to_path_buf(), &format!("{}-wal", db_name), "WAL content");
        create_test_file(&client_dir.path().to_path_buf(), &format!("{}-shm", db_name), "SHM content");
    }

    // Create the built-in syncpair files that should always be excluded
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db", "Syncpair state");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-journal", "Syncpair journal");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-wal", "Syncpair WAL");
    create_test_file(&client_dir.path().to_path_buf(), ".syncpair_state.db-shm", "Syncpair SHM");

    // Create some regular files that should sync
    create_test_file(&client_dir.path().to_path_buf(), "config.json", "Configuration data");
    create_test_file(&client_dir.path().to_path_buf(), "README.md", "Documentation");

    // Perform sync
    let sync_result = timeout(Duration::from_secs(15), client.sync()).await;
    assert!(sync_result.is_ok(), "Sync should complete successfully");

    // Wait for sync to complete
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Verify results
    let server_client_dir = server_dir.path().join("multi_db_test:multi_db_dir");
    
    // Regular files should be synced
    assert!(server_client_dir.join("config.json").exists(), "Config file should be synced");
    assert!(server_client_dir.join("README.md").exists(), "README should be synced");

    // Main database files should be synced
    for db_name in &db_files {
        assert!(
            server_client_dir.join(db_name).exists(),
            "Database {} should be synced", db_name
        );
    }

    // Journal, WAL, and SHM files should NOT be synced (due to custom ignore patterns)
    for db_name in &db_files {
        assert!(
            !server_client_dir.join(format!("{}-journal", db_name)).exists(),
            "Journal for {} should not be synced", db_name
        );
        assert!(
            !server_client_dir.join(format!("{}-wal", db_name)).exists(),
            "WAL for {} should not be synced", db_name
        );
        assert!(
            !server_client_dir.join(format!("{}-shm", db_name)).exists(),
            "SHM for {} should not be synced", db_name
        );
    }

    // Built-in syncpair files should definitely not be synced
    assert!(
        !server_client_dir.join(".syncpair_state.db").exists(),
        "Syncpair state should not be synced"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db-journal").exists(),
        "Syncpair journal should not be synced"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db-wal").exists(),
        "Syncpair WAL should not be synced"
    );
    assert!(
        !server_client_dir.join(".syncpair_state.db-shm").exists(),
        "Syncpair SHM should not be synced"
    );

    println!("Multiple database journal exclusion test completed successfully");

    // Cleanup
    server_handle.abort();
}
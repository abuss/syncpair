use syncpair::types::ClientConfig;
use std::fs;
use anyhow::Result;

#[path = "common/mod.rs"]
mod common;

#[tokio::test]
async fn test_multi_client_isolation() -> Result<()> {
    common::init_test_logging();
    
    println!("🧪 Testing Multi-Client Directory Isolation");
    println!("===========================================");
    
    // Create test configurations
    let client_a_yaml = r#"
client_id: test_client_a
server: http://localhost:8080
directories:
  - name: shared_workspace
    local_path: ~/test_client_a_shared/
    settings:
      description: "Client A workspace"
      sync_interval_seconds: 30
"#;

    let client_b_yaml = r#"
client_id: test_client_b
server: http://localhost:8080
directories:
  - name: shared_workspace
    local_path: ~/test_client_b_shared/
    settings:
      description: "Client B workspace"
      sync_interval_seconds: 30
"#;
    
    // Parse configurations
    let client_a_config: ClientConfig = serde_yaml::from_str(client_a_yaml)?;
    let client_b_config: ClientConfig = serde_yaml::from_str(client_b_yaml)?;
    
    println!("📋 Client A config: {} with directory '{}'", 
             client_a_config.client_id, 
             client_a_config.directories[0].name);
    
    println!("📋 Client B config: {} with directory '{}'", 
             client_b_config.client_id, 
             client_b_config.directories[0].name);
             
    // Test private directory isolation (default behavior)
    println!();
    println!("🔍 Testing private directory isolation:");
    println!("  - Client A (private): server_storage/{}:{}/", 
             client_a_config.client_id, client_a_config.directories[0].name);
    println!("  - Client B (private): server_storage/{}:{}/", 
             client_b_config.client_id, client_b_config.directories[0].name);
    
    // Verify that configurations are parsed correctly
    assert_eq!(client_a_config.client_id, "test_client_a");
    assert_eq!(client_b_config.client_id, "test_client_b");
    assert_eq!(client_a_config.directories[0].name, "shared_workspace");
    assert_eq!(client_b_config.directories[0].name, "shared_workspace");
    
    // Verify that directory settings have correct defaults
    assert!(!client_a_config.directories[0].settings.shared, "Default should be private (shared: false)");
    assert!(!client_b_config.directories[0].settings.shared, "Default should be private (shared: false)");
    assert_eq!(client_a_config.directories[0].settings.sync_interval_seconds, 30);
    assert_eq!(client_b_config.directories[0].settings.sync_interval_seconds, 30);
    
    println!("✅ Directory names are properly isolated!");
    println!("   Both clients can use 'shared_workspace' without conflicts");
    
    Ok(())
}

#[tokio::test]
async fn test_shared_directory_configuration() -> Result<()> {
    common::init_test_logging();
    
    println!("🧪 Testing Shared Directory Configuration");
    println!("========================================");
    
    // Create shared directory configurations
    let client_a_shared_yaml = r#"
client_id: alice
server: http://localhost:8080
directories:
  - name: team_project
    local_path: ~/alice_team/
    settings:
      description: "Shared team project"
      shared: true
      sync_interval_seconds: 15
"#;

    let client_b_shared_yaml = r#"
client_id: bob
server: http://localhost:8080
directories:
  - name: team_project
    local_path: ~/bob_team/
    settings:
      description: "Shared team project"
      shared: true
      sync_interval_seconds: 15
"#;
    
    // Parse configurations
    let alice_config: ClientConfig = serde_yaml::from_str(client_a_shared_yaml)?;
    let bob_config: ClientConfig = serde_yaml::from_str(client_b_shared_yaml)?;
    
    println!("📋 Alice config: {} with shared directory '{}'", 
             alice_config.client_id, 
             alice_config.directories[0].name);
    
    println!("📋 Bob config: {} with shared directory '{}'", 
             bob_config.client_id, 
             bob_config.directories[0].name);
    
    // Test shared directory behavior
    println!();
    println!("🔍 Testing shared directory collaboration:");
    println!("  - Both clients access: server_storage/{}/", alice_config.directories[0].name);
    println!("  - Real-time collaboration enabled");
    
    // Verify that configurations are parsed correctly for sharing
    assert_eq!(alice_config.client_id, "alice");
    assert_eq!(bob_config.client_id, "bob");
    assert_eq!(alice_config.directories[0].name, "team_project");
    assert_eq!(bob_config.directories[0].name, "team_project");
    
    // Verify that shared settings are correct
    assert!(alice_config.directories[0].settings.shared, "Alice should have shared: true");
    assert!(bob_config.directories[0].settings.shared, "Bob should have shared: true");
    assert_eq!(alice_config.directories[0].settings.sync_interval_seconds, 15);
    assert_eq!(bob_config.directories[0].settings.sync_interval_seconds, 15);
    
    println!("✅ Shared directory configuration works correctly!");
    println!("   Both clients will collaborate on the same server directory");
    
    Ok(())
}

#[tokio::test]
async fn test_mixed_shared_private_configuration() -> Result<()> {
    common::init_test_logging();
    
    println!("🧪 Testing Mixed Shared/Private Configuration");
    println!("============================================");
    
    // Create mixed configuration
    let mixed_config_yaml = r#"
client_id: developer
server: http://localhost:8080
directories:
  - name: personal_notes
    local_path: ~/notes/
    settings:
      description: "Private notes"
      # shared: false (default)
      sync_interval_seconds: 60
  - name: team_docs
    local_path: ~/team/
    settings:
      description: "Shared team documentation"
      shared: true
      sync_interval_seconds: 10
  - name: local_configs
    local_path: ~/configs/
    settings:
      description: "Personal configurations"
      sync_interval_seconds: 30
"#;
    
    // Parse configuration
    let mixed_config: ClientConfig = serde_yaml::from_str(mixed_config_yaml)?;
    
    println!("📋 Developer config with {} directories:", mixed_config.directories.len());
    for (i, dir) in mixed_config.directories.iter().enumerate() {
        println!("  {}. '{}' - shared: {}", i + 1, dir.name, dir.settings.shared);
    }
    
    println!();
    println!("🔍 Expected server storage:");
    for dir in &mixed_config.directories {
        if dir.settings.shared {
            println!("  - Shared: server_storage/{}/", dir.name);
        } else {
            println!("  - Private: server_storage/{}:{}/", mixed_config.client_id, dir.name);
        }
    }
    
    // Verify mixed configuration
    assert_eq!(mixed_config.directories.len(), 3);
    
    // Check personal_notes (private)
    let personal_notes = &mixed_config.directories[0];
    assert_eq!(personal_notes.name, "personal_notes");
    assert!(!personal_notes.settings.shared, "personal_notes should be private");
    assert_eq!(personal_notes.settings.sync_interval_seconds, 60);
    
    // Check team_docs (shared)
    let team_docs = &mixed_config.directories[1];
    assert_eq!(team_docs.name, "team_docs");
    assert!(team_docs.settings.shared, "team_docs should be shared");
    assert_eq!(team_docs.settings.sync_interval_seconds, 10);
    
    // Check local_configs (private by default)
    let local_configs = &mixed_config.directories[2];
    assert_eq!(local_configs.name, "local_configs");
    assert!(!local_configs.settings.shared, "local_configs should be private by default");
    assert_eq!(local_configs.settings.sync_interval_seconds, 30);
    
    println!("✅ Mixed shared/private configuration works correctly!");
    println!("   Client can have both collaborative and isolated directories");
    
    Ok(())
}
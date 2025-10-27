# SyncPair - Advanced File Synchronization Tool

SyncPair is a lightweight Rust-based file synchronization tool that enables **bidirectional sync** between client directories and a central server with **shared directory storage**. Multiple clients can collaborate on the same shared directories in real-time. The system uses SHA-256 hashes for change detection, timestamp-based conflict resolution, and real-time filesystem watching for automatic synchronization.

## Features

- **Bidirectional synchronization**: Full two-way sync between clients and server
- **Timestamp-based conflict resolution**: Newer files automatically win conflicts
- **File deletion synchronization**: Deletions propagate between clients and server
- **Hash-based change detection**: Uses SHA-256 hashes to identify file changes efficiently
- **Real-time file watching**: Monitors filesystem changes and syncs automatically using `notify`
- **Connection resilience**: Automatic retry with exponential backoff when server unavailable
- **Comprehensive logging**: Professional logging system with multiple verbosity levels
- **Relative path preservation**: Maintains directory structure across all clients
- **Async HTTP communication**: Built with Tokio and Warp for performance
- **Single unified binary**: One executable with subcommands for both client and server modes
- **Cross-platform**: Written in Rust, works on Linux, macOS, and Windows

## Architecture

SyncPair uses an advanced bidirectional synchronization architecture:

1. **Server Mode**: HTTP server that handles file uploads, downloads, and sync coordination via REST API
2. **Client Mode**: Filesystem watcher with intelligent sync engine that handles conflicts and deletions
3. **Sync Protocol**: RESTful API with endpoints for sync negotiation, uploads, downloads, and deletions

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd syncpair

# Build the project
cargo build --release

# The binary will be available at target/release/syncpair
```

## Usage

### Command Line Options

SyncPair provides comprehensive logging and configuration options:

```bash
# Global options (available for all commands)
--log-level <LEVEL>    # Set log verbosity: error, warn, info, debug, trace (default: info)
--log-file <FILE>      # Write logs to file instead of stdout
--quiet                # Quiet mode - only show errors

# Commands
server                 # Start the server to receive file uploads
client                 # Start single-directory client to watch and sync files  
config --file <FILE>   # Start multi-directory client using YAML configuration

# Examples
./syncpair --log-level debug server --port 8080
./syncpair --quiet client --server http://localhost:8080
./syncpair --log-file sync.log config --file multi-dirs.yaml
```

### Starting the Server

```bash
# Start server on default port (8080) with default storage directory
./target/release/syncpair server

# Start server on custom port with custom storage directory
./target/release/syncpair server --port 9000 --storage-dir /path/to/server/storage

# With detailed logging
./target/release/syncpair --log-level debug server --port 8080 --storage-dir ./server_files
```

### Running the Client

```bash
# Start client to watch and sync a directory (default: ./sync_dir to localhost:8080)
./target/release/syncpair client

# Watch custom directory and sync to custom server
./target/release/syncpair client --server http://server:8080 --dir /path/to/sync/directory

# With custom sync interval (default: 30 seconds)
./target/release/syncpair client --sync-interval 60 --dir ./documents

# With quiet logging (production mode)
./target/release/syncpair --quiet client --server http://production-server:8080
```

### Multi-Directory Configuration

For advanced users managing multiple directories:

```bash
# Start multi-directory sync using YAML configuration
./target/release/syncpair config --file config.yaml

# With detailed logging for debugging
./target/release/syncpair --log-level debug config --file config.yaml

# Production multi-directory setup
./target/release/syncpair --quiet --log-file client.log config --file production-config.yaml
```

## How It Works

### Bidirectional Sync Process

1. **Server Setup**: The server starts and provides REST API endpoints for sync operations
2. **Client Connection**: Client connects with automatic retry (exponential backoff: 1s, 2s, 4s, 8s, 16s)
3. **Initial Sync**: Client and server exchange file lists and perform bidirectional synchronization
4. **Periodic Sync**: Regular sync cycles ensure consistency (configurable interval, default 30s)
5. **Real-time Watching**: Client monitors directory for changes and syncs immediately
6. **Conflict Resolution**: Timestamp-based resolution - newer files automatically win
7. **Deletion Propagation**: Deletions sync between all clients through the server

### Advanced Features

#### Conflict Resolution
When the same file is modified on multiple clients:
- **Timestamp comparison**: File with newer modification time wins
- **Automatic resolution**: No manual intervention required
- **Hash verification**: Ensures data integrity during resolution
- **Detailed logging**: All conflict decisions are logged for audit trails

#### Connection Resilience
- **Automatic retry**: Client retries failed connections up to 5 times
- **Exponential backoff**: Delays increase: 1s → 2s → 4s → 8s → 16s (max 30s)
- **Graceful degradation**: Client continues operating when server temporarily unavailable
- **Connection recovery**: Automatic resumption when server becomes available

#### Deletion Synchronization
- **Bidirectional deletion**: Deletions on any client propagate to all others
- **Timestamp tracking**: Deletion times prevent resurrection of deleted files
- **Conflict handling**: Files modified after deletion time are preserved

### File States

- **Client State**: Tracks local files and deletion history with `.syncpair_state.json` in the watch directory
- **Server State**: Maintains synchronized files and global deletion history with `server_state.json`
- **Change Detection**: Compares file hashes and modification timestamps to determine sync actions
- **Deletion Tracking**: Timestamp-based deleted file tracking prevents conflicts and resurrection

## Logging System

SyncPair provides a comprehensive logging system suitable for both development and production use:

### Log Levels

- **`error`**: Critical failures, connection errors, file operation failures
- **`warn`**: Conflicts, retries, ignored operations, stale entries  
- **`info`**: Major operations, startup/shutdown, conflict resolutions (default)
- **`debug`**: Detailed sync activities, file operations, detailed progress
- **`trace`**: Very detailed internal operations

### Logging Options

```bash
# Set log level
./syncpair --log-level debug client

# Write logs to file (no console output)
./syncpair --log-file sync.log server

# Quiet mode (errors only, perfect for production)
./syncpair --quiet client

# Examples
./syncpair --log-level info --log-file production.log server     # Production server
./syncpair --log-level debug client                              # Development debugging  
./syncpair --quiet --log-file client-prod.log client            # Production client
```

### Log Format

```
2025-10-25T08:17:44.470350Z  INFO syncpair::client: Starting bidirectional sync...
2025-10-25T08:17:44.471308Z  WARN syncpair::client: ⚠️  Conflict detected for file: document.txt
2025-10-25T08:17:44.472000Z DEBUG syncpair::client: ✓ Downloaded: document.txt
```

Each log entry includes:
- **Timestamp**: ISO 8601 format with microsecond precision
- **Level**: Color-coded log level (INFO, WARN, ERROR, DEBUG, TRACE)
- **Component**: Which part of the system generated the log (syncpair, syncpair::client, syncpair::server)
- **Message**: Human-readable description with context

## Technical Details

### Core Components

- **SyncServer**: Advanced HTTP server using Warp framework with comprehensive REST API
- **SyncClient**: Intelligent filesystem watcher with bidirectional sync, conflict resolution, and retry logic
- **File Verification**: SHA-256 hash calculation and comparison for integrity
- **State Management**: JSON-based persistence with atomic writes for reliability
- **Conflict Resolution**: Timestamp-based automatic conflict resolution system
- **Connection Management**: Resilient HTTP client with exponential backoff retry logic

### Communication Protocol

RESTful HTTP API with JSON payloads:

- `POST /sync`: Bidirectional sync negotiation with conflict detection
  - Request: `SyncRequest` with client files and deleted files
  - Response: `SyncResponse` with files to upload/download/delete and conflicts
- `POST /upload`: Upload file with metadata (path, hash, content, timestamp)
- `GET /download/{path}`: Download file by path with integrity verification
- `DELETE /delete/{path}`: Delete file from server storage and update state

### Data Structures

```rust
// Sync request from client to server
struct SyncRequest {
    files: HashMap<String, FileInfo>,           // Current client files
    deleted_files: HashMap<String, DateTime>,   // Files deleted by client
    last_sync: Option<DateTime>,                // Last successful sync time
}

// Sync response from server to client  
struct SyncResponse {
    files_to_upload: Vec<String>,              // Files client should upload
    files_to_download: Vec<FileInfo>,          // Files client should download
    files_to_delete: Vec<String>,              // Files client should delete
    conflicts: Vec<FileConflict>,              // Conflicts requiring resolution
}

// File metadata with timestamp
struct FileInfo {
    path: String,                              // Relative path
    hash: String,                              // SHA-256 hash
    modified: DateTime<Utc>,                   // Modification timestamp
    size: u64,                                 // File size in bytes
}
```

### Dependencies

Comprehensive dependency set for advanced functionality:
- `tokio` - Async runtime and utilities
- `warp` - HTTP server framework with filtering
- `notify` - Cross-platform filesystem watching
- `reqwest` - Feature-rich HTTP client with retry support
- `serde/serde_json` - Serialization and deserialization
- `sha2` - Cryptographic hash calculation
- `clap` - Command line argument parsing
- `anyhow/thiserror` - Error handling and propagation
- `chrono` - Date and time manipulation with timezone support
- `walkdir` - Recursive directory traversal
- `urlencoding` - URL-safe path encoding
- `log/env_logger` - Traditional logging interface
- `tracing/tracing-subscriber` - Structured, async-aware logging

## Project Structure

```
src/
├── lib.rs          # Module exports
├── main.rs         # Unified binary with CLI
├── types.rs        # Data structures (FileInfo, UploadRequest, etc.)
├── utils.rs        # Utility functions (hashing, state management)
├── client.rs       # SimpleClient implementation
└── server.rs       # SimpleServer implementation
```

## Development

### Building

```bash
cargo build                    # Debug build
cargo build --release          # Release build
```

### Testing

```bash
cargo test                     # Run tests
cargo clippy                   # Linting
```

### Example Usage

#### Production Deployment

```bash
# Production Server (with logging to file)
./target/release/syncpair --quiet --log-file /var/log/syncpair-server.log \
    server --port 8080 --storage-dir /data/syncpair

# Production Client (quiet mode with error-only logging)  
./target/release/syncpair --quiet --log-file /var/log/syncpair-client.log \
    client --server http://sync-server:8080 --dir /home/user/documents

# Development setup with detailed logging
./target/release/syncpair --log-level debug \
    server --port 8080 --storage-dir ./test-server

./target/release/syncpair --log-level debug \
    client --server http://localhost:8080 --dir ./test-client
```

#### Multi-Client Scenario

```bash
# Terminal 1: Start server
./target/release/syncpair --log-level info server --port 8080 --storage-dir ./shared

# Terminal 2: Client 1 (Alice's files)
./target/release/syncpair client --server http://localhost:8080 --dir ./alice-files

# Terminal 3: Client 2 (Bob's files)  
./target/release/syncpair client --server http://localhost:8080 --dir ./bob-files

# Now files sync bidirectionally between Alice and Bob through the server
# Conflicts are automatically resolved using timestamps
# Deletions propagate between all clients
```

## Multi-Directory Client Support

SyncPair now supports **multi-directory synchronization** through YAML configuration files, allowing a single client to sync multiple directories with complete isolation.

### Multi-Directory Configuration

Create a YAML configuration file to define multiple directories:

```yaml
client_id: my_unique_client
server: http://localhost:8080

directories:
  - name: documents
    local_path: ~/Documents/
    settings:
      description: "Personal documents"
      sync_interval: 30
  - name: projects
    local_path: ~/Projects/
    settings:
      description: "Development projects" 
      sync_interval: 60
```

### Running Multi-Directory Client

```bash
# Start multi-directory sync using configuration file
./target/release/syncpair config --file multi-sync.yaml

# With custom logging
./target/release/syncpair --log-level debug config --file multi-sync.yaml
```

### Multi-Client Server Architecture

The server now provides **complete client isolation**:

- **Client Namespacing**: Each `client_id` gets isolated storage on the server
- **Directory Separation**: Each directory within a client gets its own namespace (`client_id:directory_name`)
- **Independent State**: Each client-directory combination maintains separate sync state
- **Concurrent Support**: Multiple clients can sync simultaneously without interference

#### Server Storage Structure
```
server_storage/
├── client1:documents/          # Client1's documents directory
│   ├── file1.txt
│   └── server_state.json
├── client1:projects/           # Client1's projects directory  
│   ├── main.rs
│   └── server_state.json
├── client2:documents/          # Client2's documents directory
│   ├── different_file.txt
│   └── server_state.json
└── default/                    # Backward compatibility for single clients
    ├── legacy_file.txt
    └── server_state.json
```

### Benefits of Multi-Directory Support

- **Organized Sync**: Separate different types of content (documents, projects, media)
- **Independent Scheduling**: Each directory can have its own sync interval
- **Client Isolation**: Multiple users/devices can sync without conflicts
- **Scalable Architecture**: Server handles many clients and directories efficiently
- **Backward Compatibility**: Existing single-directory clients continue working

### Use Cases

#### Personal Multi-Device Sync
```yaml
client_id: john_laptop
server: http://home-server:8080

directories:
  - name: documents
    local_path: ~/Documents/
  - name: photos  
    local_path: ~/Pictures/
  - name: music
    local_path: ~/Music/
```

#### Team Development Environment
```yaml
client_id: dev_team_member_1
server: http://company-sync:8080

directories:
  - name: shared_docs
    local_path: ~/team-docs/
    settings:
      sync_interval: 15  # Frequent sync for collaboration
  - name: personal_notes
    local_path: ~/notes/
    settings: 
      sync_interval: 300  # Less frequent for personal files
```

## Current Capabilities

✅ **Implemented Features**:
- Bidirectional file synchronization between multiple clients
- **Multi-directory client support with YAML configuration**
- **Server-side client isolation and namespacing**
- **Concurrent multi-client operations**
- Timestamp-based conflict resolution (newer files win automatically)
- File deletion synchronization with timestamp tracking
- Connection resilience with exponential backoff retry (5 attempts)
- Comprehensive logging system with multiple verbosity levels
- File and console logging with professional formatting
- Real-time filesystem watching with immediate sync
- Hash-based integrity verification (SHA-256)
- Relative path preservation across all clients
- Atomic state persistence for reliability
- Cross-platform compatibility (Linux, macOS, Windows)
- Single binary with server and client modes

## Limitations & Design Decisions

- **No authentication**: Plain HTTP without security (suitable for trusted networks)
- **No encryption**: File content transferred in plain text (local network assumption)
- **Simple conflict resolution**: Timestamp-based only (newer always wins)
- **No partial file sync**: Complete file transfer for each change
- **No bandwidth throttling**: Full-speed transfers (LAN-optimized)
- **No file locking**: Relies on filesystem-level locking

## Future Enhancements

Potential improvements for advanced deployments:
- Authentication and authorization (JWT tokens, API keys)
- HTTPS/TLS encryption for secure transport
- Advanced conflict resolution strategies (user choice, merge strategies)
- Bandwidth throttling and rate limiting
- Resume capability for large files (chunked uploads)
- File compression during transport
- Delta sync for large files (binary diff)
- Web-based administration interface
- Metrics and monitoring endpoints
- Distributed server architecture (clustering)

## Production Considerations

### Recommended Setup

```bash
# Server (production)
./syncpair --quiet --log-file /var/log/syncpair-server.log \
         server --port 8080 --storage-dir /data/syncpair

# Client (production)  
./syncpair --quiet --log-file /var/log/syncpair-client.log \
         client --server http://sync-server:8080 --sync-interval 60
```

### Monitoring

- **Log files**: Monitor log files for errors and warnings
- **Storage space**: Ensure server storage directory has adequate space
- **Network connectivity**: Clients will retry automatically on connection loss
- **State files**: Back up `.syncpair_state.json` and `server_state.json` for disaster recovery

### Performance

- **Optimal for**: Small to medium files (documents, code, configs) on local networks
- **Network usage**: Efficient - only changed files are transferred
- **Memory usage**: Low memory footprint, suitable for embedded devices
- **CPU usage**: Minimal CPU usage, I/O bound operations

## License

[Add your license information here]
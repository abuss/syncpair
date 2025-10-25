# SyncIt - Advanced File Synchronization Tool

SyncIt is a lightweight Rust-based file synchronization tool that enables **bidirectional sync** between client directories and a central server. The system uses SHA-256 hashes for change detection, timestamp-based conflict resolution, and real-time filesystem watching for automatic synchronization.

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

SyncIt uses an advanced bidirectional synchronization architecture:

1. **Server Mode**: HTTP server that handles file uploads, downloads, and sync coordination via REST API
2. **Client Mode**: Filesystem watcher with intelligent sync engine that handles conflicts and deletions
3. **Sync Protocol**: RESTful API with endpoints for sync negotiation, uploads, downloads, and deletions

## Installation

```bash
# Clone the repository
git clone <repository-url>
cd syncit

# Build the project
cargo build --release

# The binary will be available at target/release/syncit
```

## Usage

### Command Line Options

SyncIt provides comprehensive logging and configuration options:

```bash
# Global options (available for both server and client)
--log-level <LEVEL>    # Set log verbosity: error, warn, info, debug, trace (default: info)
--log-file <FILE>      # Write logs to file instead of stdout
--quiet                # Quiet mode - only show errors

# Examples
./syncit --log-level debug server --port 8080
./syncit --quiet client --server http://localhost:8080
./syncit --log-file sync.log client --dir ./my-files
```

### Starting the Server

```bash
# Start server on default port (8080) with default storage directory
./target/release/syncit server

# Start server on custom port with custom storage directory
./target/release/syncit server --port 9000 --storage-dir /path/to/server/storage

# With detailed logging
./target/release/syncit --log-level debug server --port 8080 --storage-dir ./server_files
```

### Running the Client

```bash
# Start client to watch and sync a directory (default: ./sync_dir to localhost:8080)
./target/release/syncit client

# Watch custom directory and sync to custom server
./target/release/syncit client --server http://server:8080 --dir /path/to/sync/directory

# With custom sync interval (default: 30 seconds)
./target/release/syncit client --sync-interval 60 --dir ./documents

# With quiet logging (production mode)
./target/release/syncit --quiet client --server http://production-server:8080
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

- **Client State**: Tracks local files and deletion history with `.syncit_state.json` in the watch directory
- **Server State**: Maintains synchronized files and global deletion history with `server_state.json`
- **Change Detection**: Compares file hashes and modification timestamps to determine sync actions
- **Deletion Tracking**: Timestamp-based deleted file tracking prevents conflicts and resurrection

## Logging System

SyncIt provides a comprehensive logging system suitable for both development and production use:

### Log Levels

- **`error`**: Critical failures, connection errors, file operation failures
- **`warn`**: Conflicts, retries, ignored operations, stale entries  
- **`info`**: Major operations, startup/shutdown, conflict resolutions (default)
- **`debug`**: Detailed sync activities, file operations, detailed progress
- **`trace`**: Very detailed internal operations

### Logging Options

```bash
# Set log level
./syncit --log-level debug client

# Write logs to file (no console output)
./syncit --log-file sync.log server

# Quiet mode (errors only, perfect for production)
./syncit --quiet client

# Examples
./syncit --log-level info --log-file production.log server     # Production server
./syncit --log-level debug client                              # Development debugging  
./syncit --quiet --log-file client-prod.log client            # Production client
```

### Log Format

```
2025-10-25T08:17:44.470350Z  INFO syncit::client: Starting bidirectional sync...
2025-10-25T08:17:44.471308Z  WARN syncit::client: ⚠️  Conflict detected for file: document.txt
2025-10-25T08:17:44.472000Z DEBUG syncit::client: ✓ Downloaded: document.txt
```

Each log entry includes:
- **Timestamp**: ISO 8601 format with microsecond precision
- **Level**: Color-coded log level (INFO, WARN, ERROR, DEBUG, TRACE)
- **Component**: Which part of the system generated the log (syncit, syncit::client, syncit::server)
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
./target/release/syncit --quiet --log-file /var/log/syncit-server.log \
    server --port 8080 --storage-dir /data/syncit

# Production Client (quiet mode with error-only logging)  
./target/release/syncit --quiet --log-file /var/log/syncit-client.log \
    client --server http://sync-server:8080 --dir /home/user/documents

# Development setup with detailed logging
./target/release/syncit --log-level debug \
    server --port 8080 --storage-dir ./test-server

./target/release/syncit --log-level debug \
    client --server http://localhost:8080 --dir ./test-client
```

#### Multi-Client Scenario

```bash
# Terminal 1: Start server
./target/release/syncit --log-level info server --port 8080 --storage-dir ./shared

# Terminal 2: Client 1 (Alice's files)
./target/release/syncit client --server http://localhost:8080 --dir ./alice-files

# Terminal 3: Client 2 (Bob's files)  
./target/release/syncit client --server http://localhost:8080 --dir ./bob-files

# Now files sync bidirectionally between Alice and Bob through the server
# Conflicts are automatically resolved using timestamps
# Deletions propagate between all clients
```

## Current Capabilities

✅ **Implemented Features**:
- Bidirectional file synchronization between multiple clients
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
./syncit --quiet --log-file /var/log/syncit-server.log \
         server --port 8080 --storage-dir /data/syncit

# Client (production)  
./syncit --quiet --log-file /var/log/syncit-client.log \
         client --server http://sync-server:8080 --sync-interval 60
```

### Monitoring

- **Log files**: Monitor log files for errors and warnings
- **Storage space**: Ensure server storage directory has adequate space
- **Network connectivity**: Clients will retry automatically on connection loss
- **State files**: Back up `.syncit_state.json` and `server_state.json` for disaster recovery

### Performance

- **Optimal for**: Small to medium files (documents, code, configs) on local networks
- **Network usage**: Efficient - only changed files are transferred
- **Memory usage**: Low memory footprint, suitable for embedded devices
- **CPU usage**: Minimal CPU usage, I/O bound operations

## License

[Add your license information here]
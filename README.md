# SyncPair - Collaborative File Synchronization Tool

SyncPair is a lightweight Rust-based file synchronization tool that enables **real-time collaboration** through **shared directory storage**. Multiple clients can work together on the same shared directories, with automatic bidirectional synchronization, intelligent conflict resolution, and immediate change propagation. The system uses SHA-256 hashes for change detection, timestamp-based conflict resolution, and real-time filesystem watching for seamless collaboration.

## Features

- **Real-time collaboration**: Multiple clients share the same directories with instant synchronization
- **Shared & isolated directory support**: Mix shared collaborative directories with private client-specific directories
- **Smart exclude patterns**: Flexible file filtering using glob patterns (*.tmp, node_modules/, etc.)
- **Bidirectional synchronization**: Full two-way sync between all collaborating clients through the server
- **Timestamp-based conflict resolution**: Newer files automatically win conflicts across all clients
- **File deletion synchronization**: Deletions propagate between all collaborating clients and server
- **Hash-based change detection**: Uses SHA-256 hashes to identify file changes efficiently
- **Real-time file watching**: Monitors filesystem changes and syncs automatically using `notify`
- **Connection resilience**: Automatic retry with exponential backoff when server unavailable
- **Comprehensive logging**: Professional logging system with multiple verbosity levels
- **Relative path preservation**: Maintains directory structure across all collaborating clients
- **Async HTTP communication**: Built with Tokio and Warp for performance with 30-second timeouts
- **Single unified binary**: One executable with subcommands for both client and server modes
- **Cross-platform**: Written in Rust, works on Linux, macOS, and Windows

## Architecture

SyncPair uses a **hybrid shared/isolated directory architecture** that enables both real-time teamwork and private workspaces:

1. **Server Mode**: HTTP server with flexible directory storage - directories can be shared across clients or isolated per client
2. **Client Mode**: Filesystem watcher that syncs to shared or private directories based on configuration
3. **Shared Storage**: Multiple clients can connect to the same shared directory for real-time collaboration
4. **Isolated Storage**: Private client-specific directories for personal workspaces
5. **Smart File Filtering**: Configurable exclude patterns to ignore temporary files, build artifacts, etc.
6. **Sync Protocol**: RESTful API with endpoints for sync negotiation, uploads, downloads, and deletions

### Directory Types & Server Storage

The `shared` setting determines how directories are stored on the server:

- **Private Directories** (`shared: false` or omitted): Each client gets isolated storage
- **Shared Directories** (`shared: true`): Multiple clients collaborate on the same storage

#### Server Storage Structure Example
```
server_storage/
├── alice:personal_notes/          # Alice's private directory (isolated)
│   ├── personal_diary.txt
│   └── server_state.db            # Database state (was server_state.json)
├── bob:personal_workspace/        # Bob's private directory (isolated)
│   ├── bob_notes.txt
│   └── server_state.db            # Database state (was server_state.json)
└── team_project/                  # Shared directory (collaborative)
    ├── project_plan.docx          # Visible to both Alice & Bob
    ├── meeting_notes.txt          # Modified by Alice, visible to Bob
    ├── code_review.md             # Added by Bob, visible to Alice
    └── server_state.db            # Shared database state for all collaborators
```

**Key Differences:**
- **Private**: `client_id:directory_name` (e.g., `alice:personal_notes`)
- **Shared**: `directory_name` only (e.g., `team_project`)
- **Collaboration**: Multiple clients using same shared directory name work together
- **Isolation**: Private directories are completely separate per client

### Collaboration Model

- **Shared Directories**: When multiple clients configure the same directory name with `shared: true`, they collaborate in real-time
- **Private Directories**: Default behavior where each client gets isolated storage space
- **Real-time Synchronization**: Changes from any client immediately propagate to all other clients in shared directories
- **Smart File Filtering**: Exclude patterns automatically filter out unwanted files (temp files, build artifacts, etc.)
- **Automatic Conflict Resolution**: When multiple clients modify the same file, the newest version wins automatically
- **Seamless Integration**: Clients can join and leave collaborative sessions without affecting others

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
client --file <FILE>   # Start multi-directory client using YAML configuration

# Examples
./syncpair --log-level debug server --port 8080
./syncpair --quiet client --file config.yaml
./syncpair --log-file sync.log client --file multi-dirs.yaml
```

### Starting the Server

```bash
# Start server on default port (8080) with specified storage directory
./target/release/syncpair server --storage-dir /path/to/server/storage

# Start server on custom port with custom storage directory
./target/release/syncpair server --port 9000 --storage-dir /path/to/server/storage

# With detailed logging
./target/release/syncpair --log-level debug server --port 8080 --storage-dir ./server_files
```

### Running the Client

```bash
# Start client using YAML configuration file
./target/release/syncpair client --file config.yaml

# With detailed logging for debugging
./target/release/syncpair --log-level debug client --file config.yaml

# Production setup with quiet logging
./target/release/syncpair --quiet --log-file client.log client --file production-config.yaml
```

## Multi-Directory Configuration

SyncPair supports advanced multi-directory configurations with both shared collaboration and private workspaces.

### Basic Configuration

Create a YAML configuration file to define your directory setup. Each client can have a mix of shared collaborative directories and private isolated directories:

```yaml
# Alice's configuration - mix of shared and private directories
client_id: alice
server: http://localhost:8080

# Default settings applied to all directories (optional)
default:
  sync_interval_seconds: 30
  ignore_patterns:
    - "*.tmp"
    - "*.log"
    - "**/.git/**"

directories:
  # Private directory - isolated to this client only
  - name: personal_notes
    local_path: ~/notes/
    settings:
      description: "Alice's personal notes and documents"
      # sync_interval_seconds inherited from default (30)
      # shared: false (default - creates server storage at alice:personal_notes/)

  # Shared directory - collaborative workspace with other clients
  - name: team_project
    local_path: ~/team-work/
    settings:
      description: "Shared team project files"
      shared: true  # Multiple clients can use same "team_project" directory
      sync_interval_seconds: 15  # Override default
      ignore_patterns:
        - "node_modules/"  # Added to default patterns
```

```yaml
# Bob's configuration - same shared directory, different private directory
client_id: bob
server: http://localhost:8080

# Same defaults as Alice for consistency
default:
  sync_interval_seconds: 30
  ignore_patterns:
    - "*.tmp"
    - "*.log"
    - "**/.git/**"

directories:
  # Different private directory - isolated to Bob only
  - name: personal_workspace
    local_path: ~/bob-files/
    settings:
      description: "Bob's personal workspace"
      # sync_interval_seconds inherited from default (30)
      # shared: false (default - creates server storage at bob:personal_workspace/)

  # Same shared directory - collaborates with Alice
  - name: team_project
    local_path: ~/shared-work/
    settings:
      description: "Shared team project files"
      shared: true  # Both Alice and Bob access same server directory
      sync_interval_seconds: 15
```

### Advanced Configuration Options

```yaml
client_id: developer_workstation
server: http://company-sync:8080

directories:
  # Personal development workspace (private)
  - name: dev_workspace
    local_path: ~/development/
    settings:
      description: "Personal development files"
      sync_interval_seconds: 60
        ignore_patterns:
          - "target/"      # Rust build directory
          - "node_modules/" # Node.js dependencies
          - "**/.venv/**"   # Python virtual environments
          - "*.tmp"
          - "*.log"
          - ".env"         # Environment files

  # Shared configuration files (collaborative)
  - name: team_configs
    local_path: ~/shared-configs/
    settings:
      description: "Shared team configuration files"
      shared: true
      sync_interval_seconds: 10  # Fast sync for config changes
      ignore_patterns:
        - "*.bak"
        - "*.swp"
        - ".DS_Store"

  # Documentation collaboration (shared)
  - name: documentation
    local_path: ~/docs/
    settings:
      description: "Team documentation"
      shared: true
      sync_interval_seconds: 30
      ignore_patterns:
        - "*.draft"
        - "temp/*"
```

### Configuration Fields

| Field | Description | Required | Default |
|-------|-------------|----------|---------|
| `client_id` | Unique identifier for this client | Yes | - |
| `server` | Server URL (http://host:port) | Yes | - |
| `directories[].name` | Directory identifier | Yes | - |
| `directories[].local_path` | Local filesystem path | Yes | - |
| `directories[].settings.description` | Human-readable description | No | None |
| `directories[].settings.shared` | Enable sharing with other clients | No | `false` |
| `directories[].settings.sync_interval_seconds` | Sync frequency in seconds | No | `30` |
| `directories[].settings.enabled` | Enable/disable this directory | No | `true` |
| `directories[].settings.ignore_patterns` | Glob patterns to exclude | No | `[]` |
| `default` | Default settings for all directories | No | None |
| `default.description` | Default description | No | None |
| `default.sync_interval_seconds` | Default sync interval | No | `30` |
| `default.enabled` | Default enabled state | No | `true` |
| `default.shared` | Default sharing mode | No | `false` |
| `default.ignore_patterns` | Default ignore patterns | No | `[]` |

### Default Configuration

The `default` section allows you to specify common settings that apply to all directories unless explicitly overridden:

```yaml
client_id: my_client
server: http://localhost:8080

# Default settings for all directories
default:
  description: "Default workspace directory"
  sync_interval_seconds: 60
  ignore_patterns:
    - "*.tmp"
    - "*.log"
    - "**/.git/**"
    - "**/.venv/**"
    - "node_modules"

directories:
  # This directory inherits all defaults
  - name: workspace
    local_path: ~/workspace/
    settings: {}

  # This directory overrides some defaults
  - name: fast_sync
    local_path: ~/urgent/
    settings:
      description: "Urgent project"
      sync_interval_seconds: 15  # Override default
      # ignore_patterns still inherited and merged
```

**Key features:**
- **Inheritance**: Unspecified settings inherit from defaults
- **Override**: Directory settings override defaults when specified
- **Pattern Merging**: Ignore patterns are merged (defaults + directory-specific)
- **Deduplication**: Duplicate patterns are automatically removed

For detailed information, see [DEFAULT_CONFIG.md](DEFAULT_CONFIG.md).

### Exclude Patterns

Smart file filtering using glob patterns:

```yaml
ignore_patterns:
  - "*.tmp"           # All .tmp files
  - "*.log"           # All .log files
  - "node_modules/**" # Node.js dependencies directory and all contents
  - "target/**"       # Rust build directory and all contents
  - "**/.git/**"      # Git repository metadata and all contents
  - "**/.venv/**"     # Python virtual environments and all contents
  - "*.DS_Store"      # macOS system files
  - "temp/**"         # Everything in temp directory and subdirectories
  - "*.bak"           # Backup files
  - "build/**"        # Build output directory and all contents
  - "*.cache"         # Cache files
  - ".env"            # Environment configuration files

### Common Pattern Mistakes and Best Practices

**❌ Common Mistakes:**
```yaml
ignore_patterns:
  - "**/*.venv"       # WRONG: Matches files ending with .venv, not directories
  - "/.git"           # WRONG: Only matches .git in root directory
  - ".venv/"          # WRONG: Doesn't exclude files within .venv directory
  - "node_modules"    # WRONG: Only matches exact filename, not directory contents
```

**✅ Correct Patterns:**
```yaml
ignore_patterns:
  - "**/.venv/**"     # Correctly excludes Python virtual environments
  - "**/.git/**"      # Correctly excludes all git repositories
  - ".venv/**"        # Alternative: excludes .venv from root only
  - "node_modules/**" # Correctly excludes Node.js dependencies
```

**Key Rules:**
- Use `**/.pattern/**` to exclude directories named `.pattern` anywhere in the tree
- Use `pattern/**` to exclude a directory and all its contents
- Use `*.extension` to exclude files with specific extensions
- Use `**/pattern` to match files/directories anywhere in the tree
- Test your patterns to ensure they work as expected
```

**Important Directory Pattern Notes:**
- Use `directory/**` to exclude a directory and all its contents
- Pattern `directory/` only matches the directory name itself, not files within it
- Pattern `directory/**` correctly excludes all files and subdirectories within the directory
- Examples:
  - ✅ `temp/**` excludes `temp/file.txt`, `temp/subdir/file.txt`
  - ❌ `temp/` does NOT exclude files within the temp directory

### Running Client Configuration

```bash
# Start client sync using YAML configuration
./target/release/syncpair client --file config.yaml

# With detailed logging for debugging
./target/release/syncpair --log-level debug client --file config.yaml

# Production setup
./target/release/syncpair --quiet --log-file client.log client --file production-config.yaml
```

## How It Works

### Collaborative Synchronization Process

1. **Server Setup**: The server starts and provides shared directory storage accessible to all clients
2. **Client Connection**: Clients connect to shared directories with automatic retry (exponential backoff: 1s, 2s, 4s, 8s, 16s)
3. **Initial Sync**: Each client synchronizes with the current state of the shared directory
4. **Real-time Collaboration**: When any client makes changes, they immediately propagate to all other clients in the same directory
5. **Periodic Sync**: Regular sync cycles ensure consistency (configurable interval, default 30s) even with network interruptions
6. **Live Monitoring**: Clients monitor their local directories and sync changes instantly to collaborators
7. **Conflict Resolution**: When multiple clients modify the same file, timestamp-based resolution ensures the newest version wins
8. **Deletion Propagation**: When one client deletes a file, it's removed from all collaborating clients

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

- **Client State**: Tracks local files and deletion history with `.syncpair_state.db` database in the watch directory
- **Server State**: Maintains synchronized files and global deletion history with `server_state.db` database
- **Change Detection**: Compares file hashes and modification timestamps to determine sync actions
- **Deletion Tracking**: Timestamp-based deleted file tracking prevents conflicts and resurrection
- **Database Storage**: Uses embedded DuckDB for better performance, ACID transactions, and query capabilities

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
- **State Management**: DuckDB-based persistence with ACID transactions for reliability
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
- `serde/serde_json/serde_yaml` - Serialization and deserialization
- `sha2` - Cryptographic hash calculation
- `clap` - Command line argument parsing
- `anyhow/thiserror` - Error handling and propagation
- `chrono` - Date and time manipulation with timezone support
- `walkdir` - Recursive directory traversal
- `urlencoding` - URL-safe path encoding
- `glob` - Pattern matching for exclude patterns
- `log/env_logger` - Traditional logging interface
- `tracing/tracing-subscriber` - Structured, async-aware logging
- `dirs` - Directory path utilities

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
    client --file /home/user/production-config.yaml

# Development setup with detailed logging
./target/release/syncpair --log-level debug \
    server --port 8080 --storage-dir ./test-server

./target/release/syncpair --log-level debug \
    client --file ./test-config.yaml
```

## Collaboration Examples

### Real-time Team Collaboration

Multiple clients can work together on shared directories with automatic conflict resolution:

```bash
# Terminal 1: Start server
./target/release/syncpair --log-level info server --port 8080 --storage-dir ./shared

# Terminal 2: Alice joins shared collaboration
./target/release/syncpair client --file alice-config.yaml

# Terminal 3: Bob joins same shared collaboration
./target/release/syncpair client --file bob-config.yaml

# Now Alice and Bob collaborate in real-time:
# - When Alice adds a file, Bob sees it instantly
# - When Bob modifies a file, Alice gets the update
# - Conflicts are automatically resolved using timestamps
# - Deletions propagate between both collaborators
```

### Mixed Shared/Private Configuration Example

```yaml
# alice-config.yaml
client_id: alice
server: http://localhost:8080

directories:
  # Alice's private workspace
  - name: personal_notes
    local_path: ~/alice-private/
    settings:
      description: "Alice's personal files"
      # shared: false (default - isolated storage)

  # Shared team directory
  - name: team_docs
    local_path: ~/shared-team/
    settings:
      description: "Team collaboration space"
      shared: true  # Shared with other team members
      sync_interval_seconds: 15
```

```yaml
# bob-config.yaml
client_id: bob
server: http://localhost:8080

directories:
  # Bob's private workspace (different from Alice's)
  - name: personal_workspace
    local_path: ~/bob-private/
    settings:
      description: "Bob's personal files"
      # shared: false (default - isolated storage)

  # Same shared team directory
  - name: team_docs
    local_path: ~/team-collab/
    settings:
      description: "Team collaboration space"
      shared: true  # Collaborates with Alice and others
      sync_interval_seconds: 15
```

## Current Capabilities

✅ **Implemented Features**:
- **Real-time collaborative file synchronization** between multiple clients sharing directories
- **Shared directory storage architecture** enabling instant teamwork
- **Multi-client concurrent collaboration** with automatic conflict resolution
- **YAML configuration support** for flexible team and multi-directory setups
- Timestamp-based conflict resolution (newer files win automatically across all collaborators)
- File deletion synchronization with timestamp tracking across all team members
- Connection resilience with exponential backoff retry (5 attempts) and 30-second HTTP timeouts
- Comprehensive logging system with multiple verbosity levels for team coordination
- File and console logging with professional formatting for production deployments
- Real-time filesystem watching with immediate sync to all collaborators
- Hash-based integrity verification (SHA-256) ensuring data consistency across team
- Relative path preservation across all collaborating clients
- Atomic state persistence for reliability in team environments
- **Deadlock-free server operation** handling multiple concurrent clients safely
- Cross-platform compatibility (Linux, macOS, Windows) for diverse teams
- Single binary with server and client modes for easy deployment

## Recent Technical Improvements

### Deadlock-Free Server Operation
- **Fixed Critical Server Deadlock**: Resolved mutex deadlock in `atomic_save_directory_state()` method
- **Root Cause**: Method tried to acquire `directory_storage` mutex when calling code already held it
- **Solution**: Created separate `save_directory_state_with_lock()` function that doesn't acquire mutex, used within existing lock scope
- **Result**: Server now handles multiple concurrent clients without hanging

### Enhanced HTTP Resilience
- **Connection Timeouts**: Added 30-second timeouts for all HTTP requests to prevent client hangs
- **Upload Timeout**: 30-second timeout for file upload operations
- **Download Timeout**: 10-second timeout for file download operations
- **Sync Timeout**: 30-second timeout for sync negotiation requests
- **Result**: Clients no longer hang indefinitely on network issues, providing reliable team collaboration

### Shared Directory Architecture
- **Transformed Storage Model**: Changed from client-isolated storage to shared directory collaboration
- **Before**: Each client had isolated namespace (`server_storage/client:directory/`)
- **After**: Multiple clients share same directory (`server_storage/documents/` accessed by all clients)
- **Real-time Collaboration**: Alice and Bob can now work on the same files simultaneously with instant synchronization

## Limitations & Design Decisions

- **No authentication**: Plain HTTP without security (suitable for trusted team networks)
- **No encryption**: File content transferred in plain text (local team network assumption)
- **Simple conflict resolution**: Timestamp-based only (newer always wins across all team members)
- **No partial file sync**: Complete file transfer for each change (optimized for team document collaboration)
- **No bandwidth throttling**: Full-speed transfers (LAN team environment optimized)
- **No file locking**: Relies on filesystem-level locking for individual team member protection

## Future Enhancements

Potential improvements for advanced team deployments:
- Authentication and authorization (JWT tokens, API keys) for secure team access
- HTTPS/TLS encryption for secure transport in distributed teams
- Advanced conflict resolution strategies (user choice, merge strategies) for complex team scenarios
- Bandwidth throttling and rate limiting for large distributed teams
- Resume capability for large files (chunked uploads) for teams with large media files
- File compression during transport for teams with limited bandwidth
- Delta sync for large files (binary diff) optimizing team collaboration on large documents
- Web-based team administration interface for managing collaborative directories
- Metrics and monitoring endpoints for team usage analytics
- Distributed server architecture (clustering) for large-scale team deployments

## Production Considerations

### Recommended Team Setup

```bash
# Server (production team environment)
./syncpair --quiet --log-file /var/log/syncpair-server.log \
          server --port 8080 --storage-dir /data/team-syncpair

# Team Members (production)
./syncpair --quiet --log-file /var/log/syncpair-alice.log \
          client --file /home/alice/team-config.yaml

./syncpair --quiet --log-file /var/log/syncpair-bob.log \
          client --file /home/bob/team-config.yaml
```

### Team Monitoring

- **Log files**: Monitor team member log files for errors and collaboration conflicts
- **Storage space**: Ensure server storage directory has adequate space for all team files
- **Network connectivity**: Team members will retry automatically on connection loss
- **State files**: Back up `.syncpair_state.json` and `server_state.json` for team disaster recovery

### Team Performance

- **Optimal for**: Small to medium files (documents, code, configs) on local team networks
- **Network usage**: Efficient - only changed files are transferred between team members
- **Memory usage**: Low memory footprint, suitable for team deployment on various devices
- **CPU usage**: Minimal CPU usage, I/O bound operations suitable for team collaboration workloads
- **Concurrency**: Server handles unlimited concurrent team members without performance degradation

## License

[Add your license information here]

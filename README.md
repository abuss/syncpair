# SyncIt - Simple File Synchronization Tool

SyncIt is a lightweight Rust-based file synchronization tool that enables one-way sync from client directories to a central server. The system uses SHA-256 hashes for change detection and real-time filesystem watching for automatic uploads.

## Features

- **One-way client→server sync**: Simple upload-only synchronization model
- **Hash-based change detection**: Uses SHA-256 hashes to identify file changes efficiently
- **Real-time file watching**: Monitors filesystem changes and syncs automatically using `notify`
- **Relative path preservation**: Maintains directory structure on the server
- **Async HTTP communication**: Built with Tokio and Warp for performance
- **Single unified binary**: One executable with subcommands for both client and server modes
- **Cross-platform**: Written in Rust, works on Linux, macOS, and Windows

## Architecture

SyncIt uses a simplified architecture with two modes in a single binary:

1. **Server Mode**: HTTP server that receives file uploads via `/upload` endpoint
2. **Client Mode**: Filesystem watcher that automatically uploads changed files

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

### Starting the Server

```bash
# Start server on default port (8080) with default storage directory
./target/release/syncit server

# Start server on custom port with custom storage directory
./target/release/syncit server --port 9000 --storage-dir /path/to/server/storage
```

### Running the Client

```bash
# Start client to watch and sync a directory (default: ./sync_dir to localhost:8080)
./target/release/syncit client

# Watch custom directory and sync to custom server
./target/release/syncit client --server http://server:8080 --dir /path/to/sync/directory
```

## How It Works

### Sync Process

1. **Server Setup**: The server starts and listens for file uploads on the `/upload` endpoint
2. **Client Initialization**: Client scans the watch directory and performs initial sync of all files
3. **Real-time Watching**: Client monitors the directory for changes using filesystem events
4. **Automatic Upload**: When files are created, modified, or renamed, client immediately uploads them
5. **Hash Verification**: Files are verified using SHA-256 hashes to ensure integrity
6. **Relative Paths**: Directory structure is preserved on the server (e.g., `sync_dir/folder/file.txt`)

### File States

- **Client State**: Tracks local files with `.syncit_state.json` in the watch directory
- **Server State**: Maintains uploaded files with `server_state.json` in storage directory
- **Change Detection**: Compares file hashes to determine if upload is needed

## Technical Details

### Core Components

- **SimpleServer**: HTTP server using Warp framework with `/upload` endpoint
- **SimpleClient**: Filesystem watcher using `notify` crate with automatic upload capability
- **File Verification**: SHA-256 hash calculation and comparison for integrity
- **State Management**: JSON-based persistence for both client and server

### Communication Protocol

Simple HTTP API:
- `POST /upload`: Upload file with metadata (path, hash, content)
- Response includes success/failure status

### Dependencies

Minimal dependency set for core functionality:
- `tokio` - Async runtime
- `warp` - HTTP server framework  
- `notify` - Filesystem watching
- `reqwest` - HTTP client
- `serde/serde_json` - Serialization
- `sha2` - Hash calculation
- `clap` - Command line parsing
- `anyhow/thiserror` - Error handling
- `chrono` - Timestamp handling
- `walkdir` - Directory traversal

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

```bash
# Terminal 1: Start server
./target/release/syncit server --port 8080 --storage-dir ./server_files

# Terminal 2: Start client 
./target/release/syncit client --server http://localhost:8080 --dir ./my_files

# Now any files added/modified in ./my_files will automatically sync to server
```

## Limitations

- **One-way sync only**: Client→Server uploads only, no downloads
- **No conflict resolution**: Last upload wins (simplified model)
- **No authentication**: Plain HTTP without security (suitable for trusted networks)
- **No encryption**: File content transferred in plain text

## Future Enhancements

- Bidirectional synchronization
- Authentication and authorization
- HTTPS/TLS encryption
- Conflict detection and resolution
- File deletion synchronization
- Bandwidth throttling
- Resume capability for large files

## License

[Add your license information here]
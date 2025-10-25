# SyncPair - Advanced File Synchronization Tool

SyncPair is a professional-grade, bidirectional file synchronization tool written in Rust. It enables automatic synchronization of files between multiple client directories and a central server using real-time filesystem monitoring, intelligent conflict resolution, and comprehensive logging.

## Key Features

- **Bidirectional Synchronization**: Full two-way sync between multiple clients through central server
- **Intelligent Conflict Resolution**: Timestamp-based automatic conflict resolution (newer files win)
- **Deletion Synchronization**: File deletions propagate between all clients with timestamp tracking
- **Connection Resilience**: Automatic retry with exponential backoff when server unavailable (5 attempts)
- **Professional Logging**: Comprehensive logging system with multiple verbosity levels and file output
- **Real-Time Monitoring**: Uses filesystem watchers to detect changes immediately
- **Hash-Based Verification**: SHA-256 hashes ensure file integrity during all transfers
- **Unified Binary**: Single executable with `server` and `client` subcommands
- **Async Communication**: Built with Tokio for high-performance async I/O
- **Path Preservation**: Maintains relative directory structure across all clients

## Architecture

The system uses an advanced bidirectional synchronization model:

1. **Server Mode**: HTTP server with REST API for sync coordination, uploads, downloads, and deletions
2. **Client Mode**: Intelligent filesystem watcher with conflict resolution and resilient connection handling
3. **Sync Protocol**: RESTful API with comprehensive sync negotiation and atomic operations

Files are verified using SHA-256 hashes, conflicts are resolved using modification timestamps, and deletions are tracked with timestamps to prevent resurrection of deleted files.

## Usage

```bash
# Production Server (with professional logging)
./syncpair --quiet --log-file /var/log/syncpair-server.log \
    server --port 8080 --storage-dir /data/syncpair

# Production Client (quiet mode with error logging)
./syncpair --quiet --log-file /var/log/syncpair-client.log \
    client --server http://sync-server:8080 --dir /home/user/documents

# Development with debug logging
./syncpair --log-level debug server --port 8080 --storage-dir ./test-server
./syncpair --log-level debug client --server http://localhost:8080 --dir ./test-client
```

## Advanced Capabilities

### Multi-Client Synchronization
- Multiple clients can connect to the same server
- All changes (files and deletions) sync bidirectionally between clients
- Automatic conflict resolution prevents sync conflicts
- Each client maintains independent state and operates autonomously

### Connection Resilience
- Clients automatically retry failed connections (1s, 2s, 4s, 8s, 16s delays)
- Graceful handling of temporary network outages
- Automatic recovery when server becomes available
- No data loss during connection interruptions

### Professional Logging
- Multiple log levels: error, warn, info, debug, trace
- File and console output options
- Quiet mode for production deployments
- Structured logging with timestamps and component attribution

## Implementation Details

- **Language**: Rust (async/await with Tokio runtime)
- **Server**: Warp HTTP framework with comprehensive REST API
- **Client**: notify crate for filesystem watching + intelligent sync engine
- **Storage**: JSON state files with atomic writes for reliability
- **Communication**: HTTP with JSON payloads for all sync operations
- **Conflict Resolution**: Timestamp-based automatic resolution system
- **State Management**: Comprehensive tracking of files and deletions with timestamps

This advanced design provides enterprise-grade reliability, performance, and operational visibility for professional file synchronization deployments.

## Production Ready

SyncPair is designed for production use with:
- Comprehensive error handling and recovery
- Professional logging suitable for monitoring and debugging
- Atomic operations to prevent data corruption
- Efficient bandwidth usage (only changed files transfer)
- Cross-platform compatibility (Linux, macOS, Windows)
- Low resource usage suitable for embedded devices


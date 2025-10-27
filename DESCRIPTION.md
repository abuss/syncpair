# SyncPair - Collaborative File Synchronization Tool

SyncPair is a professional-grade, real-time collaborative file synchronization tool written in Rust. It enables automatic synchronization and teamwork through **shared directory storage**, where multiple clients can work together on the same directories with instant bidirectional synchronization, intelligent conflict resolution, and seamless collaboration features.

## Key Features

- **Real-time Collaboration**: Multiple clients can work together on shared directories with instant synchronization
- **Shared Directory Storage**: Teams can collaborate on the same directory namespace with immediate change propagation  
- **Intelligent Conflict Resolution**: Timestamp-based automatic conflict resolution (newer files win) across all collaborators
- **Deletion Synchronization**: File deletions propagate between all team members with timestamp tracking
- **Connection Resilience**: Automatic retry with exponential backoff and 30-second HTTP timeouts when server unavailable (5 attempts)
- **Professional Logging**: Comprehensive logging system with multiple verbosity levels and file output for team coordination
- **Real-Time Monitoring**: Uses filesystem watchers to detect changes immediately and sync to all collaborators
- **Hash-Based Verification**: SHA-256 hashes ensure file integrity during all transfers across the team
- **Unified Binary**: Single executable with `server` and `client` subcommands for easy team deployment
- **Async Communication**: Built with Tokio for high-performance async I/O supporting concurrent collaborators
- **Path Preservation**: Maintains relative directory structure across all collaborating clients

## Architecture

The system uses a **shared directory collaboration model** for real-time teamwork:

1. **Server Mode**: HTTP server with shared directory storage - each directory (e.g., "documents") is accessible to all team members
2. **Client Mode**: Intelligent filesystem watcher that syncs to shared directories, enabling instant collaboration between team members
3. **Collaboration Protocol**: RESTful API with comprehensive sync negotiation and atomic operations for team coordination

Files are verified using SHA-256 hashes, conflicts are resolved using modification timestamps across all collaborators, and deletions are tracked with timestamps to prevent resurrection of deleted files. The server now includes deadlock-free operation for handling multiple concurrent team members safely.

## Usage

```bash
# Production Server (supporting team collaboration)
./syncpair --quiet --log-file /var/log/syncpair-server.log \
    server --port 8080 --storage-dir /data/team-syncpair

# Team Member 1 (Alice joining shared "documents" directory)
./syncpair --quiet --log-file /var/log/syncpair-alice.log \
    client --server http://sync-server:8080 --dir /home/alice/team-documents

# Team Member 2 (Bob joining same shared "documents" directory)  
./syncpair --quiet --log-file /var/log/syncpair-bob.log \
    client --server http://sync-server:8080 --dir /home/bob/team-documents

# Development with debug logging for team coordination
./syncpair --log-level debug server --port 8080 --storage-dir ./team-server
./syncpair --log-level debug client --server http://localhost:8080 --dir ./alice-workspace
```

## Advanced Collaboration Capabilities

### Multi-Team Synchronization
- Multiple teams can use different shared directories on the same server
- All changes (files and deletions) sync bidirectionally between all team members in real-time
- Automatic conflict resolution prevents sync conflicts during active collaboration
- Each team member maintains independent state and operates autonomously

### Connection Resilience
- Team members automatically retry failed connections (1s, 2s, 4s, 8s, 16s delays)
- Graceful handling of temporary network outages during team collaboration
- Automatic recovery when server becomes available without losing team synchronization
- No data loss during connection interruptions in collaborative sessions

### Professional Team Logging
- Multiple log levels: error, warn, info, debug, trace for team coordination
- File and console output options for team monitoring
- Quiet mode for production team deployments
- Structured logging with timestamps and component attribution for team debugging

## Implementation Details

- **Language**: Rust (async/await with Tokio runtime) for high-performance team collaboration
- **Server**: Warp HTTP framework with comprehensive REST API and deadlock-free concurrent client handling
- **Client**: notify crate for filesystem watching + intelligent sync engine optimized for team collaboration
- **Storage**: JSON state files with atomic writes for reliability in team environments
- **Communication**: HTTP with JSON payloads and 30-second timeouts for all team sync operations  
- **Conflict Resolution**: Timestamp-based automatic resolution system for seamless team collaboration
- **State Management**: Comprehensive tracking of files and deletions with timestamps across all team members

This advanced design provides enterprise-grade reliability, performance, and operational visibility for professional team file synchronization and collaboration deployments.

## Production Ready for Teams

SyncPair is designed for production team use with:
- Comprehensive error handling and recovery for collaborative environments
- Professional logging suitable for team monitoring and debugging
- Atomic operations to prevent data corruption during team collaboration
- Efficient bandwidth usage (only changed files transfer) optimized for team workflows
- Cross-platform compatibility (Linux, macOS, Windows) supporting diverse team environments
- Low resource usage suitable for team deployment on various devices
- **Deadlock-free server operation** ensuring reliable service for concurrent team members


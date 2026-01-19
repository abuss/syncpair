# Build stage
FROM rust:latest as builder

WORKDIR /usr/src/syncpair
COPY . .

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install necessary runtime dependencies (if any)
# ca-certificates is usually needed for HTTPS
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Create a non-root user (optional but good practice, though for simple file ops strict permission handling might be tricky with volumes. 
# For simplicity in this user-facing tool, we'll run as root to avoid volume permission issues unless requested otherwise, 
# or just stick to standard practices. Let's keep it simple for now standard root is easier for volume mounts.)

WORKDIR /app

# Copy binary from builder
COPY --from=builder /usr/src/syncpair/target/release/syncpair /usr/local/bin/syncpair

# Create data directory
RUN mkdir -p /data

# Expose server port
EXPOSE 8080

# Environment variables
ENV RUST_LOG=info

# Default command: run server using /data for storage
CMD ["syncpair", "server", "--port", "8080", "--storage-dir", "/data"]

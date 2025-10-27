#!/bin/bash

cd /home/abuss/Work/devel/syncpair

echo "=== Testing Client 2 Shared Directory Access ==="
echo

# Start server in background
./target/debug/syncpair server --port 8080 --storage-dir server_storage > server2.log 2>&1 &
SERVER_PID=$!
sleep 2

echo "Server started (PID: $SERVER_PID)"

# Run client 2 for a few seconds
echo "Starting client 2 (demo2)..."
timeout 10 ./target/debug/syncpair config --file test_shared_demo2.yaml > client2.log 2>&1 &
CLIENT2_PID=$!

sleep 5

# Kill client 2
kill $CLIENT2_PID 2>/dev/null || true

echo "Client 2 stopped, checking server storage..."

# Check what directories were created
echo "=== Server Storage Structure After Client 2 ==="
find server_storage -type d | sort

echo 
echo "=== Files in shared directory (nix-config) after both clients ==="
ls -la server_storage/nix-config/

echo
echo "=== Expected: Should see files from both clients in nix-config ==="

# Kill server
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo
echo "Test completed."
#!/bin/bash

cd /home/abuss/Work/devel/syncpair

echo "=== Testing Shared Directory Implementation ==="
echo

# Start server in background
./target/debug/syncpair server --port 8080 --storage-dir server_storage > server.log 2>&1 &
SERVER_PID=$!
sleep 2

echo "Server started (PID: $SERVER_PID)"

# Run client 1 for a few seconds
echo "Starting client 1 (demo1)..."
timeout 10 ./target/debug/syncpair config --file test_shared_demo1.yaml > client1.log 2>&1 &
CLIENT1_PID=$!

sleep 5

# Kill client 1 
kill $CLIENT1_PID 2>/dev/null || true

echo "Client 1 stopped, checking server storage..."

# Check what directories were created
echo "=== Server Storage Structure After Client 1 ==="
find server_storage -type d | sort

echo 
echo "=== Expected Behavior ==="
echo "Should see:"
echo "  - server_storage/demo1:demo1/ (client-specific)"
echo "  - server_storage/nix-config/ (shared)"

# Kill server
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

echo
echo "Test completed."
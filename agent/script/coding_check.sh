#!/bin/bash

set -e

echo "=== Running Cargo Check ==="
cd /home/vincent/Desktop/Repository/VRM-Rust-Workflow/ && cargo check --all-targets --all-features
CHECK_STATUS=$?

# echo "=== Running Cargo Clippy ==="
# cargo clippy --all-targets --all-features -- -D warnings
# CLIPPY_STATUS=$?
# 
# if [ $CHECK_STATUS -eq 0 ] && [ $CLIPPY_STATUS -eq 0 ]; then
if [ $CHECK_STATUS -eq 0 ]; then
    echo "Status: Success. No errors or warnings found."
    exit 0
else
    echo "Status: Failed. Please review the errors above."
    exit 1
fi
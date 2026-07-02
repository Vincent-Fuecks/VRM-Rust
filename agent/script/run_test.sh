#!/bin/bash
PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"
cd "$PROJECT_DIR" || exit 1

# Defaults: run all tests, 20-second timeout
TEST_FILTER="${1:-""}"
TIMEOUT_SECS="${2:-20}"

echo "=== Running Cargo Tests ==="

timeout "$TIMEOUT_SECS" cargo test "$TEST_FILTER" -- --nocapture
TEST_STATUS=$?

if [ $TEST_STATUS -eq 124 ]; then
    echo "[!] Test execution TIMED OUT after ${TIMEOUT_SECS}s. Potential infinite loop detected."
    exit 124
elif [ $TEST_STATUS -eq 0 ]; then
    echo "Status: All tests passed successfully."
    exit 0
else
    echo "Status: Tests failed."
    exit 1
fi
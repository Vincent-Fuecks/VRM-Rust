#!/bin/bash
# Runs workflow-related tests with a configurable timeout.
# Usage: ./check_workflow_tests.sh [filter] [timeout_secs]

PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"
cd "$PROJECT_DIR" || exit 1

FILTER="${1:-workflow}"
TIMEOUT="${2:-30}"

echo "=== Running workflow tests (filter: '$FILTER', timeout: ${TIMEOUT}s) ==="

# Run lib tests matching the filter
timeout "$TIMEOUT" cargo test --lib -- "$FILTER" --nocapture 2>&1
LIB_STATUS=$?

# Run integration tests matching the filter
timeout "$TIMEOUT" cargo test --test main -- "$FILTER" --nocapture 2>&1
INT_STATUS=$?

if [ $LIB_STATUS -eq 0 ] && [ $INT_STATUS -eq 0 ]; then
    echo "=== All workflow tests passed ==="
    exit 0
else
    echo "=== Some workflow tests failed (lib: $LIB_STATUS, integration: $INT_STATUS) ==="
    exit 1
fi

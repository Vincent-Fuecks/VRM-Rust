#!/bin/bash
# Full quality gate: cargo check + verify removed items.
# Usage: ./check_all.sh

set -e

PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"
SCRIPT_DIR="$PROJECT_DIR/agent/script"

echo "=== [1/2] Cargo Check ==="
bash "$SCRIPT_DIR/coding_check.sh"
echo ""

echo "=== [2/2] Verify Removed Legacy Items ==="
bash "$SCRIPT_DIR/verify_removed.sh"
echo ""

echo "=== All checks passed ==="

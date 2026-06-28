#!/bin/bash
PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"
cd "$PROJECT_DIR" || exit 1

echo "=== Running Cargo Fmt ==="
cargo fmt

echo "=== Running Cargo Clippy Auto-Fix ==="
cargo clippy --fix --allow-dirty --allow-staged

echo "Status: Formatting and auto-fixes complete."
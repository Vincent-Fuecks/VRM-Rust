#!/bin/bash

# Project root directory
PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"

# Jump into the repository
cd "$PROJECT_DIR" || { echo "Error: Directory $PROJECT_DIR not found."; exit 1; }

# Clever Defaults: Use arguments if provided, otherwise fall back to your defaults
WORKFLOW_FILE="${1:-src/data/demo/workflow_direct_mapping_no_links.json}"
CONFIG_FILE="${2:-src/data/demo/vrm_config_direct_mapping.json}"
TIMEOUT_SECS="${3:-15}"
LINE_LIMIT="${4:-20}"
FILTER_PATTERNS="${5:-(Thread|Deadlock|Thread ID|Backtrace|DEBUG|ERROR)}"

# Execute the pipeline
timeout "$TIMEOUT_SECS" cargo run -- -f "$WORKFLOW_FILE" -c "$CONFIG_FILE" 2>&1 \
    | grep -Ei "$FILTER_PATTERNS" \
    | head -n "$LINE_LIMIT"

# PIPESTATUS[0] captures the exit code of the FIRST command in the pipe (the cargo run/timeout)
RUN_STATUS=${PIPESTATUS[0]}

# Alert the agent regarding specific failures
if [ $RUN_STATUS -eq 124 ]; then
    echo -e "\n[!] Script aborted: Execution exceeded the ${TIMEOUT_SECS}s timeout limit."
    exit 124
elif [ $RUN_STATUS -ne 0 ] && [ $RUN_STATUS -ne 141 ]; then
    # Note: Exit code 141 is normal here; it just means 'head' closed the pipe early because it hit 20 lines.
    echo -e "\n[!] Cargo run failed with exit code: $RUN_STATUS"
    exit "$RUN_STATUS"
fi
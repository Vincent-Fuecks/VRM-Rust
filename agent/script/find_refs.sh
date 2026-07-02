#!/bin/bash
# Fast multi-pattern search across the source tree.
# Usage: ./find_refs.sh "pattern1|pattern2|pattern3"

PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"
cd "$PROJECT_DIR" || exit 1

if [ -z "$1" ]; then
    echo "Usage: $0 \"pattern1|pattern2|...\""
    echo "Example: $0 \"gateway_router_id|get_component_gateway|cascade_delete\""
    exit 1
fi

echo "=== Searching for: $1 ==="
grep -rnI --exclude-dir=target --exclude-dir=.git -E "$1" src/ tests/ 2>/dev/null

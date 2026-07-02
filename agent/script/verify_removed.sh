#!/bin/bash
# Verifies that removed legacy items have zero references in source code.
# Usage: ./verify_removed.sh

PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"
cd "$PROJECT_DIR" || exit 1

ITEMS=("RMS_GATEWAY_NAME" "get_component_router_list")
FAILED=0

for item in "${ITEMS[@]}"; do
    echo -n "Checking '$item'... "
    COUNT=$(grep -rnI "$item" src/ --include="*.rs" 2>/dev/null | grep -v "//.*$item" | wc -l)
    if [ "$COUNT" -eq 0 ]; then
        echo "CLEAN"
    else
        echo "FOUND ($COUNT reference(s))"
        grep -rnI "$item" src/ --include="*.rs" 2>/dev/null | grep -v "//.*$item"
        FAILED=1
    fi
done

if [ "$FAILED" -eq 0 ]; then
    echo "=== All legacy items verified removed ==="
    exit 0
else
    echo "=== Some legacy items still have references ==="
    exit 1
fi

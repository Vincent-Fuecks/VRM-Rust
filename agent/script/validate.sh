#!/bin/bash

OUTPUT=$(cargo run -- "$@" 2>&1)
EXIT_CODE=$?

if [ $EXIT_CODE -eq 0 ]; then
    echo "Accepted"
    exit 0
else
    echo "Status: Execution Failed."
    echo "=== Filtered Error & Log Output ==="
    echo "$OUTPUT" | grep -E -i "Deadlock|Thread ID|Backtrace|error|panic|fail|fatal|DEBUG|ERROR"
    
    exit 1
fi
#!/bin/bash

PROJECT_DIR="/home/vincent/Desktop/Repository/VRM-Rust-Workflow"

if [ -z "$1" ]; then
    echo "Error: Please provide a search pattern."
    echo "Usage: $0 \"<search_term>\" [extra grep options]"
    echo "Example: $0 \"rand\" -i --include=\"*.toml\""
    exit 1
fi

SEARCH_TERM="$1"
shift 

# Flags used here:
# -r : Recursive search
# -n : Show line numbers in the output
# -I : Ignore binary files (prevents printing compiled gibberish)
# --exclude-dir : Skips target/ and .git/ completely to ensure lightning-fast speed
grep -rnI \
    --exclude-dir=target \
    --exclude-dir=.git \
    "$SEARCH_TERM" "$@" "$PROJECT_DIR"
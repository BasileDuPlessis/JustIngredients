#!/bin/bash

# Check for unwrap() violations in Rust source code
# This script enforces the project policy that unwrap() is strictly prohibited
# Only expect() is allowed for mutex poisoning scenarios

set -e

echo "Checking for unwrap() violations..."

# Find all .rs files and check for unwrap() usage
# Exclude test files and target directory
UNWRAP_FILES=$(find . -name "*.rs" -type f -not -path "./tests/*" -not -path "./target/*" -not -path "./.git/*" -exec grep -l "\.unwrap()" {} \; || true)

if [ -n "$UNWRAP_FILES" ]; then
    echo "❌ ERROR: Found unwrap() usage in the following files:"
    echo "$UNWRAP_FILES"
    echo ""
    echo "unwrap() is strictly prohibited in this project."
    echo "Please replace unwrap() calls with proper error handling using:"
    echo "- ? operator for propagating errors"
    echo "- match statements for explicit error handling"
    echo "- expect() only for mutex poisoning (with descriptive messages)"
    echo ""
    echo "Files containing unwrap():"
    for file in $UNWRAP_FILES; do
        echo "  $file"
        grep -n "\.unwrap()" "$file" | head -5
    done
    exit 1
else
    echo "✅ No unwrap() violations found."
fi
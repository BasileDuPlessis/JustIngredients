#!/bin/bash
# Check for unwrap violations in the codebase
# This script is used by CI and can be run locally by developers

set -e

echo "🔍 Checking for unwrap violations..."

if grep -r "\.unwrap()" src/ --include="*.rs"; then
    echo "❌ ERROR: Found .unwrap() calls in source code."
    echo "Replace with proper error handling using ? operator and custom error types."
    echo ""
    echo "Allowed exceptions:"
    echo "- expect() for mutex poisoning with descriptive messages"
    echo "- Static initialization where unwrap is safe"
    echo ""
    echo "Fix these violations before committing."
    exit 1
else
    echo "✅ No unwrap violations found"
fi
#!/bin/sh
# Points git at the versioned hooks of scripts/hooks/ (this clone only).
set -e
cd "$(dirname "$0")/../.."
git config core.hooksPath scripts/hooks
echo "Git hooks installed from scripts/hooks/"

#!/bin/bash
# Run clippy with the same settings as CI
# This ensures local development matches CI quality standards

set -e

echo "🔍 Running clippy with project quality standards..."

cargo clippy --all-targets --all-features -- -D warnings -A clippy::expect_used

echo "✅ Clippy passed - code meets quality standards!"
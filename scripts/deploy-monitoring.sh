#!/bin/bash

# Deploy monitoring stack (Prometheus)
# This script deploys Prometheus to monitor the JustIngredients app

set -e

echo "📊 Deploying JustIngredients Monitoring (Prometheus)..."

# Check if fly CLI is available
if ! command -v fly &> /dev/null; then
    echo "❌ Fly CLI is not installed. Please install it from https://fly.io/docs/flyctl/install/"
    exit 1
fi

# Check authentication
if ! fly auth whoami &> /dev/null; then
    echo "❌ Not logged in to Fly.io. Please run 'fly auth login'"
    exit 1
fi

APP_NAME="just-ingredients-monitoring"

# Create monitoring app if it doesn't exist
if fly apps list | grep -q "^$APP_NAME"; then
    echo "✅ Monitoring app '$APP_NAME' already exists"
else
    echo "📦 Creating monitoring app '$APP_NAME'..."
    fly apps create "$APP_NAME" --org personal
fi

# Deploy Prometheus
echo "🚀 Deploying Prometheus..."
fly deploy --config deploy/fly-monitoring.toml --app "$APP_NAME"

# Wait for Prometheus to be ready
echo "⏳ Waiting for Prometheus to start..."
MAX_WAIT=60
WAIT_COUNT=0
while [ $WAIT_COUNT -lt $MAX_WAIT ]; do
    if curl -s "https://$APP_NAME.fly.dev/-/healthy" | grep -q "Prometheus"; then
        echo "✅ Prometheus is ready!"
        break
    fi
    echo "   Waiting... ($WAIT_COUNT/$MAX_WAIT seconds)"
    sleep 5
    WAIT_COUNT=$((WAIT_COUNT + 5))
done

if [ $WAIT_COUNT -ge $MAX_WAIT ]; then
    echo "⚠️  Prometheus deployment completed, but health check timed out."
    echo "   Check status manually."
fi

echo "✅ Monitoring deployment completed!"
echo "🌐 Prometheus UI: https://$APP_NAME.fly.dev"
echo "📊 Metrics from JustIngredients should be visible at: https://$APP_NAME.fly.dev/graph?g0.expr=up&g0.tab=1"
echo ""
echo "📝 Next steps:"
echo "1. Visit the Prometheus UI to explore metrics"
echo "2. Check that 'just-ingredients' job is scraping data"
echo "3. Query metrics like: health_checks_total"
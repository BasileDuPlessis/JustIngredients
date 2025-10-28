#!/bin/bash

# Deploy Grafana for JustIngredients monitoring
# This script deploys Grafana to Fly.io with Prometheus data source

set -e

echo "📊 Deploying JustIngredients Grafana..."

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

APP_NAME="just-ingredients-grafana"

# Create Grafana app if it doesn't exist
if fly apps list | grep -q "^$APP_NAME"; then
    echo "✅ Grafana app '$APP_NAME' already exists"
else
    echo "📦 Creating Grafana app '$APP_NAME'..."
    fly apps create "$APP_NAME" --org personal
fi

# Deploy Grafana
echo "🚀 Deploying Grafana..."
fly deploy --config fly-grafana.toml --app "$APP_NAME"

# Wait for Grafana to be ready
echo "⏳ Waiting for Grafana to start..."
MAX_WAIT=120
WAIT_COUNT=0
while [ $WAIT_COUNT -lt $MAX_WAIT ]; do
    if curl -s "https://$APP_NAME.fly.dev/api/health" | grep -q "ok"; then
        echo "✅ Grafana is ready!"
        break
    fi
    echo "   Waiting... ($WAIT_COUNT/$MAX_WAIT seconds)"
    sleep 5
    WAIT_COUNT=$((WAIT_COUNT + 5))
done

if [ $WAIT_COUNT -ge $MAX_WAIT ]; then
    echo "⚠️  Grafana deployment completed, but health check timed out."
    echo "   Check status manually."
fi

echo "✅ Grafana deployment completed!"
echo "🌐 Grafana UI: https://$APP_NAME.fly.dev"
echo "👤 Login: admin / admin123"
echo ""
echo "📝 Next steps:"
echo "1. Visit Grafana UI and explore the JustIngredients dashboard"
echo "2. Check that Prometheus data source is connected"
echo "3. Customize dashboards as needed"
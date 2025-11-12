#!/bin/bash

# Deployment script for JustIngredients Telegram Bot
# This script automates the deployment of the app and database on Fly.io from scratch

set -e  # Exit on any error

echo "🚀 Starting JustIngredients deployment..."

# Check if fly CLI is installed
if ! command -v fly &> /dev/null; then
    echo "❌ Fly CLI is not installed. Please install it from https://fly.io/docs/flyctl/install/"
    exit 1
fi

# Check if logged in
if ! fly auth whoami &> /dev/null; then
    echo "❌ Not logged in to Fly.io. Please run 'fly auth login'"
    exit 1
fi

APP_NAME="just-ingredients"
DB_NAME="just-ingredients-db"
REGION="cdg"

# Create app if it doesn't exist
if fly apps list | grep -q "^$APP_NAME"; then
    echo "✅ App '$APP_NAME' already exists"
else
    echo "📦 Creating Fly.io app '$APP_NAME'..."
    fly apps create "$APP_NAME" --org personal
fi

# Create PostgreSQL database if it doesn't exist
if fly postgres list | grep -q "$DB_NAME"; then
    echo "✅ Database '$DB_NAME' already exists"
else
    echo "🗄️ Creating PostgreSQL database '$DB_NAME'..."
    fly postgres create --name "$DB_NAME" --region "$REGION" --org personal --vm-size shared-cpu-1x --volume-size 1
    echo "⏳ Waiting for database to initialize..."
    sleep 10
fi

# Attach database to app
echo "🔗 Attaching database to app..."
if fly postgres attach "$DB_NAME" --app "$APP_NAME"; then
    echo "✅ Database attached successfully"
else
    echo "ℹ️  Database may already be attached (continuing...)"
fi
echo "⏳ Waiting for database attachment to complete..."
sleep 5

# Set required secrets (TELEGRAM_BOT_TOKEN must be provided)
if [ -z "$TELEGRAM_BOT_TOKEN" ]; then
    echo "🔑 Please enter your Telegram Bot Token:"
    echo "   Get it from @BotFather on Telegram"
    read -s -p "TELEGRAM_BOT_TOKEN: " TELEGRAM_BOT_TOKEN
    echo ""  # New line after hidden input
    if [ -z "$TELEGRAM_BOT_TOKEN" ]; then
        echo "❌ Telegram bot token is required. Exiting."
        exit 1
    fi
else
    echo "✅ Using TELEGRAM_BOT_TOKEN from environment variable"
fi

echo "🔐 Setting secrets..."
fly secrets set TELEGRAM_BOT_TOKEN="$TELEGRAM_BOT_TOKEN" --app "$APP_NAME"

# Deploy the app
echo "🚀 Deploying the application..."
fly deploy --config deploy/fly.toml --app "$APP_NAME"

# Wait for app to be ready
echo "⏳ Waiting for application to start..."
MAX_WAIT=120  # 2 minutes
WAIT_COUNT=0
while [ $WAIT_COUNT -lt $MAX_WAIT ]; do
    if curl -s "https://$APP_NAME.fly.dev/health/live" | grep -q "OK"; then
        echo "✅ Application is ready!"
        break
    fi
    echo "   Waiting... ($WAIT_COUNT/$MAX_WAIT seconds)"
    sleep 5
    WAIT_COUNT=$((WAIT_COUNT + 5))
done

if [ $WAIT_COUNT -ge $MAX_WAIT ]; then
    echo "⚠️  Application deployment completed, but health check timed out."
    echo "   The app may still be starting. Check status manually."
fi

echo "✅ Deployment completed successfully!"
echo "🌐 Your app is available at: https://$APP_NAME.fly.dev"
echo "💚 Health check: https://$APP_NAME.fly.dev/health/live"
echo ""
echo "📝 Next steps:"
echo "1. Test the bot by sending a message to your Telegram bot"
echo "2. Monitor logs with: fly logs --app $APP_NAME"
echo "3. Check status with: fly status --app $APP_NAME"
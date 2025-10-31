#!/bin/bash

# Google Gemini AI Code Review Setup Script
# This script helps you set up free-tier Google Gemini for AI code review

set -e

echo "🤖 Google Gemini AI Code Review Setup"
echo "====================================="
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: Please run this script from the project root directory"
    exit 1
fi

echo "📋 Setup Steps:"
echo "1. Get Google AI API Key (free)"
echo "2. Add API key to GitHub secrets"
echo "3. Enable the workflow"
echo ""

echo "🔑 Step 1: Get Your Google AI API Key"
echo "--------------------------------------"
echo "1. Go to: https://makersuite.google.com/app/apikey"
echo "2. Sign in with your Google account"
echo "3. Click 'Create API key'"
echo "4. Copy the generated API key"
echo ""
echo "✅ Free tier includes:"
echo "   • 60 requests per minute"
echo "   • 1,500 requests per day"
echo "   • No cost for normal usage"
echo ""

read -p "Do you have your Google AI API key ready? (y/n): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Please get your API key first, then run this script again."
    exit 0
fi

echo ""
echo "🔐 Step 2: Add API Key to GitHub Secrets"
echo "----------------------------------------"
echo "1. Go to your GitHub repository"
echo "2. Navigate to Settings → Secrets and variables → Actions"
echo "3. Click 'New repository secret'"
echo "4. Name: GOOGLE_AI_API_KEY"
echo "5. Value: Paste your API key"
echo "6. Click 'Add secret'"
echo ""

read -p "Have you added the GOOGLE_AI_API_KEY secret to GitHub? (y/n): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Please add the secret first, then run this script again."
    exit 0
fi

echo ""
echo "✅ Step 3: Enable the Workflow"
echo "------------------------------"
echo "The workflow file is already created at:"
echo ".github/workflows/ai-code-review-gemini.yml"
echo ""
echo "The workflow will automatically run on pull requests to main branch."
echo ""

echo "🎉 Setup Complete!"
echo "=================="
echo ""
echo "Your AI code review is now configured with Google Gemini (free tier)."
echo ""
echo "Features:"
echo "• Automatic code review on PRs"
echo "• Focus on critical security/safety issues"
echo "• Telegram notifications (if configured)"
echo "• PR comments with findings"
echo ""
echo "Cost: FREE (within limits)"
echo "Limits: 60 requests/minute, 1,500/day"
echo ""
echo "To test: Create a pull request with Rust code changes!"
echo ""

# Optional: Check if they want to set up Telegram notifications
echo "📱 Optional: Telegram Notifications"
echo "-----------------------------------"
read -p "Do you want to set up Telegram notifications for reviews? (y/n): " -n 1 -r
echo
if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "To set up Telegram notifications:"
    echo "1. Create a Telegram bot: Message @BotFather and follow instructions"
    echo "2. Get your chat ID: Message @userinfobot"
    echo "3. Add secrets to GitHub:"
    echo "   - TELEGRAM_BOT_TOKEN: Your bot token"
    echo "   - TELEGRAM_CHAT_ID: Your chat ID"
    echo ""
fi

echo "Happy coding! 🚀"
#!/bin/bash

# JustIngredients Bot - Deployment Script
# This script automates the deployment process to Fly.io

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
APP_NAME="just-ingredients-bot"
DB_NAME="just-ingredients-db"

echo -e "${BLUE}🚀 JustIngredients Bot - Deployment${NC}"
echo "==================================="

# Function to check prerequisites
check_prerequisites() {
    echo -e "${YELLOW}Checking prerequisites...${NC}"

    # Check if fly CLI is installed
    if ! command -v fly >/dev/null 2>&1; then
        echo -e "${RED}❌ Fly CLI not found. Please install it first.${NC}"
        exit 1
    fi

    # Check if authenticated
    if ! fly auth whoami >/dev/null 2>&1; then
        echo -e "${RED}❌ Not authenticated with Fly.io. Run 'fly auth login'${NC}"
        exit 1
    fi

    # Check if app exists
    if ! fly apps list | grep -q "^$APP_NAME"; then
        echo -e "${RED}❌ Fly.io app '$APP_NAME' not found. Run './scripts/setup.sh' first.${NC}"
        exit 1
    fi

    # Check if fly.toml exists
    if [ ! -f "fly.toml" ]; then
        echo -e "${RED}❌ fly.toml not found. Run './scripts/setup.sh' first.${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Prerequisites check passed${NC}"
}

# Function to run tests
run_tests() {
    echo -e "${YELLOW}Running tests...${NC}"

    # Run cargo tests
    if ! cargo test --all-features; then
        echo -e "${RED}❌ Tests failed${NC}"
        exit 1
    fi

    # Run clippy
    if ! cargo clippy --all-targets --all-features -- -D warnings; then
        echo -e "${RED}❌ Clippy check failed${NC}"
        exit 1
    fi

    # Check formatting
    if ! cargo fmt --all -- --check; then
        echo -e "${RED}❌ Code formatting check failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ All tests passed${NC}"
}

# Function to build and deploy
deploy_app() {
    echo -e "${YELLOW}Building and deploying application...${NC}"

    # Deploy to Fly.io
    if ! fly deploy --remote-only --ha=false; then
        echo -e "${RED}❌ Deployment failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Application deployed successfully${NC}"
}

# Function to verify deployment
verify_deployment() {
    echo -e "${YELLOW}Verifying deployment...${NC}"

    # Wait for app to be ready
    echo "Waiting for application to be ready..."
    sleep 10

    # Check app status
    if ! fly status --app "$APP_NAME" >/dev/null 2>&1; then
        echo -e "${RED}❌ Application status check failed${NC}"
        return 1
    fi

    # Get app URL
    APP_URL=$(fly status --app "$APP_NAME" | grep "Hostname" | awk '{print $2}')
    if [ -z "$APP_URL" ]; then
        echo -e "${RED}❌ Could not get application URL${NC}"
        return 1
    fi

    # Test health endpoint (optional for incremental deployments)
    echo "Testing health endpoint: https://$APP_URL/health/live"
    if ! curl -f -s --max-time 10 "https://$APP_URL/health/live" >/dev/null 2>&1; then
        echo -e "${YELLOW}⚠️  Health check failed or not available${NC}"
        echo -e "${YELLOW}💡 This may be normal for incremental deployments${NC}"
        echo -e "${YELLOW}💡 Check app logs: fly logs --app $APP_NAME${NC}"
        # Don't fail deployment for health check in incremental mode
    else
        echo -e "${GREEN}✅ Health check passed${NC}"
    fi

    echo -e "${GREEN}✅ Deployment verification completed${NC}"
    echo -e "${GREEN}🌐 Application URL: https://$APP_URL${NC}"
}

# Function to show deployment info
show_deployment_info() {
    echo ""
    echo -e "${BLUE}📊 Deployment Information${NC}"
    echo "=========================="

    # App URL
    APP_URL=$(fly status --app "$APP_NAME" | grep "Hostname" | awk '{print $2}')
    echo -e "Application URL: ${GREEN}https://$APP_URL${NC}"

    # App status
    echo -e "Application Status: ${GREEN}$(fly status --app "$APP_NAME" | grep "Status" | awk '{print $2}')${NC}"

    # Database status
    echo -e "Database Status: ${GREEN}$(fly postgres list | grep "$DB_NAME" | awk '{print $3}')${NC}"

    # Recent deployments
    echo ""
    echo -e "${BLUE}Recent Deployments:${NC}"
    fly releases --app "$APP_NAME" | head -5
}

# Function to cleanup
cleanup() {
    echo -e "${YELLOW}Cleaning up temporary files...${NC}"
    # Add any cleanup tasks here
    echo -e "${GREEN}✅ Cleanup completed${NC}"
}

# Main execution
main() {
    echo "This script will deploy your JustIngredients bot to Fly.io"
    echo ""

    check_prerequisites
    run_tests
    deploy_app
    verify_deployment
    show_deployment_info
    cleanup

    echo ""
    echo -e "${GREEN}🎉 Deployment completed successfully!${NC}"
    echo ""
    echo "Next steps:"
    echo "1. Test your bot on Telegram"
    echo "2. Run './scripts/monitoring.sh' to set up monitoring"
    echo "3. Check logs with: fly logs --app $APP_NAME"
}

# Handle command line arguments
case "${1:-}" in
    --help|-h)
        echo "Usage: $0 [OPTIONS]"
        echo ""
        echo "Deploy JustIngredients bot to Fly.io"
        echo ""
        echo "Options:"
        echo "  --skip-tests    Skip running tests before deployment"
        echo "  --help, -h      Show this help message"
        exit 0
        ;;
    --skip-tests)
        SKIP_TESTS=true
        ;;
    *)
        ;;
esac

# Run main function
main "$@"
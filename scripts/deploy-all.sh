#!/bin/bash

# JustIngredients Bot - Main Deployment Orchestrator
# This script runs the complete deployment process

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
PURPLE='\033[0;35m'
NC='\033[0m' # No Color

echo -e "${PURPLE}🚀 JustIngredients Bot - Complete Deployment${NC}"
echo "=============================================="

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Function to check prerequisites
check_global_prerequisites() {
    echo -e "${YELLOW}Checking global prerequisites...${NC}"

    # Check if in correct directory
    if [ ! -f "$PROJECT_ROOT/Cargo.toml" ]; then
        echo -e "${RED}❌ Not in project root. Please run from the project directory.${NC}"
        exit 1
    fi

    # Check if fly CLI is installed
    if ! command -v fly >/dev/null 2>&1; then
        echo -e "${RED}❌ Fly CLI not found. Please install it: curl -L https://fly.io/install.sh | sh${NC}"
        exit 1
    fi

    # Check if authenticated
    if ! fly auth whoami >/dev/null 2>&1; then
        echo -e "${RED}❌ Not authenticated with Fly.io. Run: fly auth login${NC}"
        exit 1
    fi

    # Check if Rust is installed
    if ! command -v cargo >/dev/null 2>&1; then
        echo -e "${RED}❌ Rust/Cargo not found. Please install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Global prerequisites check passed${NC}"
}

# Function to run setup
run_setup() {
    echo -e "${BLUE}Phase 1: Infrastructure Setup${NC}"
    echo "================================"

    cd "$PROJECT_ROOT"
    if ! bash "$SCRIPT_DIR/setup.sh"; then
        echo -e "${RED}❌ Setup failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Setup completed${NC}"
    echo ""
}

# Function to run deployment
run_deployment() {
    echo -e "${BLUE}Phase 2: Application Deployment${NC}"
    echo "=================================="

    cd "$PROJECT_ROOT"
    if ! bash "$SCRIPT_DIR/deploy.sh"; then
        echo -e "${RED}❌ Deployment failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Deployment completed${NC}"
    echo ""
}

# Function to run monitoring setup
run_monitoring() {
    echo -e "${BLUE}Phase 3: Monitoring Setup${NC}"
    echo "==========================="

    cd "$PROJECT_ROOT"
    if ! bash "$SCRIPT_DIR/monitoring.sh"; then
        echo -e "${RED}❌ Monitoring setup failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Monitoring setup completed${NC}"
    echo ""
}

# Function to run final verification
run_verification() {
    echo -e "${BLUE}Phase 4: Final Verification${NC}"
    echo "============================="

    cd "$PROJECT_ROOT"
    if ! bash "$SCRIPT_DIR/maintenance.sh" health; then
        echo -e "${RED}❌ Final verification failed${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Final verification completed${NC}"
    echo ""
}

# Function to show completion summary
show_completion_summary() {
    echo -e "${PURPLE}🎉 Deployment Completed Successfully!${NC}"
    echo "========================================"

    # Get app information
    APP_URL=$(fly status --app just-ingredients-bot 2>/dev/null | grep "Hostname" | awk '{print $2}')
    MONITORING_URL=$(fly status --app just-ingredients-monitoring 2>/dev/null | grep "Hostname" | awk '{print $2}')

    echo ""
    echo -e "${GREEN}🌐 Application URLs:${NC}"
    if [ -n "$APP_URL" ]; then
        echo -e "  Bot Application: https://$APP_URL"
        echo -e "  Health Check:    https://$APP_URL/health"
    fi

    if [ -n "$MONITORING_URL" ]; then
        echo -e "  Grafana:         https://$MONITORING_URL"
        echo -e "  Grafana Admin:   admin / admin"
    fi

    echo ""
    echo -e "${GREEN}🔧 Useful Commands:${NC}"
    echo -e "  View logs:       ./scripts/maintenance.sh logs"
    echo -e "  Check status:    ./scripts/maintenance.sh status"
    echo -e "  Create backup:   ./scripts/maintenance.sh backup"
    echo -e "  Scale resources: ./scripts/maintenance.sh scale 2 4096"
    echo -e "  Health checks:   ./scripts/maintenance.sh health"
    echo -e "  View costs:      ./scripts/maintenance.sh costs"

    echo ""
    echo -e "${GREEN}📚 Next Steps:${NC}"
    echo "1. Test your bot on Telegram with /start"
    echo "2. Send an image to test OCR functionality"
    echo "3. Monitor your application via Grafana"
    echo "4. Set up alerts and notifications as needed"
    echo "5. Regularly backup your database"

    echo ""
    echo -e "${YELLOW}⚠️  Important:${NC}"
    echo "  - Change the default Grafana password"
    echo "  - Monitor costs and scale as needed"
    echo "  - Set up automated backups"
    echo "  - Keep your dependencies updated"
}

# Function to show usage
show_usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Complete deployment orchestrator for JustIngredients bot"
    echo ""
    echo "Options:"
    echo "  --setup-only     Run only the infrastructure setup"
    echo "  --deploy-only    Run only the application deployment"
    echo "  --monitoring-only Run only the monitoring setup"
    echo "  --verify-only    Run only the final verification"
    echo "  --with-monitoring Include monitoring setup in complete deployment"
    echo "  --help, -h       Show this help message"
    echo ""
    echo "Without options, runs setup, deployment, and verification (monitoring is optional)"
    echo ""
    echo "Examples:"
    echo "  $0                    # Setup, deploy, and verify (no monitoring)"
    echo "  $0 --with-monitoring # Complete deployment including monitoring"
    echo "  $0 --setup-only      # Only setup infrastructure"
    echo "  $0 --deploy-only     # Only deploy application"
}

# Main execution
main() {
    echo "This script will deploy your JustIngredients bot to Fly.io"
    echo "It will run through setup, deployment, and verification phases"
    echo "Monitoring setup is optional and can be included with --with-monitoring"
    echo ""

    # Parse command line arguments
    case "${1:-}" in
        --help|-h)
            show_usage
            exit 0
            ;;
        --setup-only)
            check_global_prerequisites
            run_setup
            ;;
        --deploy-only)
            check_global_prerequisites
            run_deployment
            ;;
        --monitoring-only)
            check_global_prerequisites
            run_monitoring
            ;;
        --verify-only)
            check_global_prerequisites
            run_verification
            ;;
        --with-monitoring)
            # Complete deployment including monitoring
            check_global_prerequisites
            run_setup
            run_deployment
            run_monitoring
            run_verification
            show_completion_summary
            ;;
        "")
            # Default deployment (without monitoring)
            check_global_prerequisites
            run_setup
            run_deployment
            run_verification
            show_completion_summary
            ;;
        *)
            echo -e "${RED}❌ Unknown option: $1${NC}"
            echo ""
            show_usage
            exit 1
            ;;
    esac
}

# Run main function
main "$@"
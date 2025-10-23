#!/bin/bash

# JustIngredients Bot - Maintenance Script
# This script handles ongoing maintenance tasks

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
MONITORING_APP_NAME="just-ingredients-monitoring"

echo -e "${BLUE}🔧 JustIngredients Bot - Maintenance${NC}"
echo "====================================="

# Function to show usage
show_usage() {
    echo "Usage: $0 <command> [options]"
    echo ""
    echo "Commands:"
    echo "  status          Show application and database status"
    echo "  logs            Show application logs"
    echo "  backup          Create database backup"
    echo "  restore <id>    Restore database from backup"
    echo "  scale <cpu> <mem> Scale application resources"
    echo "  restart         Restart application"
    echo "  cleanup         Clean up old deployments and resources"
    echo "  health          Run health checks"
    echo "  costs           Show cost information"
    echo "  help            Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 status"
    echo "  $0 logs --tail 50"
    echo "  $0 backup"
    echo "  $0 scale 2 4096"
    echo "  $0 health"
}

# Function to show status
show_status() {
    echo -e "${BLUE}📊 Application Status${NC}"
    echo "======================"

    # App status
    echo -e "${YELLOW}Application:${NC}"
    fly status --app "$APP_NAME" | cat

    echo ""
    echo -e "${YELLOW}Database:${NC}"
    fly postgres list | grep "$DB_NAME" | cat

    echo ""
    echo -e "${YELLOW}Monitoring:${NC}"
    if fly apps list | grep -q "^$MONITORING_APP_NAME"; then
        fly status --app "$MONITORING_APP_NAME" | cat
    else
        echo "Monitoring not set up"
    fi

    echo ""
    echo -e "${YELLOW}Resources:${NC}"
    fly scale show --app "$APP_NAME" | cat
}

# Function to show logs
show_logs() {
    local tail_lines="${1:-50}"

    echo -e "${BLUE}📋 Application Logs (last $tail_lines lines)${NC}"
    echo "=========================================="

    fly logs --app "$APP_NAME" --tail "$tail_lines" | cat
}

# Function to create backup
create_backup() {
    echo -e "${YELLOW}Creating database backup...${NC}"

    # Create backup
    BACKUP_ID=$(fly postgres backups create --app "$DB_NAME")

    if [ -n "$BACKUP_ID" ]; then
        echo -e "${GREEN}✅ Backup created with ID: $BACKUP_ID${NC}"
    else
        echo -e "${RED}❌ Backup creation failed${NC}"
        exit 1
    fi
}

# Function to restore backup
restore_backup() {
    local backup_id="$1"

    if [ -z "$backup_id" ]; then
        echo -e "${RED}❌ Backup ID required${NC}"
        echo "Usage: $0 restore <backup_id>"
        echo ""
        echo "Available backups:"
        fly postgres backups list --app "$DB_NAME" | cat
        exit 1
    fi

    echo -e "${YELLOW}Restoring database from backup $backup_id...${NC}"
    echo -e "${RED}⚠️  This will overwrite your current database!${NC}"

    read -p "Are you sure? (yes/no): " confirm
    if [ "$confirm" != "yes" ]; then
        echo "Restore cancelled"
        exit 0
    fi

    if fly postgres backups restore "$backup_id" --app "$DB_NAME"; then
        echo -e "${GREEN}✅ Database restored from backup${NC}"
    else
        echo -e "${RED}❌ Database restore failed${NC}"
        exit 1
    fi
}

# Function to scale resources
scale_resources() {
    local cpu="$1"
    local memory="$2"

    if [ -z "$cpu" ] || [ -z "$memory" ]; then
        echo -e "${RED}❌ CPU and memory values required${NC}"
        echo "Usage: $0 scale <cpu_count> <memory_mb>"
        echo "Example: $0 scale 2 4096"
        exit 1
    fi

    echo -e "${YELLOW}Scaling application to ${cpu} CPU(s) and ${memory}MB RAM...${NC}"

    fly scale cpu "$cpu" --app "$APP_NAME"
    fly scale memory "$memory" --app "$APP_NAME"

    echo -e "${GREEN}✅ Application scaled${NC}"
    fly scale show --app "$APP_NAME"
}

# Function to restart application
restart_app() {
    echo -e "${YELLOW}Restarting application...${NC}"

    fly restart --app "$APP_NAME"

    echo -e "${GREEN}✅ Application restarted${NC}"
}

# Function to cleanup resources
cleanup_resources() {
    echo -e "${YELLOW}Cleaning up resources...${NC}"

    # Remove old deployments (keep last 3)
    echo "Removing old deployments..."
    fly releases --app "$APP_NAME" | tail -n +4 | awk '{print $1}' | xargs -I {} fly releases delete {} --app "$APP_NAME" 2>/dev/null || true

    # Clean up old database backups (keep last 7)
    echo "Cleaning up old database backups..."
    fly postgres backups list --app "$DB_NAME" | tail -n +8 | awk '{print $1}' | xargs -I {} fly postgres backups delete {} --app "$DB_NAME" 2>/dev/null || true

    echo -e "${GREEN}✅ Cleanup completed${NC}"
}

# Function to run health checks
run_health_checks() {
    echo -e "${BLUE}🏥 Health Checks${NC}"
    echo "================="

    # Get app URL
    APP_URL=$(fly status --app "$APP_NAME" | grep "Hostname" | awk '{print $2}')

    if [ -z "$APP_URL" ]; then
        echo -e "${RED}❌ Could not get application URL${NC}"
        return 1
    fi

    echo -e "${YELLOW}Checking application health...${NC}"
    if curl -f -s "https://$APP_URL/health" >/dev/null; then
        echo -e "${GREEN}✅ Application health check passed${NC}"
    else
        echo -e "${RED}❌ Application health check failed${NC}"
    fi

    echo -e "${YELLOW}Checking database connectivity...${NC}"
    if timeout 10 fly postgres connect --app "$DB_NAME" -c "SELECT 1;" >/dev/null 2>&1; then
        echo -e "${GREEN}✅ Database connectivity check passed${NC}"
    else
        echo -e "${RED}❌ Database connectivity check failed or timed out${NC}"
    fi

    echo -e "${YELLOW}Checking Fly.io app status...${NC}"
    if fly status --app "$APP_NAME" >/dev/null 2>&1; then
        echo -e "${GREEN}✅ Fly.io app status check passed${NC}"
    else
        echo -e "${RED}❌ Fly.io app status check failed${NC}"
    fi
}

# Function to show costs
show_costs() {
    echo -e "${BLUE}💰 Cost Information${NC}"
    echo "==================="

    echo -e "${YELLOW}Application costs:${NC}"
    fly dashboard --app "$APP_NAME" 2>/dev/null | grep -i cost || echo "Cost information not available in CLI"

    echo ""
    echo -e "${YELLOW}Database costs:${NC}"
    fly postgres list | grep "$DB_NAME" | cat

    echo ""
    echo -e "${BLUE}💡 Cost Optimization Tips:${NC}"
    echo "- Monitor usage with: fly metrics --app $APP_NAME"
    echo "- Scale down when not in use: fly scale count 0"
    echo "- Use auto-scaling: fly autoscale set min=1 max=3"
    echo "- Check resource usage: fly scale show"
}

# Main execution
main() {
    case "${1:-}" in
        status)
            show_status
            ;;
        logs)
            show_logs "$2"
            ;;
        backup)
            create_backup
            ;;
        restore)
            restore_backup "$2"
            ;;
        scale)
            scale_resources "$2" "$3"
            ;;
        restart)
            restart_app
            ;;
        cleanup)
            cleanup_resources
            ;;
        health)
            run_health_checks
            ;;
        costs)
            show_costs
            ;;
        help|--help|-h)
            show_usage
            ;;
        *)
            echo -e "${RED}❌ Unknown command: $1${NC}"
            echo ""
            show_usage
            exit 1
            ;;
    esac
}

# Run main function
main "$@"
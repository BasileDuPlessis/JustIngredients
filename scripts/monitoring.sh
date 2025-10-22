#!/bin/bash

# JustIngredients Bot - Monitoring Setup Script
# This script sets up monitoring infrastructure (Grafana + Prometheus)

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
APP_NAME="just-ingredients-bot"
MONITORING_APP_NAME="just-ingredients-monitoring"
REGION="cdg"  # Paris, France
ORG="personal"

echo -e "${BLUE}📊 JustIngredients Bot - Monitoring Setup${NC}"
echo "==========================================="

# Function to check prerequisites
check_prerequisites() {
    echo -e "${YELLOW}Checking prerequisites...${NC}"

    # Check if fly CLI is installed
    if ! command -v fly >/dev/null 2>&1; then
        echo -e "${RED}❌ Fly CLI not found${NC}"
        exit 1
    fi

    # Check if main app exists
    if ! fly apps list | grep -q "^$APP_NAME"; then
        echo -e "${RED}❌ Main application '$APP_NAME' not found. Run setup first.${NC}"
        exit 1
    fi

    echo -e "${GREEN}✅ Prerequisites check passed${NC}"
}

# Function to create monitoring app
create_monitoring_app() {
    echo -e "${YELLOW}Creating monitoring application...${NC}"

    if fly apps list | grep -q "^$MONITORING_APP_NAME"; then
        echo -e "${YELLOW}⚠️  Monitoring app '$MONITORING_APP_NAME' already exists${NC}"
        return
    fi

    # Create monitoring app
    fly launch --name "$MONITORING_APP_NAME" --region "$REGION" --no-deploy --org "$ORG"

    echo -e "${GREEN}✅ Monitoring application created${NC}"
}

# Function to configure monitoring fly.toml
configure_monitoring_fly_toml() {
    echo -e "${YELLOW}Configuring monitoring fly.toml...${NC}"

    # Create monitoring directory if it doesn't exist
    mkdir -p monitoring

    # Create fly.toml for monitoring
    cat > monitoring/fly.toml << EOF
app = "$MONITORING_APP_NAME"
primary_region = "$REGION"

[build]
  dockerfile = "Dockerfile.monitoring"

[env]
  GF_SECURITY_ADMIN_PASSWORD = "admin"
  GF_USERS_ALLOW_SIGN_UP = "false"

[http_service]
  internal_port = 3000
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 1
  processes = ["app"]

[[vm]]
  cpu_kind = "shared"
  cpus = 1
  memory_mb = 1024
EOF

    echo -e "${GREEN}✅ Monitoring fly.toml configured${NC}"
}

# Function to create monitoring Dockerfile
create_monitoring_dockerfile() {
    echo -e "${YELLOW}Creating monitoring Dockerfile...${NC}"

    cat > monitoring/Dockerfile.monitoring << 'EOF'
FROM grafana/grafana:latest

# Copy provisioning files
COPY provisioning /etc/grafana/provisioning/

# Copy dashboards
COPY dashboards /var/lib/grafana/dashboards/

# Set permissions
USER root
RUN chown -R grafana:grafana /etc/grafana /var/lib/grafana
USER grafana
EOF

    echo -e "${GREEN}✅ Monitoring Dockerfile created${NC}"
}

# Function to copy Grafana configuration
copy_grafana_config() {
    echo -e "${YELLOW}Setting up Grafana configuration...${NC}"

    # Copy existing grafana configuration
    if [ -d "grafana" ]; then
        cp -r grafana/* monitoring/
        echo -e "${GREEN}✅ Grafana configuration copied${NC}"
    else
        echo -e "${YELLOW}⚠️  No existing grafana directory found, creating basic config${NC}"

        # Create basic provisioning
        mkdir -p monitoring/provisioning/datasources
        mkdir -p monitoring/provisioning/dashboards
        mkdir -p monitoring/dashboards

        # Create Prometheus datasource
        cat > monitoring/provisioning/datasources/prometheus.yml << EOF
apiVersion: 1

datasources:
  - name: Prometheus
    type: prometheus
    access: proxy
    url: http://prometheus:9090
    isDefault: true
EOF

        # Create dashboard provisioning
        cat > monitoring/provisioning/dashboards/dashboards.yml << EOF
apiVersion: 1

providers:
  - name: 'JustIngredients'
    type: file
    disableDeletion: false
    updateIntervalSeconds: 10
    allowUiUpdates: true
    options:
      path: /var/lib/grafana/dashboards
EOF

        echo -e "${GREEN}✅ Basic Grafana configuration created${NC}"
    fi
}

# Function to deploy monitoring
deploy_monitoring() {
    echo -e "${YELLOW}Deploying monitoring stack...${NC}"

    cd monitoring

    # Deploy to Fly.io
    if ! fly deploy --remote-only; then
        echo -e "${RED}❌ Monitoring deployment failed${NC}"
        cd ..
        exit 1
    fi

    cd ..
    echo -e "${GREEN}✅ Monitoring stack deployed${NC}"
}

# Function to configure health checks
configure_health_checks() {
    echo -e "${YELLOW}Configuring health checks...${NC}"

    # Get main app URL
    MAIN_APP_URL=$(fly status --app "$APP_NAME" | grep "Hostname" | awk '{print $2}')

    if [ -n "$MAIN_APP_URL" ]; then
        echo "Main application URL: https://$MAIN_APP_URL"

        # Test health endpoint
        if curl -f -s "https://$MAIN_APP_URL/health" >/dev/null; then
            echo -e "${GREEN}✅ Health check endpoint is responding${NC}"
        else
            echo -e "${YELLOW}⚠️  Health check endpoint not responding yet${NC}"
        fi
    fi
}

# Function to show monitoring info
show_monitoring_info() {
    echo ""
    echo -e "${BLUE}📊 Monitoring Information${NC}"
    echo "========================="

    # Monitoring app URL
    MONITORING_URL=$(fly status --app "$MONITORING_APP_NAME" 2>/dev/null | grep "Hostname" | awk '{print $2}')
    if [ -n "$MONITORING_URL" ]; then
        echo -e "Grafana URL: ${GREEN}https://$MONITORING_URL${NC}"
        echo -e "Grafana Admin: ${GREEN}admin / admin${NC}"
    fi

    # Main app URL
    MAIN_APP_URL=$(fly status --app "$APP_NAME" | grep "Hostname" | awk '{print $2}')
    echo -e "Application URL: ${GREEN}https://$MAIN_APP_URL${NC}"
    echo -e "Health Check: ${GREEN}https://$MAIN_APP_URL/health${NC}"

    echo ""
    echo -e "${BLUE}Useful Commands:${NC}"
    echo "View app logs: fly logs --app $APP_NAME"
    echo "View monitoring logs: fly logs --app $MONITORING_APP_NAME"
    echo "Check app status: fly status --app $APP_NAME"
    echo "Check database: fly postgres connect --app just-ingredients-db"
}

# Main execution
main() {
    echo "This script will set up monitoring for your JustIngredients bot"
    echo ""

    check_prerequisites
    create_monitoring_app
    configure_monitoring_fly_toml
    create_monitoring_dockerfile
    copy_grafana_config
    deploy_monitoring
    configure_health_checks
    show_monitoring_info

    echo ""
    echo -e "${GREEN}🎉 Monitoring setup completed!${NC}"
    echo ""
    echo "Next steps:"
    echo "1. Access Grafana at the URL shown above"
    echo "2. Import your dashboards"
    echo "3. Set up alerts if needed"
}

# Run main function
main "$@"
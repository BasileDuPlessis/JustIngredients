#!/bin/bash

# JustIngredients Monitoring Setup Script

set -e

echo "🚀 Setting up JustIngredients Monitoring Stack"

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is not installed. Please install Docker first."
    exit 1
fi

# Check if Docker Compose is installed
if ! command -v docker-compose &> /dev/null && ! docker compose version &> /dev/null; then
    echo "❌ Docker Compose is not installed. Please install Docker Compose first."
    exit 1
fi

# Create necessary directories
echo "📁 Creating directories..."
mkdir -p grafana/provisioning/datasources
mkdir -p grafana/provisioning/dashboards

# Start the monitoring stack
echo "🐳 Starting monitoring services..."
if command -v docker-compose &> /dev/null; then
    docker-compose up -d
else
    docker compose up -d
fi

echo "⏳ Waiting for services to start..."
sleep 10

# Check if services are running
echo "🔍 Checking service status..."

if curl -s http://localhost:9090/-/healthy > /dev/null; then
    echo "✅ Prometheus is running on http://localhost:9090"
else
    echo "❌ Prometheus is not responding"
fi

if curl -s http://localhost:3000/api/health > /dev/null; then
    echo "✅ Grafana is running on http://localhost:3000 (admin/admin)"
else
    echo "❌ Grafana is not responding"
fi

if curl -s http://localhost:9093/-/healthy > /dev/null; then
    echo "✅ Alertmanager is running on http://localhost:9093"
else
    echo "❌ Alertmanager is not responding"
fi

echo ""
echo "🎉 Monitoring stack setup complete!"
echo ""
echo "Next steps:"
echo "1. Start your JustIngredients bot: cargo run"
echo "2. Open Grafana: http://localhost:3000 (admin/admin)"
echo "3. Import dashboards from the grafana/ directory"
echo "4. Check metrics: http://localhost:9090"
echo ""
echo "Useful commands:"
echo "- View logs: docker-compose logs -f [service]"
echo "- Stop services: docker-compose down"
echo "- Restart services: docker-compose restart"
# Production Deployment Strategy for JustIngredients Telegram Bot - Fly.io

## Overview
This document outlines the automated deployment strategy for the JustIngredients Telegram bot using Fly.io. All deployment tasks are scripted to minimize manual intervention.

## Prerequisites
- [ ] Fly.io account created and CLI installed (`curl -L https://fly.io/install.sh | sh`)
- [ ] GitHub repository set up
- [ ] Telegram bot token obtained from @BotFather
- [ ] Rust toolchain installed

## One-Command Deployment

### Complete Automated Deployment
```bash
# Run the complete deployment process
./scripts/deploy-all.sh
```

This single command will:
1. Set up Fly.io infrastructure (app, database, secrets)
2. Deploy the application
3. Configure monitoring
4. Run final verification

### Individual Phase Deployment
```bash
# Setup infrastructure only
./scripts/deploy-all.sh --setup-only

# Deploy application only
./scripts/deploy-all.sh --deploy-only

# Setup monitoring only
./scripts/deploy-all.sh --monitoring-only

# Run verification only
./scripts/deploy-all.sh --verify-only
```

## Scripts Overview

### `scripts/setup.sh`
**Purpose**: Initial Fly.io infrastructure setup
- Creates Fly.io application
- Sets up PostgreSQL database
- Configures secrets (Telegram token)
- Enables database backups

**Usage**:
```bash
./scripts/setup.sh
```

### `scripts/deploy.sh`
**Purpose**: Application deployment
- Runs tests and linting
- Builds and deploys to Fly.io
- Verifies deployment health
- Shows deployment information

**Usage**:
```bash
./scripts/deploy.sh
```

**Options**:
- `--skip-tests`: Skip running tests before deployment

### `scripts/monitoring.sh`
**Purpose**: Monitoring infrastructure setup
- Creates Grafana monitoring application
- Configures dashboards and datasources
- Deploys monitoring stack

**Usage**:
```bash
./scripts/monitoring.sh
```

### `scripts/maintenance.sh`
**Purpose**: Ongoing maintenance operations

**Commands**:
```bash
./scripts/maintenance.sh status          # Show app and database status
./scripts/maintenance.sh logs [lines]    # Show application logs
./scripts/maintenance.sh backup          # Create database backup
./scripts/maintenance.sh restore <id>    # Restore from backup
./scripts/maintenance.sh scale <cpu> <mem>  # Scale resources
./scripts/maintenance.sh restart         # Restart application
./scripts/maintenance.sh cleanup         # Clean up old resources
./scripts/maintenance.sh health          # Run health checks
./scripts/maintenance.sh costs           # Show cost information
```

## Configuration Files

### `fly.toml`
The Fly.io configuration file is pre-configured with optimal settings:

```toml
app = "just-ingredients-bot"
primary_region = "cdg"

[build]
  builder = "rust"
  buildpacks = []

[env]
  RUST_LOG = "info,sqlx=warn"
  LOG_FORMAT = "json"
  OCR_LANGUAGES = "eng+fra"
  CIRCUIT_BREAKER_THRESHOLD = "5"
  CIRCUIT_BREAKER_TIMEOUT_SECS = "60"
  HEALTH_PORT = "8080"

[http_service]
  internal_port = 8080
  force_https = true
  auto_stop_machines = true
  auto_start_machines = true
  min_machines_running = 1
  processes = ["app"]

[[vm]]
  cpu_kind = "shared"
  cpus = 1
  memory_mb = 2048
```

## Environment Variables

The following environment variables are automatically configured:

| Variable | Description | Default |
|----------|-------------|---------|
| `TELEGRAM_BOT_TOKEN` | Bot authentication token | User input required |
| `DATABASE_URL` | PostgreSQL connection string | Auto-configured |
| `RUST_LOG` | Logging level | `info,sqlx=warn` |
| `LOG_FORMAT` | Log output format | `json` |
| `OCR_LANGUAGES` | Tesseract languages | `eng+fra` |
| `CIRCUIT_BREAKER_THRESHOLD` | OCR failure threshold | `5` |
| `CIRCUIT_BREAKER_TIMEOUT_SECS` | Circuit breaker reset time | `60` |
| `HEALTH_PORT` | Health check port | `8080` |

### Infrastructure Details

### Application
- **Runtime**: Rust application on Fly.io
- **Region**: Paris, France (cdg) for optimal performance
- **Resources**: 1 CPU, 2GB RAM (configurable)
- **Auto-scaling**: Enabled with min 1, max configurable

### Database
- **Type**: PostgreSQL via Fly Postgres
- **Region**: Paris, France (cdg)
- **Backups**: Automated daily backups
- **Connection**: Automatic via `DATABASE_URL`

### Monitoring
- **Grafana**: Separate Fly.io application
- **Dashboards**: Pre-configured for application metrics
- **Access**: Admin credentials (change after setup)

## Post-Deployment Tasks

### Immediate Actions
1. **Test the Bot**: Send `/start` command on Telegram
2. **Test OCR**: Send an image to verify processing
3. **Change Grafana Password**: Update default admin credentials
4. **Verify Monitoring**: Check Grafana dashboards

### Ongoing Maintenance
```bash
# Daily health checks
./scripts/maintenance.sh health

# Weekly cleanup
./scripts/maintenance.sh cleanup

# Monthly cost review
./scripts/maintenance.sh costs

# Regular backups (automated)
./scripts/maintenance.sh backup
```

## Troubleshooting

### Quick Status Commands

**Health Check**:
```bash
curl -v https://just-ingredients-bot.fly.dev/health/live
```

**Application Status**:
```bash
fly status --app just-ingredients-bot
```

**Recent Application Logs**:
```bash
fly logs --app just-ingredients-bot
```

### Common Issues

**Deployment Fails**
```bash
# Check Fly.io status
fly status --app just-ingredients-bot

# View deployment logs
fly logs --app just-ingredients-bot

# Check build status
fly builds list --app just-ingredients-bot
```

**Database Connection Issues**
```bash
# Test database connectivity
./scripts/maintenance.sh health

# Check database status
fly postgres list
```

**Application Not Responding**
```bash
# Check application status
./scripts/maintenance.sh status

# Restart application
./scripts/maintenance.sh restart

# View recent logs
./scripts/maintenance.sh logs 100
```

### Emergency Procedures

**Rollback Deployment**
```bash
# List recent deployments
fly releases --app just-ingredients-bot

# Rollback to previous version
fly deploy --image <previous-image-id>
```

**Database Restore**
```bash
# List available backups
fly postgres backups list --app just-ingredients-db

# Restore from backup
./scripts/maintenance.sh restore <backup-id>
```

**Scale Down for Issues**
```bash
# Temporarily stop application
fly scale count 0 --app just-ingredients-bot

# Restart with minimal resources
fly scale count 1 --app just-ingredients-bot
```

## Cost Optimization

### Monitoring Costs
```bash
# View current costs
./scripts/maintenance.sh costs

# Monitor resource usage
fly metrics --app just-ingredients-bot
```

### Scaling Strategies
```bash
# Scale up for high load
./scripts/maintenance.sh scale 2 4096

# Enable auto-scaling
fly autoscale set min=1 max=3 --app just-ingredients-bot

# Scale down when not in use
fly scale count 0 --app just-ingredients-bot
```

## Security Considerations

### Secrets Management
- All secrets stored securely in Fly.io
- Telegram token encrypted at rest
- Database credentials managed by Fly.io

### Network Security
- HTTPS enforced on all endpoints
- Database access restricted to application
- Private networking between services

### Access Control
- Grafana admin password should be changed
- Database backups encrypted
- Application logs monitored

## Performance Tuning

### Resource Allocation
- Start with 1 CPU, 2GB RAM for OCR processing
- Monitor memory usage during peak times
- Scale vertically before horizontal scaling

### Database Optimization
- Connection pooling handled by sqlx
- Full-text search indexes maintained automatically
- Regular backup cleanup prevents storage bloat

### OCR Optimization
- Instance pooling reduces initialization time
- Circuit breaker prevents cascade failures
- Timeout protection for long-running operations

This automated deployment strategy ensures reliable, secure, and maintainable production deployment of the JustIngredients bot with minimal manual intervention.
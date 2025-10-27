# Minimal Cost Fly.io Deployment Guide for JustIngredients Telegram Bot

This guide provides steps to deploy the JustIngredients Telegram bot and PostgreSQL database to Fly.io with **minimal infrastructure costs** (targeting free/low-cost tiers). The deployment is optimized for cost efficiency while maintaining functionality.

## Cost Optimization Overview

**Target Costs**: Free tier usage with minimal charges (~$0-5/month)

**Key Strategies**:
- Use shared-cpu-1x VMs with 256MB RAM
- Scale app to 0 instances during low traffic
- Use Fly Postgres Lite (1GB volume, shared CPU)
- Leverage free outbound bandwidth and requests
- Monitor usage to stay within free tiers

**Estimated Monthly Costs**:
- App: $0 (free tier with scaling to 0)
- Database: $0-2 (Fly Postgres Lite)
- Bandwidth: $0 (free outbound)
- **Total: $0-2/month** with proper scaling

## Prerequisites

Before starting, ensure you have:

- A Fly.io account (free tier available)
- Fly CLI installed (`curl -L https://fly.io/install.sh | sh`)
- Telegram bot token from [@BotFather](https://t.me/botfather)
- Rust toolchain installed locally for testing
- Docker installed locally for testing builds

## Architecture Overview (Cost-Optimized)

The minimal cost deployment consists of:
- **Bot Service**: Single shared-cpu-1x VM (256MB RAM), scales to 0 when idle
- **Database**: Fly Postgres Lite (shared CPU, 1GB volume)
- **Secrets**: Environment variables for tokens and database credentials

```
Telegram ← HTTPS → Fly App (bot, scales 0-1) ← TLS → Fly Postgres Lite
```

**Cost Benefits**:
- App scales to 0 when no traffic (no compute costs)
- Database uses shared resources (minimal fixed cost)
- No load balancer or additional services needed

## Step 1: Authenticate with Fly.io

```bash
fly auth login
```

Verify your account:
```bash
fly auth whoami
```

## Step 2: Create Fly Postgres Database (Minimal Cost)

Create the cheapest PostgreSQL database:

```bash
# Use Lite plan for minimal cost (shared-cpu-1x, 1GB volume)
fly postgres create --name just-ingredients-db --region cdg --vm-size shared-cpu-1x --volume-size 1
```

This creates:
- **Fly Postgres Lite**: Shared CPU, 1GB volume (~$0-2/month)
- Internal DNS: `just-ingredients-db.internal:5432`
- Connection string format: `postgres://username:password@just-ingredients-db.internal:5432/ingredients`

**Cost Note**: Lite plan is free for low usage, charges only for actual compute/bandwidth used.

## Step 3: Create Fly Application

Launch the application (this creates fly.toml):

```bash
fly launch --name just-ingredients --region cdg --no-deploy
```

This generates a `fly.toml` file. Update it with minimal resource allocation:

```toml
[build]
  dockerfile = "Dockerfile"

[env]
  HEALTH_PORT = "8080"
  LOG_FORMAT = "pretty"
  RUST_LOG = "info,sqlx=warn"

[checks]
  [checks.health]
    port = 8080
    type = "http"
    interval = "15s"
    timeout = "2s"
    grace_period = "30s"
    path = "/health"

[[vm]]
  size = "shared-cpu-1x"
  memory_mb = 256  # Minimal memory for OCR processing
```

**Cost Optimization**: 256MB is the minimum for reliable OCR processing while staying in free tier limits.

## Step 4: Configure Secrets

Set required secrets:

```bash
# Telegram bot token
fly secrets set TELEGRAM_BOT_TOKEN=your_telegram_bot_token_here

# Database connection (get from fly postgres attach or status)
fly secrets set DATABASE_URL=postgres://username:password@just-ingredients-db.internal:5432/ingredients
```

**Security Note**: Never commit secrets to version control.

## Step 5: Deploy the Application

Deploy the container:

```bash
fly deploy
```

Monitor deployment:

```bash
fly logs
fly status
```

## Step 6: Verify Deployment

Check application health:

```bash
# Check if app is running
fly status

# Test health endpoint
curl https://just-ingredients.fly.dev/health

# Check logs for errors
fly logs --tail=50
```

Test the bot by sending a message to your Telegram bot.

## Step 8: Enable Auto-Scaling to Zero (Cost Optimization)

**Critical for minimal costs**: Scale to 0 when idle to avoid compute charges.

```bash
# Scale to 0 instances when no traffic (free when stopped)
fly scale count 0

# Verify scaling
fly status
```

**How it works**:
- Fly automatically starts the app when HTTP requests arrive
- App runs only when processing messages
- No charges when scaled to 0
- Instant startup from stopped state

**Monitor scaling**:
```bash
# Check current scale
fly status

# View scaling events
fly logs | grep -i scale
```

**Note**: Database remains running (minimal cost), app auto-starts on demand.

## Common Deployment Issues and Solutions

### 1. Build Failures

**Problem**: Docker build fails due to missing dependencies.

**Solution**:
- Ensure Dockerfile includes all required packages
- Test locally: `docker build -t just-ingredients .`
- Check Fly build logs: `fly logs --build`

**Potential Issues**:
- Tesseract libraries not installed in runtime image
- Rust version mismatch between local and Fly
- Missing build dependencies in builder stage

### 2. Database Connection Issues

**Problem**: App can't connect to PostgreSQL.

**Solutions**:
```bash
# Check database status
fly postgres status -a just-ingredients-db

# Verify connection string
fly secrets list

# Test connection from app
fly logs | grep -i "database\|postgres"
```

**Potential Issues**:
- Wrong DATABASE_URL format
- Database not in same region as app
- Firewall rules blocking internal connections
- Database credentials expired

### 3. Runtime Errors

**Problem**: App starts but encounters runtime errors.

**Common Issues**:
- Missing environment variables
- File system permissions (Fly uses read-only root filesystem)
- Temporary file creation failures
- OCR processing timeouts

**Debugging**:
```bash
# Check environment variables
fly secrets list
fly env list

# Monitor resource usage
fly vm status <vm-id>

# Check for OOM kills
fly logs | grep -i "killed\|oom"
```

### 4. Telegram Bot Issues

**Problem**: Bot doesn't respond to messages.

**Solutions**:
- Verify TELEGRAM_BOT_TOKEN is correct
- Check webhook vs polling mode (app uses polling)
- Ensure outbound HTTPS is allowed (Fly allows it)
- Test bot token locally first

### 5. Health Check Failures

**Problem**: Health checks fail, causing restarts.

**Solutions**:
- Ensure HEALTH_PORT is set and /health endpoint responds
- Check health check configuration in fly.toml
- Verify database connectivity in health checks

### 6. Scaling and Performance (Cost-Conscious)

**Problem**: App is slow or unresponsive under load.

**Cost-Optimized Solutions**:
```bash
# Scale up only when needed (avoid permanent scaling)
fly scale count 2

# Scale back down immediately after peak
fly scale count 1

# For minimal cost, prefer scaling to 0 over permanent instances
fly scale count 0  # When not in use
```

**Performance vs Cost Balance**:
- Use 256MB RAM for normal operation
- Scale up to 512MB only during high OCR load
- Monitor memory usage: `fly vm status`
- Database connection pooling prevents over-scaling

**Potential Issues**:
- Shared CPU limitations (acceptable for low traffic)
- Memory constraints (256MB may cause OOM for large images)
- Database connection pool exhaustion (rare with low traffic)

### 7. Cost Optimization (Critical)

**Problem**: Unexpected charges exceeding free tier.

**Solutions**:
```bash
# Scale to 0 during low traffic (primary cost saving)
fly scale count 0

# Monitor current costs
fly dashboard

# Check resource usage
fly vm status
fly postgres status
```

**Free Tier Limits** (as of 2024):
- 512MB RAM hours/month free
- 1 shared CPU free
- Unlimited outbound bandwidth
- 100GB data transfer free

**Cost Breakdown**:
- **App**: $0 when scaled to 0, ~$0.02/hour when running
- **Database**: $0-2/month (Lite plan)
- **Bandwidth**: $0 (outbound free)
- **Total**: $0-2/month with proper scaling

**Optimization Tips**:
- Scale to 0 outside business hours
- Monitor usage weekly
- Use smallest possible VM size
- Keep database volume under 1GB

### 8. Local vs Fly Environment Differences

**Critical Differences**:

| Aspect | Local | Fly.io |
|--------|-------|--------|
| File System | Read-write | Read-only root, /tmp available |
| Commands | All available | Limited to container commands |
| Networking | Full access | Internal DNS, outbound HTTPS |
| Environment | Custom | Containerized, secrets via env |
| Processes | Background allowed | Single foreground process |
| Memory | System RAM | VM limits (512MB default) |

**Problem**: Code works locally but fails on Fly.

**Common Causes**:
- Hardcoded file paths (`/tmp` vs `/app/tmp`)
- Missing environment variable fallbacks
- Synchronous operations blocking the event loop
- Large file processing exceeding memory limits

### 9. Database Migration Issues

**Problem**: Schema changes break existing deployments.

**Solutions**:
- Test migrations locally first
- Use transactions for schema changes
- Backup before deploying: `fly postgres backup`
- Rollback strategy: redeploy previous version

### 10. Monitoring and Debugging

**Essential Commands**:
```bash
# Real-time logs
fly logs --tail

# Historical logs
fly logs --since 1h

# App status
fly status

# Database status
fly postgres status

# VM information
fly vm status

# Metrics dashboard
fly dashboard
```

## Staging Environment Setup

For testing before production:

```bash
# Create staging app
fly launch --name just-ingredients-staging --region cdg --no-deploy

# Create staging database
fly postgres create --name just-ingredients-staging-db --region cdg

# Use separate Telegram bot token for staging
fly secrets set -a just-ingredients-staging TELEGRAM_BOT_TOKEN=staging_token

# Deploy to staging
fly deploy -a just-ingredients-staging
```

## Rollback Strategy

If deployment fails:

```bash
# Rollback to previous version
fly releases

# Redeploy specific version
fly deploy --image <previous-image-id>

# Emergency stop
fly scale count 0
```

## Maintenance Tasks

### Regular Monitoring (Cost-Focused)
- **Daily**: Check if app scaled to 0: `fly status`
- **Weekly**: Review costs: `fly dashboard`
- **Weekly**: Monitor database size: `fly postgres status`
- **Monthly**: Analyze usage patterns and adjust scaling strategy

### Cost Alerts Setup
```bash
# Set up billing alerts in Fly dashboard
# Monitor for unexpected usage spikes
fly dashboard  # Check billing section
```

### Updates
```bash
# Update Fly CLI
fly version update

# Update app (minimal downtime with scaling)
fly deploy

# Update database (if needed)
fly postgres update
```

### Backup Strategy
```bash
# Manual backup (minimal cost)
fly postgres backup

# Automated backups (enabled by default on Lite plan)
fly postgres status  # Check backup schedule
```

## Troubleshooting Checklist

Before deploying:
- [ ] `cargo test` passes locally
- [ ] `cargo build --release` succeeds
- [ ] `docker build -t just-ingredients .` works
- [ ] Database credentials are valid
- [ ] Telegram token is correct
- [ ] fly.toml uses minimal resources (256MB RAM, shared-cpu-1x)

After deploying:
- [ ] `fly status` shows running (or scaled to 0)
- [ ] `fly logs` shows no critical errors
- [ ] Health checks pass when running
- [ ] Bot responds to test messages
- [ ] Database tables are created
- [ ] **Cost check**: `fly dashboard` shows minimal charges
- [ ] **Scaling check**: App scales to 0 when idle

## Cost Troubleshooting

**High Costs Detected**:
- [ ] Check `fly status` - is app running when it should be scaled to 0?
- [ ] Review `fly logs --since 24h` for unexpected restarts
- [ ] Verify VM size: `fly vm status` (should be shared-cpu-1x, 256MB)
- [ ] Check database usage: `fly postgres status` (should be Lite plan)
- [ ] Monitor bandwidth: high usage may indicate inefficient requests

**Free Tier Optimization**:
- [ ] Scale to 0 during off-hours: `fly scale count 0`
- [ ] Use smallest possible memory allocation
- [ ] Minimize database storage (keep under 1GB)
- [ ] Avoid permanent scaling - use on-demand scaling

## Support Resources

- [Fly.io Docs](https://fly.io/docs/)
- [Fly Postgres Guide](https://fly.io/docs/postgres/)
- [Telegram Bot API](https://core.telegram.org/bots/api)
- [Rust on Fly](https://fly.io/docs/languages-and-frameworks/rust/)

## Emergency Contacts

If critical issues occur:
1. Check `fly logs --tail=100`
2. Scale down: `fly scale count 0`
3. Contact Fly support via dashboard
4. Check Telegram bot status
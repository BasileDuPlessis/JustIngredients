# Deployment Guide

This guide covers deploying JustIngredients to Fly.io from scratch, including database setup and application deployment.

## Prerequisites

- [Fly.io account](https://fly.io/docs/getting-started/)
- [Fly CLI installed](https://fly.io/docs/flyctl/install/)
- Telegram bot token from [@BotFather](https://t.me/botfather)

## Automated Deployment

The easiest way to deploy is using the included `deploy.sh` script:

```bash
# Clone the repository
git clone https://github.com/BasileDuPlessis/JustIngredients.git
cd JustIngredients

# Set your bot token (will be prompted if not set)
export TELEGRAM_BOT_TOKEN='your_bot_token_here'  # Optional: set beforehand or enter interactively

# Run the deployment script
# Run the deployment script
./scripts/deploy.sh
```

The script will:
1. Verify Fly CLI installation and authentication
2. Create the Fly.io app (if it doesn't exist)
3. Create a PostgreSQL database (if it doesn't exist)
4. Attach the database to the app
5. Set required secrets
6. Deploy the application

## Manual Deployment Steps

If you prefer manual control or need to customize the deployment:

### 1. Authenticate with Fly.io

```bash
fly auth login
```

### 2. Create the Application

```bash
fly apps create just-ingredients --org personal
```

### 3. Create PostgreSQL Database

```bash
fly postgres create --name just-ingredients-db --region cdg --org personal --vm-size shared-cpu-1x --volume-size 1
```

### 4. Attach Database to App

```bash
fly postgres attach just-ingredients-db --app just-ingredients
```

This automatically sets the `DATABASE_URL` environment variable.

### 5. Set Secrets

```bash
fly secrets set TELEGRAM_BOT_TOKEN='your_bot_token_here' --app just-ingredients
```

### 6. Deploy the Application

```bash
fly deploy --app just-ingredients
```

## Configuration

### Environment Variables

The application supports the following environment variables (set automatically by Fly.io):

- `TELEGRAM_BOT_TOKEN`: Your Telegram bot token (required)
- `DATABASE_URL`: PostgreSQL connection string (set by database attachment)
- `HEALTH_PORT`: Port for health checks (default: 8080)
- `LOG_FORMAT`: Log format - `json` or `pretty` (default: pretty)
- `RUST_LOG`: Log level configuration (default: info,sqlx=warn)

### Fly.io Configuration

The `deploy/fly.toml` file contains:

- HTTP service configuration with auto-scaling settings
- Health check endpoints
- Build configuration with Docker
- **High availability disabled** (`enable_ha = false`) for single-machine operation
- Environment variables

Key settings for cost optimization:
- `min_machines_running = 1`: Keeps app running continuously
- `auto_stop_machines = false`: Prevents auto-stopping
- `enable_ha = false`: Single machine only (no HA overhead)

## Post-Deployment Verification

### Check Application Status

```bash
fly status --app just-ingredients
```

### View Logs

```bash
fly logs --app just-ingredients
```

### Test Health Endpoint

```bash
curl https://just-ingredients.fly.dev/health/live
# Should return "OK"
```

### Test the Bot

1. Start a chat with your Telegram bot
2. Send a test message or image
3. Check logs for processing confirmation

## Troubleshooting

### Common Issues

**App not starting:**
- Check health checks are passing: `fly checks list --app just-ingredients`
- Verify secrets are set: `fly secrets list --app just-ingredients`

**Database connection errors:**
- Ensure database is attached: `fly postgres list`
- Check DATABASE_URL is set correctly

**OCR failures:**
- Verify Tesseract dependencies in deploy/Dockerfile
- Check instance pooling configuration

**Telegram webhook errors:**
- Confirm bot token is valid
- Check polling vs webhook configuration

### Scaling and Performance

For production use:
- Monitor resource usage: `fly vm status --app just-ingredients`
- Adjust RAM if needed (current: 256MB)
- Consider upgrading to dedicated CPU for high traffic

### Cost Optimization

Current configuration is optimized for low cost:
- Free tier eligible with shared CPU
- Minimal RAM allocation
- Database: 1GB storage, shared CPU

Monitor usage at: https://fly.io/dashboard

## Updating Deployment

To update an existing deployment:

```bash
# Make changes to code
git commit -am "Your changes"

# Deploy updates
fly deploy --app just-ingredients
```

The script handles existing resources gracefully and will only create what's missing.

## Cleanup

To remove everything:

```bash
# Remove app
fly apps destroy just-ingredients

# Remove database
fly postgres destroy just-ingredients-db
```

**Warning:** This permanently deletes all data. Backup first if needed.
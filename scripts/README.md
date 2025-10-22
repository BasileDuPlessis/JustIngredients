# Deployment Scripts

This directory contains automated scripts for deploying the JustIngredients Telegram bot to Fly.io.

## Scripts Overview

### `deploy-all.sh` - Main Orchestrator
**Purpose**: Complete automated deployment process
```bash
# Complete deployment
./scripts/deploy-all.sh

# Individual phases
./scripts/deploy-all.sh --setup-only      # Infrastructure setup only
./scripts/deploy-all.sh --deploy-only     # Application deployment only
./scripts/deploy-all.sh --monitoring-only # Monitoring setup only
./scripts/deploy-all.sh --verify-only     # Verification only
```

### `setup.sh` - Infrastructure Setup
**Purpose**: Set up Fly.io infrastructure (database, app, secrets)
- Creates PostgreSQL database with auto-generated secure password
- Creates Fly.io application
- Attaches database to application
- Configures secrets (Telegram bot token, auto-generated database URL)
- Enables automated backups

**Database Security**: Fly.io automatically generates a cryptographically secure password for your database, providing enterprise-grade security without manual password management.

### `deploy.sh` - Application Deployment
**Purpose**: Build and deploy the application
- Runs tests and linting
- Builds and deploys to Fly.io
- Verifies deployment health
- Shows deployment information

**Options**:
- `--skip-tests`: Skip test execution before deployment

### `monitoring.sh` - Monitoring Setup
**Purpose**: Set up monitoring infrastructure
- Creates Grafana application
- Configures dashboards and datasources
- Deploys monitoring stack

### `maintenance.sh` - Ongoing Maintenance
**Purpose**: Manage deployed application

**Commands**:
```bash
./scripts/maintenance.sh status           # Show status
./scripts/maintenance.sh logs [lines]     # Show logs
./scripts/maintenance.sh backup           # Create backup
./scripts/maintenance.sh restore <id>     # Restore backup
./scripts/maintenance.sh scale <cpu> <mem> # Scale resources
./scripts/maintenance.sh restart          # Restart app
./scripts/maintenance.sh cleanup          # Clean up resources
./scripts/maintenance.sh health           # Health checks
./scripts/maintenance.sh costs            # Cost information
```

## Prerequisites

Before running any scripts, ensure you have:

1. **Fly.io Account**: Sign up at https://fly.io
2. **Fly CLI**: Install with `curl -L https://fly.io/install.sh | sh`
3. **Authentication**: Run `fly auth login`
4. **Telegram Bot Token**: Get from @BotFather on Telegram
5. **Rust Toolchain**: Install with rustup

## Database Security

The deployment uses Fly.io's automatically generated database passwords:

- **Auto-Generated**: Fly.io creates cryptographically secure passwords
- **Enterprise Security**: Passwords are long, random, and follow security best practices
- **No User Interaction**: Setup is streamlined without password prompts
- **Managed Lifecycle**: Fly.io handles password security and rotation

**Benefits**: Simpler deployment, stronger security, and automatic management while maintaining all functionality.

## Quick Start

1. **Clone and navigate to the project**:
   ```bash
   cd /path/to/just-ingredients
   ```

2. **Make scripts executable**:
   ```bash
   chmod +x scripts/*.sh
   ```

3. **Prepare your credentials**:
   - Get Telegram Bot Token from @BotFather

4. **Run complete deployment**:
   ```bash
   ./scripts/deploy-all.sh
   ```
   The script will prompt you for your Telegram Bot Token. Fly.io will automatically generate a secure database password.

5. **Test your bot**:
   - Send `/start` to your bot on Telegram
   - Send an image to test OCR functionality

## Configuration

### Environment Variables
The scripts automatically configure these environment variables:

- `TELEGRAM_BOT_TOKEN`: Your bot's authentication token
- `DATABASE_URL`: PostgreSQL connection string (auto-configured)
- `RUST_LOG`: Logging level (`info,sqlx=warn`)
- `LOG_FORMAT`: Log format (`json`)
- `OCR_LANGUAGES`: Tesseract languages (`eng+fra`)
- `HEALTH_PORT`: Health check port (`8080`)

### Fly.io Configuration
The `fly.toml` file in the project root contains the Fly.io configuration with optimal settings for the bot.

## Troubleshooting

### Common Issues

**Script fails with authentication error**:
```bash
fly auth login
```

**Database connection fails**:
```bash
./scripts/maintenance.sh health
fly postgres list
```

**Deployment fails**:
```bash
fly logs --app just-ingredients-bot
fly status --app just-ingredients-bot
```

**Application not responding**:
```bash
./scripts/maintenance.sh restart
./scripts/maintenance.sh logs 50
```

### Getting Help

- **Script help**: Run any script with `--help` or `-h`
- **Fly.io docs**: https://fly.io/docs
- **Telegram Bot API**: https://core.telegram.org/bots/api

## File Structure

```
scripts/
├── deploy-all.sh    # Main orchestrator
├── setup.sh         # Infrastructure setup
├── deploy.sh        # Application deployment
├── monitoring.sh    # Monitoring setup
└── maintenance.sh   # Ongoing maintenance
```

## Security Notes

- Secrets are stored securely in Fly.io
- Database backups are encrypted
- HTTPS is enforced on all endpoints
- Change default Grafana password after setup

## Cost Optimization

- Monitor usage with `./scripts/maintenance.sh costs`
- Scale resources with `./scripts/maintenance.sh scale`
- Use auto-scaling for variable loads
- Clean up old resources regularly
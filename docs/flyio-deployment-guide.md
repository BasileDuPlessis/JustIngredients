# Fly.io Deployment Guide for JustIngredients Bot

## Overview
This document contains critical learnings and best practices for deploying the JustIngredients Telegram bot application to Fly.io. Based on 3 days of troubleshooting production issues, here are the essential deployment patterns and gotchas to avoid.

## Database Deployment

### Single Machine Database
- **Always deploy PostgreSQL on a single machine** to avoid replication complexity
- Use `--ha=false` flag when creating the database:
  ```bash
  fly postgres create --name just-ingredients-db --ha=false
  ```

### Database Connection Issues
- **Never try to connect to PostgreSQL and run SQL queries directly** - this doesn't work reliably
- When attaching database to app, Fly.io creates a database named after the app (e.g., `just_ingredients_database`)
- The default `postgres` database in the cluster is different from the app-specific database
- Always verify which database the app is actually connecting to by checking the DATABASE_URL

### Database Schema Initialization
- Schema initialization happens at application startup via `db::init_database_schema()`
- Tables are created with `CREATE TABLE IF NOT EXISTS` - should be idempotent
- If tables don't exist but app claims to create users successfully, check you're querying the correct database
- Use `fly postgres db list -a <db-name>` to see all databases in the cluster

## Application Deployment

### Single Machine Bot
- **Deploy bot on a single machine** to avoid complexity and ensure consistent state
- Use `--ha=false` when deploying:
  ```bash
  fly deploy --ha=false
  ```

### Environment Variables
- DATABASE_URL is automatically set when attaching PostgreSQL to the app
- Never manually set DATABASE_URL - let Fly.io manage it
- **MEASUREMENT_UNITS_CONFIG_PATH** specifies the path to measurement units configuration
  - Set to `/app/config/measurement_units.json` for Docker deployments
  - Falls back to hardcoded paths if not set
- Validate all required environment variables at startup

### File Paths and URLs
- **Anticipate file path adjustments for Fly.io** - container paths may differ from local development
- **Anticipate URL adjustments** - replace localhost URLs with Fly.io internal services
  - Example: `http://localhost:9090` in Prometheus config becomes internal service URL
- **Configure measurement units path** via `MEASUREMENT_UNITS_CONFIG_PATH` environment variable
  - Set to `/app/config/measurement_units.json` for Docker deployments
  - Falls back to hardcoded paths if not set
- Use relative paths and environment-based configuration

## Monitoring Deployment

### Single Machine Monitoring Stack
- **Deploy Prometheus + Grafana monitoring on a single machine**
- Use `--ha=false` for monitoring apps to avoid configuration complexity
- Ensure proper internal networking between bot, database, and monitoring

### Log Management
- **Do not try to get logs from a stopped instance** - this doesn't work
- Always check app status with `fly apps list` before attempting log retrieval
- Use `fly logs` only on running/deployed applications

## Testing and Validation

### Database Connection Testing
- **Find a solution to test simply that bot can connect to database**
- Add health check endpoints that verify database connectivity
- Use application logs to verify successful database operations
- Check that schema initialization completes without errors

### Deployment Validation Checklist
- [ ] Database deployed with `--ha=false`
- [ ] App deployed with `--ha=false`
- [ ] DATABASE_URL secret exists and is valid
- [ ] Schema initialization logs show success
- [ ] Health check endpoints respond
- [ ] Bot can receive and process messages
- [ ] Database operations (user creation, ingredient saving) work

## Common Issues and Solutions

### "Tables don't exist" but app works
- **Problem**: App logs show successful operations but direct database queries show no tables
- **Solution**: Check you're connecting to the correct database (app-specific, not default postgres)

### User creation works, ingredient creation fails
- **Problem**: Partial schema initialization or permissions issue
- **Solution**: Verify all tables exist in the correct database, check foreign key constraints

### Connection refused errors
- **Problem**: App can't connect to database despite proper DATABASE_URL
- **Solution**: Ensure database and app are in the same Fly.io organization and region

### Log retrieval fails
- **Problem**: `fly logs` returns no output or errors
- **Solution**: Check app status first - only works on running instances

## Deployment Commands Reference

```bash
# Create database (single machine)
fly postgres create --name just-ingredients-db --ha=false

# Deploy database
fly postgres deploy --app just-ingredients-db

# Attach database to app
fly postgres attach just-ingredients-db --app just-ingredients-bot

# Deploy app (single machine)
fly deploy --app just-ingredients-bot --ha=false

# Check app status
fly apps list

# Get logs (only from running apps)
fly logs --app just-ingredients-bot

# Connect to correct database for inspection
fly postgres connect -a just-ingredients-db -d just_ingredients_bot
```

### Configuration Files to Review

- `fly.toml`: Ensure single machine configuration, proper environment variables
  - Add `MEASUREMENT_UNITS_CONFIG_PATH = "/app/config/measurement_units.json"`
- `prometheus.yml`: Update localhost URLs to Fly.io internal services
- `grafana/datasources/prometheus.yml`: Update URLs for Fly.io networking
- `docker-compose.yml`: May need adjustments for Fly.io container paths

## Monitoring and Debugging

- Use health check endpoints to verify service status
- Check application logs for database connection errors
- Verify schema initialization completes successfully
- Test bot functionality end-to-end after deployment
- Monitor database connections and query performance

## Lessons Learned

1. **Always use single machines** for development/production unless you need high availability
2. **Verify database connections** by checking the correct database, not just the default postgres
3. **Test schema initialization** thoroughly - partial failures can cause confusing symptoms
4. **Check app status** before attempting log retrieval or other operations
5. **Use Fly.io's automatic secrets management** for DATABASE_URL rather than manual configuration
6. **Plan for URL/path changes** when moving from localhost to Fly.io networking
7. **Add comprehensive logging** for database operations to aid troubleshooting

## Emergency Recovery

If deployment fails:
1. Check app status: `fly apps list`
2. Review recent logs: `fly logs --app <app-name>`
3. Verify database connectivity: check health endpoints
4. Redeploy if needed: `fly deploy --app <app-name> --ha=false`
5. Check database state in correct database: `fly postgres connect -a <db-name> -d <app_db>`

This guide should prevent the issues encountered during the initial deployment and provide a reliable path for future deployments.</content>
<filePath>/Users/basile.du.plessis/Documents/JustIngredients/docs/flyio-deployment-guide.md
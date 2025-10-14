# Monitoring Runbooks - JustIngredients Telegram Bot

## Overview
This document provides operational procedures for monitoring, incident response, and metric interpretation for the JustIngredients Telegram bot.

## Key Metrics to Monitor

### Core Application Metrics
- **ocr_operations_total**: Total OCR processing operations (success/failure)
- **ocr_duration_seconds**: OCR processing duration histogram
- **ocr_image_size_bytes**: Size of processed images
- **db_operations_total**: Database operations by type
- **db_operation_duration_seconds**: Database query duration
- **requests_total**: HTTP requests by method and status
- **telegram_messages_total**: Telegram messages processed by type

### Health Check Endpoints
- **/health/live**: Liveness probe (service running)
- **/health/ready**: Readiness probe (all dependencies available)
- **/metrics**: Prometheus metrics endpoint

### Circuit Breaker Metrics
- **circuit_breaker_state**: Current circuit breaker state (0=closed, 1=open)

## Alert Conditions

### Critical Alerts (Immediate Response Required)
1. **OCR Failure Rate > 50%** (5-minute window)
   - Indicates OCR engine issues or image processing problems
   - Response: Check Tesseract installation, instance pool status

2. **Database Connection Failures**
   - Readiness probe fails for database health check
   - Response: Check PostgreSQL connectivity, connection pool limits

3. **High Error Rate > 10%** (HTTP 5xx responses)
   - Indicates application errors or dependency failures
   - Response: Check application logs, resource utilization

### Warning Alerts (Investigate Within 30 Minutes)
1. **OCR Duration P95 > 30 seconds**
   - Processing taking too long, may indicate resource constraints
   - Response: Check system resources, image sizes, OCR instance pool

2. **Database Query Duration P95 > 5 seconds**
   - Slow database queries affecting performance
   - Response: Check query plans, database indexes, connection pooling

3. **Circuit Breaker Open**
   - OCR operations failing repeatedly, circuit breaker activated
   - Response: Investigate root cause of OCR failures

## Incident Response Procedures

### OCR Processing Failures

**Symptoms:**
- High OCR failure rate
- Circuit breaker opens
- Users report failed ingredient extraction

**Investigation Steps:**
1. Check application logs for OCR errors
2. Verify Tesseract installation: `tesseract --version`
3. Check OCR instance pool status in logs
4. Test with sample image: `tesseract test.png stdout`
5. Verify temporary file permissions

**Resolution:**
- Restart application to reset instance pool
- Check available memory for large images
- Verify supported image formats (PNG, JPEG, BMP, TIFF)

### Database Connectivity Issues

**Symptoms:**
- Readiness probe fails
- Database operation errors in logs
- Slow response times

**Investigation Steps:**
1. Check PostgreSQL service status
2. Verify connection string and credentials
3. Test database connectivity: `psql $DATABASE_URL -c "SELECT 1"`
4. Check connection pool utilization
5. Verify database server resources

**Resolution:**
- Restart database service if needed
- Scale database resources if overloaded
- Check application connection pool configuration

### High Memory Usage

**Symptoms:**
- Application restarts due to OOM
- Slow OCR processing
- System memory alerts

**Investigation Steps:**
1. Check current memory usage: `ps aux | grep just-ingredients`
2. Review OCR image size metrics
3. Check for memory leaks in logs
4. Verify image size limits are enforced

**Resolution:**
- Implement image size validation
- Adjust OCR instance pool size
- Add memory monitoring alerts

### Telegram Bot Token Issues

**Symptoms:**
- Bot not responding to messages
- Authentication errors in logs
- Readiness probe fails for bot token check

**Investigation Steps:**
1. Verify TELEGRAM_BOT_TOKEN environment variable
2. Check token format (should contain ':')
3. Test token validity with Telegram API
4. Check for token expiration

**Resolution:**
- Update bot token in environment
- Regenerate token from BotFather if expired
- Verify token permissions

## Metric Interpretation Guide

### OCR Operations Analysis

**Success Rate:**
```
success_rate = ocr_operations_total{result="success"} / ocr_operations_total
```
- **> 95%**: Normal operation
- **90-95%**: Monitor for trends, investigate failures
- **< 90%**: Critical, immediate investigation required

**Processing Duration:**
- **P50 < 5s**: Good performance
- **P95 < 15s**: Acceptable performance
- **P95 > 30s**: Performance degradation, investigate

**Image Size Distribution:**
- Monitor for unusually large images causing timeouts
- Set up alerts for images > 10MB (JPEG) or > 15MB (PNG)

### Database Performance

**Query Duration:**
- **P50 < 100ms**: Excellent
- **P95 < 1s**: Good
- **P95 > 5s**: Investigate slow queries

**Connection Pool:**
- Monitor active connections vs. pool size
- Alert when > 80% of pool utilized

### System Resources

**Memory Usage:**
- Alert when > 80% of available memory
- Monitor for memory leaks in long-running processes

**CPU Usage:**
- Alert when sustained > 70% CPU utilization
- Check for CPU-intensive OCR operations

## Environment-Specific Monitoring

### Development Environment
- Use pretty logging format
- Enable debug-level logging for observability module
- Monitor with local Prometheus instance

### Staging Environment
- Use JSON logging
- Enable trace sampling (10-20%)
- Test all health checks and metrics

### Production Environment
- JSON logging with structured fields
- Configure OTLP tracing to external collector
- Set up comprehensive alerting
- Enable full metrics export

## Troubleshooting Commands

### Check Application Status
```bash
# Check if service is running
ps aux | grep just-ingredients

# Check health endpoints
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready

# View metrics
curl http://localhost:8080/metrics
```

### Database Diagnostics
```bash
# Test database connectivity
psql $DATABASE_URL -c "SELECT 1"

# Check active connections
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity WHERE datname = 'ingredients'"

# Check table sizes
psql $DATABASE_URL -c "SELECT schemaname, tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) FROM pg_tables WHERE schemaname = 'public' ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;"
```

### OCR Diagnostics
```bash
# Test Tesseract installation
tesseract --version

# Test OCR with sample image
tesseract test.png stdout -l eng+fra

# Check available languages
tesseract --list-langs
```

### Log Analysis
```bash
# Search for errors in recent logs
grep "ERROR" /var/log/just-ingredients/*.log | tail -20

# Check OCR operation failures
grep "ocr_operation" /var/log/just-ingredients/*.log | grep "error" | tail -10

# Monitor circuit breaker state
grep "circuit_breaker" /var/log/just-ingredients/*.log | tail -10
```

## Maintenance Procedures

### Log Rotation
- Configure log rotation for JSON logs
- Archive logs older than 30 days
- Monitor disk space usage

### Metrics Retention
- Configure Prometheus retention policies
- Archive historical metrics for trend analysis
- Set up long-term metric storage if needed

### Dependency Updates
- Monitor for security updates in dependencies
- Test updates in staging environment first
- Update Tesseract and leptonica libraries regularly

### Performance Optimization
- Monitor P95 latencies for all operations
- Optimize database queries with slow query logs
- Tune OCR instance pool based on load patterns

## Contact Information

For urgent issues outside business hours:
- On-call Engineer: [Contact Information]
- System Owner: [Contact Information]
- Development Team: [Slack Channel/Distribution List]

## Change Log

- **v1.0**: Initial monitoring runbook with core metrics and incident response procedures
- Comprehensive health checks and metric interpretation guidelines
- Environment-specific monitoring configurations
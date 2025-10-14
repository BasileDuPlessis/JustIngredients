# JustIngredients Monitoring Stack

This directory contains the complete monitoring and visualization setup for the JustIngredients Telegram bot using Prometheus, Grafana, and Alertmanager.

## Components

- **Prometheus**: Metrics collection and alerting rules
- **Grafana**: Dashboards and visualization
- **Alertmanager**: Alert routing and notifications
- **Node Exporter**: System metrics collection
- **PostgreSQL Exporter**: Database metrics collection

## Quick Start

### Using Docker Compose

1. **Start the monitoring stack:**
   ```bash
   cd grafana
   docker-compose up -d
   ```

2. **Access the services:**
   - Grafana: http://localhost:3000 (admin/admin)
   - Prometheus: http://localhost:9090
   - Alertmanager: http://localhost:9093

### Manual Setup

1. **Install Prometheus:**
   ```bash
   # Download and install Prometheus
   wget https://github.com/prometheus/prometheus/releases/download/v2.40.0/prometheus-2.40.0.linux-amd64.tar.gz
   tar xvfz prometheus-*.tar.gz
   cd prometheus-*
   ```

2. **Configure Prometheus:**
   ```bash
   # Copy configuration
   cp /path/to/justingredients/grafana/prometheus.yml ./prometheus.yml
   cp /path/to/justingredients/grafana/alert-rules.yml ./alert-rules.yml
   ```

3. **Start Prometheus:**
   ```bash
   ./prometheus --config.file=prometheus.yml
   ```

4. **Install Grafana:**
   ```bash
   # Using Docker
   docker run -d -p 3000:3000 --name=grafana grafana/grafana
   ```

5. **Import Dashboards:**
   - Open Grafana at http://localhost:3000
   - Login with admin/admin
   - Go to Dashboards → Import
   - Upload the JSON files from this directory

## Dashboards

### JustIngredients Bot - Overview
- **Request Rate**: HTTP request rates by method and status
- **Error Rate**: Percentage of failed requests
- **Response Time**: 95th percentile latency
- **Telegram Messages**: Message processing rates
- **Database Operations**: Query rates and performance
- **Circuit Breaker State**: Fault tolerance status
- **Memory Usage**: Application memory consumption

### JustIngredients Bot - OCR Performance
- **OCR Operations Rate**: Processing throughput
- **OCR Success Rate**: Percentage of successful OCR operations
- **OCR Processing Time**: 95th percentile processing latency
- **Image Size Distribution**: Histogram of processed image sizes
- **Error Rate by Type**: Breakdown of OCR failure reasons
- **Throughput Metrics**: Images processed per minute
- **Memory Usage**: Memory consumption during OCR operations

## Alert Rules

### Critical Alerts
- **High Error Rate**: >5% error rate for 5 minutes
- **Database Down**: Cannot connect to PostgreSQL
- **Health Check Failed**: Application health checks failing

### Warning Alerts
- **OCR Failures**: >10% OCR failure rate for 10 minutes
- **High Latency**: >5s 95th percentile response time
- **Circuit Breaker Open**: OCR circuit breaker has tripped
- **High Memory Usage**: >2GB memory consumption
- **OCR Backlog**: >10 items in OCR processing queue

### Info Alerts
- **Telegram Rate Limit**: High message processing rate (>30/min)

## Configuration

### Environment Variables

Set these in your JustIngredients `.env` file:

```bash
# Health check port (default: 9090)
HEALTH_PORT=8080

# Optional: OTLP tracing endpoint
OTLP_ENDPOINT=http://localhost:4317

# Optional: Log format (json/pretty)
LOG_FORMAT=json
```

### Alertmanager Configuration

Update `alertmanager.yml` with your notification settings:

- **Email**: Configure SMTP settings
- **Slack**: Add webhook URLs
- **PagerDuty/OpsGenie**: Add integration keys

## Metrics Reference

### Application Metrics
- `requests_total{method, status}`: HTTP request counter
- `request_duration_seconds`: HTTP request duration histogram
- `telegram_messages_total{type}`: Telegram message counter
- `db_operations_total{operation}`: Database operation counter
- `db_operation_duration_seconds`: Database operation duration histogram
- `circuit_breaker_state`: Circuit breaker status (0=closed, 1=open)

### OCR Metrics
- `ocr_operations_total{result}`: OCR operation counter
- `ocr_duration_seconds`: OCR processing time histogram
- `ocr_image_size_bytes`: Processed image sizes histogram

### System Metrics
- `process_resident_memory_bytes`: Application memory usage
- `up`: Service availability (1=up, 0=down)

## Troubleshooting

### Common Issues

1. **Metrics not appearing in Grafana:**
   - Check Prometheus targets: http://localhost:9090/targets
   - Verify JustIngredients is running and accessible on port 8080
   - Check firewall settings

2. **Alerts not firing:**
   - Verify alert rules syntax in Prometheus
   - Check Alertmanager configuration
   - Test notification channels

3. **High memory usage:**
   - Monitor OCR image processing
   - Check for memory leaks in Tesseract instances
   - Adjust circuit breaker thresholds

### Useful Commands

```bash
# Check Prometheus health
curl http://localhost:9090/-/healthy

# Check JustIngredients metrics
curl http://localhost:8080/metrics

# Check JustIngredients health
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready

# View active alerts
curl http://localhost:9090/api/v1/alerts

# Reload Prometheus configuration
curl -X POST http://localhost:9090/-/reload
```

## Production Deployment

For production deployment:

1. **Use external databases** for Prometheus and Grafana data
2. **Configure authentication** for Grafana
3. **Set up HTTPS** for all services
4. **Configure backup** for metrics data
5. **Set up high availability** for Prometheus and Alertmanager
6. **Configure proper notification channels** (Slack, PagerDuty, etc.)

## Contributing

When adding new metrics:

1. Update the observability code in `src/observability.rs`
2. Add corresponding panels to the appropriate dashboard
3. Update alert rules if needed
4. Test the changes locally
5. Update this documentation
# Deployment & Infrastructure Feature Specification

## Overview
The deployment and infrastructure feature manages the production deployment of the Telegram bot application on Fly.io. It includes containerization, monitoring, secrets management, and operational procedures for reliable production operation.

## Requirements

### Functional Requirements
- **Container Deployment**: Docker-based application packaging
- **Environment Management**: Secure configuration and secrets
- **Monitoring Setup**: Application and infrastructure monitoring
- **Health Checks**: Service availability verification
- **Logging**: Centralized log collection and analysis
- **Scaling**: Automatic resource adjustment based on load

### Non-Functional Requirements
- **Reliability**: 99.9% uptime target
- **Performance**: Sub-second response times
- **Security**: Secure secrets management and access control
- **Maintainability**: Easy deployment and rollback procedures
- **Observability**: Comprehensive monitoring and alerting

### Success Criteria
- [x] Application deploys successfully to Fly.io
- [x] All secrets properly managed and accessible
- [x] Monitoring dashboards functional (Prometheus + Grafana live)
- [x] Health checks passing
- [x] Logs collected and searchable
- [x] Staging environment configured (`fly-staging.toml`)

## Architecture

### Deployment Components
```
Production Infrastructure
├── Application Container    # Dockerized Rust application (deploy/Dockerfile)
├── Database Instance       # Managed PostgreSQL (Fly.io)
├── Monitoring Stack        # Prometheus + Grafana (deploy/fly-monitoring.toml + fly-grafana.toml)
├── Staging Environment     # deploy/fly-staging.toml
├── Load Balancer          # Fly.io edge network
└── Secrets Management     # Fly.io secrets (flyctl secrets set)
```

### Data Flow
1. **Code Deployment**: Git push triggers automated build
2. **Container Build**: Docker image creation and registry push
3. **Service Deployment**: Rolling update with health checks
4. **Traffic Routing**: Load balancer distributes requests
5. **Monitoring**: Metrics collection and alerting
6. **Logging**: Centralized log aggregation and analysis

### Integration Points
- **Fly.io Platform**: Infrastructure as a service
- **Docker Registry**: Container image storage
- **PostgreSQL**: Managed database service
- **Prometheus/Grafana**: Monitoring and visualization
- **GitHub Actions**: CI/CD pipeline

## API Design

### Deployment Operations
```bash
# Deploy application
fly deploy

# Check status
fly status

# View logs
fly logs

# Scale application
fly scale count 2

# Manage secrets
fly secrets set DATABASE_URL="postgresql://..."
fly secrets list
```

### Health Check Endpoints
```rust
// Application health
GET /health

// Database connectivity
GET /health/db

// OCR service status
GET /health/ocr
```

### Monitoring Endpoints
```rust
// Metrics export
GET /metrics

// Readiness probe
GET /ready

// Liveness probe
GET /live
```

## Implementation Notes

### Key Decisions
- **Fly.io Platform**: Global deployment with edge network
- **Docker Containers**: Consistent deployment artifacts
- **Managed Database**: PostgreSQL as a service
- **Prometheus Monitoring**: Industry-standard metrics collection
- **GitHub Actions CI/CD**: Automated testing and deployment

### Constraints
- **Resource Limits**: Fly.io VM size restrictions
- **Network Latency**: Global distribution considerations
- **Cost Optimization**: Efficient resource utilization
- **Security Compliance**: Secrets and data protection

### Dependencies
- **Docker**: Container runtime and build system
- **Fly CLI**: Infrastructure management
- **Prometheus**: Metrics collection
- **Grafana**: Dashboard visualization

## Container Configuration

### Dockerfile (`deploy/Dockerfile`)
```dockerfile
# syntax=docker/dockerfile:1.6
FROM rust:latest AS builder
WORKDIR /app
RUN apt-get install -y build-essential clang pkg-config libssl-dev \
    libtesseract-dev libleptonica-dev tesseract-ocr-eng tesseract-ocr-fra
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY config ./config
COPY locales ./locales
COPY .sqlx ./.sqlx
RUN cargo build --release && strip --strip-unneeded target/release/just-ingredients

FROM ubuntu:latest AS runtime
RUN apt-get install -y ca-certificates libssl3 tesseract-ocr \
    tesseract-ocr-eng tesseract-ocr-fra liblept5 libtesseract5
COPY --from=builder /app/target/release/just-ingredients /usr/local/bin/just-ingredients
COPY config ./config
ENV MEASUREMENT_UNITS_CONFIG_PATH=/app/config/measurement_units.json
ENV RUST_LOG=info,sqlx=warn
CMD ["just-ingredients"]
```

### Fly.io Configuration Files
| File | Purpose |
|---|---|
| `deploy/fly.toml` | Production deployment |
| `deploy/fly-staging.toml` | Staging environment |
| `deploy/fly-monitoring.toml` | Prometheus monitoring app |
| `deploy/fly-grafana.toml` | Grafana dashboard app |

### Fly.io Configuration
```toml
# fly.toml
app = "just-ingredients"
primary_region = "cdg"

[build]
  dockerfile = "Dockerfile"

[http_service]
  internal_port = 8080
  force_https = true

[[vm]]
  size = "shared-cpu-1x"

[env]
  RUST_LOG = "info"

[metrics]
  port = 9090
  path = "/metrics"
```

## Monitoring & Observability

### Metrics Collection
```rust
// Application metrics
ocr_requests_total: Counter
ocr_processing_duration: Histogram
db_query_duration: Histogram
telegram_webhook_errors: Counter

// System metrics
cpu_usage: Gauge
memory_usage: Gauge
disk_usage: Gauge
network_io: Counter
```

### Alerting Rules
```yaml
# Prometheus alerting rules
groups:
  - name: just-ingredients
    rules:
      - alert: HighErrorRate
        expr: rate(ocr_requests_total{status="error"}[5m]) > 0.1
        for: 5m
        labels:
          severity: warning

      - alert: SlowProcessing
        expr: histogram_quantile(0.95, rate(ocr_processing_duration_bucket[5m])) > 30
        for: 5m
        labels:
          severity: warning
```

### Dashboard Panels
- **Request Rate**: OCR requests per minute
- **Error Rate**: Failed request percentage
- **Processing Time**: 95th percentile duration
- **Resource Usage**: CPU, memory, disk metrics
- **Database Performance**: Query latency and throughput

## Security Configuration

### Secrets Management
```bash
# Set production secrets
fly secrets set TELEGRAM_BOT_TOKEN="your_bot_token"
fly secrets set DATABASE_URL="postgresql://user:pass@host:5432/db"
fly secrets set HEALTH_PORT="8080"

# List secrets
fly secrets list

# Unset secret
fly secrets unset OLD_SECRET
```

### Network Security
- **HTTPS Only**: All traffic encrypted
- **Firewall Rules**: Restrict access to necessary ports
- **API Authentication**: Telegram webhook verification
- **Database Security**: Connection encryption and access controls

## CI/CD Pipeline

### GitHub Actions Workflow
```yaml
name: Deploy to Fly.io
on:
  push:
    branches: [ main ]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --verbose

  deploy:
    needs: test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: superfly/flyctl-actions/setup-flyctl@master
      - run: flyctl deploy --remote-only
        env:
          FLY_API_TOKEN: ${{ secrets.FLY_API_TOKEN }}
```

## Testing Strategy

### Pre-deployment Tests
- **Unit Tests**: All code passes testing
- **Integration Tests**: End-to-end functionality
- **Container Tests**: Docker image validation
- **Smoke Tests**: Basic functionality in staging

### Deployment Tests
- **Health Checks**: Service availability verification
- **Load Tests**: Performance under expected traffic
- **Failover Tests**: Automatic recovery validation
- **Rollback Tests**: Deployment reversal procedures

## Operational Procedures

### Deployment Process
1. **Code Review**: PR review and approval
2. **Automated Testing**: CI pipeline execution
3. **Staging Deployment**: Test in staging environment
4. **Production Deployment**: Rolling update to production
5. **Verification**: Health checks and monitoring validation
6. **Rollback Plan**: Ready reversion procedure

### Incident Response
1. **Detection**: Monitoring alerts and notifications
2. **Assessment**: Log analysis and metric review
3. **Containment**: Scale back or rollback deployment
4. **Recovery**: Deploy fix or restore from backup
5. **Post-mortem**: Incident analysis and improvement

## Future Enhancements

### Potential Improvements
- **Multi-region Deployment**: Global distribution
- **Auto-scaling**: Dynamic resource allocation
- **Blue-Green Deployment**: Zero-downtime updates
- **Advanced Monitoring**: Distributed tracing
- **Disaster Recovery**: Cross-region failover
- **Performance Optimization**: CDN integration
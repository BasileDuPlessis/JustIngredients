# 📊 Observability Stack Implementation Tasks

## Overview
This document outlines the comprehensive implementation plan for adding a production-grade observability stack to the JustIngredients Telegram bot. The stack includes metrics, tracing, logging, health checks, and visualization capabilities.

## Recommended Tools for Rust & PostgreSQL
- **Metrics**: Prometheus + metrics crate
- **Tracing**: OpenTelemetry via opentelemetry crate and tracing
- **Logging**: tracing + tracing-subscriber
- **Visualization**: Grafana for dashboards
- **Healthchecks**: /health endpoint for readiness/liveness probes

---

## Phase 1: Core Dependencies & Setup

### Task 1.1: Add Observability Dependencies
- Add `metrics`, `metrics-exporter-prometheus` crates for metrics collection
- Add `opentelemetry`, `opentelemetry-otlp`, `tracing-opentelemetry` for distributed tracing
- Add `tracing-subscriber` with JSON formatting for structured logging
- Add `axum` or `warp` for health check endpoints (if not already present)
- Update `Cargo.toml` with all observability dependencies

### Task 1.2: Initialize Observability Infrastructure
- Create `src/observability.rs` module for centralized observability setup
- Initialize tracing subscriber with JSON output and appropriate log levels
- Set up metrics registry and Prometheus exporter
- Configure OpenTelemetry tracer provider
- Add observability initialization to `main.rs`

---

## Phase 2: Metrics Implementation

### Task 2.1: Define Core Metrics
- Request/response metrics (count, duration, status codes)
- OCR operation metrics (success/failure rate, latency, image size)
- Database operation metrics (query count, duration, connection pool stats)
- Telegram bot metrics (message processing, user interactions)
- Error rate metrics by component (OCR, DB, Telegram API)

### Task 2.2: Instrument Core Components
- Add metrics to `src/ocr.rs` for OCR operations
- Add metrics to `src/db.rs` for database operations
- Add metrics to `src/bot/message_handler.rs` for message processing
- Add metrics to `src/bot/callback_handler.rs` for callback processing
- Add circuit breaker state metrics

### Task 2.3: Expose Metrics Endpoint
- Create `/metrics` HTTP endpoint using Axum/Warp
- Configure Prometheus exporter to serve metrics
- Add endpoint to main application server
- Test metrics collection with `curl http://localhost:9090/metrics`

---

## Phase 3: Tracing Implementation

### Task 3.1: Add Tracing Spans
- Add spans to OCR processing flow (`extract_text_from_image`)
- Add spans to database operations (`create_recipe`, `save_ingredients_to_database`)
- Add spans to Telegram message handling (`handle_photo_message`, `handle_text_message`)
- Add spans to dialogue state transitions
- Include relevant context (user_id, message_id, operation_type)

### Task 3.2: Configure Trace Export
- Set up OTLP exporter for traces
- Configure trace sampling (development vs production)
- Add trace correlation IDs for request tracking
- Integrate with existing tracing setup

### Task 3.3: Add Baggage Propagation
- Propagate user context through trace spans
- Include language codes, user preferences in baggage
- Track conversation flows across multiple messages

---

## Phase 4: Enhanced Logging

### Task 4.1: Structured Logging Implementation
- Replace existing `log!` calls with `tracing::info!`, `tracing::error!`, etc.
- Add structured fields (user_id, operation, duration, error_codes)
- Include contextual information in all log entries
- Standardize log levels across the application

### Task 4.2: Log Aggregation Setup
- Configure log output format (JSON for production, pretty for development)
- Add log filtering based on module/component
- Set up log rotation and retention policies
- Consider log shipping to external aggregators (ELK stack, etc.)

---

## Phase 5: Health Checks & Monitoring

### Task 5.1: Health Check Endpoints
- Implement `/health/live` (liveness probe)
- Implement `/health/ready` (readiness probe)
- Add database connectivity health check
- Add OCR engine availability check
- Add Telegram bot token validation check

### Task 5.2: Application Metrics
- Add application startup time metric
- Add memory usage metrics
- Add thread pool utilization metrics
- Add circuit breaker state monitoring
- Add configuration validation metrics

---

## Phase 6: Visualization & Alerting

### Task 6.1: Grafana Dashboard Setup
- Create Grafana dashboard for bot metrics
- Add panels for request rates, error rates, latency distributions
- Create OCR performance dashboard
- Add database performance monitoring
- Set up alerting rules for critical metrics

### Task 6.2: Alert Configuration
- Configure alerts for high error rates
- Set up alerts for OCR failures
- Add alerts for database connection issues
- Configure alerts for high latency
- Set up notification channels (Slack, email, etc.)

---

## Phase 7: Testing & Validation

### Task 7.1: Observability Testing
- Add tests for metrics collection accuracy
- Test trace span creation and propagation
- Validate structured logging output
- Test health check endpoints
- Add integration tests for observability stack

### Task 7.2: Performance Impact Assessment
- Measure performance impact of observability features
- Optimize metrics collection for minimal overhead
- Profile tracing impact on request latency
- Ensure observability doesn't affect OCR processing speed

---

## Phase 8: Deployment & Operations

### Task 8.1: Production Configuration
- Environment-specific observability configuration
- Secure OTLP endpoint configuration
- Prometheus scraping configuration
- Log aggregation pipeline setup

### Task 8.2: Monitoring Runbooks
- Create incident response procedures
- Document metric interpretation guidelines
- Set up on-call rotation for alerts
- Create troubleshooting guides using observability data

---

## Success Criteria
- ✅ All metrics exposed via `/metrics` endpoint
- ✅ Traces collected and exported via OTLP
- ✅ Structured JSON logs with full context
- ✅ Health checks responding correctly
- ✅ Grafana dashboards showing real-time metrics
- ✅ Alerts configured for critical failures
- ✅ Zero performance impact on core OCR functionality
- ✅ Full test coverage for observability features

## Estimated Timeline
- **Phase 1-2**: 1-2 days (Core metrics & instrumentation)
- **Phase 3-4**: 1-2 days (Tracing & enhanced logging)
- **Phase 5-6**: 1 day (Health checks & Grafana setup)
- **Phase 7-8**: 1-2 days (Testing, deployment & documentation)

## Implementation Notes
- Each phase builds incrementally on the previous one
- Core bot functionality must remain unaffected during implementation
- All observability features should be configurable (enable/disable via environment variables)
- Consider resource usage impact on OCR processing performance
- Ensure GDPR compliance for user data in logs and traces

## Dependencies
```
# Core observability
metrics = "0.23"
metrics-exporter-prometheus = "0.15"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
opentelemetry = "0.23"
opentelemetry-otlp = "0.16"
tracing-opentelemetry = "0.24"

# Optional: For health check endpoints
axum = "0.7"
tokio = { version = "1.0", features = ["full"] }
```

This breakdown provides a comprehensive roadmap for implementing production-grade observability while maintaining the bot's core functionality.
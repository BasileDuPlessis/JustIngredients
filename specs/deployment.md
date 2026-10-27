# Deployment & Infrastructure Feature Specification

> Describes the setup as it is (2026-10-27, after Tasks 5.1, 5.3 and 5.4 of
> [LLM Extraction Migration](tasks/llm-extraction-migration.md#cicd-audit)): no Tesseract, one
> image per commit, permanent staging, automatic production rollback.

## Overview
The bot runs as one Fly.io machine (app `just-ingredients`, region `cdg`) with a Fly Postgres
database. It receives Telegram updates by long polling, so the only HTTP traffic is health
checks and metrics. GitHub Actions tests every push and builds one image per commit of `main`;
that image goes to a permanent staging app (with the test bot), then to production when a
deployment is started by hand and approved.

## Components
| File | Purpose |
|---|---|
| `deploy/Dockerfile` | Image built once per commit of `main` by "Build and Test" (job `image`), pushed to `ghcr.io/basileduplessis/justingredients:sha-<commit>` (public package) |
| `deploy/fly.toml` | Production app `just-ingredients` |
| `deploy/fly-staging.toml` | Permanent staging app `just-ingredients-staging` (test bot, own database) |
| `deploy/fly-monitoring.toml`, `monitoring/prometheus.yml` | Prometheus app |
| `deploy/fly-grafana.toml`, `monitoring/grafana/` | Grafana app |
| `rust-toolchain.toml` | Rust 1.99, same as CI and the Docker builder |
| `.github/workflows/*.yml` | CI/CD (below) |
| `smoke-tests.js`, `package.json` | Staging smoke tests |
| `scripts/check-health.sh` | Waits for `/health/live` and `/health/ready` (production deploy and rollback) |

## Container (`deploy/Dockerfile`)
- Builder `rust:1.99-slim-trixie`, runtime `debian:trixie-slim` (same Debian release, so the
  binary links against the runtime libraries). Bump Rust here, in `rust-toolchain.toml` and in
  the CI workflow together.
- `cargo build --release` with `.sqlx/` (offline query checks); the binary is stripped.
- Builder packages: `build-essential`, `pkg-config`, `libssl-dev`. Runtime packages: CA
  certificates and libssl only (no OCR engine).
- The build fails if `ldd` reports a missing library.
- `config/` is copied (`MEASUREMENT_UNITS_CONFIG_PATH=/app/config/measurement_units.json`);
  locales, prompt and schema are embedded in the binary. `RUST_LOG=info,sqlx=warn`.

## Fly Configuration (`deploy/fly.toml`, same in `fly-staging.toml`)
- `[env]`: `HEALTH_PORT=8080`, `LOG_FORMAT=pretty`, `RUST_LOG=info,sqlx=warn`,
  `METRICS_BIND_ALL_INTERFACES=true`.
- `[[restart]] policy = 'always'`: the bot restarts whenever it exits (the default policy gave
  up during a Postgres restart on 2026-10-16 and left the bot down for 10 days).
- Checks: `GET /health/live` every 15 s (timeout 2 s, grace 30 s), as a machine check and an
  `http_service` check.
- `[http_service]`: internal port 8080, HTTPS forced, `auto_stop_machines = false`,
  `min_machines_running = 1`.

## HTTP Endpoints
One server on `HEALTH_PORT` (8080), bound to all interfaces only when
`METRICS_BIND_ALL_INTERFACES=true` (localhost otherwise), limited to 10 requests per minute per
client IP. Code: `src/observability/metrics.rs`, `src/observability/health_checks.rs`.

| Endpoint | Behaviour |
|---|---|
| `GET /health/live` | 200 `OK` while the Telegram heartbeat is fresh; 503 after 5 minutes without a successful Telegram call (`getMe` every minute, 5-minute grace at startup). |
| `GET /health/ready` | 200 when the database answers `SELECT 1`, the heartbeat is fresh and the bot token is well formed; 503 `NOT READY: <reason>` otherwise. Gemini is not called. |
| `GET /metrics` | Prometheus text format. When `METRICS_AUTH_TOKEN` is set, requires `Authorization: Bearer <token>` (401 otherwise); open when it is unset. |

Every other path answers 404 (there is no `/health/ocr`, `/health/db`, `/ready` or `/live`).

## Secrets and Configuration
- **Production** (Fly secrets, set by hand with `fly secrets set`, never by CI):
  `DATABASE_URL` (Fly Postgres), `TELEGRAM_BOT_TOKEN`, `GEMINI_API_KEY`, `METRICS_AUTH_TOKEN`.
  The bot refuses to start without a well-formed `TELEGRAM_BOT_TOKEN` and `DATABASE_URL`, or
  without `GEMINI_API_KEY`.
- **Staging** (permanent app `just-ingredients-staging`, created once by hand, see
  [Staging Setup](#staging-setup)): Fly secrets `DATABASE_URL` (from `fly postgres attach` of
  `just-ingredients-staging-db`), `TELEGRAM_BOT_TOKEN` of the test bot, `GEMINI_API_KEY`.
  `METRICS_AUTH_TOKEN` is not set there.
- **GitHub environment secrets**: `FLY_API_TOKEN` in `staging` and in `production`, each a deploy
  token of its own app (`fly tokens create deploy -a <app>`); a repository-level `FLY_API_TOKEN`
  is only a fallback while they are missing.
- **GitHub repository secrets**: `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` for the production
  deploy notification and the uptime alert.
- Optional variables with defaults: `GEMINI_MODEL`, `GEMINI_TIMEOUT_SECS`, `GEMINI_THINKING_LEVEL`, `GEMINI_MAX_CONCURRENCY`, database pool and
  observability settings; see `.env.example`.

## CI/CD (`.github/workflows/`)
| Workflow | Trigger | What it does |
|---|---|---|
| `build-test.yml` "Build and Test" | Push and PR to `main` | `build`: Postgres 16 service, `cargo build`, `cargo test --all-targets`, doc tests, `db_tests` with `--include-ignored`. `lint`: clippy `-D warnings`, `scripts/check-unwraps.sh`, `cargo fmt --check`. `security`: cargo-deny `check advisories sources`. `image` (pushes to `main` only, after the three others): Buildx with the GitHub Actions layer cache, pushes `ghcr.io/basileduplessis/justingredients:sha-<commit>`; dependencies are compiled in their own layer, rebuilt only when `Cargo.toml` or `Cargo.lock` change. Rust 1.99, `SQLX_OFFLINE=true`, Ubuntu 26.04, no system packages. The benchmarks of `tests/performance_tests.rs` are ignored (IMP-046). |
| `deploy-staging.yml` "Deploy to Staging" | `workflow_run` after a successful "Build and Test" on a push to `main` from this repository (or manual on `main`) | Run name `Deploy to Staging <sha>`. `flyctl deploy --image ghcr.io/…:sha-<sha>` on the permanent staging app (no build), then the smoke tests (`/health/live`, `/health/ready`). Staging keeps running afterwards. |
| `deploy-production.yml` "Deploy to Production" | Manual, `main` only | Environment `production` with a required reviewer. Gate: a successful "Build and Test" push run for the commit and a successful run named `Deploy to Staging <sha>`. Records the running image (`fly image show`, by digest), deploys the image staging ran (`--image ghcr.io/…:sha-<sha>`, no build), waits for `/health/live` and `/health/ready` (`scripts/check-health.sh`, 6 tries 10 s apart); on failure deploys the recorded image again and checks it. Telegram notification: deployed, rolled back, or failed. |
| `uptime.yml` "Production Uptime" | Every 10 minutes (and manual) | `GET /health/ready` on production, 3 tries 20 s apart; Telegram alert on failure. GitHub may delay scheduled runs and disables them after 60 days without activity. |

## Monitoring
- Alerting: only the uptime workflow and the production deploy notification (Telegram).
  `monitoring/prometheus.yml` has no alert rules.
- Prometheus scrapes `https://just-ingredients.fly.dev/metrics` every 30 s. Its configuration
  sends no token, so the scrape is refused (401) while `METRICS_AUTH_TOKEN` is set.
- Logs: `fly logs` (pretty format).
- Main metrics (all names: the `metrics::counter!`, `histogram!` and `gauge!` calls in
  `src/observability/metrics.rs` and `src/extraction/`):

| Area | Metrics |
|---|---|
| Extraction | `llm_requests_total{model,outcome}`, `llm_request_duration_seconds{model}`, `llm_retries_total{model}`, `llm_tokens_total{model,kind}`, `llm_estimated_cost_micro_usd_total{model}`, `extraction_cache_hits_total{backend}`, `errors_total{type,component}` |
| Telegram | `telegram_messages_total{type}`, `telegram_duplicate_messages_total`, `telegram_processing_duration_seconds{type}`, `requests_total`, `request_duration_seconds` |
| Database | `db_operations_total{operation}`, `db_operation_duration_seconds` |
| Health and process | `health_checks_total{type,result}`, `health_check_status{type}`, `health_check_duration_seconds`, `process_memory_mb` |

## Operations
- Before a push: the versioned hook `scripts/hooks/pre-push` (install once with
  `scripts/hooks/install.sh`) runs the unwrap check, `cargo fmt --check`, clippy and
  `cargo test --lib` (about a minute); CI runs every other test.
- Deploy: push to `main`, wait for green CI and staging, then run "Deploy to Production" and
  approve it.
- Try a change before production: after a push, the test bot runs it on staging.
- Rollback: automatic when a production deployment does not come up healthy. By hand: deploy an
  older image, `fly deploy --config deploy/fly.toml -a just-ingredients --image
  ghcr.io/basileduplessis/justingredients:sha-<commit>` (or a `registry.fly.io` image from
  `fly releases --image`). There is no extraction fallback: going back to Tesseract means
  reverting the Phase 5 commits.
- Status and logs: `fly status -a just-ingredients`, `fly logs -a just-ingredients`.

## Staging Setup
Done once by hand (the workflows never create or destroy staging):

```bash
fly apps create just-ingredients-staging --org personal
fly postgres create --name just-ingredients-staging-db --region cdg --org personal \
  --vm-size shared-cpu-1x --volume-size 1 --initial-cluster-size 1
fly postgres attach just-ingredients-staging-db --app just-ingredients-staging
fly secrets set --stage --app just-ingredients-staging \
  TELEGRAM_BOT_TOKEN="<test bot token>" GEMINI_API_KEY="<staging key>"
```

The test bot then answers from staging all the time: running the bot locally with the same
token would steal its updates.

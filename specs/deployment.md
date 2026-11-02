# Deployment & Infrastructure Feature Specification

> Describes the setup as it is (2026-10-27, after Tasks 5.1, 5.3 and 5.4 of
> [LLM Extraction Migration](tasks/llm-extraction-migration.md#cicd-audit)): no Tesseract, one
> image per commit, permanent staging, automatic production rollback.

## Overview
The bot runs as one Fly.io machine (app `just-ingredients`, region `cdg`) with a Fly Postgres
database. It receives Telegram updates by long polling, so the only HTTP traffic is health
checks and metrics. GitHub Actions tests every push and pull request to `main` and builds one image per commit of
`main`;
that image goes to a permanent staging app (with the test bot; stopped when no change is
being tested), then to production when a deployment is started by hand and approved.

## Components
| File | Purpose |
|---|---|
| `deploy/Dockerfile` | Image built once per commit of `main` by "Build and Test" (job `image`), pushed to `ghcr.io/basileduplessis/justingredients:sha-<commit>` (public package) |
| `deploy/fly.toml` | Production app `just-ingredients` |
| `deploy/fly-staging.toml` | Permanent staging app `just-ingredients-staging` (test bot, own database) |
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
- Locales, the extraction prompt and its schema are embedded in the binary: the runtime image
  holds the binary only. `RUST_LOG=info,sqlx=warn`.

## Fly Configuration (`deploy/fly.toml`, same in `fly-staging.toml`)
- `[env]`: `HEALTH_PORT=8080`, `METRICS_PORT=9091`, `LOG_FORMAT=json`, `RUST_LOG=info,sqlx=warn`,
  `METRICS_BIND_ALL_INTERFACES=true`. Logs are JSON lines (one object per event, its fields at
  the top level); `LOG_FORMAT=pretty` gives readable text for local runs. At `info` a use case logs one line with ids and counts
  (photo read, review sent, recipe saved); recipe and ingredient names, captions and search
  text are never logged at `info`, `warn` or `error`, and `error` is kept for failures.
- `[[restart]] policy = 'always'`: the bot restarts whenever it exits (the default policy gave
  up during a Postgres restart on 2026-10-16 and left the bot down for 10 days).
- Checks: `GET /health/live` every 15 s (timeout 2 s, grace 30 s), as a machine check and an
  `http_service` check.
- `[http_service]`: internal port 8080, HTTPS forced, `auto_stop_machines = false`,
  `min_machines_running = 1`.
- `[metrics]`: port 9091, path `/metrics`. Fly reads it every 15 s over its private network
  ([Monitoring](#monitoring)); it is not a service, so it is not reachable from the Internet.

## HTTP Endpoints
Two servers, bound to all interfaces only when `METRICS_BIND_ALL_INTERFACES=true` (localhost
otherwise). Code: `src/observability/server.rs`, `src/observability/health_checks.rs`.

The public one on `HEALTH_PORT` (8080), behind Fly's proxy, is limited to 10 requests per minute per
client, checked before routing (except `/health/live`, which does no I/O and is called by Fly's
two checks every 15 s): the client is `Fly-Client-IP` (set by Fly's proxy) when present,
else the TCP peer; at most 1,024 clients are remembered, the least recently seen is forgotten
first. Bodies are fixed texts: failure reasons go to the logs only.

| Endpoint | Behaviour |
|---|---|
| `GET /health/live` | 200 `OK` while the Telegram heartbeat is fresh; 503 `NOT LIVE` after 5 minutes without a successful Telegram call (`getMe` every minute, 5-minute grace at startup). |
| `GET /health/ready` | 200 when the database answers `SELECT 1`, a Telegram call has succeeded since the start and recently (no startup grace: required from before the server starts, and the startup `getMe` counts), and the bot token is well formed; 503 `NOT READY` otherwise, the reason logged as a warning. Gemini is not called. |

Every other path answers 404, `/metrics` included (there is no `/health/ocr`, `/health/db`,
`/ready` or `/live`).

The internal one on `METRICS_PORT` (9091) answers only `GET /metrics`, the Prometheus text
format, without a token: only Fly and the organization's other apps can reach it (no public
service). Every other path answers 404.

## Secrets and Configuration
- **Production** (Fly secrets, set by hand with `fly secrets set`, never by CI):
  `DATABASE_URL` (Fly Postgres), `TELEGRAM_BOT_TOKEN`, `GEMINI_API_KEY`, `OWNER_CHAT_ID` (the
  owner's Telegram id: [alerts and weekly report](#monitoring); optional).
  The bot refuses to start without a well-formed `TELEGRAM_BOT_TOKEN` and `DATABASE_URL`, or
  without `GEMINI_API_KEY`; it exits with `Telegram rejected TELEGRAM_BOT_TOKEN` when Telegram
  refuses the token (the restart policy then restarts it, and a deploy fails its health check).
- **Staging** (permanent app `just-ingredients-staging`, created once by hand, see
  [Staging Setup](#staging-setup)): Fly secrets `DATABASE_URL` (from `fly postgres attach` of
  `just-ingredients-staging-db`), `TELEGRAM_BOT_TOKEN` of the test bot, `GEMINI_API_KEY`.
- **GitHub environment secrets**: `FLY_API_TOKEN` in `staging` and in `production`, each a deploy
  token of its own app (`fly tokens create deploy -a <app>`); `FLY_STAGING_DB_TOKEN` in `staging`,
  a deploy token of `just-ingredients-staging-db`, to start and stop the staging database
  (without it the database keeps running); there is no repository-level token (deleted on
  2026-10-27).
- **GitHub repository secrets**: `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` for the production
  deploy notification and the uptime alert.
- Optional variables with defaults: `GEMINI_MODEL`, `GEMINI_TIMEOUT_SECS`, `GEMINI_THINKING_LEVEL`,
  `GEMINI_MAX_CONCURRENCY`, `EXTRACTION_DAILY_LIMIT_PER_USER`, `HEALTH_PORT`, `METRICS_PORT`,
  `METRICS_BIND_ALL_INTERFACES`, `DATABASE_MAX_CONNECTIONS`, `DATABASE_MIN_CONNECTIONS`,
  `DATABASE_CONNECT_TIMEOUT_SECS`, `LOG_FORMAT`, `RUST_LOG`, `OWNER_CHAT_ID`; see `.env.example`.
- Not applied although read: `HTTP_CLIENT_TIMEOUT_SECS` is fixed at 30 s in `main.rs`; `REQUEST_DEDUPLICATION_TTL_SECS` is
  parsed but the deduplicator keeps 300 s and 10,000 entries.

## CI/CD (`.github/workflows/`)
| Workflow | Trigger | What it does |
|---|---|---|
| `build-test.yml` "Build and Test" | Push and PR to `main` | `build`: Postgres 16 service, `cargo build`, `cargo test --all-targets`, doc tests, `db_tests` with `--include-ignored`. `lint`: clippy `-D warnings`, `scripts/check-unwraps.sh`, `cargo fmt --check`. `security`: cargo-deny `check advisories sources`. `image` (pushes to `main` only, after the three others): Buildx with the GitHub Actions layer cache, pushes `ghcr.io/basileduplessis/justingredients:sha-<commit>`; dependencies are compiled in their own layer, rebuilt only when `Cargo.toml` or `Cargo.lock` change. Rust 1.99, `SQLX_OFFLINE=true`, Ubuntu 26.04, no system packages. The benchmarks of `tests/performance_tests.rs` are ignored (IMP-046). |
| `deploy-staging.yml` "Deploy to Staging" | `workflow_run` after a successful "Build and Test" on a push to `main` from this repository (or manual on `main`) | Run name `Deploy to Staging <sha>`. `flyctl deploy --ha=false --image ghcr.io/…:sha-<sha>` on the permanent staging app (no build; one machine, since two pollers of one bot token steal each other's updates), then the smoke tests (`/health/live`, `/health/ready`). It first starts the staging database (stopped when idle, below), and after the deploy starts the bot's machine (a deploy updates a stopped machine without starting it) and waits up to a minute for `/health/live`. |
| `stop-staging.yml` "Stop Staging" | After a successful "Deploy to Production", every night at 02:00 UTC, and manual | Stops the staging bot's machine and, with `FLY_STAGING_DB_TOKEN`, the staging database's: staging runs only while a change is tested (2026-11-02: four machines running all the time went over the account's free allowance; a stopped machine is billed for its disk only). Same concurrency group as the staging deployments. `auto_start_machines = false` in `fly-staging.toml`: a request does not wake it up. |
| `deploy-production.yml` "Deploy to Production" | Manual, `main` only | Environment `production` with a required reviewer. Gate: a successful "Build and Test" push run for the commit and a successful run named `Deploy to Staging <sha>`. Records the running release: image digest (`fly machines list`) and deployed configuration (`fly config save`). Deploys the image staging ran (`--ha=false --image ghcr.io/…:sha-<sha>`, no build), waits for `/health/live` and `/health/ready` (`scripts/check-health.sh`, 6 tries 10 s apart); on failure deploys the recorded image with the recorded configuration and checks it. Telegram notification: deployed, rolled back, or failed. |
| `uptime.yml` "Production Uptime" | Every 10 minutes (and manual) | `GET /health/ready` on production, 3 tries 20 s apart; Telegram alert on failure. GitHub may delay scheduled runs and disables them after 60 days without activity. |

## Monitoring
Plan O, [specs/tasks/observability.md](tasks/observability.md). No monitoring app of our own:
the Prometheus and Grafana apps were removed in 2026-11 (they never collected anything).

- Metrics: Fly collects for free, every 15 s, and keeps about 15 days of our metrics (the
  `[metrics]` port), the machines' (CPU, memory, restarts, out of memory), the proxy's and
  Postgres' (`pg_*`). They are shown in Fly's managed Grafana, [fly-metrics.net](https://fly-metrics.net)
  (Fly account), and can be queried on `https://api.fly.io/prometheus/<org>/` with a read-only
  token.
- Our metrics are all named `ji_…` (`src/observability/metrics.rs`) and carry no user id nor
  recipe content:

| Question | Metrics |
|---|---|
| Usage | `ji_photos_total{result}` (`read`, `not_recipe`, `unreadable`, `too_large`, `daily_limit`, `gemini_error`), `ji_recipes_created_total{kind}` (`photo`, `linked`), `ji_actions_total{action}` |
| Gemini and its cost | `ji_gemini_requests_total{model,result}`, `ji_gemini_duration_seconds{model}`, `ji_gemini_retries_total{model}`, `ji_gemini_tokens_total{model,kind}`, `ji_gemini_cost_micro_usd_total{model}`, `ji_gemini_cache_hits_total` |
| Health | `ji_errors_total{component,kind}`, `ji_gemini_breaker_open`, `ji_started_at_seconds` |
| Speed | `ji_update_duration_seconds{kind}` (`message`, `callback`), `ji_card_shown_seconds`, `ji_db_duration_seconds{operation}` |

- Gauges computed from the database every hour: `ji_users{period}` (`day`, `week`: users seen
  in the last 24 hours and 7 days; `total`), `ji_recipes`.
- History beyond Fly's 15 days (migration 7): `users.last_seen_at`, written at most once an
  hour per user and at the first update of a Paris day; `usage_days`, one line per Paris day
  (active users, photos read and failed, recipes created, Gemini calls, failures and estimated
  cost), counted in memory and written every minute (`src/observability/usage.rs`);
  `app_state`, the release running and the last weekly report.
- Alerts the bot sends to `OWNER_CHAT_ID`, in French, each subject at most once an hour
  (`src/observability/alerts.rs`): 🔴 Gemini down (the circuit breaker opened) then 🟢 back;
  🟠 Gemini failing (over 30 % of at least 5 calls in the last hour); 💸 the day's estimated
  Gemini cost over 1 $; 🟠 database unreachable (two checks a minute apart) then 🟢 back; 🔁 a
  restart without a new release (same Fly machine and configuration version as at the previous
  start). Without `OWNER_CHAT_ID` none is sent.
- Weekly report to `OWNER_CHAT_ID` on Monday from 9:00, Paris time (or at the next start if the
  bot was down then), for the week before: active users (seen since that Monday: a user keeps
  only the last visit), new and all users, photos, recipes, Gemini's cost and failures
  (`src/observability/report.rs`). `/stats`, for the owner only and not in the menu, gives the
  same for the last 7 and 30 days and since the history started.
- The uptime workflow and the production deploy notification stay the alerts for a bot that does
  not answer and for releases.
- Logs: `fly logs` and the app's Monitoring page on fly.io (JSON lines).

## Operations
- Before a push: the versioned hook `scripts/hooks/pre-push` (install once with
  `scripts/hooks/install.sh`) runs the unwrap check, `cargo fmt --check`, clippy and
  `cargo test --lib` (about a minute); CI runs every other test.
- Deploy: push to `main`, wait for green CI and staging, then run "Deploy to Production" and
  approve it.
- Try a change before production: after a push, the test bot runs it on staging (started by the
  deployment, stopped again after the release or at night). To start it without a push:
  `gh workflow run "Deploy to Staging" --ref main`; to stop it at once: `gh workflow run
  "Stop Staging"`.
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

The test bot then answers from staging while staging runs: running the bot locally with the
same token would steal its updates.

To let the workflows start and stop the staging database (done once by the user, the token
never printed):

```bash
fly tokens create deploy -a just-ingredients-staging-db -x 8760h -n github-staging-db \
  | gh secret set FLY_STAGING_DB_TOKEN --env staging
```

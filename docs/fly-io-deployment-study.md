# Fly.io Deployment Study

## 1. Deployment Goals
- Ship the Telegram OCR bot with minimal operational overhead.
- Guarantee reliable database access with schema alignment and automatic migrations.
- Protect secrets (`TELEGRAM_BOT_TOKEN`, `DATABASE_URL`, optional observability vars).
- Roll out progressively with rigorous pre-deploy and post-deploy testing.
- Keep Fly.io resource usage in the free or lowest paid tiers whenever possible.

## 2. Architecture Overview
- **Bot service**: Rust binary (`just-ingredients`) built into a container, exposes no public HTTP API but requires outbound HTTPS (Telegram + Postgres).
- **Database**: PostgreSQL 15+ instance (Fly Postgres cluster or external managed service). Schema initialized by `db::init_database_schema()` on startup.
- **Supporting services**: Optional health and metrics endpoints if `HEALTH_PORT` is set. Observability configurable via environment variables.

```
Telegram <-HTTPS-> Fly App (bot) <-TLS-> Fly Postgres
```

## 3. Database Strategy
- **Provisioning**: Use `fly postgres create --name just-ingredients-db --region <region>` (shared-cpu-1x, 1GB volume fits free tier). For minimal cost, keep single node; enable nightly snapshots.
- **Connectivity**: The app consumes a URL formatted as `postgres://username:password@just-ingredients-db.internal:5432/ingredients`. Fly automatically injects a private DNS record (`.internal`).
- **Schema management**:
	- The application initializes tables (`users`, `ocr_entries`, `ingredients`) via `db::init_database_schema()` on boot.
	- To avoid startup race conditions, run an isolated migration job (`fly run ./target/release/just-ingredients migrate`) before scaling app to >0 if future manual migrations are added.
	- Keep local schema docs in `data_model.md` synced with runtime schema.
- **Validation**: Execute `cargo sqlx prepare --check` locally prior to deploy to ensure query metadata remains valid.

## 4. Application Container & Configuration
- **Dockerfile**: Build a multi-stage image (Rust build stage → slim runtime). Use `cargo build --release` and copy `target/release/just-ingredients` into an Alpine or Debian runtime with `libtesseract` deps.
- **Runtime environment variables** (set via `fly secrets set`):
	- `TELEGRAM_BOT_TOKEN=<telegram token>`
	- `DATABASE_URL=postgres://username:password@just-ingredients-db.internal:5432/ingredients`
	- Optional: `HEALTH_PORT=8080`, `LOG_FORMAT=pretty`, `RUST_LOG=info,sqlx=warn`.
- **File system**: Bot downloads images to temp files; ensure the container has enough `/tmp` space (default Fly ephemeral disk is sufficient for <20MB files).
- **Bot <-> DB connectivity**: `sqlx` uses pooled connections; default pool size is adequate. If latency spikes, pin app to same Fly region as Postgres.

## 5. Progressive Deployment Strategy
- **Local gating**: Before any deploy run `cargo fmt --all`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test` (93 tests) locally.
- **Staging app**:
	- Create a staging Fly app (`fly launch --name just-ingredients-staging`).
	- Point staging to a separate small Postgres DB (`just-ingredients-staging-db`).
	- Use a dedicated Telegram bot token to avoid cross-environment chatter.
	- Run smoke tests by sending sample images; verify ingredient parsing and DB inserts.
- **Blue/green rollout**:
	- Deploy to production app with `fly deploy --config fly.prod.toml`.
	- Keep previous version warm by setting `fly scale count 2` temporarily (1 old, 1 new). After verifying logs/metrics, scale back to `count 1`.
- **Health verification**:
	- Expose `/health` via `HEALTH_PORT` and configure Fly checks (`services.concurrency` + `checks` block in `fly.toml`).
	- Monitor `fly logs` for OCR errors, DB failures, circuit breaker trips.
- **Automated regression**:
	- Schedule nightly `fly machines run` job hitting `cargo test --lib` in a build image, or maintain GitHub Actions that run tests before `fly deploy`.
	- Add synthetic transactions (cron Telegram message) to verify real traffic.

## 6. Cost Optimization
- **App machine**: Use `shared-cpu-1x` with `memory = 256` or `512` MB (fits bot footprint). Enable auto-stop by scaling to 0 during low traffic using `fly scale count 0` overnight if desired.
- **Postgres**: Stick to Fly Postgres Lite (single node, 1GB volume). Monitor storage growth; archive OCR history periodically if approaching limits.
- **Traffic**: Telegram interactions are outbound; no extra bandwidth costs on Fly free tier. Keep logs in `info` level to limit volume.
- **Observability**: Reuse built-in Fly metrics rather than external stacks until load justifies.

## 7. Deployment Checklist
- [ ] `cargo fmt --all`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test`
- [ ] Build & push image: `fly deploy --build-only` (optional for validation)
- [ ] `fly secrets set TELEGRAM_BOT_TOKEN=... DATABASE_URL=...`
- [ ] Verify staging bot end-to-end
- [ ] Deploy to production and monitor `fly status` & health checks for 30 minutes
- [ ] Scale down redundant instances once stable

## 8. Future Enhancements
- Automate migrations with `sqlx migrate run` once schema evolves.
- Introduce per-environment `fly.toml` files (staging vs production) committed to repo.
- Add alerting hooks (Fly autoscale alerts + Telegram admin channel) for circuit breaker activation or DB failures.

## 9. Deployment TODOs
- [ ] **Containerization**: Write a multi-stage Dockerfile, ensure Tesseract dependencies are installed, add to repo.
- [ ] **Fly configs**: Generate `fly.staging.toml` and `fly.prod.toml` with region, health checks, machine sizing, and secrets placeholders.
- [ ] **Secrets management**: Script `fly secrets set` commands for staging and production tokens/URLs.
- [ ] **Staging infra**: Provision Fly Postgres and app (`fly launch`) for staging; wire dedicated Telegram token.
- [ ] **Smoke scripts**: Create helper to send sample Telegram images and validate staged output.
- [ ] **CI pipeline**: Extend existing CI to run `cargo fmt`, `cargo clippy`, `cargo test`, and build image before deploy.
- [ ] **Migration job**: Add optional CLI flag or subcommand to run schema migrations separately if needed.
- [ ] **Production rollout**: Provision production Fly Postgres + app, apply secrets, execute first `fly deploy`.
- [ ] **Monitoring hooks**: Enable Fly health checks, capture logs, and set up basic alerts or dashboards.
- [ ] **Cost review**: Schedule periodic review of Fly usage and database volume to stay within free/low-cost tiers.

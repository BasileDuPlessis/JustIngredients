# LLM Extraction Migration (Tesseract → Gemini)

## Goal
Replace Tesseract OCR and the regex-based ingredient parsing of photos with a single
Gemini vision call that returns the ingredients as structured JSON.

## Scope
- In scope: Gemini client, extraction domain types, bot integration, configuration and
  secrets, metrics, tests, CI/CD and Docker changes, removal of the OCR code.
- Out of scope: unit conversion between systems (g ↔ cup), translation of ingredient
  names, UI redesign. General code-quality items live in
  [improvement-backlog.md](improvement-backlog.md) and are only pulled in when a task
  touches the same code.

## Decisions (2026-10-26)
| Topic | Decision |
|---|---|
| Provider / model | Google Gemini API, `gemini-3.5-flash-lite`, thinking level `minimal`. Model is configuration, not code. |
| Benchmark | `scripts/llm_benchmark/gemini_benchmark.py`: 100% on `test_images/recipe_with_fraction.jpg`, ~$0.0015/photo, ~1.8 s. `gemini-3.1-flash-lite` is cheaper but already "legacy"; `gemini-2.5-flash-lite` is no longer available to new users. |
| API surface | `POST /v1beta/models/{model}:generateContent`, header `x-goog-api-key`, `generationConfig.responseMimeType = application/json` + `responseJsonSchema`, `thinkingConfig.thinkingLevel`. Validated live by the benchmark. |
| Input images | Telegram photos and image documents: screenshots of recipe web pages and photos of printed cookbook pages, French or English. |
| Transition | `EXTRACTION_BACKEND=gemini\|tesseract` (default `gemini`) for one release, then Tesseract is removed (Phase 5). |
| Branching | Work directly on `main`, small commits, CI green before starting the next task. |
| Output contract | See "Extraction contract" below. |

Answered 2026-10-26:
- Ingredients without quantity are kept ("sel" → `quantity = null`, `unit = null`). Vague amounts
  such as "une pincée", "un filet", "à volonté" are neither ingredients nor units: "une pincée de
  sel" → name `sel`, `quantity = null`, `unit = null`.
- The Gemini API key is on the paid tier (content not used by Google to improve its products).
- Required status checks on `main` are not used (work happens directly on `main`); gating is done
  in the workflows (Task 0.9). Outage alerting is a scheduled workflow (Task 0.11).

## Open Questions
1. Counted containers ("2 gousses d'ail", "1 sachet de levure", "3 tranches de jambon"): keep the
   container as unit (`gousse`, `sachet`, `tranche`), or name `ail` with `quantity = 2`,
   `unit = null`?
2. _(answered)_
3. GitHub secret name: reuse the existing `GOOGLE_AI_API_KEY` (created 2025-10-31) or create
   `GEMINI_API_KEY`? The application reads `GEMINI_API_KEY`.
4. 5–10 real images (web screenshots + cookbook photos, some skewed or poorly lit) are needed
   to extend the benchmark before Phase 4.
5. Keep the full `transcription` in the response (feeds `recipes.content` full-text search) or
   rebuild it from `source_text` lines (~30–40% cheaper, output tokens dominate the cost)?

## Target Architecture
```
Telegram photo/document
  → download into memory (size cap, timeout, no temp file, token never logged)
  → ImageInput { bytes, mime }            (JPEG/PNG/WebP; other formats re-encoded or rejected)
  → ExtractionCache (sha256 of bytes)     (never pay twice for the same image)
  → Semaphore (max concurrent calls)
  → IngredientExtractor::extract()        (GeminiExtractor | TesseractExtractor during transition)
  → Extraction { transcription, language, is_recipe, ingredients }
  → units::canonicalize + validation      (shared with manual edits)
  → Vec<MeasurementMatch>                 (existing review UI, editing and DB code unchanged)
```

Module layout:
- `src/extraction/mod.rs`: `IngredientExtractor` trait, `ImageInput`, `Extraction`,
  `ExtractedIngredient`, `ExtractionError` (thiserror, already a dependency).
- `src/extraction/gemini/{client.rs, types.rs}`: shared `reqwest::Client`, serde request/response
  types, retry and circuit breaker, usage → metrics.
- `src/extraction/mapping.rs`: `Extraction` → `Vec<MeasurementMatch>`.
- `src/extraction/tesseract.rs`: adapter over the current OCR pipeline (deleted in Phase 5).
- `src/units.rs`: canonical units (measurement-standardization Task 2, relocated here so it
  serves both the LLM output and manual edits).
- `config/extraction/prompt.md` + `config/extraction/schema.json`: single source of truth,
  embedded with `include_str!` and read by the benchmark script.

### Extraction contract
```json
{
  "is_recipe": true,
  "language": "fr",
  "transcription": "150 g de farine\n2 oeufs\n...",
  "ingredients": [
    { "name": "farine", "quantity": 150, "unit": "g", "needs_review": false, "source_text": "150 g de farine" }
  ]
}
```
Mapping: `name` → `ingredient_name`, `quantity` → `quantity` (decimal string), `unit` → `measurement`
(canonicalized), `needs_review` → `requires_quantity_confirmation`, list index → `line_number`,
`transcription` → `recipes.content`, `source_text` → `ingredients.raw_text`.
Validation still applies: name ≤ 100 chars, 0 < quantity ≤ 10000.

## Rust Practices for This Work
- No `unwrap`/`expect`/`panic!` (project policy); new modules start with
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
- One `thiserror` enum per module, no `String` errors; errors carry context, never secrets.
- API key held in a type whose `Debug`/`Display` redact the value; never logged, never in URLs.
- Every I/O has a timeout; no blocking call in async code; retries only on 429/5xx/timeouts,
  honoring `Retry-After`.
- Trait-based seam (`IngredientExtractor`) so the bot is tested without network.
- Tests for every error path: serde fixtures for responses, HTTP mock server (`wiremock`,
  dev-dependency) for 429/500/timeouts/malformed JSON, `#[ignore]` live test using
  `GEMINI_API_KEY`.
- Modules under ~500 lines, public items documented, `tracing` fields instead of string
  formatting.

## Code Review Process
- Every task: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test`, `cargo test --doc`, then one commit referencing the task id (e.g. `feat(extraction): 2.2 ...`).
- Every phase ends with a **review gate**: an independent reviewer (fresh sub-agent or
  `/code-review high`) reviews `git diff <phase-start>..HEAD` against this plan and the practices
  above. Findings are fixed before the next phase or moved to the improvement backlog with a
  reason. Each gate is recorded below (date, commit range, findings, outcome).
- Phase 4 additionally requires a go/no-go on production metrics before Phase 5.

---

## Phase 0: Stabilize Before Changing (prerequisites)
The pipeline is red and the Docker image it builds cannot start (see "CI/CD Audit").
**No production deploy until Tasks 0.1 to 0.4 are done.** Production keeps running the
March 2026 image meanwhile (restarts reuse it).

### Task 0.1: Messages are dropped as duplicates (IMP-001, <= 1 hour)
- [x] Done
- Description: since commit `a0d24fa` (pushed 2026-10-26, not yet in production),
  `message_handler_with_cache` checks the deduplicator and then calls `message_handler`, which
  checks it again; `is_duplicate` records the request on the first call, so the second call
  drops every message except `/recipes` (`src/bot/message_handler.rs`). Check once.
- How to validate: handler test sending one message through `message_handler_with_cache` asserts
  it is processed; `telegram_duplicate_messages_total` stays at 0 on staging.

### Task 0.2: Recipe ownership is never checked (IMP-002, <= 1 hour)
- [x] Done
- Description: recipe ids come from callback data and the DB functions filter on `id` only
  (`delete_recipe`, `update_recipe_name`, `read_recipe*` in `src/db.rs`), so a modified Telegram
  client can read, rename, edit or delete another user's recipe. Already in production. Scope every
  recipe and ingredient query by the owner.
- How to validate: DB tests where user B gets "not found" on user A's recipe for each operation.

### Task 0.3: Unblock CI (<= 1 hour)
- [x] Done
- Description: pin `runs-on: ubuntu-24.04` in every workflow (Ubuntu 26.04 ships leptonica 1.86,
  whose headers break `leptonica-sys` bindgen: `environ.h:70 unknown type name 'atomic_int'`).
  Temporary until Tesseract is removed in Phase 5.
- How to validate: Build and Test green on `main`.

### Task 0.4: Fix the Docker image (<= 1 hour)
- [x] Done
- Description: pin builder and runtime to the same Debian release (`rust:<version>-slim-trixie`
  → `debian:trixie-slim`), install the matching Tesseract/Leptonica runtime packages, remove the
  `|| true` that swallows apt failures (`deploy/Dockerfile:39-40`), and fail the build when a
  shared library is missing (`RUN ! ldd /usr/local/bin/just-ingredients | grep 'not found'`).
- How to validate: `docker build` locally, container starts and `/health/ready` returns 200.

### Task 0.5: Survive database restarts (<= 1 hour)
- [x] Done
- Description: root cause of the 2026-10-16 outage. `PgPool::connect` (`src/main.rs:201`)
  gives up after sqlx's 30 s default and the process exits with code 1; Fly retried while Postgres
  was still recovering, then left the machine stopped for 10 days. Connect with retry and
  exponential backoff (bounded, logged), and add a `[[restart]]` policy `always` in
  `deploy/fly.toml` and `deploy/fly-staging.toml`.
- How to validate: start the bot with Postgres stopped, start Postgres 2 minutes later, bot
  connects without exiting.

### Task 0.6: Stop leaking secrets in logs (<= 1 hour)
- [x] Done
- Description: `DATABASE_URL` (with password) is logged at startup (`src/main.rs:198`); the
  Telegram file URL (contains the bot token) can appear in reqwest errors
  (`src/bot/image_processing.rs`). Fix both before introducing a third secret.
- Progress: the `DATABASE_URL` part is done with Task 0.5 (logs show host/port/database only);
  the bot token in the file download URL remains.
- How to validate: unit test on the redaction helpers; startup logs on staging.

### Task 0.7: Dependency advisories (<= 1 hour)
- [ ] Done
- Description: `cargo audit` reports 17 advisories (rustls, rustls-webpki, aws-lc-sys, h2,
  webpki-roots, crossbeam-epoch). `cargo update`, then replace the from-source
  `cargo install cargo-audit && cargo outdated` step with `EmbarkStudios/cargo-deny-action@v2`
  and a `deny.toml`; Dependabot for updates.
- How to validate: security job green in under a minute.

### Task 0.8: Run the whole test suite in CI (<= 1 hour)
- [x] Done
- Description: CI runs only `cargo test --lib`; `tests/` and doc tests never run. Add a Postgres
  service container, `cargo test --all-targets` and `cargo test --doc`; fix or quarantine failing
  tests. `db_tests` drop the tables of whatever `DATABASE_URL` points to and keep
  `schema_migrations`, so a second run fails: move them to `#[sqlx::test]` (one database per
  test). Add `rust-cache` to every job.
- How to validate: green run listing the integration test binaries.

### Task 0.9: Gate deployments (<= 1 hour)
- [ ] Done
- Description: staging only after Build and Test succeeds (`workflow_run`), production only
  after staging; `concurrency` groups; required status checks on `main` (GitHub setting, user
  action).
- How to validate: a red CI run does not trigger the staging deploy.

### Task 0.10: Staging workflow hygiene (<= 1 hour)
- [x] Done
- Description: remove the hardcoded `staging_password_123` (public repo, printed in logs), the
  `continue-on-error` on DB create/attach, the retry step that can never run (its `if:` lacks
  `failure()`), the export of `FLY_API_TOKEN` to `$GITHUB_ENV`, and `curl | sh` + `@master` actions
  (pin `superfly/flyctl-actions/setup-flyctl` by SHA). Confirm staging uses its own bot token (Fly
  secret digests differ: `9c50…` staging vs `f12d…` prod, so most likely yes) and move it to an
  environment-scoped secret.
- How to validate: staging run green; no secret value in logs.
- Progress: workflow part done (no hardcoded password, generated password kept out of logs, no
  `continue-on-error`, dead retry removed, token not exported to `$GITHUB_ENV`, `setup-flyctl`
  pinned by SHA, Node 22). Staging bot token set as a `staging` environment secret (2026-10-27);
  first staging run with it: 40476257980, green, production unaffected.

### Task 0.11: Outage alerting (<= 1 hour)
- [x] Done
- Description: external uptime check on `https://just-ingredients.fly.dev/health/ready` with a
  Telegram or email alert (free monitor, user action), plus a scheduled workflow (`*/10`) as a
  backup. `/health/live` must reflect the bot loop, not always return "OK".
- How to validate: stopping the staging machine triggers an alert within 15 minutes.
- Progress: `.github/workflows/uptime.yml` checks production every 10 minutes and alerts on
  Telegram (first scheduled runs green). `/health/live` reflecting the bot loop moved to Task 3.5,
  which rewrites the health checks anyway; an external monitor stays optional.

### Review gate R0
- [x] Done

## Phase 1: Foundations (no behavior change)

### Task 1.1: Canonical units module (<= 1 hour)
- [x] Done
- Description: `src/units.rs` with `canonicalize_unit(&str) -> Option<CanonicalUnit>` per the
  measurement contract, plus French spoon units (`cuillère à soupe`/`c. à s.` → `tbsp`,
  `cuillère à café`/`c. à c.` → `tsp`). Fix the contract: `cl`/`dl` are not aliases of `ml`
  (they need quantity conversion ×10/×100), `cc` is ambiguous (teaspoon in French recipes).
- How to validate: table-driven tests (`g`, `gr`, `GR`, `g.`, `grammes`, `c. à s.`, `cl`).

### Task 1.2: Gemini configuration (<= 1 hour)
- [x] Done
- Description: `GeminiConfig` in `AppConfig`: `GEMINI_API_KEY` (required when backend is
  `gemini`), `GEMINI_MODEL` (default `gemini-3.5-flash-lite`), `GEMINI_TIMEOUT_SECS` (30),
  `GEMINI_THINKING_LEVEL` (`minimal`), `GEMINI_MAX_CONCURRENCY` (4), `EXTRACTION_BACKEND`.
  Redacted in `Debug` and `AppConfig::summary()`. Document in `.env.example`.
- How to validate: tests for defaults, missing key, invalid values, redaction.
- Done in `src/extraction/config.rs` and `src/secret.rs` (`SecretString`, redacted `Debug`).
  `EXTRACTION_BACKEND` defaults to `tesseract` until Task 3.1 wires the Gemini path and flips the
  default; requiring `GEMINI_API_KEY` earlier would stop production at startup.

### Task 1.3: Extraction domain types and trait (<= 1 hour)
- [x] Done
- Description: `src/extraction/mod.rs` and `mapping.rs` (types, trait, `ExtractionError`,
  mapping to `MeasurementMatch` with canonical units and validation).
- How to validate: mapping tests (decimal quantities, null quantity, unknown unit, long name,
  quantity > 10000 flagged).

### Task 1.4: Prompt and schema as shared assets (<= 1 hour)
- [x] Done
- Description: move the prompt and schema from the benchmark script to `config/extraction/`;
  the script and Rust (`include_str!`) both read them.
- How to validate: benchmark still at 100%; Rust test that the schema parses as JSON.

### Review gate R1
- [ ] Done

## Phase 2: Gemini Client

### Task 2.1: Request/response types (<= 1 hour)
- [ ] Done
- Description: serde types for `generateContent` (inline image, generation config, usage
  metadata, finish reason, prompt feedback). Response text parsed into `Extraction`.
- How to validate: fixture tests from recorded responses in `tests/fixtures/gemini/`
  (success, `MAX_TOKENS`, `SAFETY` block, malformed JSON, empty candidates).

### Task 2.2: HTTP client with retries and circuit breaker (<= 1 hour)
- [ ] Done
- Description: `GeminiExtractor` with a shared `reqwest::Client` (connect + total timeouts),
  retry with jittered backoff on 429/500/503/timeouts honoring `Retry-After`, no retry on 4xx,
  existing `CircuitBreaker` (moved out of OCR config, and given a real half-open state first: today
  it has none), key only in the header.
- How to validate: `wiremock` tests for each status, timeout, and breaker opening.

### Task 2.3: In-memory image input (<= 1 hour)
- [ ] Done
- Description: download to `Bytes` with a size cap and timeout, magic-byte format detection,
  re-encode BMP/TIFF to PNG with the `image` crate or reject with a clear message.
- How to validate: tests with PNG/JPEG/WebP/BMP/oversized/garbage inputs.

### Task 2.4: Metrics and tracing (<= 1 hour)
- [ ] Done
- Description: `llm_requests_total{model,outcome}`, `llm_request_duration_seconds`,
  `llm_tokens_total{model,kind}`, `llm_estimated_cost_usd_total{model}`, one tracing span per
  call (model, tokens, latency, finish reason). `/metrics` and `/health/*` are publicly reachable
  through `[http_service]`: protect `/metrics` (existing `METRICS_AUTH_TOKEN` or a private port)
  before exposing cost data.
- How to validate: metrics test with the in-process recorder; `/metrics` on staging returns 401
  without the token.

### Task 2.5: Live test (<= 1 hour)
- [ ] Done
- Description: `#[ignore]` test running the real API on `test_images/`, skipped without
  `GEMINI_API_KEY`. Documented in README.
- How to validate: `cargo test --test gemini_live -- --ignored` passes locally.

### Review gate R2
- [ ] Done

## Phase 3: Bot Integration

### Task 3.1: Wire the extractor behind the backend switch (<= 1 hour)
- [ ] Done
- Description: switch the `EXTRACTION_BACKEND` default to `gemini` (set `GEMINI_API_KEY` on Fly first,
  Task 4.1). Replace the OCR call in `download_and_process_image`
  (`src/bot/image_processing.rs`) with `IngredientExtractor`; `TesseractExtractor` adapter keeps
  the old path available.
- How to validate: bot tests with a fake extractor; manual test on staging with both backends.

### Task 3.2: Persistence mapping (<= 1 hour)
- [ ] Done
- Description: `transcription` → `recipes.content`, `source_text` → `ingredients.raw_text`
  (instead of the whole text in every row), apply the decision of open question 1. The Confirm
  button must refuse to save items still flagged `requires_quantity_confirmation` (IMP-004: today
  it saves quantity 0), otherwise `needs_review` from Gemini has no effect.
- How to validate: DB test on saved rows; review test where Confirm with a flagged item asks for
  the quantity.

### Task 3.3: User-facing errors and messages (<= 1 hour)
- [ ] Done
- Description: `error-extraction-*` keys in `locales/{en,fr}/main.ftl` (unavailable, rate
  limited, image unsupported, not a recipe), fix the `{$msg}` placeholder bug, remove "OCR"
  wording.
- How to validate: localization tests for both languages.

### Task 3.4: Cost and load protection (<= 1 hour)
- [ ] Done
- Description: `tokio::sync::Semaphore` around calls, bounded `ExtractionCache` keyed by sha256
  with TTL and size limit (do not reuse `OcrResultCache`: unbounded and never pruned), optional
  per-user daily limit.
- How to validate: tests for cache hit (no second call) and concurrency limit.

### Task 3.5: Health checks without Tesseract (<= 1 hour)
- [ ] Done
- Description: replace `check_ocr_health` with a configuration check; readiness must not
  call the paid API. Make `/health/live` reflect the bot (moved from Task 0.11): track the last
  successful Telegram call instead of always answering "OK" (see IMP-039).
- How to validate: `/health/ready` returns 200 without Tesseract installed.

### Review gate R3
- [ ] Done

## Phase 4: Rollout

### Task 4.1: Secrets (user action)
- [ ] Done
- Description: `fly secrets set GEMINI_API_KEY=... -a just-ingredients-staging` and
  `-a just-ingredients`; GitHub secret for the staging workflow.

### Task 4.2: Staging then production
- [ ] Done
- Description: deploy staging, end-to-end test with the staging bot (separate token), then
  production through the manual workflow.
- How to validate: recipe photo → review → save on production.

### Task 4.3: One week of observation
- [ ] Done
- Description: watch cost per day, error rate, `needs_review` rate, latency; extend the
  benchmark with real images.
- How to validate: go/no-go recorded in review gate R4.

### Review gate R4 (go/no-go for Tesseract removal)
- [ ] Done

## Phase 5: Remove Tesseract

### Task 5.1: Delete OCR code and dependencies
- [ ] Done
- Description: delete `src/ocr.rs`, `src/ocr/`, `src/preprocessing/`, `src/error_correction.rs`,
  `src/instance_manager.rs`, `src/path_validation.rs`, `src/ocr_errors.rs`, most of
  `src/ocr_config.rs`, the hOCR recovery in `image_processing.rs`, `TesseractExtractor`,
  OCR-only tests, `config/user_words.txt`, `config/user_patterns.txt`; remove `leptess`.
  ~8,200 lines of `src` and ~1,850 lines of tests.
- How to validate: build, clippy and tests green; `cargo tree | grep -i tess` empty.

### Task 5.2: Trim the text parser
- [ ] Done
- Description: remove OCR-noise handling from `text_processing.rs` (multi-line merging, OCR
  fraction fixes, letter-mix anomalies), keep the parser used for manual input.
- How to validate: `text_processing_tests` for manual input still green.

### Task 5.3: Docker image and CI
- [ ] Done
- Description: remove tesseract/leptonica/clang packages from the Dockerfile, remove apt steps
  from CI, move CI back to a current runner image.
- How to validate: image size before/after; container starts on staging.

### Task 5.4: Target CI/CD pipeline
- [ ] Done
- Description: implement the target pipeline described in "CI/CD Audit": one `ci.yml`
  (lint, test with Postgres, deny, single image build with cache pushed by commit SHA) and one
  `deploy.yml` (staging then production deploying the same image digest, smoke tests including a
  Gemini call on a fixture, notification, rollback to the previous digest on failure). Remove the
  duplicated Docker build and the destroy/recreate of staging (permanent staging app and DB).
  Pre-push hook reduced to fmt + clippy + `cargo test --lib`, versioned in `scripts/hooks/`.
- How to validate: one push produces one image that reaches staging, then production after
  approval.

### Task 5.5: Documentation
- [ ] Done
- Description: replace `specs/ocr-processing.md` and `specs/ocr-text-cleaning.md` with
  `specs/llm-extraction.md`; update README, `docs/`, `specs/deployment.md`,
  `.github/copilot-instructions.md`.

### Review gate R5 (final)
- [ ] Done

---

## CI/CD Audit
Audit of 2026-10-26 on commit `6537576` (runs 40400712736, 40400712652, 40400712808).

**Verdict: the pipeline does not work.**
- Build and Test: red. `ubuntu-latest` moved to Ubuntu 26.04; `leptonica-sys` bindgen fails on
  the new leptonica headers. Since January, 6 of 18 runs passed (mostly lint failures).
- Build and Push Docker Image: green but false. `E: Unable to locate package liblept5` is
  swallowed by `|| true`; the image (42 MB) lacks libssl, CA certificates and Tesseract and
  cannot start.
- Deploy to Staging: red. The machine never passed health checks; cleanup destroyed the app
  and its logs. Staging has deployed on red CI 8 times since January (no gating).
- Security: 17 advisories; compiling `cargo-audit` alone takes ~3 minutes; `cargo outdated`
  never runs.
- Production deploy is manual and independent of CI; `main` has no required status checks.
- Monitoring: nothing alerted during the 10-day outage (no restart policy, `/health/live`
  always "OK", no alert rules).

P0 and quick wins are Phase 0 tasks. Remaining items (Node 20 deprecations, smoke tests on Node
18, unused debug artifact, README claiming automatic prod deploys, pre-push hook without
`cargo fmt`) are in the improvement backlog.

Target pipeline (Task 5.4):
- `ci.yml` on PR and push to `main`: `permissions: contents: read`, concurrency with
  cancel-in-progress, `rust-cache`; jobs `lint` (fmt, clippy `-D warnings` with
  `unwrap_used`/`expect_used` denied in `[lints.clippy]`), `test` (Postgres service,
  `cargo sqlx prepare --check`, all targets + doc tests, Gemini mocked: no API key in CI),
  `deny` (cargo-deny-action), `image` (main only, after lint and test: buildx with gha cache,
  pushed as `sha-<commit>`).
- `deploy.yml` on successful CI (`workflow_run`): staging (`flyctl deploy --image <digest>`,
  smoke tests), then production behind the existing environment approval with the same digest,
  smoke tests, notification, rollback on failure.
- Secrets per environment: one Fly deploy token per app (`fly tokens create deploy`), one
  Telegram bot per environment, `GEMINI_API_KEY` as a Fly secret per app (separate keys).

## Review Log
| Gate | Date | Commits | Findings | Outcome |
|---|---|---|---|---|
| R0 | 2026-10-27 | `9c9c970..9ce3e40` (14 commits) | 3 should-fix: fork PRs could trigger the staging deploy via `workflow_run`; production gate bypassable by a manual staging run; retry test did not exercise the retry loop. 4 nits: missing locale key (fixed, IMP-045 added), handler-level dedup test (IMP-031), owner identity split between `chat.id` and `from.id` (IMP-024, do before Phase 3), adjacent `i64` owner/recipe parameters (IMP-038). Independent run: fmt, clippy, 441 tests + 43 doc tests, 26/26 DB tests. | Passed with fixes: the 3 should-fix items and the locale key fixed in the commits following `9ce3e40`. |

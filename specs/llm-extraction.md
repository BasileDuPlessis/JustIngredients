# Ingredient Extraction (Gemini) Specification

> Describes the code as it is (2026-10-27, after migration Tasks 5.1 to 5.5). It replaces the OCR specs:
> the Tesseract pipeline was removed in [LLM Extraction Migration](tasks/llm-extraction-migration.md)
> Task 5.1. How the photo flow uses it: [Telegram Bot](telegram-bot.md#photo-flow).

## Overview
A recipe photo is sent to the Gemini vision API, which returns the ingredient list as JSON
(names, quantities, units, a flag for doubtful lines and a transcription). The bot shows it for
review; nothing is saved before the user confirms. There is no OCR and no fallback engine:
rolling back to Tesseract means reverting the Phase 5 commits.

## Components (`src/extraction/`)
| Module | Role |
|---|---|
| `mod.rs` | `IngredientExtractor` trait, `ImageInput`, `Extraction`, `ExtractedIngredient`, `ExtractionError`, `SharedExtractor` |
| `config.rs` | `ExtractionConfig` / `GeminiConfig` from environment variables, validated at startup |
| `gemini/client.rs` | `GeminiExtractor`: `generateContent` call, retries, timeouts, breaker, metrics |
| `gemini/types.rs` | Request and response JSON types |
| `prompt.rs` | Prompt and response schema, embedded from `config/extraction/` |
| `image.rs` | `ImageInput::prepare`: format detection and conversion, `MAX_IMAGE_BYTES` |
| `guard.rs` | `GuardedExtractor`: concurrency slots, queue timeout, result cache, per-user daily limit |
| `breaker.rs` | Circuit breaker (closed, open, half-open) |
| `mapping.rs` | `Extraction` → `MeasurementMatch` (units, name length, quantity range) |
| `pricing.rs` | Estimated cost per call for the cost metric |

The bot only knows `SharedExtractor` (`Arc<dyn IngredientExtractor>`), built once in `main.rs`:
`GuardedExtractor` around `GeminiExtractor`. Tests use fake extractors.

## Configuration
| Variable | Default | Meaning |
|---|---|---|
| `GEMINI_API_KEY` | (none) | required: the bot does not start without it |
| `GEMINI_MODEL` | `gemini-3.5-flash-lite` | model id (letters, digits, `.`, `-`, `_`) |
| `GEMINI_TIMEOUT_SECS` | `40` | budget for one image, retries included, 1–300 |
| `GEMINI_THINKING_LEVEL` | `minimal` | `minimal`, `low`, `medium` or `high` |
| `GEMINI_MAX_CONCURRENCY` | `2` | concurrent calls, 1–32 |
| `EXTRACTION_DAILY_LIMIT_PER_USER` | `50` | photos read per user and UTC day, 1–10000 |

`EXTRACTION_BACKEND` is no longer read. At startup `verify_access` checks the key and model; a
failure is logged as an error and the bot still starts (photos then fail with
`error-extraction-unavailable`).

## Image Preparation (`image.rs`)
- At most 10 MB (`MAX_IMAGE_BYTES`), checked before and during the download.
- JPEG, PNG and WebP are sent as they are, detected from their signature.
- Other formats the `image` crate decodes (BMP, TIFF, GIF…) are converted to JPEG on the blocking
  pool, one conversion at a time, with an 8,000 px and 96 MB decoding limit.

## Request and Response
- Prompt `config/extraction/prompt.md` and JSON schema `config/extraction/schema.json`, embedded
  at compile time and shared with `scripts/llm_benchmark`.
- Response fields: `is_recipe`, `title` (the recipe title as printed, or null: the prompt forbids
  inventing one), `language` (`fr`, `en`, `other`), `transcription` (ingredient
  section, one line per ingredient), `ingredients[]` with `name`, `quantity` (number or null),
  `unit` (string or null), `needs_review`, `source_text`.
- `mapping::to_measurement_matches`: canonical units, names cut to 100 bytes (the validation limit), quantities
  outside (0, `MAX_QUANTITY`] or not storable in `DECIMAL(10,3)` and `needs_review` lines are
  flagged ⚠️ (`requires_quantity_confirmation`) for the review's quantity guard.

## Reliability
- One deadline per image (`GEMINI_TIMEOUT_SECS`), 20 s per attempt, 10 s to connect, responses
  over 1 MB refused.
- Retries with backoff (1 s doubling to 8 s, 20 % jitter) on network errors, timeouts, HTTP 429
  and 5xx; never on other 4xx. A `Retry-After` is a minimum delay; one over 8 s ends the
  retries at once (the photo gets the busy message).
- Circuit breaker: 5 images in a row ending in a transient failure (429, 5xx, timeout, network)
  open it for 60 s, then one trial call (half-open); any other outcome resets the count.
- `GuardedExtractor`: `GEMINI_MAX_CONCURRENCY` slots; a photo waiting more than 30 s for a slot
  gets `error-extraction-busy`. Successful results are cached by SHA-256 of the image (100
  entries, 24 h); "not a recipe" answers and answers without ingredients are not cached.
  Each user (the image's `requested_by`, set by the bot) gets `EXTRACTION_DAILY_LIMIT_PER_USER`
  paid calls per UTC day; the next photo gets `error-extraction-daily-limit` without a call.
  Cache hits are free. Counts are in memory: a restart resets them.

## Errors
`ExtractionError` messages never contain the key or a URL with credentials. User messages come
from `bot::image_processing::extraction_error_key`:

| Key | Errors |
|---|---|
| `error-extraction-unsupported-image` | `UnsupportedImage` |
| `error-extraction-image-too-large` | `ImageTooLarge` |
| `error-extraction-busy` | `RateLimited` (HTTP 429 or no free slot within 30 s) |
| `error-extraction-daily-limit` | `DailyLimitReached` |
| `error-extraction-unreadable` | `Blocked`, `Recitation`, `Truncated`, `InvalidResponse`, other 4xx about the image |
| `error-extraction-unavailable` | `Config`, `Timeout`, `Network`, 5xx, invalid key or model, `CircuitOpen` |

Configuration errors (invalid key, unknown model) are logged as errors, the others as warnings.

## Metrics
`llm_requests_total`, `llm_request_duration_seconds`, `llm_retries_total`, `llm_tokens_total`,
`llm_estimated_cost_micro_usd_total` (prices in `pricing.rs`, checked 2026-10-26) and
`extraction_cache_hits_total`; failures also count in `errors_total{type, component="gemini"}`.

## Testing
- Unit tests in each module; the Gemini client is tested against a `wiremock` server with the
  fixtures of `tests/fixtures/gemini/` (no API key needed in CI).
- `tests/gemini_live.rs`: ignored live test against the real API (needs `GEMINI_API_KEY`).
- `scripts/llm_benchmark`: model comparison on a set of recipe photos.

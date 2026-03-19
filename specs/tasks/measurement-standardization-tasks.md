# Measurement Unit Standardization Tasks

## Goal
Standardize unit variants so inputs like `g`, `gr`, and `GR` are stored and processed as one canonical unit.

## Scope
- In scope: unit alias mapping, canonicalization in parsing flow, tests, and optional DB backfill.
- Out of scope: quantity conversion between units (e.g., g to kg), UI redesign, multilingual ingredient translation.

## Task 1: Define Canonical Unit Contract (<= 1 hour)
- [ ] Done
- Description:
  - Define canonical units and alias rules (example: `g`, `gr`, `gram`, `gramme`, `grammes` -> `g`).
  - Decide normalization policy: lowercase, trim spaces, remove trailing punctuation.
  - Document unknown-unit behavior (keep as-is or map to `None`).
- How to validate:
  1. Open `config/measurement_units.json` and verify alias sets exist for key units (`g`, `kg`, `ml`, `l`, `tsp`, `tbsp`).
  2. Confirm a written canonical mapping list exists in the task PR/commit notes.
  3. Ensure no duplicate aliases conflict across canonical groups.

## Task 2: Add Canonicalization Helper in Parsing Layer (<= 1 hour)
- [ ] Done
- Description:
  - Implement a helper in `src/text_processing.rs` to normalize detected units to canonical values.
  - Apply it where `measurement` is currently lowercased.
  - Keep matching behavior unchanged; only normalize output unit value.
- How to validate:
  1. Add/adjust unit tests in `tests/text_processing_tests.rs`.
  2. Verify these examples normalize correctly:
     - `500 g sugar` -> `g`
     - `500 gr sugar` -> `g`
     - `500 GR sugar` -> `g`
  3. Run: `cargo test text_processing_tests -- --nocapture`

## Task 3: Normalize Units Before Persistence (<= 1 hour)
- [ ] Done
- Description:
  - Ensure canonical units are used in DB write paths (new recipe save and ingredient editing callbacks).
  - Confirm same normalization applies to both automated OCR flow and manual edits.
- How to validate:
  1. Save ingredients using mixed variants (`g`, `gr`, `GR`) through both flows.
  2. Query DB and confirm `ingredients.unit` stores only canonical value (`g`).
  3. Run: `cargo test integration_tests -- --nocapture`

## Task 4: Add Regression and Edge-Case Tests (<= 1 hour)
- [ ] Done
- Description:
  - Add tests for alias collisions, punctuation, and case-insensitivity.
  - Add tests for unknown units to validate chosen fallback policy.
- How to validate:
  1. New test cases cover: `g`, `gr`, `GR`, `g.`, `kg`, `Kg`, `KILOGRAMME`.
  2. Unknown unit behavior is explicitly asserted.
  3. Run: `cargo test --test text_processing_tests -- --nocapture`

## Task 5: Optional Existing Data Backfill (<= 1 hour)
- [ ] Done
- Description:
  - Add a one-off SQL migration or script to canonicalize existing `ingredients.unit` values.
  - Keep migration idempotent.
- How to validate:
  1. Dry-run select counts by unit before/after backfill.
  2. Confirm aliases (`gr`, `gram`, `GRAM`) are rewritten to canonical (`g`).
  3. Re-run backfill safely with no further changes.

## Task 6: Full Quality Gate (<= 1 hour)
- [ ] Done
- Description:
  - Run full repository quality checks after implementation.
  - Fix lint/test/format issues introduced by the change.
- How to validate:
  1. `cargo test`
  2. `cargo test --doc`
  3. `cargo clippy --all-targets --all-features -- -D warnings`
  4. `cargo fmt --all -- --check`

## Completion Criteria
- All tasks checked `[x]`.
- Unit alias inputs map to one canonical value in parse output and DB.
- Tests and quality gates pass.

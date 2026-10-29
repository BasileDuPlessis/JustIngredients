# Measurement Unit Standardization Tasks

> 2026-10-29: `config/measurement_units.json` and `tests/text_processing_tests.rs`, named in the
> tasks below, were removed with the OCR code (IMP-048). The spellings live in `src/units.rs`
> (with count units such as "gousses"), its unit tests and `validation.rs` (typed lines) replace
> them.

## Goal
Standardize unit variants so inputs like `g`, `gr`, and `GR` are stored and processed as one canonical unit.

## Scope
- In scope: unit alias mapping, canonicalization in parsing flow, tests, and optional DB backfill.
- Out of scope: quantity conversion between units (e.g., g to kg), UI redesign, multilingual ingredient translation.

## Task 1: Define Canonical Unit Contract (<= 1 hour)
- [x] Done
- Description:
  - Define canonical units and alias rules (example: `g`, `gr`, `gram`, `gramme`, `grammes` -> `g`).
  - Decide normalization policy: lowercase, trim spaces, remove trailing punctuation.
  - Document unknown-unit behavior (keep as-is or map to `None`).
- Canonical Unit Mappings:
  - Weight Units:
    - `g` (aliases: `gr`, `gram`, `grams`, `gramme`, `grammes`)
    - `kg` (aliases: `kilogram`, `kilograms`, `kilogramme`, `kilogrammes`)
    - `mg`
    - `lb` (aliases: `pound`, `pounds`)
    - `oz` (aliases: `ounce`, `ounces`)
  - Volume Units:
    - `cup` (aliases: `cups`)
    - `tsp` (aliases: `teaspoon`, `teaspoons`)
    - `tbsp` (aliases: `tablespoon`, `tablespoons`)
    - `pint` (aliases: `pints`)
    - `quart` (aliases: `quarts`)
    - `gallon` (aliases: `gallons`)
    - `l` (aliases: `liter`, `liters`, `litre`, `litres`)
    - `ml` (aliases: `milliliter`, `milliliters`, `millilitre`, `millilitres`, `cm3`)
  - Scaled units (converted, quantity multiplied): `cl` -> `ml` x10, `dl` -> `ml` x100.
  - French spoons: `cuillère à soupe`, `c. à s.`, `cs` -> `tbsp`; `cuillère à café`, `c. à c.`, `cc` -> `tsp`
    (`cc` is a teaspoon in French recipes, not a cubic centimetre).
  - Revised 2026-10-27: `cl`/`dl` were listed as aliases of `ml`, which stored 20 cl as 20 ml.
  - Normalization Policy: lowercase, trim spaces, remove trailing punctuation (e.g., `g.` -> `g`);
    matching also ignores inner dots, spaces and French accents (`c.à.s` = `c. à s.`).
  - Unknown Unit Behavior: keep as-is (preserve user input).
- How to validate:
  1. Open `config/measurement_units.json` and verify alias sets exist for key units (`g`, `kg`, `ml`, `l`, `tsp`, `tbsp`).
  2. Confirm a written canonical mapping list exists in the task PR/commit notes.
  3. Ensure no duplicate aliases conflict across canonical groups.

## Task 2: Add Canonicalization Helper in Parsing Layer (<= 1 hour)
- [x] Done
- Done as Task 1.1 of `llm-extraction-migration.md`: the helper lives in `src/units.rs`
  (`canonicalize_unit`, `normalize_measurement`) so that both the LLM output and manual edits use it.
  Wiring it into the parsing and save paths is Task 3 below and Task 1.3 of the migration plan.
- Description (original):
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
- [x] Done (2026-10-29): photos through `extraction::mapping`, typed lines through
  `validation::parse_ingredient_from_text` and quantity answers through
  `validation::parse_quantity_with_unit` all call `normalize_measurement`. Units saved before
  stay as they are until edited (Task 5).
- Description:
  - Ensure canonical units are used in DB write paths (new recipe save and ingredient editing callbacks).
  - Confirm same normalization applies to both automated OCR flow and manual edits.
- How to validate:
  1. Save ingredients using mixed variants (`g`, `gr`, `GR`) through both flows.
  2. Query DB and confirm `ingredients.unit` stores only canonical value (`g`).
  3. Run: `cargo test integration_tests -- --nocapture`

## Task 4: Add Regression and Edge-Case Tests (<= 1 hour)
- [x] Done (2026-10-29): `units.rs` tests (spellings including `KILOGRAMME`, no spelling for two
  units, unknown units kept, count units) and the typed-line tables of `validation.rs`.
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

# Fix Expect Calls Task

## Overview
Replace all `.expect()` calls with proper error handling and propagation, eliminating unwrap-like behavior.

## Success Criteria
- All `.expect()` calls replaced with proper error handling
- No unwraps or expects in production code
- Code compiles and runs without panics
- All tests pass
- Linting passes

## Micro-Tasks (1-hour each)

### Task 1: Fix circuit_breaker.rs expects [DONE]
- Replace multiple `.expect()` calls for mutex access (line ~153)
- Convert functions to return `Result`
- Propagate errors through the circuit breaker logic
- Run tests

### Task 2: Fix cache.rs expects [DONE]
- Replace 7+ `.expect()` calls on RwLock operations (lines ~108-126, ~318, ~325, ~334, ~407, ~414, ~431-456)
- Convert helper methods to return `Result` with `CacheError` enum
- Handle lock failures gracefully in cache operations
- Run tests

### Task 3: Fix instance_manager.rs expect [DONE]
- Replace 5 `.expect("Failed to acquire instances lock")` calls (lines ~111, ~175, ~217, ~225, ~253)
- Convert instance management methods to return `Result<(), InstanceManagerError>`
- Update all callers and test functions
- Run tests

## Validation
After each task:
- `cargo build`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
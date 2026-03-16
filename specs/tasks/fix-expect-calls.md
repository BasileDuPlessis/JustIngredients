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

### Task 1: Fix circuit_breaker.rs expects
- Replace multiple `.expect()` calls for mutex access (line ~153)
- Convert functions to return `Result`
- Propagate errors through the circuit breaker logic
- Run tests

### Task 2: Fix cache.rs expects
- Replace `.expect("Failed to acquire read lock")` (line ~126)
- Replace `.expect("Failed to acquire write lock")` (line ~133)
- Convert cache operations to return `Result`
- Update all callers
- Run tests

### Task 3: Fix instance_manager.rs expect
- Replace `.expect("Failed to acquire instances lock")` (line ~111)
- Convert instance management to return `Result`
- Update OCR instance pooling logic
- Run tests

## Validation
After each task:
- `cargo build`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt --all -- --check`
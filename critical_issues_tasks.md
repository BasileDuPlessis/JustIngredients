# Critical Issues Resolution Tasks

## Overview
This document outlines the critical issues identified in the JustIngredients codebase that must be resolved before production deployment. These issues pose significant risks to correctness, safety, and reliability.

## Issues and Resolution Tasks

### ~~1. MemoryCache Deadlock Risk~~ (Downgraded — Not a real issue)
**Location**: `src/cache.rs:156-170`  
**Severity**: ~~Critical~~ → **Not an issue**  
**Risk**: None — no deadlock is possible

**Analysis**: After closer inspection, `data` and `stats` are **two independent `RwLock`s** protecting different resources. A deadlock requires a circular lock dependency, but `insert()` and `cleanup()` never acquire `stats`, so no thread can hold `data` while waiting on `stats`. The worst case is temporary blocking (Thread A in `get()` waits for Thread B in `insert()` to release `data`), which is normal and resolves immediately.

**No action required.**

---

### 2. Circuit Breaker TOCTOU Race Condition
**Location**: `src/circuit_breaker.rs:149-175`  
**Severity**: Critical  
**Risk**: Inconsistent circuit breaker state leading to unexpected failures

**Problem**: The `is_open()` method reads failure state, then resets it in separate lock acquisitions. Multiple threads can both detect "should reset" and attempt reset simultaneously.

**Tasks**:
- [ ] Refactor `is_open()` to perform state check and reset atomically
- [ ] Use single lock scope for the entire operation
- [ ] Add comprehensive tests for concurrent circuit breaker access
- [ ] Test edge cases: multiple threads hitting failure threshold simultaneously
- [ ] Verify reset logic works correctly under load

**Estimated Effort**: 3-4 hours
**Priority**: P0 - Must fix before deployment

---

### 3. Panic Violations Throughout Codebase
**Locations**: Multiple files (20+ instances)  
**Severity**: Critical  
**Risk**: Application crashes instead of graceful error handling

**Problem**: Extensive use of `panic!()` in production code violates the strict no-panic policy. This can cause the entire bot to crash on recoverable errors.

**Tasks**:
- [ ] Audit all `panic!()` calls in `src/` directory (exclude tests)
- [ ] Replace each `panic!()` with proper `Result` return type
- [ ] Update calling code to handle the new error returns
- [ ] Ensure error propagation reaches appropriate error handlers
- [ ] Add integration tests to verify graceful error handling

**Specific Files to Fix**:
- [ ] `src/bot/ui_components.rs` (3 panics)
- [ ] `src/preprocessing/cropping.rs` (6 panics)
- [ ] `src/preprocessing/targeted.rs` (3 panics)
- [ ] `src/preprocessing/filtering.rs` (2 panics)
- [ ] `src/preprocessing/deskewing.rs` (1 panic)
- [ ] `src/validation.rs` (2 panics)
- [ ] `src/deduplication.rs` (4 panics)

**Estimated Effort**: 8-12 hours
**Priority**: P0 - Must fix before deployment

---

### 4. TempFileGuard Resource Leak Risk
**Location**: `src/bot/image_processing.rs:130-134`  
**Severity**: Critical  
**Risk**: Temporary files not cleaned up if guard creation fails

**Problem**: `std::mem::forget(temp_file)` is called before `TempFileGuard::new()`, creating a window where the temp file could leak if the constructor panics.

**Tasks**:
- [ ] Analyze `TempFileGuard::new()` to confirm it cannot panic
- [ ] Implement safer RAII pattern to prevent leaks
- [ ] Consider using a different temporary file management approach
- [ ] Add tests to verify cleanup happens even on errors
- [ ] Test edge cases like disk full, permission errors

**Estimated Effort**: 2-3 hours
**Priority**: P0 - Must fix before deployment

---

## Verification Steps

After implementing fixes:

1. **Concurrency Testing**:
   - Run cache with high concurrency (100+ threads)
   - Test circuit breaker under failure storms
   - Verify no deadlocks or race conditions

2. **Error Handling Testing**:
   - Simulate all error conditions that previously caused panics
   - Verify graceful degradation and proper error messages
   - Test resource cleanup on all error paths

3. **Resource Leak Testing**:
   - Monitor temp file creation/deletion
   - Test with limited disk space
   - Verify cleanup on process interruption

4. **Integration Testing**:
   - Full pipeline tests with concurrent users
   - Load testing with realistic Telegram traffic patterns
   - Memory usage monitoring over extended periods

## Success Criteria

- [ ] All critical issues resolved
- [ ] No `panic!()` calls in production code
- [ ] Concurrent access works without deadlocks
- [ ] Resources properly cleaned up in all scenarios
- [ ] Comprehensive tests pass for all fixes
- [ ] Code review approval for all changes

## Timeline

- **Week 1**: Fix concurrency issues (cache deadlock, circuit breaker race)
- **Week 1**: Fix resource leak and panic violations
- **Week 2**: Comprehensive testing and verification
- **Week 2**: Code review and deployment preparation

## Risk Assessment

**Without fixes**: High risk of production outages, data loss, and poor user experience.

**With fixes**: Reliable, safe operation under concurrent load with proper error handling.

## Dependencies

- Requires understanding of Rust concurrency patterns
- May need to update error handling throughout call stacks
- Testing requires concurrent execution capabilities
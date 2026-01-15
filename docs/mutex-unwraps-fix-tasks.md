# Mutex Unwrap Fixes - High Priority Tasks

## Overview
Replace all `.unwrap()` calls on Mutex/RwLock operations with proper error handling to prevent panics from poisoned locks. Poisoned locks occur when a thread panics while holding the lock, and subsequent `.unwrap()` calls will panic, potentially crashing the application.

## Strategy
- Replace `.unwrap()` with `.expect("descriptive error message")` for immediate visibility
- Consider adding mutex poisoning recovery in critical paths where possible
- Add helper methods to reduce code duplication in heavily affected files

## Affected Files and Tasks

### src/cache.rs (20+ unwraps)
- [x] **Add helper methods** in `MemoryCache` impl block:
  ```rust
  fn read_data(&self) -> RwLockReadGuard<'_, HashMap<K, CacheEntry<V>>> {
      self.data.read().expect("Failed to acquire read lock on cache data")
  }

  fn write_data(&self) -> RwLockWriteGuard<'_, HashMap<K, CacheEntry<V>>> {
      self.data.write().expect("Failed to acquire write lock on cache data")
  }

  fn read_stats(&self) -> RwLockReadGuard<'_, CacheStats> {
      self.stats.read().expect("Failed to acquire read lock on cache stats")
  }

  fn write_stats(&self) -> RwLockWriteGuard<'_, CacheStats> {
      self.stats.write().expect("Failed to acquire write lock on cache stats")
  }
  ```

- [x] Line 114: Replace `self.data.read().unwrap().len()` with `self.read_data().len()`
- [x] Line 119: Replace `self.data.read().unwrap().is_empty()` with `self.read_data().is_empty()`
- [x] Line 129: Replace `let mut stats = self.stats.write().unwrap();` with `let mut stats = self.write_stats();`
- [x] Line 130: Replace `let data = self.data.read().unwrap();` with `let data = self.read_data();`
- [x] Line 151: Replace `self.data.write().unwrap().insert(key, entry);` with `self.write_data().insert(key, entry);`
- [x] Line 157: Replace `.unwrap()` with `.expect("Failed to acquire write lock for cache cleanup")`
- [x] Line 163: Replace `let mut data = self.data.write().unwrap();` with `let mut data = self.write_data();`
- [x] Line 175: Replace `let mut stats = self.stats.read().unwrap().clone();` with `let mut stats = self.read_stats().clone();`
- [x] Line 176: Replace `let data = self.data.read().unwrap();` with `let data = self.read_data();`
- [x] Line 189: Replace `self.data.write().unwrap().clear();` with `self.write_data().clear();`
- [x] Line 190: Replace `*self.stats.write().unwrap() = CacheStats::default();` with `*self.write_stats() = CacheStats::default();`
- [x] Line 337: Replace `let current_size = *self.current_size_bytes.read().unwrap();` with `let current_size = *self.current_size_bytes.read().expect("Failed to read current size");`
- [x] Line 344: Replace `*self.current_size_bytes.write().unwrap() += value_size;` with `*self.current_size_bytes.write().expect("Failed to write current size") += value_size;`
- [x] Line 351: Replace `*self.current_size_bytes.write().unwrap() -= value.size_bytes;` with `*self.current_size_bytes.write().expect("Failed to write current size") -= value.size_bytes;`
- [x] Line 371: Replace `*self.current_size_bytes.write().unwrap() = 0;` with `*self.current_size_bytes.write().expect("Failed to write current size") = 0;`
- [x] Line 376: Replace `*self.current_size_bytes.read().unwrap()` with `*self.current_size_bytes.read().expect("Failed to read current size")`
- [x] Line 388: Replace `let mut data = self.cache.data.write().unwrap();` with `let mut data = self.cache.write_data();`
- [x] Line 395: Replace `*self.current_size_bytes.write().unwrap() -= entry.value.size_bytes;` with `*self.current_size_bytes.write().expect("Failed to write current size") -= entry.value.size_bytes;`
- [x] Line 400: Replace `let current_size = *self.current_size_bytes.read().unwrap();` with `let current_size = *self.current_size_bytes.read().expect("Failed to read current size");`
- [x] Line 415: Replace `*self.current_size_bytes.write().unwrap() -= entry.value.size_bytes;` with `*self.current_size_bytes.write().expect("Failed to write current size") -= entry.value.size_bytes;`
- [x] Line 475: Replace `let data = self.user_cache.data.read().unwrap();` with `let data = self.user_cache.read_data();`

### src/circuit_breaker.rs (8 unwraps)
- [x] Line 150: Replace `let failure_count = *self.failure_count.lock().unwrap();` with `let failure_count = *self.failure_count.lock().expect("Failed to acquire failure count lock");`
- [x] Line 151: Replace `let last_failure = *self.last_failure_time.lock().unwrap();` with `let last_failure = *self.last_failure_time.lock().expect("Failed to acquire last failure time lock");`
- [x] Line 160: Replace `*self.failure_count.lock().unwrap() = 0;` with `*self.failure_count.lock().expect("Failed to acquire failure count lock") = 0;`
- [x] Line 161: Replace `*self.last_failure_time.lock().unwrap() = None;` with `*self.last_failure_time.lock().expect("Failed to acquire last failure time lock") = None;`
- [x] Line 176: Replace `*self.failure_count.lock().unwrap() += 1;` with `*self.failure_count.lock().expect("Failed to acquire failure count lock") += 1;`
- [x] Line 177: Replace `*self.last_failure_time.lock().unwrap() = Some(Instant::now());` with `*self.last_failure_time.lock().expect("Failed to acquire last failure time lock") = Some(Instant::now());`
- [x] Line 189: Replace `*self.failure_count.lock().unwrap() = 0;` with `*self.failure_count.lock().expect("Failed to acquire failure count lock") = 0;`
- [x] Line 190: Replace `*self.last_failure_time.lock().unwrap() = None;` with `*self.last_failure_time.lock().expect("Failed to acquire last failure time lock") = None;`

### src/instance_manager.rs (5 unwraps)
- [x] Line 107: Replace `let instances = self.instances.lock().unwrap();` with `let instances = self.instances.lock().expect("Failed to acquire instances lock");`
- [x] Line 122: Replace `let mut instances = self.instances.lock().unwrap();` with `let mut instances = self.instances.lock().expect("Failed to acquire instances lock");`
- [x] Line 131: Replace `let mut instances = self.instances.lock().unwrap();` with `let mut instances = self.instances.lock().expect("Failed to acquire instances lock");`
- [x] Line 139: Replace `let mut instances = self.instances.lock().unwrap();` with `let mut instances = self.instances.lock().expect("Failed to acquire instances lock");`
- [x] Line 149: Replace `let instances = self.instances.lock().unwrap();` with `let instances = self.instances.lock().expect("Failed to acquire instances lock");`

### src/db.rs (6 unwraps)
- [x] Line 316: Replace `let cache_manager = cache.lock().unwrap();` with `let cache_manager = cache.lock().expect("Failed to acquire cache manager lock");`
- [x] Line 328: Replace `let mut cache_manager = cache.lock().unwrap();` with `let mut cache_manager = cache.lock().expect("Failed to acquire cache manager lock");`
- [x] Line 347: Replace `let cache_manager = cache.lock().unwrap();` with `let cache_manager = cache.lock().expect("Failed to acquire cache manager lock");`
- [x] Line 359: Replace `let mut cache_manager = cache.lock().unwrap();` with `let mut cache_manager = cache.lock().expect("Failed to acquire cache manager lock");`
- [x] Line 378: Replace `let cache_manager = cache.lock().unwrap();` with `let cache_manager = cache.lock().expect("Failed to acquire cache manager lock");`
- [x] Line 390: Replace `let mut cache_manager = cache.lock().unwrap();` with `let mut cache_manager = cache.lock().expect("Failed to acquire cache manager lock");`

### src/ocr.rs (1 unwrap)
- [x] Line 827: Replace `let mut tess = instance.lock().unwrap();` with `let mut tess = instance.lock().expect("Failed to acquire Tesseract instance lock");`

### tests/performance_tests.rs (1 unwrap)
- [x] Line 988: Replace `let today_start = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();` with `let today_start = now.date_naive().and_hms_opt(0, 0, 0).expect("Failed to create start of day datetime").and_utc();`

## Validation Tasks
- [x] Run `cargo check` to ensure all changes compile
- [x] Run `cargo test` to ensure all 93 tests still pass
- [x] Run `cargo clippy --all-targets --all-features -- -D warnings` to ensure no new warnings
- [x] Run `cargo fmt --all -- --check` to ensure formatting is maintained
- [ ] Consider adding tests for mutex poisoning recovery scenarios

## Additional Considerations
- **Mutex Poisoning Recovery**: For critical application components, consider implementing poisoning recovery:
  ```rust
  let data = match self.data.read() {
      Ok(guard) => guard,
      Err(poisoned) => {
          tracing::warn!("Mutex was poisoned, recovering from panic");
          poisoned.into_inner()
      }
  };
  ```
- **Performance Impact**: `.expect()` has minimal performance impact compared to `.unwrap()`
- **Logging**: The descriptive messages in `.expect()` will help with debugging if issues occur

## Priority
**HIGH** - This prevents potential application crashes from poisoned mutexes in production.

## Estimated Effort
- Individual fixes: 1-2 hours
- Adding helper methods: 30 minutes
- Testing and validation: 30 minutes
- **Total: 2-3 hours**

## Files to Modify
- `src/cache.rs`
- `src/circuit_breaker.rs`
- `src/instance_manager.rs`
- `src/db.rs`
- `src/ocr.rs`
- `tests/performance_tests.rs`
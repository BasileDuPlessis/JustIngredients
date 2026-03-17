# Optimize Performance Task

## Overview
Reduce excessive cloning in hot paths, particularly in text processing and caching logic, to improve performance.

## Success Criteria
- Eliminate unnecessary `.clone()` calls in performance-critical paths
- Maintain correctness and thread safety
- Performance tests show improvement or no regression
- All tests pass

## Micro-Tasks (1-hour each)

### Task 1: Optimize text_processing.rs cloning [DONE]
- Review `DEFAULT_REGEX.clone()` usage (line ~446)
- Review `chars.clone()` in processing loops (line ~794)
- Replace with references or Arc where appropriate
- Ensure thread safety maintained
- Run performance tests

### Task 2: Review caching logic for cloning
- **Task 2.1**: Optimize cache key cloning (OcrCacheKey, DbCacheKey)
- **Task 2.2**: Optimize cache value cloning and storage
- **Task 2.3**: Optimize cache statistics collection
- **Task 2.4**: Review CacheManager operations for cloning efficiency
- Run benchmarks after each subtask

#### Task 2.1: Optimize cache key cloning [DONE]
- Review OcrCacheKey and DbCacheKey usage patterns
- Consider using references instead of owned strings where possible
- Optimize key generation to avoid unnecessary allocations
- Update callers to use references when safe

#### Task 2.2: Optimize cache value cloning and storage
- Analyze cache value return patterns (lines ~185, ~559)
- Consider using Arc/Rc for large cache values
- Optimize eviction logic cloning (lines ~468, ~492)
- Evaluate if cache values can be returned as references

#### Task 2.3: Optimize cache statistics collection
- Review stats cloning in get() method (line ~225)
- Optimize stats calculation and return patterns
- Consider lazy stats computation where appropriate

#### Task 2.4: Review CacheManager operations for cloning efficiency
- Analyze CacheManager usage in db.rs and other modules
- Optimize user/recipe data cloning in cache operations
- Review cache manager initialization and configuration

### Task 3: Profile and measure improvements
- Add performance benchmarks if missing
- Measure before/after performance
- Ensure no regressions in functionality
- Document optimizations

## Validation
After each task:
- `cargo build`
- `cargo test`
- `cargo bench` (if benchmarks exist)
- Performance tests pass
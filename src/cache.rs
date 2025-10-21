//! Caching infrastructure for performance optimization
//!
//! This module provides various caching strategies to improve application performance
//! by reducing expensive operations like OCR processing and database queries.
//!
//! ## Cache Types
//!
//! - **Memory Cache**: In-memory TTL-based cache for fast access
//! - **OCR Result Cache**: Specialized cache for OCR processing results
//! - **Database Query Cache**: Cache for frequently accessed database queries
//!
//! ## Usage Examples
//!
//! ```rust,no_run
//! use just_ingredients::cache::{Cache, MemoryCache, OcrResultCache};
//!
//! // Create a memory cache for string keys and values
//! let mut cache: MemoryCache<String, String> = MemoryCache::new();
//! cache.insert("key".to_string(), "value".to_string(), std::time::Duration::from_secs(300));
//!
//! // Cache OCR results
//! let ocr_cache = OcrResultCache::new(std::time::Duration::from_secs(3600)); // 1 hour
//! ```

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Generic cache entry with expiration time
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    /// The cached value
    pub value: T,
    /// When this entry expires
    pub expires_at: Instant,
}

impl<T> CacheEntry<T> {
    /// Create a new cache entry
    pub fn new(value: T, ttl: Duration) -> Self {
        Self {
            value,
            expires_at: Instant::now() + ttl,
        }
    }

    /// Check if this entry has expired
    pub fn is_expired(&self) -> bool {
        Instant::now() > self.expires_at
    }

    /// Get the remaining time to live
    pub fn ttl_remaining(&self) -> Duration {
        self.expires_at.saturating_duration_since(Instant::now())
    }
}

/// Generic cache trait
pub trait Cache<K, V> {
    /// Get a value from the cache
    fn get(&self, key: &K) -> Option<V>;

    /// Insert a value into the cache
    fn insert(&mut self, key: K, value: V, ttl: Duration);

    /// Remove a value from the cache
    fn remove(&mut self, key: &K) -> Option<V>;

    /// Clear all expired entries
    fn cleanup(&mut self);

    /// Get cache statistics
    fn stats(&self) -> CacheStats;

    /// Clear all entries
    fn clear(&mut self);
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total number of entries
    pub entries: usize,
    /// Number of hits
    pub hits: u64,
    /// Number of misses
    pub misses: u64,
    /// Hit rate (hits / (hits + misses))
    pub hit_rate: f64,
}

/// Thread-safe in-memory cache implementation
pub struct MemoryCache<K, V> {
    data: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    stats: Arc<RwLock<CacheStats>>,
}

impl<K, V> MemoryCache<K, V>
where
    K: Clone + Eq + Hash + std::fmt::Debug,
    V: Clone + std::fmt::Debug,
{
    /// Create a new memory cache
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(CacheStats::default())),
        }
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.data.read().unwrap().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.data.read().unwrap().is_empty()
    }
}

impl<K, V> Cache<K, V> for MemoryCache<K, V>
where
    K: Clone + Eq + Hash + std::fmt::Debug,
    V: Clone + std::fmt::Debug,
{
    fn get(&self, key: &K) -> Option<V> {
        let mut stats = self.stats.write().unwrap();
        let data = self.data.read().unwrap();

        match data.get(key) {
            Some(entry) if !entry.is_expired() => {
                stats.hits += 1;
                Some(entry.value.clone())
            }
            Some(_) => {
                // Entry exists but is expired
                stats.misses += 1;
                None
            }
            None => {
                stats.misses += 1;
                None
            }
        }
    }

    fn insert(&mut self, key: K, value: V, ttl: Duration) {
        let entry = CacheEntry::new(value, ttl);
        self.data.write().unwrap().insert(key, entry);
    }

    fn remove(&mut self, key: &K) -> Option<V> {
        self.data
            .write()
            .unwrap()
            .remove(key)
            .map(|entry| entry.value)
    }

    fn cleanup(&mut self) {
        let mut data = self.data.write().unwrap();
        let initial_len = data.len();

        data.retain(|_, entry| !entry.is_expired());

        let removed = initial_len - data.len();
        if removed > 0 {
            tracing::debug!("Cache cleanup removed {} expired entries", removed);
        }
    }

    fn stats(&self) -> CacheStats {
        let mut stats = self.stats.read().unwrap().clone();
        let data = self.data.read().unwrap();

        stats.entries = data.len();

        let total_requests = stats.hits + stats.misses;
        if total_requests > 0 {
            stats.hit_rate = stats.hits as f64 / total_requests as f64;
        }

        stats
    }

    fn clear(&mut self) {
        self.data.write().unwrap().clear();
        *self.stats.write().unwrap() = CacheStats::default();
    }
}

impl<K, V> Default for MemoryCache<K, V>
where
    K: Clone + Eq + Hash + std::fmt::Debug,
    V: Clone + std::fmt::Debug,
{
    fn default() -> Self {
        Self::new()
    }
}

/// OCR result cache key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OcrCacheKey {
    /// Hash of the image content
    pub image_hash: String,
    /// OCR configuration hash (to invalidate when config changes)
    pub config_hash: String,
}

impl OcrCacheKey {
    /// Create a new OCR cache key
    pub fn new(image_hash: String, config_hash: String) -> Self {
        Self {
            image_hash,
            config_hash,
        }
    }
}

/// OCR result cache value
#[derive(Debug, Clone)]
pub struct OcrCacheValue {
    /// Extracted text
    pub text: String,
    /// Processing time
    pub processing_time_ms: u64,
    /// Cached at timestamp
    pub cached_at: Instant,
}

/// Specialized cache for OCR results
pub struct OcrResultCache {
    cache: MemoryCache<OcrCacheKey, OcrCacheValue>,
}

impl OcrResultCache {
    /// Create a new OCR result cache
    pub fn new(_default_ttl: Duration) -> Self {
        Self {
            cache: MemoryCache::new(),
        }
    }

    /// Get cached OCR result
    pub fn get(&self, key: &OcrCacheKey) -> Option<OcrCacheValue> {
        self.cache.get(key)
    }

    /// Cache OCR result
    pub fn insert(&mut self, key: OcrCacheKey, value: OcrCacheValue, ttl: Duration) {
        self.cache.insert(key, value, ttl);
    }

    /// Remove OCR result from cache
    pub fn remove(&mut self, key: &OcrCacheKey) -> Option<OcrCacheValue> {
        self.cache.remove(key)
    }

    /// Clean up expired entries
    pub fn cleanup(&mut self) {
        self.cache.cleanup();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear all cached results
    pub fn clear(&mut self) {
        self.cache.clear();
    }
}

/// Database query cache key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DbCacheKey {
    /// Query type identifier
    pub query_type: String,
    /// Query parameters hash
    pub params_hash: String,
}

impl DbCacheKey {
    /// Create a new database cache key
    pub fn new(query_type: impl Into<String>, params_hash: impl Into<String>) -> Self {
        Self {
            query_type: query_type.into(),
            params_hash: params_hash.into(),
        }
    }
}

/// Database query cache value
#[derive(Debug, Clone)]
pub struct DbCacheValue {
    /// Query result data
    pub data: Vec<u8>,
    /// Result size in bytes
    pub size_bytes: usize,
    /// Cached at timestamp
    pub cached_at: Instant,
}

/// Specialized cache for database queries
pub struct DbQueryCache {
    cache: MemoryCache<DbCacheKey, DbCacheValue>,
    /// Maximum total cache size in bytes
    max_size_bytes: usize,
    /// Current cache size in bytes
    current_size_bytes: Arc<RwLock<usize>>,
}

impl DbQueryCache {
    /// Create a new database query cache
    pub fn new(_default_ttl: Duration, max_size_bytes: usize) -> Self {
        Self {
            cache: MemoryCache::new(),
            max_size_bytes,
            current_size_bytes: Arc::new(RwLock::new(0)),
        }
    }

    /// Get cached query result
    pub fn get(&self, key: &DbCacheKey) -> Option<DbCacheValue> {
        self.cache.get(key)
    }

    /// Cache query result with size management
    pub fn insert(&mut self, key: DbCacheKey, value: DbCacheValue, ttl: Duration) {
        let value_size = value.size_bytes;

        // Check if adding this entry would exceed max size
        let current_size = *self.current_size_bytes.read().unwrap();
        if current_size + value_size > self.max_size_bytes {
            // Evict some entries to make room (simple LRU-like eviction)
            self.evict_to_make_room(value_size);
        }

        self.cache.insert(key, value, ttl);
        *self.current_size_bytes.write().unwrap() += value_size;
    }

    /// Remove query result from cache
    pub fn remove(&mut self, key: &DbCacheKey) -> Option<DbCacheValue> {
        let result = self.cache.remove(key);
        if let Some(ref value) = result {
            *self.current_size_bytes.write().unwrap() -= value.size_bytes;
        }
        result
    }

    /// Clean up expired entries
    pub fn cleanup(&mut self) {
        // Note: cleanup doesn't reduce current_size_bytes as we don't track individual entry sizes
        // This is a limitation of the current implementation
        self.cache.cleanup();
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// Clear all cached results
    pub fn clear(&mut self) {
        self.cache.clear();
        *self.current_size_bytes.write().unwrap() = 0;
    }

    /// Get current cache size in bytes
    pub fn current_size_bytes(&self) -> usize {
        *self.current_size_bytes.read().unwrap()
    }

    /// Get maximum cache size in bytes
    pub fn max_size_bytes(&self) -> usize {
        self.max_size_bytes
    }

    /// Evict entries to make room for new data
    fn evict_to_make_room(&mut self, needed_bytes: usize) {
        // Simple eviction strategy: remove oldest entries until we have enough space
        // In a real implementation, this could be more sophisticated (LRU, LFU, etc.)
        let mut data = self.cache.data.write().unwrap();
        let mut entries_to_remove = Vec::new();

        // Find entries to evict (starting with expired ones, then oldest)
        for (key, entry) in data.iter() {
            if entry.is_expired() {
                entries_to_remove.push(key.clone());
                *self.current_size_bytes.write().unwrap() -= entry.value.size_bytes;
            }
        }

        // If we still need more space, remove additional entries
        let current_size = *self.current_size_bytes.read().unwrap();
        let space_needed = (current_size + needed_bytes).saturating_sub(self.max_size_bytes);

        if space_needed > 0 {
            // Sort by expiration time (oldest first) and remove until we have enough space
            let mut entries: Vec<_> = data.iter().collect();
            entries.sort_by_key(|(_, entry)| entry.expires_at);

            let mut freed_space = 0;
            for (key, entry) in entries {
                if freed_space >= space_needed {
                    break;
                }
                entries_to_remove.push(key.clone());
                freed_space += entry.value.size_bytes;
                *self.current_size_bytes.write().unwrap() -= entry.value.size_bytes;
            }
        }

        let evicted_count = entries_to_remove.len();
        for key in entries_to_remove {
            data.remove(&key);
        }

        tracing::debug!(
            "Evicted {} entries to make room for {} bytes",
            evicted_count,
            needed_bytes
        );
    }
}

/// Global cache manager for coordinating multiple caches
pub struct CacheManager {
    /// OCR result cache
    pub ocr_cache: OcrResultCache,
    /// Database query cache
    pub db_cache: DbQueryCache,
    /// User data cache
    pub user_cache: MemoryCache<i64, crate::db::User>,
    /// Recipe data cache
    pub recipe_cache: MemoryCache<i64, crate::db::Recipe>,
}

impl CacheManager {
    /// Create a new cache manager with default settings
    pub fn new() -> Self {
        Self {
            ocr_cache: OcrResultCache::new(Duration::from_secs(3600)), // 1 hour
            db_cache: DbQueryCache::new(Duration::from_secs(300), 50 * 1024 * 1024), // 5 min, 50MB
            user_cache: MemoryCache::new(),
            recipe_cache: MemoryCache::new(),
        }
    }

    /// Create a cache manager with custom settings
    pub fn with_config(
        ocr_ttl: Duration,
        db_ttl: Duration,
        db_max_size_bytes: usize,
        _user_ttl: Duration,
        _recipe_ttl: Duration,
    ) -> Self {
        Self {
            ocr_cache: OcrResultCache::new(ocr_ttl),
            db_cache: DbQueryCache::new(db_ttl, db_max_size_bytes),
            user_cache: MemoryCache::new(),
            recipe_cache: MemoryCache::new(),
        }
    }

    /// Find a user by internal ID across the user cache
    pub fn find_user_by_id(&self, user_id: i64) -> Option<crate::db::User> {
        // This is not the most efficient approach, but works for small caches
        // In production, you might want a separate cache or index
        let data = self.user_cache.data.read().unwrap();
        for (_, entry) in data.iter() {
            if !entry.is_expired() && entry.value.id == user_id {
                return Some(entry.value.clone());
            }
        }
        None
    }

    /// Clean up all expired entries across all caches
    pub fn cleanup_all(&mut self) {
        self.ocr_cache.cleanup();
        self.db_cache.cleanup();
        // Note: user_cache and recipe_cache cleanup would need to be implemented
        // if they had their own cleanup methods
    }

    /// Get comprehensive cache statistics
    pub fn stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            ocr_cache: self.ocr_cache.stats(),
            db_cache: self.db_cache.stats(),
            user_cache_entries: self.user_cache.len(),
            recipe_cache_entries: self.recipe_cache.len(),
            db_cache_size_bytes: self.db_cache.current_size_bytes(),
            db_cache_max_size_bytes: self.db_cache.max_size_bytes(),
        }
    }

    /// Clear all caches
    pub fn clear_all(&mut self) {
        self.ocr_cache.clear();
        self.db_cache.clear();
        self.user_cache.clear();
        self.recipe_cache.clear();
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Comprehensive cache statistics for the cache manager
#[derive(Debug, Clone)]
pub struct CacheManagerStats {
    /// OCR cache statistics
    pub ocr_cache: CacheStats,
    /// Database cache statistics
    pub db_cache: CacheStats,
    /// Number of user entries in cache
    pub user_cache_entries: usize,
    /// Number of recipe entries in cache
    pub recipe_cache_entries: usize,
    /// Current database cache size in bytes
    pub db_cache_size_bytes: usize,
    /// Maximum database cache size in bytes
    pub db_cache_max_size_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_memory_cache_basic_operations() {
        let mut cache = MemoryCache::new();

        // Test insert and get
        cache.insert("key1", "value1", Duration::from_secs(60));
        assert_eq!(cache.get(&"key1"), Some("value1"));
        assert_eq!(cache.get(&"key2"), None);

        // Test stats
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }

    #[test]
    fn test_memory_cache_expiration() {
        let mut cache = MemoryCache::new();

        // Insert with very short TTL
        cache.insert("key1", "value1", Duration::from_millis(10));

        // Should work immediately
        assert_eq!(cache.get(&"key1"), Some("value1"));

        // Wait for expiration
        thread::sleep(Duration::from_millis(20));

        // Should be expired
        assert_eq!(cache.get(&"key1"), None);
    }

    #[test]
    fn test_memory_cache_cleanup() {
        let mut cache = MemoryCache::new();

        // Insert multiple entries with different TTLs
        cache.insert("key1", "value1", Duration::from_millis(10));
        cache.insert("key2", "value2", Duration::from_secs(60));

        // Wait for first entry to expire
        thread::sleep(Duration::from_millis(20));

        // Cleanup should remove expired entries
        cache.cleanup();

        assert_eq!(cache.get(&"key1"), None);
        assert_eq!(cache.get(&"key2"), Some("value2"));
    }

    #[test]
    fn test_ocr_cache_key() {
        let key1 = OcrCacheKey::new("hash1".to_string(), "config1".to_string());
        let key2 = OcrCacheKey::new("hash1".to_string(), "config1".to_string());
        let key3 = OcrCacheKey::new("hash2".to_string(), "config1".to_string());

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_db_cache_size_management() {
        let mut cache = DbQueryCache::new(Duration::from_secs(60), 100); // 100 bytes max

        let key1 = DbCacheKey::new("query1", "params1");
        let value1 = DbCacheValue {
            data: vec![0; 60], // 60 bytes
            size_bytes: 60,
            cached_at: Instant::now(),
        };

        let key2 = DbCacheKey::new("query2", "params2");
        let value2 = DbCacheValue {
            data: vec![0; 50], // 50 bytes
            size_bytes: 50,
            cached_at: Instant::now(),
        };

        // Insert first value (60 bytes)
        cache.insert(key1.clone(), value1, Duration::from_secs(60));
        assert_eq!(cache.current_size_bytes(), 60);

        // Insert second value (50 bytes) - would exceed limit, should evict first entry
        cache.insert(key2.clone(), value2, Duration::from_secs(60));
        assert_eq!(cache.current_size_bytes(), 50); // Evicted 60-byte entry, added 50-byte entry

        // Insert third value that would exceed limit - should trigger eviction
        let key3 = DbCacheKey::new("query3", "params3");
        let value3 = DbCacheValue {
            data: vec![0; 60], // 60 bytes
            size_bytes: 60,
            cached_at: Instant::now(),
        };

        cache.insert(key3.clone(), value3, Duration::from_secs(60));
        // Should have evicted some entries to make room
        assert!(cache.current_size_bytes() <= 100);
    }
}

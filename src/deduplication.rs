//! # Request Deduplication Module
//!
//! This module provides mechanisms to prevent processing duplicate requests,
//! particularly important for Telegram bot operations where the same message
//! might be delivered multiple times due to network issues or retries.

use crate::errors::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use teloxide::types::{ChatId, MessageId};

/// Represents a unique request identifier
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct RequestId {
    /// Telegram chat ID
    pub chat_id: ChatId,
    /// Telegram message ID
    pub message_id: MessageId,
}

impl RequestId {
    /// Create a new request ID from chat and message IDs
    pub fn new(chat_id: ChatId, message_id: MessageId) -> Self {
        Self { chat_id, message_id }
    }
}

/// Tracks when a request was first seen
#[derive(Debug, Clone)]
struct RequestEntry {
    /// When the request was first seen
    first_seen: Instant,
    /// Number of times this request has been seen
    count: u32,
}

/// In-memory request deduplication store
#[derive(Debug)]
pub struct RequestDeduplicator {
    /// Storage for request tracking
    requests: Mutex<HashMap<RequestId, RequestEntry>>,
    /// Time-to-live for request entries
    ttl: Duration,
    /// Maximum number of entries to keep in memory
    max_entries: usize,
}

impl RequestDeduplicator {
    /// Create a new request deduplicator
    pub fn new(ttl_secs: u64, max_entries: usize) -> Self {
        Self {
            requests: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs),
            max_entries,
        }
    }

    /// Check if a request is a duplicate
    ///
    /// Returns true if this is a duplicate request that should be ignored,
    /// false if it's a new request that should be processed.
    pub fn is_duplicate(&self, request_id: &RequestId) -> AppResult<bool> {
        let mut requests = self.requests.lock().map_err(|e| {
            AppError::Internal(format!("Failed to acquire deduplication lock: {}", e))
        })?;

        let now = Instant::now();

        // Clean up expired entries
        requests.retain(|_, entry| now.duration_since(entry.first_seen) < self.ttl);

        // Check if we've seen this request before
        if let Some(entry) = requests.get_mut(request_id) {
            // Update the count
            entry.count += 1;
            Ok(true) // This is a duplicate
        } else {
            // Add new entry
            if requests.len() >= self.max_entries {
                // If we're at capacity, remove the oldest entry
                if let Some(oldest_key) = requests
                    .iter()
                    .min_by_key(|(_, entry)| entry.first_seen)
                    .map(|(key, _)| key.clone())
                {
                    requests.remove(&oldest_key);
                }
            }

            requests.insert(
                request_id.clone(),
                RequestEntry {
                    first_seen: now,
                    count: 1,
                },
            );
            Ok(false) // This is not a duplicate
        }
    }

    /// Get statistics about the deduplicator
    pub fn stats(&self) -> AppResult<DeduplicationStats> {
        let requests = self.requests.lock().map_err(|e| {
            AppError::Internal(format!("Failed to acquire deduplication lock: {}", e))
        })?;

        let now = Instant::now();
        let total_entries = requests.len();
        let expired_entries = requests
            .values()
            .filter(|entry| now.duration_since(entry.first_seen) >= self.ttl)
            .count();
        let active_entries = total_entries.saturating_sub(expired_entries);
        let total_duplicates = requests.values().map(|entry| entry.count.saturating_sub(1)).sum();

        Ok(DeduplicationStats {
            total_entries,
            active_entries,
            expired_entries,
            total_duplicates,
            max_entries: self.max_entries,
            ttl: self.ttl,
        })
    }

    /// Clear all entries (useful for testing or manual cleanup)
    pub fn clear(&self) -> AppResult<()> {
        let mut requests = self.requests.lock().map_err(|e| {
            AppError::Internal(format!("Failed to acquire deduplication lock: {}", e))
        })?;
        requests.clear();
        Ok(())
    }
}

/// Statistics about the deduplication system
#[derive(Debug, Clone)]
pub struct DeduplicationStats {
    /// Total number of entries currently stored
    pub total_entries: usize,
    /// Number of active (non-expired) entries
    pub active_entries: usize,
    /// Number of expired entries (will be cleaned up on next operation)
    pub expired_entries: usize,
    /// Total number of duplicate requests detected
    pub total_duplicates: u32,
    /// Maximum number of entries allowed
    pub max_entries: usize,
    /// Time-to-live for entries
    pub ttl: Duration,
}

impl Default for RequestDeduplicator {
    fn default() -> Self {
        Self::new(300, 10000) // 5 minutes TTL, 10k max entries
    }
}

/// Thread-safe wrapper for request deduplication
pub type SharedDeduplicator = Arc<RequestDeduplicator>;

/// Create a new shared deduplicator instance
pub fn create_shared_deduplicator(ttl_secs: u64, max_entries: usize) -> SharedDeduplicator {
    Arc::new(RequestDeduplicator::new(ttl_secs, max_entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_request_id_creation() {
        let chat_id = ChatId(12345);
        let message_id = MessageId(678);
        let request_id = RequestId::new(chat_id, message_id);

        assert_eq!(request_id.chat_id, chat_id);
        assert_eq!(request_id.message_id, message_id);
    }

    #[test]
    fn test_deduplication_basic() {
        let deduplicator = RequestDeduplicator::new(60, 100); // 1 minute TTL
        let request_id = RequestId::new(ChatId(123), MessageId(456));

        // First request should not be duplicate
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());

        // Second request with same ID should be duplicate
        assert!(deduplicator.is_duplicate(&request_id).unwrap());

        // Different request ID should not be duplicate
        let different_id = RequestId::new(ChatId(123), MessageId(457));
        assert!(!deduplicator.is_duplicate(&different_id).unwrap());
    }

    #[test]
    fn test_deduplication_expiration() {
        let deduplicator = RequestDeduplicator::new(1, 100); // 1 second TTL
        let request_id = RequestId::new(ChatId(123), MessageId(456));

        // First request
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());

        // Wait for expiration
        thread::sleep(Duration::from_secs(2));

        // Same request should not be considered duplicate after expiration
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());
    }

    #[test]
    fn test_max_entries_limit() {
        let deduplicator = RequestDeduplicator::new(300, 3); // Only 3 entries max

        // Add 3 unique requests
        for i in 0..3 {
            let request_id = RequestId::new(ChatId(i as i64), MessageId(i));
            assert!(!deduplicator.is_duplicate(&request_id).unwrap());
        }

        // Add a 4th request - should trigger cleanup of oldest
        let fourth_id = RequestId::new(ChatId(999), MessageId(999));
        assert!(!deduplicator.is_duplicate(&fourth_id).unwrap());

        // Check that we don't exceed max entries
        let stats = deduplicator.stats().unwrap();
        assert!(stats.total_entries <= 3);
    }

    #[test]
    fn test_statistics() {
        let deduplicator = RequestDeduplicator::new(300, 100);
        let request_id = RequestId::new(ChatId(123), MessageId(456));

        // Initial stats should be empty
        let initial_stats = deduplicator.stats().unwrap();
        assert_eq!(initial_stats.total_entries, 0);
        assert_eq!(initial_stats.total_duplicates, 0);

        // Add a request
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());

        // Check stats after first request
        let stats = deduplicator.stats().unwrap();
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.active_entries, 1);
        assert_eq!(stats.total_duplicates, 0);

        // Add duplicate
        assert!(deduplicator.is_duplicate(&request_id).unwrap());

        // Check stats after duplicate
        let stats = deduplicator.stats().unwrap();
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.active_entries, 1);
        assert_eq!(stats.total_duplicates, 1);
    }

    #[test]
    fn test_clear_functionality() {
        let deduplicator = RequestDeduplicator::new(300, 100);
        let request_id = RequestId::new(ChatId(123), MessageId(456));

        // Add a request
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());
        assert_eq!(deduplicator.stats().unwrap().total_entries, 1);

        // Clear all entries
        deduplicator.clear().unwrap();
        assert_eq!(deduplicator.stats().unwrap().total_entries, 0);

        // Same request should be treated as new after clear
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());
    }

    #[test]
    fn test_shared_deduplicator() {
        let deduplicator = create_shared_deduplicator(300, 100);
        let request_id = RequestId::new(ChatId(123), MessageId(456));

        // Should work with Arc
        assert!(!deduplicator.is_duplicate(&request_id).unwrap());
        assert!(deduplicator.is_duplicate(&request_id).unwrap());
    }
}
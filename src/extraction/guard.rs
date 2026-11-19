//! Load and cost protection around any [`IngredientExtractor`]:
//!
//! - at most `max_concurrency` extractions run at once; a photo that waits longer than the
//!   queue timeout gets the "busy" message instead of waiting forever;
//! - successful results are cached by SHA-256 of the image bytes, so the same photo sent again
//!   (or forwarded by another user) is never paid twice. The cache is bounded in size and age;
//! - each user gets at most a daily number of paid calls. Cache hits and images without a
//!   requester do not count. The count is kept by a [`DailyCalls`]: in the database in
//!   production (`engine::photos`: shared by every door, kept across deploys, Paris days), in
//!   memory otherwise (UTC days, a restart resets it).

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use chrono::NaiveDate;

use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

use crate::extraction::{
    Extraction, ExtractionError, ExtractionFuture, ImageInput, IngredientExtractor, SharedExtractor,
};

/// Longest wait for a free extraction slot.
pub const DEFAULT_QUEUE_TIMEOUT: Duration = Duration::from_secs(30);
/// Cached results kept, oldest evicted first; an extraction is a few KB.
pub const DEFAULT_CACHE_CAPACITY: usize = 100;
/// How long a cached result is reused.
pub const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

type ImageHash = [u8; 32];

/// Bounded, time-limited cache of extraction results.
#[derive(Debug, Default)]
struct ResultCache {
    entries: HashMap<ImageHash, (Instant, Extraction)>,
    /// Insertion order, for eviction.
    order: VecDeque<ImageHash>,
}

/// Future returned by a [`DailyCalls`].
pub type DailyFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Where each user's paid calls of the day are counted.
pub trait DailyCalls: Send + Sync + std::fmt::Debug {
    /// Counts one paid call of `user` today, unless they already made `limit`: the day it was
    /// counted on, `None` when the limit is reached. An error when the count cannot be read: the
    /// call is not made.
    fn take(&self, user: i64, limit: u32) -> DailyFuture<'_, anyhow::Result<Option<NaiveDate>>>;

    /// Gives back a call counted on `day` that never reached the service.
    fn give_back(&self, user: i64, day: NaiveDate) -> DailyFuture<'_, ()>;
}

/// [`DailyCalls`] in memory, UTC days: tests and local runs (a restart resets it).
#[derive(Debug, Default)]
pub struct MemoryDailyCalls {
    usage: Mutex<DailyUsage>,
}

impl MemoryDailyCalls {
    fn lock(&self) -> MutexGuard<'_, DailyUsage> {
        // Plain counters: a panic while holding the lock cannot leave them inconsistent.
        self.usage.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl DailyCalls for MemoryDailyCalls {
    fn take(&self, user: i64, limit: u32) -> DailyFuture<'_, anyhow::Result<Option<NaiveDate>>> {
        let day = chrono::Utc::now().date_naive();
        let taken = self.lock().take(user, day, limit);
        Box::pin(async move { Ok(taken.then_some(day)) })
    }

    fn give_back(&self, user: i64, day: NaiveDate) -> DailyFuture<'_, ()> {
        self.lock().give_back(user, day);
        Box::pin(async {})
    }
}

/// Paid calls of each user during one day.
#[derive(Debug, Default)]
struct DailyUsage {
    day: Option<NaiveDate>,
    calls: HashMap<i64, u32>,
}

impl DailyUsage {
    /// Counts one call of `user` on `day`, unless they already made `limit` calls that day.
    fn take(&mut self, user: i64, day: NaiveDate, limit: u32) -> bool {
        if self.day != Some(day) {
            self.day = Some(day);
            self.calls.clear();
        }
        let calls = self.calls.entry(user).or_insert(0);
        if *calls >= limit {
            return false;
        }
        *calls += 1;
        true
    }

    /// Gives back a call counted on `day` that did not reach the service.
    fn give_back(&mut self, user: i64, day: NaiveDate) {
        if self.day == Some(day)
            && let Some(calls) = self.calls.get_mut(&user)
        {
            *calls = calls.saturating_sub(1);
        }
    }
}

/// Wraps an extractor with a concurrency limit, a result cache and a per-user daily limit.
#[derive(Debug)]
pub struct GuardedExtractor {
    inner: SharedExtractor,
    permits: Semaphore,
    queue_timeout: Duration,
    cache: Mutex<ResultCache>,
    cache_capacity: usize,
    cache_ttl: Duration,
    daily_limit: Option<u32>,
    daily_calls: Arc<dyn DailyCalls>,
}

impl GuardedExtractor {
    /// Guards `inner` with `max_concurrency` slots and the default queue timeout and cache.
    pub fn new(inner: SharedExtractor, max_concurrency: usize) -> Self {
        Self {
            inner,
            permits: Semaphore::new(max_concurrency.max(1)),
            queue_timeout: DEFAULT_QUEUE_TIMEOUT,
            cache: Mutex::new(ResultCache::default()),
            cache_capacity: DEFAULT_CACHE_CAPACITY,
            cache_ttl: DEFAULT_CACHE_TTL,
            daily_limit: None,
            daily_calls: Arc::new(MemoryDailyCalls::default()),
        }
    }

    /// Limits each user to `limit` paid calls a day (counted in memory unless
    /// [`Self::with_daily_calls`]).
    pub fn with_daily_limit(mut self, limit: u32) -> Self {
        self.daily_limit = Some(limit);
        self
    }

    /// Counts the daily calls in `daily_calls` (the database in production).
    pub fn with_daily_calls(mut self, daily_calls: Arc<dyn DailyCalls>) -> Self {
        self.daily_calls = daily_calls;
        self
    }

    /// Replaces the queue timeout.
    pub fn with_queue_timeout(mut self, timeout: Duration) -> Self {
        self.queue_timeout = timeout;
        self
    }

    /// Replaces the cache size and age limits.
    pub fn with_cache(mut self, capacity: usize, ttl: Duration) -> Self {
        self.cache_capacity = capacity;
        self.cache_ttl = ttl;
        self
    }

    async fn extract_guarded(&self, image: &ImageInput) -> Result<Extraction, ExtractionError> {
        let hash: ImageHash = Sha256::digest(image.bytes()).into();
        if let Some(cached) = self.cached(&hash) {
            crate::observability::gemini_cache_hit();
            return Ok(cached);
        }

        let _permit = tokio::time::timeout(self.queue_timeout, self.permits.acquire())
            .await
            .map_err(|_| ExtractionError::RateLimited { retry_after: None })?
            .map_err(|_| ExtractionError::Config("extraction slots closed".to_string()))?;

        // Another request for the same image may have finished while this one waited.
        if let Some(cached) = self.cached(&hash) {
            crate::observability::gemini_cache_hit();
            return Ok(cached);
        }

        let day = self.take_daily_call(image.requester()).await?;
        let extraction = match self.inner.extract(image).await {
            Ok(extraction) => extraction,
            Err(error) => {
                // Refused before any request (breaker open, configuration): not a paid call
                if matches!(
                    error,
                    ExtractionError::CircuitOpen | ExtractionError::Config(_)
                ) {
                    self.give_back_daily_call(image.requester(), day).await;
                }
                return Err(error);
            }
        };
        // Only useful answers are cached: "not a recipe" or no ingredient may be a model miss,
        // and sending the same photo again must try again.
        if extraction.is_recipe && !extraction.ingredients.is_empty() {
            self.store(hash, &extraction);
        }
        Ok(extraction)
    }

    /// Counts a paid call toward the requester's daily limit, failing when it is reached.
    /// Returns the day it was counted on (none without a limit or a requester). A count that
    /// cannot be read refuses the call as "busy": the bill is protected first.
    async fn take_daily_call(
        &self,
        requester: Option<i64>,
    ) -> Result<Option<NaiveDate>, ExtractionError> {
        let (Some(limit), Some(user)) = (self.daily_limit, requester) else {
            return Ok(None);
        };
        match self.daily_calls.take(user, limit).await {
            Ok(Some(day)) => Ok(Some(day)),
            Ok(None) => Err(ExtractionError::DailyLimitReached { limit }),
            Err(error) => {
                tracing::warn!(error = %error, "Could not count a photo reading: refused");
                Err(ExtractionError::RateLimited { retry_after: None })
            }
        }
    }

    async fn give_back_daily_call(&self, requester: Option<i64>, day: Option<NaiveDate>) {
        if let (Some(user), Some(day)) = (requester, day) {
            self.daily_calls.give_back(user, day).await;
        }
    }

    fn lock(&self) -> MutexGuard<'_, ResultCache> {
        // Plain data: a panic while holding the lock cannot leave it inconsistent.
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn cached(&self, hash: &ImageHash) -> Option<Extraction> {
        let mut cache = self.lock();
        match cache.entries.get(hash) {
            Some((stored, extraction)) if stored.elapsed() < self.cache_ttl => {
                Some(extraction.clone())
            }
            Some(_) => {
                cache.entries.remove(hash);
                cache.order.retain(|key| key != hash);
                None
            }
            None => None,
        }
    }

    fn store(&self, hash: ImageHash, extraction: &Extraction) {
        if self.cache_capacity == 0 {
            return;
        }
        let mut cache = self.lock();
        if cache
            .entries
            .insert(hash, (Instant::now(), extraction.clone()))
            .is_none()
        {
            cache.order.push_back(hash);
        }
        while cache.order.len() > self.cache_capacity {
            if let Some(oldest) = cache.order.pop_front() {
                cache.entries.remove(&oldest);
            }
        }
    }
}

impl IngredientExtractor for GuardedExtractor {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn extract<'a>(&'a self, image: &'a ImageInput) -> ExtractionFuture<'a> {
        Box::pin(self.extract_guarded(image))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::{ExtractedIngredient, RecipeLanguage};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Counts calls and the highest number of calls in flight at once.
    #[derive(Debug, Default)]
    struct FakeExtractor {
        calls: AtomicUsize,
        in_flight: AtomicUsize,
        max_in_flight: AtomicUsize,
        fail: bool,
        not_a_recipe: bool,
        delay: Duration,
    }

    impl IngredientExtractor for FakeExtractor {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn extract<'a>(&'a self, _image: &'a ImageInput) -> ExtractionFuture<'a> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_in_flight.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(self.delay).await;
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                if self.fail {
                    Err(ExtractionError::Timeout)
                } else {
                    Ok(Extraction {
                        is_recipe: !self.not_a_recipe,
                        title: None,
                        language: RecipeLanguage::Fr,
                        transcription: "sel".to_string(),
                        ingredients: vec![ExtractedIngredient {
                            name: "sel".to_string(),
                            quantity: None,
                            unit: None,
                            needs_review: false,
                            source_text: "sel".to_string(),
                            section: None,
                        }],
                    })
                }
            })
        }
    }

    fn image(seed: u8) -> Result<ImageInput, ExtractionError> {
        ImageInput::from_bytes(vec![0xFF, 0xD8, 0xFF, seed])
    }

    #[tokio::test]
    async fn the_same_image_is_extracted_once() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor::default());
        let guarded = GuardedExtractor::new(fake.clone(), 2);
        guarded.extract(&image(1)?).await?;
        guarded.extract(&image(1)?).await?;
        guarded.extract(&image(2)?).await?;
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[tokio::test]
    async fn failures_are_not_cached() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor {
            fail: true,
            ..FakeExtractor::default()
        });
        let guarded = GuardedExtractor::new(fake.clone(), 2);
        for _ in 0..2 {
            assert!(guarded.extract(&image(1)?).await.is_err());
        }
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
        Ok(())
    }

    #[tokio::test]
    async fn not_a_recipe_answers_are_not_cached() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor {
            not_a_recipe: true,
            ..FakeExtractor::default()
        });
        let guarded = GuardedExtractor::new(fake.clone(), 2);
        guarded.extract(&image(1)?).await?;
        guarded.extract(&image(1)?).await?;
        assert_eq!(
            fake.calls.load(Ordering::SeqCst),
            2,
            "the photo is read again"
        );
        Ok(())
    }

    #[tokio::test]
    async fn concurrency_is_limited() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor {
            delay: Duration::from_millis(50),
            ..FakeExtractor::default()
        });
        let guarded = GuardedExtractor::new(fake.clone(), 1);
        let (first, second) = (image(1)?, image(2)?);
        let (a, b) = tokio::join!(guarded.extract(&first), guarded.extract(&second));
        a?;
        b?;
        assert_eq!(fake.max_in_flight.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn waiting_too_long_for_a_slot_reports_busy() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor {
            delay: Duration::from_millis(300),
            ..FakeExtractor::default()
        });
        let guarded =
            GuardedExtractor::new(fake.clone(), 1).with_queue_timeout(Duration::from_millis(50));
        let (first, second) = (image(1)?, image(2)?);
        let (a, b) = tokio::join!(guarded.extract(&first), guarded.extract(&second));
        let results = [a, b];
        assert!(results.iter().any(Result::is_ok));
        assert!(
            results
                .iter()
                .any(|r| matches!(r, Err(ExtractionError::RateLimited { .. })))
        );
        Ok(())
    }

    #[tokio::test]
    async fn a_user_over_the_daily_limit_is_refused_without_a_call() -> Result<(), ExtractionError>
    {
        let fake = Arc::new(FakeExtractor::default());
        let guarded = GuardedExtractor::new(fake.clone(), 2).with_daily_limit(2);
        guarded.extract(&image(1)?.requested_by(7)).await?;
        guarded.extract(&image(2)?.requested_by(7)).await?;
        let refused = guarded.extract(&image(3)?.requested_by(7)).await;
        assert!(matches!(
            refused,
            Err(ExtractionError::DailyLimitReached { limit: 2 })
        ));
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2, "no paid call refused");

        // A photo already read is free, and other users have their own count
        guarded.extract(&image(1)?.requested_by(7)).await?;
        guarded.extract(&image(3)?.requested_by(8)).await?;
        assert_eq!(fake.calls.load(Ordering::SeqCst), 3);
        Ok(())
    }

    #[tokio::test]
    async fn a_call_refused_before_the_service_is_not_counted() -> Result<(), ExtractionError> {
        let failing = Arc::new(FakeExtractor {
            fail: true,
            ..FakeExtractor::default()
        });
        let guarded = GuardedExtractor::new(failing, 2).with_daily_limit(1);
        // The fake fails with Timeout: the call may have been paid, so it counts
        assert!(guarded.extract(&image(1)?.requested_by(7)).await.is_err());
        assert!(matches!(
            guarded.extract(&image(2)?.requested_by(7)).await,
            Err(ExtractionError::DailyLimitReached { .. })
        ));

        let mut usage = DailyUsage::default();
        let (day, before) = (date(2026, 11, 19), date(2026, 11, 18));
        assert!(usage.take(7, day, 1));
        usage.give_back(7, day);
        assert!(usage.take(7, day, 1), "given back");
        usage.give_back(7, before);
        assert!(
            !usage.take(7, day, 1),
            "a give-back for another day is ignored"
        );
        Ok(())
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).unwrap_or_default()
    }

    #[test]
    fn daily_counts_start_again_the_next_day() {
        let mut usage = DailyUsage::default();
        assert!(usage.take(7, date(2026, 11, 19), 1));
        assert!(!usage.take(7, date(2026, 11, 19), 1));
        assert!(usage.take(7, date(2026, 11, 20), 1), "a new day");
        assert_eq!(usage.calls.len(), 1, "the previous day is forgotten");
    }

    /// A count that cannot be read refuses the call ("busy") and costs nothing.
    #[derive(Debug)]
    struct BrokenDailyCalls;

    impl DailyCalls for BrokenDailyCalls {
        fn take(&self, _: i64, _: u32) -> DailyFuture<'_, anyhow::Result<Option<NaiveDate>>> {
            Box::pin(async { Err(anyhow::anyhow!("database down")) })
        }

        fn give_back(&self, _: i64, _: NaiveDate) -> DailyFuture<'_, ()> {
            Box::pin(async {})
        }
    }

    #[tokio::test]
    async fn an_unreadable_count_refuses_the_call() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor::default());
        let guarded = GuardedExtractor::new(fake.clone(), 2)
            .with_daily_limit(5)
            .with_daily_calls(Arc::new(BrokenDailyCalls));
        assert!(matches!(
            guarded.extract(&image(1)?.requested_by(7)).await,
            Err(ExtractionError::RateLimited { .. })
        ));
        assert_eq!(fake.calls.load(Ordering::SeqCst), 0, "nothing paid");
        // Without a requester (no user to count), the reading goes on
        guarded.extract(&image(1)?).await?;
        assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn the_cache_is_bounded_and_expires() -> Result<(), ExtractionError> {
        let fake = Arc::new(FakeExtractor::default());
        let guarded = GuardedExtractor::new(fake.clone(), 2).with_cache(2, Duration::from_secs(60));
        for seed in [1, 2, 3, 1] {
            guarded.extract(&image(seed)?).await?;
        }
        assert_eq!(
            fake.calls.load(Ordering::SeqCst),
            4,
            "image 1 was evicted by image 3"
        );

        let fake = Arc::new(FakeExtractor::default());
        let expiring = GuardedExtractor::new(fake.clone(), 2).with_cache(10, Duration::ZERO);
        expiring.extract(&image(1)?).await?;
        expiring.extract(&image(1)?).await?;
        assert_eq!(
            fake.calls.load(Ordering::SeqCst),
            2,
            "expired entries are not reused"
        );
        Ok(())
    }
}

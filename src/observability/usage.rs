//! The usage history (plan O3, `specs/tasks/observability.md`): each user's last visit and one
//! line of totals per day (`usage_days`, Paris time), kept beyond the 15 days of Fly's metrics,
//! and the `ji_users` and `ji_recipes` gauges, computed from the database every hour.
//!
//! Counts are added in memory as they happen and written every minute: a crash loses at most a
//! minute of them. A user's visit is written at most once an hour, and at the user's first
//! update of a Paris day.

use crate::db::UsageCounts;
use chrono::NaiveDate;
use sqlx::PgPool;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Counts not written yet, by Paris day.
static PENDING: Mutex<BTreeMap<NaiveDate, UsageCounts>> = Mutex::new(BTreeMap::new());

/// When each user's visit was last written, and on which Paris day.
static VISITS: LazyLock<Mutex<HashMap<i64, (Instant, NaiveDate)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// A visit is written at most this often per user.
const VISIT_EVERY: Duration = Duration::from_secs(3600);

/// Users whose last write is remembered; older ones are forgotten past this.
const MAX_REMEMBERED: usize = 10_000;

/// The Paris day now: the day of `usage_days`.
pub fn today() -> NaiveDate {
    crate::bot::ui_builder::paris_time(chrono::Utc::now()).date()
}

/// Adds to today's counts.
pub fn add(change: impl FnOnce(&mut UsageCounts)) {
    let mut pending = PENDING.lock().unwrap_or_else(PoisonError::into_inner);
    change(pending.entry(today()).or_default());
}

/// Whether the visit of `telegram_id` is to be written now; if so, it counts as written.
fn visit_is_due(
    visits: &mut HashMap<i64, (Instant, NaiveDate)>,
    telegram_id: i64,
    now: Instant,
    day: NaiveDate,
) -> bool {
    if let Some(&(at, seen_on)) = visits.get(&telegram_id)
        && seen_on == day
        && now.duration_since(at) < VISIT_EVERY
    {
        return false;
    }
    if visits.len() >= MAX_REMEMBERED {
        visits.retain(|_, (at, _)| now.duration_since(*at) < VISIT_EVERY);
    }
    visits.insert(telegram_id, (now, day));
    true
}

/// Records that `telegram_id` uses the bot, in the background: the update is not delayed.
pub fn seen(pool: &Arc<PgPool>, telegram_id: i64, language_code: Option<&str>) {
    let due = visit_is_due(
        &mut VISITS.lock().unwrap_or_else(PoisonError::into_inner),
        telegram_id,
        Instant::now(),
        today(),
    );
    if !due {
        return;
    }
    let pool = Arc::clone(pool);
    let language_code = language_code.map(str::to_string);
    tokio::spawn(async move {
        if let Err(error) =
            crate::db::record_user_seen(&pool, telegram_id, language_code.as_deref()).await
        {
            // Not tried again at each update (a database down would get one write per
            // update): the next one is due in an hour or the next day
            tracing::warn!(error = %error, "Could not record a visit");
        }
    });
}

/// Held while counts are written: a flush waits for the one running (the last one, before
/// exiting, must not end while the minute's one still holds counts taken from `PENDING`).
static FLUSHING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Writes the counts added since the last write; those that fail are kept for the next one.
pub async fn flush(pool: &PgPool) {
    let _flushing = FLUSHING.lock().await;
    let pending = std::mem::take(&mut *PENDING.lock().unwrap_or_else(PoisonError::into_inner));
    for (day, counts) in pending {
        if let Err(error) = crate::db::add_usage(pool, day, &counts).await {
            tracing::warn!(error = %error, %day, "Could not save the day's usage, kept for later");
            *PENDING
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(day)
                .or_default() += counts;
        }
    }
}

async fn refresh_gauges(pool: &PgPool) {
    match crate::db::usage_totals(pool).await {
        Ok(totals) => {
            let count = |n: i64| u64::try_from(n).unwrap_or(0);
            super::metrics::set_users(
                count(totals.users_last_day),
                count(totals.users_last_week),
                count(totals.users),
            );
            super::metrics::set_recipes(count(totals.recipes));
        }
        Err(error) => tracing::warn!(error = %error, "Could not count users and recipes"),
    }
}

/// Writes the counts every minute and refreshes the gauges every hour (at once on start).
pub async fn run(pool: Arc<PgPool>) {
    let mut every_minute = tokio::time::interval(Duration::from_secs(60));
    every_minute.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut minute = 0u32;
    loop {
        every_minute.tick().await;
        flush(&pool).await;
        if minute == 0 {
            refresh_gauges(&pool).await;
        }
        minute = (minute + 1) % 60;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_visit_is_written_once_an_hour_and_on_a_new_day() {
        let mut visits = HashMap::new();
        let start = Instant::now();
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap_or_default();
        let next_day = day.succ_opt().unwrap_or_default();
        assert!(visit_is_due(&mut visits, 1, start, day));
        assert!(!visit_is_due(
            &mut visits,
            1,
            start + Duration::from_secs(60),
            day
        ));
        assert!(visit_is_due(&mut visits, 2, start, day), "another user");
        assert!(
            visit_is_due(&mut visits, 1, start + Duration::from_secs(120), next_day),
            "the first update after midnight counts the user on the new day"
        );
        assert!(!visit_is_due(
            &mut visits,
            1,
            start + Duration::from_secs(180),
            next_day
        ));
        assert!(visit_is_due(
            &mut visits,
            1,
            start + VISIT_EVERY + Duration::from_secs(120),
            next_day
        ));
    }

    #[test]
    fn remembered_visits_stay_bounded() {
        let mut visits = HashMap::new();
        let start = Instant::now();
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap_or_default();
        for user in 0..MAX_REMEMBERED as i64 {
            visit_is_due(&mut visits, user, start, day);
        }
        let later = start + VISIT_EVERY;
        assert!(visit_is_due(&mut visits, -1, later, day));
        assert_eq!(
            visits.len(),
            1,
            "the visits older than an hour are forgotten"
        );
    }
}

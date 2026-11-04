//! Alerts the bot sends its owner on Telegram (plan O4, `specs/tasks/observability.md`): Fly
//! has no alerting on metrics, and the bot knows its own failures. `OWNER_CHAT_ID` is the
//! owner's private chat with the bot (their Telegram id); without it nothing is sent. A subject
//! is sent at most once an hour, and a "back" message only after its "down" one was sent. The
//! messages are in French, the owner's language.
//!
//! "Production Uptime" (GitHub) stays the alert for a bot that does not answer at all: a bot
//! that is down cannot say so.

use chrono::NaiveDate;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use teloxide::prelude::*;
use tokio::sync::mpsc;

/// Gemini's estimated cost in a Paris day above which the owner is told (1 $, 2026-11-02).
pub const DAILY_COST_LIMIT_MICRO_USD: u64 = 1_000_000;

/// The same subject is sent at most this often.
const QUIET_FOR: Duration = Duration::from_secs(3600);

/// Gemini "fails often": more than 30 % of at least 5 calls in the last hour.
const FAILING_WINDOW: Duration = Duration::from_secs(3600);
const FAILING_MIN_CALLS: usize = 5;
const FAILING_PERCENT: usize = 30;
/// Calls remembered for that, at most.
const MAX_CALLS_KEPT: usize = 10_000;

/// The database is down after this many failed checks in a row, one a minute.
const DATABASE_DOWN_AFTER: u32 = 2;

/// `app_state` key: when the last restart alert was sent, in seconds since 1970.
const RESTART_TOLD: &str = "restart_alert";

/// Something the owner is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    /// The circuit breaker opened: several Gemini failures in a row.
    GeminiDown,
    /// A call succeeded again after [`Alert::GeminiDown`].
    GeminiBack,
    /// Many failures in the last hour, without the breaker opening.
    GeminiFailing {
        failures: usize,
        calls: usize,
    },
    /// Today's estimated cost is over [`DAILY_COST_LIMIT_MICRO_USD`].
    DailyCost {
        micro_usd: u64,
    },
    DatabaseDown,
    DatabaseBack,
    /// The bot started again without a new release (a crash, or Fly moving the machine).
    Restarted,
    /// The startup check of Gemini failed (a key refused, a model gone, unreachable): photos
    /// will fail. Not a down/back pair: the breaker's alerts tell what comes next.
    GeminiFailedAtStart {
        reason: &'static str,
    },
}

impl Alert {
    /// What is limited to once an hour; a "back" shares its "down"'s subject.
    fn subject(&self) -> &'static str {
        match self {
            Self::GeminiDown | Self::GeminiBack => "gemini_down",
            Self::GeminiFailing { .. } => "gemini_failing",
            Self::DailyCost { .. } => "daily_cost",
            Self::DatabaseDown | Self::DatabaseBack => "database_down",
            Self::Restarted => "restarted",
            Self::GeminiFailedAtStart { .. } => "gemini_start",
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::GeminiDown => "🔴 Gemini en panne : plusieurs échecs de suite, les photos sont \
                                 refusées quelques minutes avant un nouvel essai."
                .to_string(),
            Self::GeminiBack => "🟢 Gemini répond de nouveau.".to_string(),
            Self::GeminiFailing { failures, calls } => format!(
                "🟠 Gemini échoue souvent : {failures} échecs sur {calls} appels dans l'heure."
            ),
            Self::DailyCost { micro_usd } => format!(
                "💸 Coût Gemini du jour : {} $, au-dessus du seuil de {} $.",
                super::report::dollars(*micro_usd, 2),
                super::report::dollars(DAILY_COST_LIMIT_MICRO_USD, 0)
            ),
            Self::DatabaseDown => "🟠 Base de données injoignable : les recettes ne peuvent être \
                                   ni lues ni enregistrées."
                .to_string(),
            Self::DatabaseBack => "🟢 Base de données de nouveau joignable.".to_string(),
            Self::Restarted => "🔁 Le bot a redémarré (sans mise en production).".to_string(),
            Self::GeminiFailedAtStart { reason } => format!(
                "🔴 Gemini ne répond pas au démarrage ({reason}) : les photos vont échouer. \
                 À vérifier : la clé GEMINI_API_KEY et le modèle."
            ),
        }
    }
}

/// Which alerts are sent: each subject once an hour; a "back" only after its "down" was sent,
/// and a "down" after a "back" always, so that the owner's last message tells the current
/// state.
#[derive(Debug, Default)]
struct Gate {
    last_sent: HashMap<&'static str, Instant>,
    down: HashSet<&'static str>,
    back: HashSet<&'static str>,
}

impl Gate {
    fn admits(&mut self, alert: &Alert, now: Instant) -> bool {
        let subject = alert.subject();
        if matches!(alert, Alert::GeminiBack | Alert::DatabaseBack) {
            let admitted = self.down.remove(subject);
            if admitted {
                self.back.insert(subject);
            }
            return admitted;
        }
        let is_down = matches!(alert, Alert::GeminiDown | Alert::DatabaseDown);
        let after_back = is_down && self.back.remove(subject);
        if !after_back
            && self
                .last_sent
                .get(subject)
                .is_some_and(|at| now.duration_since(*at) < QUIET_FOR)
        {
            return false;
        }
        self.last_sent.insert(subject, now);
        if is_down {
            self.down.insert(subject);
        }
        true
    }
}

/// What the alerts watch between two events: Gemini's last calls (whether each failed because
/// of Gemini), the breaker, and today's cost.
#[derive(Debug)]
struct Watch {
    calls: VecDeque<(Instant, bool)>,
    breaker_open: bool,
    cost_day: Option<NaiveDate>,
    cost: u64,
    cost_told: bool,
}

impl Watch {
    const fn new() -> Self {
        Self {
            calls: VecDeque::new(),
            breaker_open: false,
            cost_day: None,
            cost: 0,
            cost_told: false,
        }
    }

    /// A call ended; `outage` when it failed because of Gemini (timeout, quota, network, server
    /// error), not because of the photo. Told while the breaker is closed only: open, "Gemini en
    /// panne" says more.
    fn call(&mut self, outage: bool, now: Instant) -> Option<Alert> {
        if self.calls.len() >= MAX_CALLS_KEPT {
            self.calls.pop_front();
        }
        self.calls.push_back((now, outage));
        while self
            .calls
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) > FAILING_WINDOW)
        {
            self.calls.pop_front();
        }
        if !outage || self.breaker_open {
            return None;
        }
        let calls = self.calls.len();
        let failures = self.calls.iter().filter(|(_, outage)| *outage).count();
        (calls >= FAILING_MIN_CALLS && failures * 100 > calls * FAILING_PERCENT)
            .then_some(Alert::GeminiFailing { failures, calls })
    }

    /// Adds to the cost of `day`; told once a day, when it goes over the limit.
    fn cost(&mut self, micro_usd: u64, day: NaiveDate) -> Option<Alert> {
        if self.cost_day != Some(day) {
            self.cost_day = Some(day);
            self.cost = 0;
            self.cost_told = false;
        }
        self.cost += micro_usd;
        if self.cost > DAILY_COST_LIMIT_MICRO_USD && !self.cost_told {
            self.cost_told = true;
            return Some(Alert::DailyCost {
                micro_usd: self.cost,
            });
        }
        None
    }

    /// Today's cost so far, read from the database at startup. Already over the limit, it was
    /// told before the restart.
    fn start_cost(&mut self, micro_usd: u64, day: NaiveDate) {
        self.cost_day = Some(day);
        self.cost = micro_usd;
        self.cost_told = micro_usd > DAILY_COST_LIMIT_MICRO_USD;
    }
}

static WATCH: Mutex<Watch> = Mutex::new(Watch::new());
static SENDER: OnceLock<mpsc::UnboundedSender<Alert>> = OnceLock::new();

fn watch() -> std::sync::MutexGuard<'static, Watch> {
    WATCH.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Tells the owner, when alerts are on.
pub fn raise(alert: Alert) {
    if let Some(sender) = SENDER.get() {
        let _ = sender.send(alert);
    }
}

/// A Gemini call ended (not one the breaker refused); `outage` when it failed because of
/// Gemini.
pub fn gemini_call(outage: bool) {
    let alert = watch().call(outage, Instant::now());
    if let Some(alert) = alert {
        raise(alert);
    }
}

/// The breaker opened, or a call succeeded again after it opened.
pub fn breaker_changed(open: bool) {
    watch().breaker_open = open;
    raise(if open {
        Alert::GeminiDown
    } else {
        Alert::GeminiBack
    });
}

/// The estimated cost of a Gemini call.
pub fn gemini_cost(micro_usd: u64) {
    let alert = watch().cost(micro_usd, super::usage::today());
    if let Some(alert) = alert {
        raise(alert);
    }
}

/// The startup check of Gemini failed: tells the owner why, in a few words. Call it after
/// [`start`], which opens the way to the owner.
pub fn gemini_failed_at_start(error: &crate::extraction::ExtractionError) {
    raise(Alert::GeminiFailedAtStart {
        reason: start_failure_reason(error),
    });
}

/// A few words for the owner on why the startup check of Gemini failed.
fn start_failure_reason(error: &crate::extraction::ExtractionError) -> &'static str {
    use crate::extraction::ExtractionError;
    match error {
        ExtractionError::Api {
            status: 400 | 401 | 403,
            ..
        }
        | ExtractionError::Config(_) => "clé refusée",
        ExtractionError::Api { status: 404, .. } => "modèle introuvable",
        ExtractionError::RateLimited { .. } => "limite d'appels atteinte",
        ExtractionError::Timeout | ExtractionError::Network(_) => "injoignable",
        _ => "réponse inattendue",
    }
}

/// The owner's chat, from `OWNER_CHAT_ID`; `None` when unset or not a number.
pub fn owner_chat() -> Option<ChatId> {
    std::env::var("OWNER_CHAT_ID")
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
        .map(ChatId)
}

/// Starts the alerts when `OWNER_CHAT_ID` is set: the sender, the database watch, and the
/// restart check. Returns the owner's chat.
pub async fn start(bot: Bot, pool: Arc<PgPool>) -> Option<ChatId> {
    let Some(owner) = owner_chat() else {
        tracing::info!("OWNER_CHAT_ID is not set: no alerts nor weekly report");
        return None;
    };
    let (sender, mut alerts) = mpsc::unbounded_channel();
    if SENDER.set(sender).is_err() {
        return Some(owner);
    }

    let today = super::usage::today();
    match crate::db::usage_summary(&pool, Some(today), today).await {
        Ok(summary) => watch().start_cost(
            u64::try_from(summary.counts.gemini_cost_micro_usd).unwrap_or(0),
            today,
        ),
        Err(error) => tracing::warn!(error = %error, "Could not read today's Gemini cost"),
    }

    tokio::spawn(async move {
        let mut gate = Gate::default();
        while let Some(alert) = alerts.recv().await {
            if !gate.admits(&alert, Instant::now()) {
                continue;
            }
            match bot.send_message(owner, alert.text()).await {
                Ok(_) => tracing::info!(subject = alert.subject(), "Alert sent to the owner"),
                Err(error) => tracing::warn!(error = %error, "Could not send an alert"),
            }
        }
    });
    check_restart(&pool).await;
    tracing::info!("Alerts to the owner started");
    Some(owner)
}

/// Checks the database every minute, alerts on or off: each failed check counts in
/// `ji_errors_total{component="database"}`; a "down" after two in a row.
pub async fn watch_database(pool: Arc<PgPool>) {
    let mut every_minute = tokio::time::interval(Duration::from_secs(60));
    let mut failed = 0u32;
    loop {
        every_minute.tick().await;
        let answered = tokio::time::timeout(
            Duration::from_secs(10),
            sqlx::query("SELECT 1").execute(&*pool),
        )
        .await
        .is_ok_and(|result| result.is_ok());
        if answered {
            if failed >= DATABASE_DOWN_AFTER {
                raise(Alert::DatabaseBack);
            }
            failed = 0;
        } else {
            failed += 1;
            super::metrics::error("database", "unreachable");
            if failed == DATABASE_DOWN_AFTER {
                raise(Alert::DatabaseDown);
            }
        }
    }
}

/// Tells a restart without a new release: Fly's machine and its configuration version are the
/// same as at the previous start (a deployment changes the version, and is notified by the
/// "Deploy to Production" workflow). Off Fly, nothing is checked.
async fn check_restart(pool: &PgPool) {
    let (Ok(machine), Ok(version)) = (
        std::env::var("FLY_MACHINE_ID"),
        std::env::var("FLY_MACHINE_VERSION"),
    ) else {
        return;
    };
    let release = format!("{machine}:{version}");
    match crate::db::app_state(pool, "release").await {
        Ok(Some(previous)) if previous == release => restarted(pool).await,
        Ok(_) => {
            if let Err(error) = crate::db::set_app_state(pool, "release", &release).await {
                tracing::warn!(error = %error, "Could not record the release");
            }
        }
        Err(error) => tracing::warn!(error = %error, "Could not read the previous release"),
    }
}

/// Tells the restart, at most once an hour across restarts: the gate starts empty with each
/// process, and a bot crashing in a loop would otherwise send one each time.
async fn restarted(pool: &PgPool) {
    let now = chrono::Utc::now().timestamp();
    let quiet = i64::try_from(QUIET_FOR.as_secs()).unwrap_or(i64::MAX);
    let last = crate::db::app_state(pool, RESTART_TOLD)
        .await
        .ok()
        .flatten()
        .and_then(|at| at.parse::<i64>().ok());
    if last.is_some_and(|at| now - at < quiet) {
        return;
    }
    raise(Alert::Restarted);
    if let Err(error) = crate::db::set_app_state(pool, RESTART_TOLD, &now.to_string()).await {
        tracing::warn!(error = %error, "Could not record the restart alert");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subject_is_sent_once_an_hour_and_back_only_after_down() {
        let mut gate = Gate::default();
        let start = Instant::now();
        assert!(!gate.admits(&Alert::GeminiBack, start), "no down was sent");
        assert!(gate.admits(&Alert::GeminiDown, start));
        assert!(gate.admits(&Alert::DatabaseDown, start), "another subject");
        assert!(gate.admits(&Alert::GeminiBack, start));
        assert!(!gate.admits(&Alert::GeminiBack, start), "already told");
        let soon = start + Duration::from_secs(600);
        assert!(
            gate.admits(&Alert::GeminiDown, soon),
            "down again after a back: the last message must not say it is back"
        );
        assert!(
            !gate.admits(&Alert::GeminiDown, soon),
            "still down: once an hour"
        );
        assert!(gate.admits(&Alert::GeminiBack, soon));
        assert!(gate.admits(&Alert::Restarted, start));
        assert!(!gate.admits(&Alert::Restarted, soon), "once an hour");
        assert!(gate.admits(&Alert::Restarted, start + QUIET_FOR));
    }

    #[test]
    fn gemini_fails_often_over_30_percent_of_5_calls_in_the_hour() {
        let mut watch = Watch::new();
        let start = Instant::now();
        for _ in 0..3 {
            assert_eq!(watch.call(false, start), None);
        }
        assert_eq!(watch.call(true, start), None, "4 calls only");
        assert_eq!(
            watch.call(true, start),
            Some(Alert::GeminiFailing {
                failures: 2,
                calls: 5
            }),
            "40 %"
        );
        let later = start + FAILING_WINDOW + Duration::from_secs(1);
        assert_eq!(watch.call(true, later), None, "the old calls are forgotten");
    }

    #[test]
    fn no_failing_alert_while_the_breaker_is_open() {
        let mut watch = Watch::new();
        let start = Instant::now();
        for _ in 0..4 {
            watch.call(true, start);
        }
        watch.breaker_open = true;
        assert_eq!(watch.call(true, start), None, "Gemini down is sent instead");
    }

    #[test]
    fn the_daily_cost_is_told_once_a_day() {
        let mut watch = Watch::new();
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap_or_default();
        assert_eq!(watch.cost(600_000, day), None);
        assert_eq!(
            watch.cost(600_000, day),
            Some(Alert::DailyCost {
                micro_usd: 1_200_000
            })
        );
        assert_eq!(watch.cost(600_000, day), None, "once a day");
        let next = day.succ_opt().unwrap_or_default();
        assert_eq!(watch.cost(600_000, next), None, "a new day starts at 0");
        watch.start_cost(1_500_000, next);
        assert_eq!(watch.cost(1, next), None, "told before the restart");
    }

    #[test]
    fn alerts_read_in_french() {
        assert_eq!(
            Alert::DailyCost {
                micro_usd: 1_234_567
            }
            .text(),
            "💸 Coût Gemini du jour : 1,23 $, au-dessus du seuil de 1 $."
        );
        assert!(
            Alert::GeminiFailing {
                failures: 2,
                calls: 5
            }
            .text()
            .contains("2 échecs sur 5 appels")
        );
    }

    #[test]
    fn a_failed_startup_check_says_why_in_a_few_words() {
        use crate::extraction::ExtractionError;
        let api = |status| ExtractionError::Api {
            status,
            message: "the API's own text".to_string(),
        };
        assert_eq!(start_failure_reason(&api(403)), "clé refusée");
        assert_eq!(start_failure_reason(&api(400)), "clé refusée");
        assert_eq!(start_failure_reason(&api(404)), "modèle introuvable");
        assert_eq!(start_failure_reason(&api(500)), "réponse inattendue");
        assert_eq!(
            start_failure_reason(&ExtractionError::Timeout),
            "injoignable"
        );
        assert_eq!(
            start_failure_reason(&ExtractionError::Network("dns".to_string())),
            "injoignable"
        );
        let text = Alert::GeminiFailedAtStart {
            reason: "clé refusée",
        }
        .text();
        assert!(text.contains("(clé refusée)") && text.contains("les photos vont échouer"));
        assert!(!text.contains("the API's own text"));
    }

    #[test]
    fn the_startup_check_alert_has_its_own_subject() {
        let mut gate = Gate::default();
        let now = Instant::now();
        assert!(gate.admits(&Alert::GeminiDown, now));
        let failed = Alert::GeminiFailedAtStart {
            reason: "injoignable",
        };
        assert!(
            gate.admits(&failed, now),
            "not held back by the breaker's alert"
        );
        assert!(!gate.admits(&failed, now), "once an hour");
    }
}

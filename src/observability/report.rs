//! The weekly report sent to the owner on Monday at 9:00, Paris time, and `/stats` (plan O5,
//! `specs/tasks/observability.md`): users, photos, recipes and Gemini's cost from
//! `usage_days`, kept beyond the 15 days of Fly's metrics. In French, the owner's language.

use crate::db::UsageSummary;
use chrono::{Datelike, Days, NaiveDate, NaiveDateTime, NaiveTime};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use teloxide::prelude::*;

/// `app_state` key: the Monday of the last week reported.
const LAST_REPORT: &str = "weekly_report";

/// Micro-USD as dollars with a decimal comma: `dollars(210_000, 2)` is "0,21".
pub fn dollars(micro_usd: u64, decimals: usize) -> String {
    format!("{:.*}", decimals, micro_usd as f64 / 1e6).replace('.', ",")
}

/// "26 oct.", "1er nov.".
pub fn french_date(day: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.",
        "déc.",
    ];
    let month = MONTHS
        .get(day.month0() as usize)
        .copied()
        .unwrap_or_default();
    match day.day() {
        1 => format!("1er {month}"),
        n => format!("{n} {month}"),
    }
}

/// "1 photo lue", "0 photo lue", "2 photos lues".
fn count(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n <= 1 { one } else { many })
}

/// A report: `title`, then users (only the number of users `since_start`, where active users
/// mean nothing), photos and recipes, and Gemini.
pub fn render(title: &str, summary: &UsageSummary, since_start: bool) -> String {
    let counts = &summary.counts;
    let users = count(summary.users, "inscrit", "inscrits");
    let people = if since_start {
        format!("👥 {users}")
    } else {
        let new = match summary.new_users {
            0 => String::new(),
            1 => " (dont 1 nouveau)".to_string(),
            n => format!(" (dont {n} nouveaux)"),
        };
        format!(
            "👥 {}{new} · {users}",
            count(
                summary.active_users,
                "utilisateur actif",
                "utilisateurs actifs"
            )
        )
    };
    let failed = match counts.photos_failed {
        0 => String::new(),
        n => format!(" ({} sans succès)", n),
    };
    let cost = u64::try_from(counts.gemini_cost_micro_usd).unwrap_or(0);
    let per_photo = u64::try_from(counts.photos_read)
        .ok()
        .filter(|&photos| photos > 0)
        .map(|photos| format!(" ({} $ par photo)", dollars(cost / photos, 3)))
        .unwrap_or_default();
    format!(
        "📊 {title}\n{people}\n📸 {}{failed} · 🍲 {}\n💸 Gemini : {} ${per_photo} · {} sur {}",
        count(counts.photos_read, "photo lue", "photos lues"),
        count(counts.recipes_created, "recette créée", "recettes créées"),
        dollars(cost, 2),
        count(counts.gemini_failures, "échec", "échecs"),
        count(counts.gemini_calls, "appel", "appels"),
    )
}

/// The Monday of the week of `now` (Paris time) once its report is due, at 9:00 that Monday.
fn report_week(now: NaiveDateTime) -> Option<NaiveDate> {
    let monday = now
        .date()
        .checked_sub_days(Days::new(u64::from(now.weekday().num_days_from_monday())))?;
    let nine = NaiveTime::from_hms_opt(9, 0, 0)?;
    (now >= monday.and_time(nine)).then_some(monday)
}

/// The report of the week before `monday`.
async fn weekly(pool: &PgPool, monday: NaiveDate) -> anyhow::Result<String> {
    let from = monday
        .checked_sub_days(Days::new(7))
        .ok_or_else(|| anyhow::anyhow!("no week before {monday}"))?;
    let to = monday
        .pred_opt()
        .ok_or_else(|| anyhow::anyhow!("no day before {monday}"))?;
    let summary = crate::db::usage_summary(pool, Some(from), to).await?;
    let title = format!("Semaine du {} au {}", french_date(from), french_date(to));
    Ok(render(&title, &summary, false))
}

/// Sends the weekly report when due, checking every 5 minutes: on Monday from 9:00, or as soon
/// as the bot runs again if it was down then. The first start only records the current week.
pub async fn run(bot: Bot, pool: Arc<PgPool>, owner: ChatId) {
    let mut every = tokio::time::interval(Duration::from_secs(300));
    loop {
        every.tick().await;
        let now = crate::bot::ui_builder::paris_time(chrono::Utc::now());
        let Some(monday) = report_week(now) else {
            continue;
        };
        let week = monday.to_string();
        match crate::db::app_state(&pool, LAST_REPORT).await {
            Ok(Some(sent)) if sent == week => continue,
            Ok(Some(_)) => {}
            Ok(None) => {
                if let Err(error) = crate::db::set_app_state(&pool, LAST_REPORT, &week).await {
                    tracing::warn!(error = %error, "Could not record the report week");
                }
                continue;
            }
            Err(error) => {
                tracing::warn!(error = %error, "Could not read the last weekly report");
                continue;
            }
        }
        // The counts of the last minute too
        super::usage::flush(&pool).await;
        let text = match weekly(&pool, monday).await {
            Ok(text) => text,
            Err(error) => {
                tracing::warn!(error = %error, "Could not build the weekly report");
                continue;
            }
        };
        match bot.send_message(owner, text).await {
            Ok(_) => {
                tracing::info!(week = %week, "Weekly report sent to the owner");
                if let Err(error) = crate::db::set_app_state(&pool, LAST_REPORT, &week).await {
                    tracing::warn!(error = %error, "Could not record the weekly report");
                }
            }
            Err(error) => tracing::warn!(error = %error, "Could not send the weekly report"),
        }
    }
}

/// `/stats`: the last 7 and 30 days, and since the history started.
pub async fn stats(pool: &PgPool) -> anyhow::Result<String> {
    super::usage::flush(pool).await;
    let today = super::usage::today();
    let mut blocks = Vec::new();
    for (title, days) in [("7 derniers jours", 7), ("30 derniers jours", 30)] {
        let from = today.checked_sub_days(Days::new(days - 1));
        let summary = crate::db::usage_summary(pool, from, today).await?;
        blocks.push(render(title, &summary, false));
    }
    let summary = crate::db::usage_summary(pool, None, today).await?;
    blocks.push(render("Depuis le début du suivi", &summary, true));
    Ok(blocks.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::UsageCounts;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap_or_default()
    }

    #[test]
    fn the_report_reads_like_the_mockup() {
        let summary = UsageSummary {
            active_users: 5,
            new_users: 1,
            users: 7,
            counts: UsageCounts {
                photos_read: 23,
                photos_failed: 2,
                recipes_created: 19,
                gemini_calls: 25,
                gemini_failures: 0,
                gemini_cost_micro_usd: 210_000,
            },
        };
        let title = format!(
            "Semaine du {} au {}",
            french_date(date(2026, 10, 26)),
            french_date(date(2026, 11, 1))
        );
        assert_eq!(
            render(&title, &summary, false),
            "📊 Semaine du 26 oct. au 1er nov.\n\
             👥 5 utilisateurs actifs (dont 1 nouveau) · 7 inscrits\n\
             📸 23 photos lues (2 sans succès) · 🍲 19 recettes créées\n\
             💸 Gemini : 0,21 $ (0,009 $ par photo) · 0 échec sur 25 appels"
        );
        let empty = render("Depuis le début du suivi", &UsageSummary::default(), true);
        assert_eq!(
            empty,
            "📊 Depuis le début du suivi\n👥 0 inscrit\n📸 0 photo lue · 🍲 0 recette créée\n\
             💸 Gemini : 0,00 $ · 0 échec sur 0 appel"
        );
    }

    #[test]
    fn the_report_is_due_on_monday_from_nine() {
        let at = |d: NaiveDate, h: u32, m: u32| {
            d.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default())
        };
        let monday = date(2026, 11, 2);
        assert_eq!(
            report_week(at(monday, 8, 59)),
            None,
            "the week before is due last Monday, already sent"
        );
        assert_eq!(report_week(at(monday, 9, 0)), Some(monday));
        assert_eq!(report_week(at(date(2026, 11, 8), 23, 0)), Some(monday));
        assert_eq!(
            report_week(at(date(2026, 11, 9), 9, 5)),
            Some(date(2026, 11, 9))
        );
    }
}

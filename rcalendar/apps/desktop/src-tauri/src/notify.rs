//! Reminder delivery: computes when each reminder should fire and posts native
//! notifications from a background scheduler that keeps running while the main
//! window is open-less (the process stays alive — full "launch at login" is
//! Tier 1.44). Handles catch-up after sleep, permission denial, and snooze.

use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::{NotificationExt, PermissionState};

use calendar_core::model::{Event, Reminder, TimeRange};
use calendar_core::recurrence::expand;

use crate::store::{ReminderDelivery, SqliteStore};

/// Default minutes-since-midnight at which all-day reminders are anchored (9:00).
const DEFAULT_DAY_START_MINUTES: i64 = 9 * 60;

/// The lookback window (in days) over which recurring occurrences are replayed
/// for catch-up after the app was asleep.
const CATCHUP_DAYS: i64 = 7;

/// The scheduler cadence in seconds.
const POLL_INTERVAL_SECS: u64 = 15;

/// Computes the instant a reminder should fire for a given occurrence start.
///
/// - Absolute reminders fire at their absolute time regardless of recurrence.
/// - Offset reminders fire `offset_minutes` relative to the occurrence start.
/// - All-day events anchor offset math to the configured day-start (e.g. "9am
///   on the day") rather than midnight — Tier 1.5.
pub fn trigger_at(
    reminder: &Reminder,
    event: &Event,
    occurrence_start: DateTime<Utc>,
    day_start_minutes: i64,
) -> Option<DateTime<Utc>> {
    if let Some(abs) = reminder.absolute_at {
        return Some(abs);
    }
    let offset = reminder.offset_minutes?;

    let base = if event.all_day {
        day_start_base(occurrence_start, event.tz.as_deref(), day_start_minutes)?
    } else {
        occurrence_start
    };

    base.checked_add_signed(Duration::minutes(offset))
}

/// Returns local `day_start_minutes` on the occurrence's date, resolved in the
/// event's IANA zone (or UTC when the event is floating).
fn day_start_base(
    occurrence_start: DateTime<Utc>,
    tz: Option<&str>,
    day_start_minutes: i64,
) -> Option<DateTime<Utc>> {
    let hour = day_start_minutes / 60;
    let minute = day_start_minutes % 60;

    match tz {
        Some(id) => match id.parse::<chrono_tz::Tz>() {
            Ok(zone) => {
                let local = occurrence_start.with_timezone(&zone);
                let naive = local
                    .date_naive()
                    .and_hms_opt(hour as u32, minute as u32, 0)?;
                zone.from_local_datetime(&naive)
                    .earliest()
                    .map(|dt| dt.with_timezone(&Utc))
            }
            Err(_) => utc_day_start_base(occurrence_start, hour, minute),
        },
        None => utc_day_start_base(occurrence_start, hour, minute),
    }
}

fn utc_day_start_base(
    occurrence_start: DateTime<Utc>,
    hour: i64,
    minute: i64,
) -> Option<DateTime<Utc>> {
    let naive = occurrence_start
        .date_naive()
        .and_hms_opt(hour as u32, minute as u32, 0)?;
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

/// Spawns a background thread that polls for due reminders forever.
pub fn spawn_scheduler(app: AppHandle, store: Arc<SqliteStore>) {
    std::thread::spawn(move || loop {
        poll_once(&app, &store);
        std::thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS));
    });
}

/// One scheduler pass: ensure permission, then deliver due reminders.
fn poll_once(app: &AppHandle, store: &SqliteStore) {
    if !ensure_permission(app) {
        return;
    }
    let now = Utc::now();
    let day_start = day_start_minutes(store);

    let deliveries = match store.list_due_reminders() {
        Ok(d) => d,
        Err(err) => {
            eprintln!("[notify] failed to list reminders: {err}");
            return;
        }
    };

    for delivery in deliveries {
        deliver(app, store, &delivery, now, day_start);
    }
}

/// Requests notification permission if it hasn't been resolved yet.
fn ensure_permission(app: &AppHandle) -> bool {
    let notification = app.notification();
    match notification.permission_state() {
        Ok(PermissionState::Granted) => true,
        Ok(PermissionState::Prompt) | Ok(PermissionState::PromptWithRationale) => {
            matches!(
                notification.request_permission(),
                Ok(PermissionState::Granted)
            )
        }
        _ => false,
    }
}

fn day_start_minutes(store: &SqliteStore) -> i64 {
    store
        .get_setting("day_start_minutes")
        .ok()
        .flatten()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_DAY_START_MINUTES)
}

/// Delivers a single reminder if it is due and hasn't been delivered yet.
fn deliver(
    app: &AppHandle,
    store: &SqliteStore,
    delivery: &ReminderDelivery,
    now: DateTime<Utc>,
    day_start: i64,
) {
    // A snoozed reminder re-fires once `snooze_until` passes.
    if let Some(until) = delivery.snooze_until {
        if until <= now {
            fire(app, delivery);
            let _ = store.mark_reminder_delivered(delivery.reminder.id, now);
        }
        return;
    }

    for occurrence_start in occurrence_starts(&delivery.event, now) {
        let Some(trigger) = trigger_at(
            &delivery.reminder,
            &delivery.event,
            occurrence_start,
            day_start,
        ) else {
            continue;
        };
        // Already delivered for this (or a later) trigger.
        if delivery.delivered_at.is_some_and(|da| trigger <= da) {
            continue;
        }
        if trigger <= now {
            fire(app, delivery);
            let _ = store.mark_reminder_delivered(delivery.reminder.id, trigger);
            return;
        }
    }
}

/// The occurrence start times to consider, in chronological order.
fn occurrence_starts(event: &Event, now: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    if event.rrule.as_ref().is_some_and(|r| !r.trim().is_empty()) {
        let lookback = now - Duration::days(CATCHUP_DAYS);
        let lookahead = now + Duration::hours(2);
        let Ok(range) = TimeRange::new(lookback, lookahead) else {
            return Vec::new();
        };
        expand(event, &range)
            .map(|occs| occs.into_iter().map(|o| o.starts_at).collect())
            .unwrap_or_else(|_| match event.starts_at < range.end {
                true => vec![event.starts_at],
                false => Vec::new(),
            })
    } else {
        vec![event.starts_at]
    }
}

/// Posts the notification and notifies the frontend (for in-app snooze/toasts).
fn fire(app: &AppHandle, delivery: &ReminderDelivery) {
    let event = &delivery.event;
    let title = if event.title.is_empty() {
        "Reminder".to_string()
    } else {
        event.title.clone()
    };
    let body = if event.all_day {
        "All-day event".to_string()
    } else {
        format!("Starts at {}", event.starts_at.format("%H:%M UTC"))
    };

    let _ = app
        .notification()
        .builder()
        .title(title.clone())
        .body(body)
        .extra("event_id", event.id.to_string())
        .show();

    let payload = serde_json::json!({
        "eventId": event.id.to_string(),
        "title": title,
        "reminderId": delivery.reminder.id.to_string(),
    });
    let _ = app.emit("almanac://reminder-fired", payload);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn dt(s: &str) -> DateTime<Utc> {
        Utc.from_utc_datetime(&s.parse().unwrap())
    }

    fn reminder(offset_minutes: Option<i64>, absolute_at: Option<DateTime<Utc>>) -> Reminder {
        Reminder {
            id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
            offset_minutes,
            absolute_at,
            created_at: dt("2026-08-01T00:00:00"),
            updated_at: dt("2026-08-01T00:00:00"),
            deleted_at: None,
        }
    }

    fn event(all_day: bool, tz: Option<&str>, starts_at: &str) -> Event {
        Event {
            id: Uuid::new_v4(),
            calendar_id: Uuid::new_v4(),
            uid: "test@example.com".into(),
            title: "Standup".into(),
            location: None,
            notes: None,
            starts_at: dt(starts_at),
            ends_at: dt(starts_at) + Duration::hours(1),
            all_day,
            tz: tz.map(String::from),
            rrule: None,
            exdates: vec![],
            travel_time_minutes: None,
            color: None,
            etag: None,
            attendees: vec![],
            busy: true,
            updated_at: dt("2026-08-01T00:00:00"),
            created_at: dt("2026-08-01T00:00:00"),
            deleted_at: None,
        }
    }

    #[test]
    fn offset_before_timed_event() {
        let r = reminder(Some(-10), None);
        let e = event(false, None, "2026-08-13T15:00:00");
        assert_eq!(
            trigger_at(&r, &e, dt("2026-08-13T15:00:00"), 540),
            Some(dt("2026-08-13T14:50:00"))
        );
    }

    #[test]
    fn absolute_ignores_offset() {
        let r = reminder(Some(-10), Some(dt("2026-08-13T09:00:00")));
        let e = event(false, None, "2026-08-13T15:00:00");
        assert_eq!(
            trigger_at(&r, &e, dt("2026-08-13T15:00:00"), 540),
            Some(dt("2026-08-13T09:00:00"))
        );
    }

    #[test]
    fn all_day_anchors_to_day_start_not_midnight() {
        // Midday day start: "1 day before" an all-day event on the 13th -> noon on the 12th.
        let r = reminder(Some(-1440), None);
        let e = event(true, None, "2026-08-13T00:00:00");
        assert_eq!(
            trigger_at(&r, &e, dt("2026-08-13T00:00:00"), 720),
            Some(dt("2026-08-12T12:00:00"))
        );
    }

    #[test]
    fn all_day_zero_offset_fires_at_day_start() {
        let r = reminder(Some(0), None);
        let e = event(true, None, "2026-08-13T00:00:00");
        assert_eq!(
            trigger_at(&r, &e, dt("2026-08-13T00:00:00"), 540),
            Some(dt("2026-08-13T09:00:00"))
        );
    }

    #[test]
    fn missing_trigger_is_none() {
        let r = reminder(None, None);
        let e = event(false, None, "2026-08-13T15:00:00");
        assert_eq!(trigger_at(&r, &e, dt("2026-08-13T15:00:00"), 540), None);
    }
}

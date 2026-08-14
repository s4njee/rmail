//! The domain model: one schema shared by every layer.
//!
//! Shapes match `plan.md` §5 and the design handoff's suggested Rust surface.
//! Stored entities (`Account`, `Calendar`, `Event`, `Reminder`, `Task`) carry a
//! UUIDv4 id plus `created_at` / `updated_at` / `deleted_at` for the
//! sync-friendly write path (S2.2). `Occurrence` is computed by expansion, never
//! stored. All types are `serde`-serializable so they cross the Tauri seam as
//! typed JSON.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};

/// A half-open time window `[start, end)` used for range queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl TimeRange {
    /// Creates a range, rejecting empty/inverted windows.
    pub fn new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self> {
        if end <= start {
            return Err(Error::InvalidEvent(
                "time range must have end after start".into(),
            ));
        }
        Ok(Self { start, end })
    }

    /// Whether `dt` falls inside `[start, end)`.
    pub fn contains(&self, dt: DateTime<Utc>) -> bool {
        dt >= self.start && dt < self.end
    }

    /// Whether an interval `[starts_at, ends_at)` overlaps this range.
    pub fn overlaps(&self, starts_at: DateTime<Utc>, ends_at: DateTime<Utc>) -> bool {
        starts_at < self.end && ends_at > self.start
    }
}

/// Where an account's data lives. `Local` is the on-device store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Local,
    Google,
    Caldav,
}

/// Backend state of an account, mirrored from the design's `syncStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    Idle,
    Syncing,
    Error,
}

/// A syncable data source (local store, Google, CalDAV/iCloud).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub kind: AccountKind,
    pub display_name: String,
    /// e.g. `caldav.icloud.com · casey@icloud.com`, or the local store label.
    pub detail: String,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub status: AccountStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// A color-coded calendar owned by an account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Calendar {
    pub id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    /// Hex color, e.g. `#C2410C`. Matches the design's calendar palette.
    pub color: String,
    /// Whether the calendar shows in views (sidebar toggle).
    pub enabled: bool,
    /// Number of events, for the sidebar count.
    pub event_count: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// A calendar event. Times are stored in UTC; `tz` names the IANA zone the
/// event is anchored to (absent = floating, rendered as-is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub calendar_id: Uuid,
    /// iCal UID — stable across sync/import; distinct from the row `id`.
    pub uid: String,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    /// IANA timezone id (e.g. `America/New_York`) when the event is tz-bound.
    pub tz: Option<String>,
    /// Raw RFC 5545 `RRULE` value, e.g. `FREQ=WEEKLY;BYDAY=MO,WE,FR`.
    pub rrule: Option<String>,
    /// Occurrence dates excluded from expansion (per-instance cancellations).
    pub exdates: Vec<NaiveDate>,
    /// Server/sync change tag (Google Calendar `etag`); absent for local events.
    pub etag: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Event {
    /// Rejects events whose times are structurally invalid:
    /// `ends_at < starts_at`, or a zero-length timed event. Recurrence-specific
    /// validity (the RRULE string) is checked during expansion.
    pub fn validate(&self) -> Result<()> {
        if self.ends_at < self.starts_at {
            return Err(Error::InvalidEvent("event ends before it starts".into()));
        }
        if !self.all_day && self.ends_at == self.starts_at {
            return Err(Error::InvalidEvent(
                "timed event must have a non-zero duration".into(),
            ));
        }
        Ok(())
    }
}

/// An expanded instance of an event, produced by [`crate::recurrence::expand`].
/// Computed on demand — never persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Occurrence {
    pub event_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
}

/// A reminder for an event: either `offset_minutes` before start or an
/// `absolute_at` instant — never both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: Uuid,
    pub event_id: Uuid,
    /// Minutes relative to the event start (negative = before).
    pub offset_minutes: Option<i64>,
    pub absolute_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Reminder {
    /// Requires exactly one trigger kind.
    pub fn validate(&self) -> Result<()> {
        match (self.offset_minutes, self.absolute_at) {
            (None, None) => Err(Error::InvalidEvent(
                "reminder needs an offset or an absolute time".into(),
            )),
            (Some(_), Some(_)) => Err(Error::InvalidEvent(
                "reminder cannot have both an offset and an absolute time".into(),
            )),
            _ => Ok(()),
        }
    }
}

/// A to-do item shown in the sidebar and Agenda's "Events + tasks" mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub calendar_id: Uuid,
    pub title: String,
    pub due_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Everything needed to create or update an event, as supplied by the editor
/// (and later the `save_event` Tauri command). The id/uid/timestamps are owned
/// by the store, not the form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventDraft {
    pub calendar_id: Uuid,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub all_day: bool,
    pub tz: Option<String>,
    pub rrule: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};

    fn dt(s: &str) -> DateTime<Utc> {
        Utc.from_utc_datetime(&s.parse().unwrap())
    }

    fn sample_event() -> Event {
        Event {
            id: Uuid::new_v4(),
            calendar_id: Uuid::new_v4(),
            uid: "e1@example.com".into(),
            title: "Standup".into(),
            location: None,
            notes: None,
            starts_at: dt("2026-08-10T15:00:00"),
            ends_at: dt("2026-08-10T15:30:00"),
            all_day: false,
            tz: None,
            rrule: None,
            exdates: vec![],
            etag: None,
            updated_at: dt("2026-08-10T15:00:00"),
            created_at: dt("2026-08-10T15:00:00"),
            deleted_at: None,
        }
    }

    #[test]
    fn rejects_end_before_start() {
        let mut event = sample_event();
        event.ends_at = event.starts_at - Duration::minutes(1);
        assert!(event.validate().is_err());
    }

    #[test]
    fn rejects_zero_length_timed_event() {
        let mut event = sample_event();
        event.ends_at = event.starts_at;
        assert!(event.validate().is_err());
    }

    #[test]
    fn allows_zero_length_all_day_event() {
        let mut event = sample_event();
        event.all_day = true;
        event.starts_at = dt("2026-08-13T00:00:00");
        event.ends_at = event.starts_at; // all-day: end may equal start (single day)
        assert!(event.validate().is_ok());
    }

    #[test]
    fn reminder_requires_exactly_one_trigger() {
        let mut reminder = Reminder {
            id: Uuid::new_v4(),
            event_id: Uuid::new_v4(),
            offset_minutes: None,
            absolute_at: None,
            created_at: dt("2026-08-10T15:00:00"),
            updated_at: dt("2026-08-10T15:00:00"),
            deleted_at: None,
        };
        assert!(reminder.validate().is_err(), "no trigger");

        reminder.offset_minutes = Some(-10);
        assert!(reminder.validate().is_ok());

        reminder.absolute_at = Some(dt("2026-08-10T14:50:00"));
        assert!(reminder.validate().is_err(), "both triggers");
    }

    #[test]
    fn time_range_rejects_inverted_window() {
        assert!(TimeRange::new(dt("2026-08-10T10:00:00"), dt("2026-08-10T09:00:00")).is_err());
        assert!(TimeRange::new(dt("2026-08-10T09:00:00"), dt("2026-08-10T10:00:00")).is_ok());
    }

    #[test]
    fn time_range_overlap_is_half_open() {
        let range = TimeRange::new(dt("2026-08-10T09:00:00"), dt("2026-08-10T17:00:00")).unwrap();
        assert!(range.overlaps(dt("2026-08-10T08:00:00"), dt("2026-08-10T10:00:00")));
        assert!(range.overlaps(dt("2026-08-10T16:00:00"), dt("2026-08-10T18:00:00")));
        assert!(!range.overlaps(dt("2026-08-10T17:00:00"), dt("2026-08-10T18:00:00")));
        assert!(!range.overlaps(dt("2026-08-10T08:00:00"), dt("2026-08-10T09:00:00")));
    }

    #[test]
    fn model_round_trips_through_serde_json() {
        let event = sample_event();
        let json = serde_json::to_string(&event).unwrap();
        let back: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, back);
    }
}

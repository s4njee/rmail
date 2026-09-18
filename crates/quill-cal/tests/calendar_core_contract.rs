//! Compile-time contract between Quill's calendar adapter and `calendar-core`.
//!
//! This intentionally constructs the upstream public types field-by-field.
//! Adding or changing a required field in the embedded calendar workspace must
//! therefore fail in this focused test target instead of surfacing later as an
//! unrelated mail-app build failure.

use chrono::{Duration, NaiveDate, Utc};
use quill_cal::core::model::Event;
use quill_cal::core::recurrence::{edit_occurrence, EditScope, OccurrenceChanges};
use uuid::Uuid;

#[test]
fn calendar_core_event_and_recurrence_contract_compiles() {
    let starts_at = Utc::now();
    let event = Event {
        id: Uuid::new_v4(),
        calendar_id: Uuid::nil(),
        uid: "quill-contract@example.invalid".into(),
        title: "Calendar contract".into(),
        location: None,
        notes: None,
        starts_at,
        ends_at: starts_at + Duration::hours(1),
        all_day: false,
        tz: Some("UTC".into()),
        rrule: Some("FREQ=DAILY;COUNT=2".into()),
        exdates: vec![],
        travel_time_minutes: None,
        color: None,
        etag: None,
        attendees: vec![],
        busy: true,
        updated_at: starts_at,
        created_at: starts_at,
        deleted_at: None,
    };
    let changes = OccurrenceChanges {
        starts_at: event.starts_at,
        ends_at: event.ends_at,
        all_day: event.all_day,
        title: None,
        location: None,
        notes: None,
        rrule: None,
        tz: None,
        travel_time_minutes: None,
        color: None,
    };

    let target = NaiveDate::from_ymd_opt(2026, 9, 18).expect("valid contract date");
    let edited = edit_occurrence(&event, EditScope::All, target, &changes)
        .expect("the contract fixture is valid");
    assert_eq!(edited.len(), 1);
}

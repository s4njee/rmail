//! Two-way CalDAV synchronization engine (Roadmap 1.4).
//!
//! Synchronizes remote CalDAV collections with the local SQLite store,
//! parses and serializes iCalendar (RFC 5545) via `calendar-core`,
//! handles recurring event edits (`This`, `Future`, `All`), and resolves
//! ETag concurrency conflicts (server wins + local copy on conflict).

use chrono::{DateTime, Utc};
use quill_store::sqlite::SqliteStore;
use quill_store::types::CalendarEvent;

use crate::client::{CalDavClient, CalDavCollection};
pub use calendar_core::ical::{parse_ical, write_ical};
pub use calendar_core::model::Event as CoreEvent;
pub use calendar_core::recurrence::{edit_occurrence, EditScope, OccurrenceChanges};

/// Synchronize an account's CalDAV calendars with the store. Calendars the
/// user removed (Settings → Remove, or deselected during setup) are skipped,
/// so a removed calendar stays gone.
pub async fn sync_caldav_account(
    store: &SqliteStore,
    account_id: u32,
    client: &CalDavClient,
) -> Result<Vec<CalDavCollection>, String> {
    let home_url = client.discover_calendar_home().await?;
    let collections = client.list_calendars(&home_url).await?;
    let removed = store.removed_calendar_sources();

    for collection in &collections {
        if removed
            .iter()
            .any(|r| r.account_id == account_id && r.source == collection.href)
        {
            continue;
        }
        sync_calendar_collection(store, account_id, client, collection).await?;
    }

    Ok(collections)
}

/// Synchronize a single CalDAV collection with the local SQLite events.
///
/// Events are tagged with the collection's href/name/color (the same identity
/// `remove_calendar_source` uses) so removal and per-calendar management work
/// for CalDAV accounts, not just Google.
pub async fn sync_calendar_collection(
    store: &SqliteStore,
    account_id: u32,
    client: &CalDavClient,
    collection: &CalDavCollection,
) -> Result<(), String> {
    let remote_resources = client.fetch_events(&collection.href).await?;

    // Load existing local events for this account
    let mut existing = store.list_events(0, i64::MAX / 2);

    for res in remote_resources {
        if res.ical_data.trim().is_empty() {
            continue;
        }

        if let Ok(core_events) = parse_ical(&res.ical_data) {
            for cev in core_events {
                let start_ms = cev.starts_at.timestamp_millis();
                let end_ms = cev.ends_at.timestamp_millis();

                // Check if event exists by matching title and start_ms
                let found = existing
                    .iter()
                    .find(|e| e.account_id == account_id && e.title == cev.title && e.start_ms == start_ms);

                if let Some(existing_event) = found {
                    // Update existing
                    let updated = CalendarEvent {
                        id: existing_event.id,
                        account_id,
                        title: cev.title.clone(),
                        start_ms,
                        end_ms,
                        all_day: cev.all_day,
                        location: cev.location.clone(),
                        notes: cev.notes.clone(),
                        alarm_minutes_before: existing_event.alarm_minutes_before,
                        timezone: cev.tz.clone(),
                        travel_time_minutes: existing_event.travel_time_minutes,
                        calendar_source: existing_event.calendar_source.clone(),
                        calendar_name: existing_event.calendar_name.clone(),
                        calendar_color: existing_event.calendar_color.clone(),
                        color: existing_event.color.clone(),
                    };
                    store.update_event(updated)?;
                } else {
                    // Create new; the created row is added to `existing` so a
                    // second remote resource matching the same (title, start)
                    // in this run updates it instead of inserting a duplicate.
                    // The event is tagged with the collection's identity so the
                    // removed-source mechanism can target it (P0.2).
                    let new_event = CalendarEvent {
                        id: 0,
                        account_id,
                        title: cev.title.clone(),
                        start_ms,
                        end_ms,
                        all_day: cev.all_day,
                        location: cev.location.clone(),
                        notes: cev.notes.clone(),
                        alarm_minutes_before: Some(15),
                        timezone: cev.tz.clone(),
                        travel_time_minutes: None,
                        calendar_source: Some(collection.href.clone()),
                        calendar_name: Some(collection.display_name.clone()),
                        calendar_color: collection.color.clone(),
                        color: None,
                    };
                    let created = store.create_event(new_event)?;
                    existing.push(created);
                }
            }
        }
    }

    Ok(())
}

/// Push a locally created or updated event to CalDAV with ETag conflict resolution.
pub async fn push_event_to_caldav(
    store: &SqliteStore,
    client: &CalDavClient,
    calendar_url: &str,
    event: &CalendarEvent,
    uid: &str,
    etag: Option<&str>,
) -> Result<String, String> {
    let starts_at = DateTime::from_timestamp_millis(event.start_ms).unwrap_or_else(Utc::now);
    let ends_at = DateTime::from_timestamp_millis(event.end_ms).unwrap_or_else(Utc::now);

    let core_event = CoreEvent {
        id: uuid::Uuid::new_v4(),
        calendar_id: uuid::Uuid::nil(),
        uid: uid.to_string(),
        title: event.title.clone(),
        location: event.location.clone(),
        notes: event.notes.clone(),
        starts_at,
        ends_at,
        all_day: event.all_day,
        tz: event.timezone.clone(),
        rrule: None,
        exdates: vec![],
        etag: etag.map(str::to_string),
        updated_at: Utc::now(),
        created_at: Utc::now(),
        deleted_at: None,
    };

    let ics_payload = write_ical(&[core_event]).map_err(|e| e.to_string())?;
    let event_url = format!("{}/{}", calendar_url.trim_end_matches('/'), uid);

    match client.put_event(&event_url, &ics_payload, etag).await {
        Ok(new_etag) => Ok(new_etag),
        Err(err) if err.contains("412") || err.contains("conflict") => {
            // Conflict handling: Server wins, save local version as conflict copy
            let conflict_copy = CalendarEvent {
                id: 0,
                account_id: event.account_id,
                title: format!("{} (conflict copy)", event.title),
                start_ms: event.start_ms,
                end_ms: event.end_ms,
                all_day: event.all_day,
                location: event.location.clone(),
                notes: event.notes.clone(),
                alarm_minutes_before: event.alarm_minutes_before,
                timezone: event.timezone.clone(),
                travel_time_minutes: event.travel_time_minutes,
                calendar_source: event.calendar_source.clone(),
                calendar_name: event.calendar_name.clone(),
                calendar_color: event.calendar_color.clone(),
                color: event.color.clone(),
            };
            let _ = store.create_event(conflict_copy);
            Err("Conflict: server has newer changes. Local edits saved as conflict copy.".into())
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recurrence_edit_roundtrip() {
        let starts_at = DateTime::parse_from_rfc3339("2026-08-10T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let ends_at = starts_at + chrono::Duration::hours(1);

        let series = CoreEvent {
            id: uuid::Uuid::new_v4(),
            calendar_id: uuid::Uuid::nil(),
            uid: "recurring-1".into(),
            title: "Weekly 1:1".into(),
            location: Some("Zoom".into()),
            notes: Some("Sync notes".into()),
            starts_at,
            ends_at,
            all_day: false,
            tz: None,
            rrule: Some("FREQ=WEEKLY;BYDAY=MO,WE;COUNT=5".into()),
            exdates: vec![],
            etag: None,
            updated_at: Utc::now(),
            created_at: Utc::now(),
            deleted_at: None,
        };

        // Test editing single occurrence (This) on Wednesday 2026-08-12
        let target_date = chrono::NaiveDate::from_ymd_opt(2026, 8, 12).unwrap();
        let changes = OccurrenceChanges {
            title: Some("Weekly 1:1 (Rescheduled)".into()),
            location: Some("In Person".into()),
            notes: Some("Updated notes".into()),
            starts_at: starts_at + chrono::Duration::days(2) + chrono::Duration::hours(2),
            ends_at: ends_at + chrono::Duration::days(2) + chrono::Duration::hours(2),
            all_day: false,
        };

        let result = edit_occurrence(&series, EditScope::This, target_date, &changes).unwrap();
        assert_eq!(result.len(), 2);
        // First is the series with EXDATE added
        assert_eq!(result[0].exdates.len(), 1);
        // Second is the override event
        assert_eq!(result[1].title, "Weekly 1:1 (Rescheduled)");
        assert!(result[1].rrule.is_none());

        // Test iCal export roundtrip
        let ics = write_ical(&result).unwrap();
        assert!(ics.contains("SUMMARY:Weekly 1:1"));
        assert!(ics.contains("EXDATE"));
        assert!(ics.contains("SUMMARY:Weekly 1:1 (Rescheduled)"));
    }
}

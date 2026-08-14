//! Google Calendar API v3 data structures and domain mapping (S6.5).
//!
//! Provides pure JSON mapping between Google Calendar API representations
//! and `calendar-core` domain models with zero network or Tauri dependencies.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::model::Event;

/// A Google Calendar list response (`GET https://www.googleapis.com/calendar/v3/users/me/calendarList`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleCalendarList {
    #[serde(default)]
    pub items: Vec<GoogleCalendarEntry>,
    #[serde(rename = "nextSyncToken", default)]
    pub next_sync_token: Option<String>,
}

/// A calendar entry from Google Calendar API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleCalendarEntry {
    pub id: String,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "backgroundColor", default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub primary: Option<bool>,
}

/// An events list response from Google Calendar API (`GET https://www.googleapis.com/calendar/v3/calendars/{id}/events`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleEventList {
    #[serde(default)]
    pub items: Vec<GoogleEvent>,
    #[serde(rename = "nextPageToken", default)]
    pub next_page_token: Option<String>,
    #[serde(rename = "nextSyncToken", default)]
    pub next_sync_token: Option<String>,
}

/// An individual Google Calendar Event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleEvent {
    pub id: String,
    #[serde(default)]
    pub status: Option<String>, // "confirmed" | "tentative" | "cancelled"
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub start: Option<GoogleEventDateTime>,
    #[serde(default)]
    pub end: Option<GoogleEventDateTime>,
    #[serde(default)]
    pub recurrence: Option<Vec<String>>, // e.g. ["RRULE:FREQ=WEEKLY;BYDAY=MO,WE,FR"]
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub updated: Option<DateTime<Utc>>,
}

/// Date / DateTime specifier for Google Calendar events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoogleEventDateTime {
    #[serde(rename = "dateTime", default)]
    pub date_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub date: Option<NaiveDate>,
    #[serde(rename = "timeZone", default)]
    pub time_zone: Option<String>,
}

/// Parses a Google Calendar events API JSON response into a list of domain [`Event`]s.
///
/// Uses resilient parsing on each item to ensure that unexpected or non-standard
/// fields on one event do not prevent other events from being imported.
pub fn parse_gcal_events_json(json_str: &str, calendar_id: Uuid) -> Vec<Event> {
    let root: Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let items = match root.get("items").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    let now = Utc::now();
    let mut events = Vec::new();

    for item in items {
        let is_cancelled = item.get("status").and_then(|s| s.as_str()) == Some("cancelled");
        let uid = item
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string();
        if uid.is_empty() {
            continue;
        }

        let title = item
            .get("summary")
            .and_then(|s| s.as_str())
            .unwrap_or("(Untitled)")
            .to_string();
        let location = item
            .get("location")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());
        let notes = item
            .get("description")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());

        let start_obj = item.get("start");
        let (starts_at, all_day) = if let Some(so) = start_obj {
            if let Some(dt_str) = so.get("dateTime").and_then(|d| d.as_str()) {
                match DateTime::parse_from_rfc3339(dt_str) {
                    Ok(dt) => (dt.with_timezone(&Utc), false),
                    Err(_) => continue,
                }
            } else if let Some(d_str) = so.get("date").and_then(|d| d.as_str()) {
                match NaiveDate::parse_from_str(d_str, "%Y-%m-%d") {
                    Ok(nd) => {
                        let naive = match nd.and_hms_opt(0, 0, 0) {
                            Some(t) => t,
                            None => continue,
                        };
                        (DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc), true)
                    }
                    Err(_) => continue,
                }
            } else {
                continue;
            }
        } else {
            continue;
        };

        let end_obj = item.get("end");
        let ends_at = if let Some(eo) = end_obj {
            if let Some(dt_str) = eo.get("dateTime").and_then(|d| d.as_str()) {
                match DateTime::parse_from_rfc3339(dt_str) {
                    Ok(dt) => {
                        let mut ut = dt.with_timezone(&Utc);
                        if ut <= starts_at && !all_day {
                            ut = starts_at + chrono::Duration::minutes(30);
                        }
                        ut
                    }
                    Err(_) => {
                        if all_day {
                            starts_at + chrono::Duration::days(1)
                        } else {
                            starts_at + chrono::Duration::hours(1)
                        }
                    }
                }
            } else if let Some(d_str) = eo.get("date").and_then(|d| d.as_str()) {
                match NaiveDate::parse_from_str(d_str, "%Y-%m-%d") {
                    Ok(nd) => {
                        let naive = nd
                            .and_hms_opt(0, 0, 0)
                            .unwrap_or_else(|| starts_at.naive_utc());
                        let mut dt = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
                        if dt <= starts_at {
                            dt = starts_at + chrono::Duration::days(1);
                        }
                        dt
                    }
                    Err(_) => starts_at + chrono::Duration::days(1),
                }
            } else if all_day {
                starts_at + chrono::Duration::days(1)
            } else {
                starts_at + chrono::Duration::hours(1)
            }
        } else if all_day {
            starts_at + chrono::Duration::days(1)
        } else {
            starts_at + chrono::Duration::hours(1)
        };

        let rrule = item
            .get("recurrence")
            .and_then(|r| r.as_array())
            .and_then(|lines| {
                lines
                    .iter()
                    .filter_map(|l| l.as_str())
                    .find(|l| l.starts_with("RRULE:"))
                    .map(|l| l.trim_start_matches("RRULE:").to_string())
            });

        let tz = start_obj
            .and_then(|s| s.get("timeZone"))
            .and_then(|t| t.as_str())
            .or_else(|| {
                end_obj
                    .and_then(|e| e.get("timeZone"))
                    .and_then(|t| t.as_str())
            })
            .map(|s| s.to_string());

        let event = Event {
            id: Uuid::new_v4(),
            calendar_id,
            uid,
            title,
            location,
            notes,
            starts_at,
            ends_at,
            all_day,
            tz,
            rrule,
            exdates: vec![],
            etag: item
                .get("etag")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string()),
            created_at: now,
            updated_at: now,
            deleted_at: if is_cancelled { Some(now) } else { None },
        };

        events.push(event);
    }

    events
}

/// Converts a Google Calendar event into an Almanac domain [`Event`].
pub fn gcal_to_domain_event(
    gcal: &GoogleEvent,
    calendar_id: Uuid,
    existing_id: Option<Uuid>,
) -> Option<Event> {
    let now = Utc::now();
    let is_cancelled = gcal.status.as_deref() == Some("cancelled");

    let start_spec = gcal.start.as_ref()?;
    let all_day = start_spec.date.is_some();

    let starts_at = if let Some(dt) = start_spec.date_time {
        dt
    } else {
        let d = start_spec.date?;
        let naive = d.and_hms_opt(0, 0, 0)?;
        DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)
    };

    let ends_at = if let Some(end_spec) = gcal.end.as_ref() {
        if let Some(dt) = end_spec.date_time {
            if dt <= starts_at && !all_day {
                starts_at + chrono::Duration::minutes(30)
            } else {
                dt
            }
        } else if let Some(d) = end_spec.date {
            let naive = d.and_hms_opt(0, 0, 0)?;
            let dt = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
            if dt <= starts_at {
                starts_at + chrono::Duration::days(1)
            } else {
                dt
            }
        } else if all_day {
            starts_at + chrono::Duration::days(1)
        } else {
            starts_at + chrono::Duration::hours(1)
        }
    } else if all_day {
        starts_at + chrono::Duration::days(1)
    } else {
        starts_at + chrono::Duration::hours(1)
    };

    let rrule = gcal
        .recurrence
        .as_ref()
        .and_then(|lines| lines.iter().find(|l| l.starts_with("RRULE:")))
        .map(|l| l.trim_start_matches("RRULE:").to_string());

    let tz = gcal
        .start
        .as_ref()
        .and_then(|s| s.time_zone.clone())
        .or_else(|| gcal.end.as_ref().and_then(|e| e.time_zone.clone()));

    let id = existing_id.unwrap_or_else(Uuid::new_v4);

    Some(Event {
        id,
        calendar_id,
        uid: gcal.id.clone(),
        title: gcal.summary.clone().unwrap_or_else(|| "(Untitled)".into()),
        location: gcal.location.clone(),
        notes: gcal.description.clone(),
        starts_at,
        ends_at,
        all_day,
        tz,
        rrule,
        exdates: vec![],
        etag: gcal.etag.clone(),
        created_at: now,
        updated_at: gcal.updated.unwrap_or(now),
        deleted_at: if is_cancelled { Some(now) } else { None },
    })
}

/// Converts an Almanac domain [`Event`] into a Google Calendar API [`GoogleEvent`].
pub fn domain_to_gcal_event(event: &Event) -> GoogleEvent {
    let (start, end) = if event.all_day {
        let start_date = event.starts_at.date_naive();
        let end_date = event.ends_at.date_naive();
        (
            GoogleEventDateTime {
                date_time: None,
                date: Some(start_date),
                time_zone: event.tz.clone(),
            },
            GoogleEventDateTime {
                date_time: None,
                date: Some(if end_date <= start_date {
                    start_date.succ_opt().unwrap()
                } else {
                    end_date
                }),
                time_zone: event.tz.clone(),
            },
        )
    } else {
        (
            GoogleEventDateTime {
                date_time: Some(event.starts_at),
                date: None,
                time_zone: event.tz.clone(),
            },
            GoogleEventDateTime {
                date_time: Some(event.ends_at),
                date: None,
                time_zone: event.tz.clone(),
            },
        )
    };

    let recurrence = event.rrule.as_ref().map(|r| vec![format!("RRULE:{r}")]);

    GoogleEvent {
        id: event.uid.clone(),
        status: Some(if event.deleted_at.is_some() {
            "cancelled".into()
        } else {
            "confirmed".into()
        }),
        summary: Some(event.title.clone()),
        description: event.notes.clone(),
        location: event.location.clone(),
        start: Some(start),
        end: Some(end),
        recurrence,
        etag: event.etag.clone(),
        updated: Some(event.updated_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_bidirectional_gcal_event_conversion() {
        let now = Utc::now();
        let cal_id = Uuid::new_v4();

        let domain_evt = Event {
            id: Uuid::new_v4(),
            calendar_id: cal_id,
            uid: "gcal_test_123".into(),
            title: "Project Sync".into(),
            location: Some("Google Meet".into()),
            notes: Some("Quarterly review".into()),
            starts_at: now,
            ends_at: now + Duration::hours(1),
            all_day: false,
            tz: Some("America/New_York".into()),
            rrule: Some("FREQ=WEEKLY;BYDAY=MO".into()),
            exdates: vec![],
            etag: Some("\"etag123\"".into()),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        // Domain -> Google
        let gcal = domain_to_gcal_event(&domain_evt);
        assert_eq!(gcal.summary.as_deref(), Some("Project Sync"));
        assert_eq!(gcal.status.as_deref(), Some("confirmed"));
        assert_eq!(
            gcal.recurrence,
            Some(vec!["RRULE:FREQ=WEEKLY;BYDAY=MO".into()])
        );

        // Google -> Domain
        let converted = gcal_to_domain_event(&gcal, cal_id, Some(domain_evt.id)).unwrap();
        assert_eq!(converted.id, domain_evt.id);
        assert_eq!(converted.title, domain_evt.title);
        assert_eq!(converted.rrule, domain_evt.rrule);
        assert_eq!(converted.tz, domain_evt.tz);
    }

    #[test]
    fn test_parse_gcal_events_json_resilient() {
        let raw_json = r#"{
            "items": [
                {
                    "id": "event_1",
                    "summary": "Team Weekly",
                    "start": { "dateTime": "2026-08-14T10:00:00-04:00" },
                    "end": { "dateTime": "2026-08-14T11:00:00-04:00" }
                },
                {
                    "id": "event_2",
                    "summary": "All Day Conference",
                    "start": { "date": "2026-08-15" },
                    "end": { "date": "2026-08-16" }
                }
            ]
        }"#;

        let cal_id = Uuid::new_v4();
        let events = parse_gcal_events_json(raw_json, cal_id);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "Team Weekly");
        assert_eq!(events[1].title, "All Day Conference");
        assert!(events[1].all_day);
    }
}

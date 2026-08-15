//! Free/busy calculation and VFREEBUSY query support (Roadmap 4.5).

use quill_store::sqlite::SqliteStore;
use quill_store::{CalendarEvent, FreeBusySlot};

/// Compute free/busy slots across a time range given stored calendar events.
pub fn compute_free_busy_slots(
    events: &[CalendarEvent],
    start_ms: i64,
    end_ms: i64,
    slot_duration_ms: i64,
) -> Vec<FreeBusySlot> {
    if slot_duration_ms <= 0 || end_ms <= start_ms {
        return Vec::new();
    }

    let mut slots = Vec::new();
    let mut cur = start_ms;

    while cur < end_ms {
        let slot_end = (cur + slot_duration_ms).min(end_ms);
        let mut busy = false;
        let mut conflicting_attendee: Option<String> = None;

        for ev in events {
            if ev.all_day {
                // All-day event covers the entire day
                if ev.start_ms < slot_end && ev.end_ms > cur {
                    busy = true;
                    conflicting_attendee = Some(ev.title.clone());
                    break;
                }
            } else {
                // Include travel time buffer if present
                let buffer_ms = (ev.travel_time_minutes.unwrap_or(0) as i64) * 60_000;
                let effective_start = ev.start_ms - buffer_ms;
                let effective_end = ev.end_ms;

                if effective_start < slot_end && effective_end > cur {
                    busy = true;
                    conflicting_attendee = Some(ev.title.clone());
                    break;
                }
            }
        }

        slots.push(FreeBusySlot {
            start_ms: cur,
            end_ms: slot_end,
            busy,
            attendee: conflicting_attendee,
        });

        cur += slot_duration_ms;
    }

    slots
}

/// Query free/busy slots for scheduling from the store.
pub fn query_store_free_busy(
    store: &SqliteStore,
    start_ms: i64,
    end_ms: i64,
    slot_duration_minutes: u32,
) -> Vec<FreeBusySlot> {
    let events = store.list_events(start_ms, end_ms);
    let duration_ms = (slot_duration_minutes.max(15) as i64) * 60_000;
    compute_free_busy_slots(&events, start_ms, end_ms, duration_ms)
}

/// Parse RFC 5545 VFREEBUSY text into free/busy busy periods.
pub fn parse_vfreebusy_periods(ics_text: &str) -> Vec<(i64, i64)> {
    let mut periods = Vec::new();
    for line in ics_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("FREEBUSY") {
            if let Some(pos) = trimmed.find(':') {
                let value_part = &trimmed[pos + 1..];
                for period_str in value_part.split(',') {
                    let parts: Vec<&str> = period_str.trim().split('/').collect();
                    if parts.len() == 2 {
                        let s = parse_iso_or_utc(parts[0]);
                        let e = parse_iso_or_utc(parts[1]);
                        if let (Some(start), Some(end)) = (s, e) {
                            periods.push((start, end));
                        }
                    }
                }
            }
        }
    }
    periods
}

fn parse_iso_or_utc(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp_millis())
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ")
                .map(|ndt| ndt.and_utc().timestamp_millis())
                .ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_free_busy_slots() {
        let events = vec![CalendarEvent {
            id: 1,
            account_id: 1,
            title: "Morning Sync".into(),
            start_ms: 3600_000 * 9,  // 09:00
            end_ms: 3600_000 * 10,   // 10:00
            all_day: false,
            location: None,
            notes: None,
            alarm_minutes_before: None,
            timezone: None,
            travel_time_minutes: Some(30), // Travel buffer starts at 08:30
            calendar_source: None,
            calendar_name: None,
            calendar_color: None,
            color: None,
        }];

        // Query 08:00 to 11:00 in 30-min slots
        let slots = compute_free_busy_slots(
            &events,
            3600_000 * 8,
            3600_000 * 11,
            1800_000,
        );

        assert_eq!(slots.len(), 6);
        assert!(!slots[0].busy); // 08:00 - 08:30 free
        assert!(slots[1].busy);  // 08:30 - 09:00 busy (travel buffer)
        assert!(slots[2].busy);  // 09:00 - 09:30 busy (event)
        assert!(slots[3].busy);  // 09:30 - 10:00 busy (event)
        assert!(!slots[4].busy); // 10:00 - 10:30 free
        assert!(!slots[5].busy); // 10:30 - 11:00 free
    }
}

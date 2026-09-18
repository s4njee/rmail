//! RFC 5545 VFREEBUSY and availability computation (T1.11).
//!
//! Provides free/busy interval calculation across time ranges, meeting slot recommendation,
//! and RFC 5545 `VFREEBUSY` parsing and formatting.

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{Event, TimeRange};

/// A computed free/busy interval slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FreeBusySlot {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub busy: bool,
    pub event_titles: Vec<String>,
}

/// Computes consecutive free/busy slots across `range` by checking overlap with `events`.
///
/// Accounts for all-day events (covering the whole UTC day) and travel time buffers.
pub fn compute_free_busy(
    events: &[Event],
    range: &TimeRange,
    slot_duration_minutes: u32,
) -> Vec<FreeBusySlot> {
    let slot_dur = Duration::minutes(slot_duration_minutes.max(5) as i64);
    let mut slots = Vec::new();
    let mut current_start = range.start;

    while current_start < range.end {
        let current_end = (current_start + slot_dur).min(range.end);
        let mut busy = false;
        let mut titles = Vec::new();

        for event in events {
            if event.deleted_at.is_some() || !event.busy {
                continue;
            }

            let (event_start, event_end) = if event.all_day {
                // All-day: span from start date midnight to end date midnight
                let s_date = event.starts_at.date_naive();
                let e_date = event.ends_at.date_naive();
                let s = Utc.from_utc_datetime(&s_date.and_hms_opt(0, 0, 0).unwrap());
                let mut e = Utc.from_utc_datetime(&e_date.and_hms_opt(0, 0, 0).unwrap());
                if e <= s {
                    e = s + Duration::days(1);
                }
                (s, e)
            } else {
                let buffer = Duration::minutes(event.travel_time_minutes.unwrap_or(0).max(0));
                (event.starts_at - buffer, event.ends_at)
            };

            // Half-open interval overlap: [event_start, event_end) overlaps [current_start, current_end)
            if event_start < current_end && event_end > current_start {
                busy = true;
                if !titles.contains(&event.title) {
                    titles.push(event.title.clone());
                }
            }
        }

        slots.push(FreeBusySlot {
            start: current_start,
            end: current_end,
            busy,
            event_titles: titles,
        });

        current_start = current_end;
    }

    slots
}

/// Finds available meeting slots on a specific date between `work_start_hour` and `work_end_hour` (UTC).
///
/// Returns time ranges of length `duration_minutes` that have no conflicting events.
pub fn find_available_slots(
    events: &[Event],
    date: NaiveDate,
    duration_minutes: u32,
    work_start_hour: u32,
    work_end_hour: u32,
) -> Vec<TimeRange> {
    let start_h = work_start_hour.min(23);
    let end_h = work_end_hour.max(start_h + 1).min(24);

    let start_naive = date.and_hms_opt(start_h, 0, 0).unwrap();
    let end_naive = if end_h == 24 {
        date.succ_opt()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .unwrap_or(date.and_hms_opt(23, 59, 59).unwrap())
    } else {
        date.and_hms_opt(end_h, 0, 0).unwrap()
    };

    let day_start = Utc.from_utc_datetime(&start_naive);
    let day_end = Utc.from_utc_datetime(&end_naive);

    let Ok(_day_range) = TimeRange::new(day_start, day_end) else {
        return Vec::new();
    };

    let step_minutes = 30; // Check slots starting on 30-minute boundaries
    let meeting_dur = Duration::minutes(duration_minutes.max(15) as i64);
    let mut available = Vec::new();
    let mut cand_start = day_start;

    while cand_start + meeting_dur <= day_end {
        let cand_end = cand_start + meeting_dur;
        let mut conflicts = false;

        for event in events {
            if event.deleted_at.is_some() || !event.busy {
                continue;
            }

            let (event_start, event_end) = if event.all_day {
                let s_date = event.starts_at.date_naive();
                let e_date = event.ends_at.date_naive();
                let s = Utc.from_utc_datetime(&s_date.and_hms_opt(0, 0, 0).unwrap());
                let mut e = Utc.from_utc_datetime(&e_date.and_hms_opt(0, 0, 0).unwrap());
                if e <= s {
                    e = s + Duration::days(1);
                }
                (s, e)
            } else {
                let buffer = Duration::minutes(event.travel_time_minutes.unwrap_or(0).max(0));
                (event.starts_at - buffer, event.ends_at)
            };

            if event_start < cand_end && event_end > cand_start {
                conflicts = true;
                break;
            }
        }

        if !conflicts {
            if let Ok(range) = TimeRange::new(cand_start, cand_end) {
                available.push(range);
            }
        }

        cand_start += Duration::minutes(step_minutes);
    }

    available
}

/// Serializes busy periods to an RFC 5545 `VFREEBUSY` component.
pub fn write_vfreebusy(
    attendee_email: &str,
    range: &TimeRange,
    busy_periods: &[(DateTime<Utc>, DateTime<Utc>)],
) -> String {
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//rcalendar//Almanac//EN\r\n");
    out.push_str("BEGIN:VFREEBUSY\r\n");
    out.push_str(&format!(
        "DTSTAMP:{}\r\n",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    ));
    out.push_str(&format!(
        "DTSTART:{}\r\n",
        range.start.format("%Y%m%dT%H%M%SZ")
    ));
    out.push_str(&format!("DTEND:{}\r\n", range.end.format("%Y%m%dT%H%M%SZ")));
    if !attendee_email.is_empty() {
        out.push_str(&format!("ATTENDEE:mailto:{attendee_email}\r\n"));
    }

    for (start, end) in busy_periods {
        out.push_str(&format!(
            "FREEBUSY;FBTYPE=BUSY:{}/{}\r\n",
            start.format("%Y%m%dT%H%M%SZ"),
            end.format("%Y%m%dT%H%M%SZ")
        ));
    }

    out.push_str("END:VFREEBUSY\r\nEND:VCALENDAR\r\n");
    out
}

/// Parses RFC 5545 `VFREEBUSY` text into busy time intervals.
pub fn parse_vfreebusy(ics: &str) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    let mut periods = Vec::new();
    for line in ics.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("FREEBUSY") {
            if let Some(pos) = trimmed.find(':') {
                let val_part = &trimmed[pos + 1..];
                for period_str in val_part.split(',') {
                    let parts: Vec<&str> = period_str.trim().split('/').collect();
                    if parts.len() == 2 {
                        let s = parse_dt_stamp(parts[0])?;
                        let e = if parts[1].starts_with('P') {
                            // Duration format (e.g. PT1H)
                            s + parse_duration_iso(parts[1])?
                        } else {
                            parse_dt_stamp(parts[1])?
                        };
                        periods.push((s, e));
                    }
                }
            }
        }
    }
    Ok(periods)
}

fn parse_dt_stamp(s: &str) -> Result<DateTime<Utc>> {
    let trimmed = s.trim().trim_end_matches('Z');
    let naive = NaiveDateTime::parse_from_str(trimmed, "%Y%m%dT%H%M%S")
        .map_err(|e| Error::Ical(format!("invalid timestamp {s}: {e}")))?;
    Ok(Utc.from_utc_datetime(&naive))
}

fn parse_duration_iso(s: &str) -> Result<Duration> {
    let dur = s.trim().trim_start_matches('P');
    let mut total_mins: i64 = 0;
    let mut cur_num = String::new();
    let mut in_time = false;

    for ch in dur.chars() {
        match ch {
            'T' => in_time = true,
            '0'..='9' => cur_num.push(ch),
            'D' => {
                let days: i64 = cur_num.parse().unwrap_or(0);
                total_mins += days * 24 * 60;
                cur_num.clear();
            }
            'H' => {
                let hours: i64 = cur_num.parse().unwrap_or(0);
                total_mins += hours * 60;
                cur_num.clear();
            }
            'M' if in_time => {
                let mins: i64 = cur_num.parse().unwrap_or(0);
                total_mins += mins;
                cur_num.clear();
            }
            _ => {}
        }
    }
    Ok(Duration::minutes(total_mins))
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_event(start_s: &str, end_s: &str, title: &str) -> Event {
        let s = Utc.from_utc_datetime(&start_s.parse().unwrap());
        let e = Utc.from_utc_datetime(&end_s.parse().unwrap());
        Event {
            id: Uuid::new_v4(),
            calendar_id: Uuid::new_v4(),
            uid: format!("{}@example.com", Uuid::new_v4()),
            title: title.into(),
            location: None,
            notes: None,
            starts_at: s,
            ends_at: e,
            all_day: false,
            tz: None,
            rrule: None,
            exdates: vec![],
            travel_time_minutes: None,
            color: None,
            etag: None,
            attendees: vec![],
            busy: true,
            created_at: s,
            updated_at: s,
            deleted_at: None,
        }
    }

    #[test]
    fn computes_free_busy_slots() {
        let ev = sample_event(
            "2026-08-20T10:00:00",
            "2026-08-20T11:00:00",
            "Design Review",
        );
        let range = TimeRange::new(
            Utc.from_utc_datetime(&"2026-08-20T09:00:00".parse().unwrap()),
            Utc.from_utc_datetime(&"2026-08-20T12:00:00".parse().unwrap()),
        )
        .unwrap();

        let slots = compute_free_busy(&[ev], &range, 60);
        assert_eq!(slots.len(), 3);
        assert!(!slots[0].busy); // 09:00 - 10:00 free
        assert!(slots[1].busy); // 10:00 - 11:00 busy
        assert_eq!(slots[1].event_titles, vec!["Design Review"]);
        assert!(!slots[2].busy); // 11:00 - 12:00 free
    }

    #[test]
    fn computes_free_busy_with_travel_time() {
        let mut ev = sample_event("2026-08-20T10:00:00", "2026-08-20T11:00:00", "Offsite");
        ev.travel_time_minutes = Some(30); // buffer 09:30 - 10:00

        let range = TimeRange::new(
            Utc.from_utc_datetime(&"2026-08-20T09:00:00".parse().unwrap()),
            Utc.from_utc_datetime(&"2026-08-20T11:00:00".parse().unwrap()),
        )
        .unwrap();

        let slots = compute_free_busy(&[ev], &range, 30);
        assert_eq!(slots.len(), 4);
        assert!(!slots[0].busy); // 09:00 - 09:30 free
        assert!(slots[1].busy); // 09:30 - 10:00 busy (travel buffer)
        assert!(slots[2].busy); // 10:00 - 10:30 busy
        assert!(slots[3].busy); // 10:30 - 11:00 busy
    }

    #[test]
    fn transparent_events_do_not_block_free_busy() {
        let mut ev = sample_event("2026-08-20T10:00:00", "2026-08-20T11:00:00", "Focus (free)");
        ev.busy = false;
        let range = TimeRange::new(
            Utc.from_utc_datetime(&"2026-08-20T09:00:00".parse().unwrap()),
            Utc.from_utc_datetime(&"2026-08-20T12:00:00".parse().unwrap()),
        )
        .unwrap();
        let slots = compute_free_busy(&[ev], &range, 60);
        assert!(slots.iter().all(|s| !s.busy));
    }

    #[test]
    fn finds_available_meeting_slots() {
        let ev1 = sample_event("2026-08-20T09:00:00", "2026-08-20T10:00:00", "Standup");
        let ev2 = sample_event("2026-08-20T11:00:00", "2026-08-20T12:00:00", "Sync");
        let date = NaiveDate::from_ymd_opt(2026, 8, 20).unwrap();

        let available = find_available_slots(&[ev1, ev2], date, 60, 9, 13);
        // 09:00-10:00 busy, 10:00-11:00 free, 11:00-12:00 busy, 12:00-13:00 free
        assert_eq!(available.len(), 2);
        assert_eq!(
            available[0].start,
            Utc.from_utc_datetime(&"2026-08-20T10:00:00".parse().unwrap())
        );
        assert_eq!(
            available[1].start,
            Utc.from_utc_datetime(&"2026-08-20T12:00:00".parse().unwrap())
        );
    }

    #[test]
    fn vfreebusy_round_trips() {
        let s1 = Utc.from_utc_datetime(&"2026-08-20T10:00:00".parse().unwrap());
        let e1 = Utc.from_utc_datetime(&"2026-08-20T11:00:00".parse().unwrap());
        let s2 = Utc.from_utc_datetime(&"2026-08-20T14:00:00".parse().unwrap());
        let e2 = Utc.from_utc_datetime(&"2026-08-20T15:30:00".parse().unwrap());

        let range = TimeRange::new(
            Utc.from_utc_datetime(&"2026-08-20T00:00:00".parse().unwrap()),
            Utc.from_utc_datetime(&"2026-08-20T23:59:59".parse().unwrap()),
        )
        .unwrap();

        let busy = vec![(s1, e1), (s2, e2)];
        let vfb = write_vfreebusy("alice@example.com", &range, &busy);
        assert!(vfb.contains("BEGIN:VFREEBUSY"));
        assert!(vfb.contains("ATTENDEE:mailto:alice@example.com"));

        let parsed = parse_vfreebusy(&vfb).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], (s1, e1));
        assert_eq!(parsed[1], (s2, e2));
    }
}

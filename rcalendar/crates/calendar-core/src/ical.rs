//! RFC 5545 (iCalendar) import/export (S1.6).
//!
//! Import maps `VEVENT` properties onto [`Event`] and ignores unknown
//! properties without failing. Export writes a canonical `VCALENDAR` that
//! round-trips the supported fields losslessly. Text values are unescaped on
//! import and escaped on export; lines are folded at 75 octets per the spec.

use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::Event;

/// Parses an RFC 5545 `.ics` string into events.
///
/// - Unknown properties are ignored (the import never fails on extra data).
/// - Events with `STATUS:CANCELLED` are dropped.
/// - A `DTEND` that is missing defaults to `DTSTART + 1h` (timed) or
///   `DTSTART + 1d` (all-day).
/// - Floating times (no `TZID`, no `Z`) are treated as UTC.
pub fn parse_ical(input: &str) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    for calendar in ical::parser::ical::IcalParser::new(input.as_bytes()) {
        let calendar = calendar.map_err(|e| Error::Ical(e.to_string()))?;
        for vevent in &calendar.events {
            if let Some(event) = parse_vevent(vevent)? {
                events.push(event);
            }
        }
    }
    Ok(events)
}

/// Serializes events to a canonical RFC 5545 `.ics` string.
pub fn write_ical(events: &[Event]) -> Result<String> {
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//rcalendar//Almanac//EN\r\nCALSCALE:GREGORIAN\r\n");
    for event in events {
        event.validate()?;
        out.push_str("BEGIN:VEVENT\r\n");
        push_prop(&mut out, "UID", &event.uid);
        push_prop(
            &mut out,
            "DTSTAMP",
            &Utc::now().format("%Y%m%dT%H%M%SZ").to_string(),
        );

        if event.all_day {
            let start_date = event.starts_at.date_naive();
            let mut end_date = event.ends_at.date_naive();
            if end_date <= start_date {
                end_date = start_date.succ_opt().unwrap(); // single-day all-day: end > start
            }
            push_prop(
                &mut out,
                "DTSTART;VALUE=DATE",
                &start_date.format("%Y%m%d").to_string(),
            );
            push_prop(
                &mut out,
                "DTEND;VALUE=DATE",
                &end_date.format("%Y%m%d").to_string(),
            );
        } else if let Some(tz_name) = &event.tz {
            let tz = tz_name
                .parse::<chrono_tz::Tz>()
                .map_err(|_| Error::InvalidEvent(format!("unknown timezone {tz_name:?}")))?;
            let start_local = event.starts_at.with_timezone(&tz);
            let end_local = event.ends_at.with_timezone(&tz);
            push_prop(
                &mut out,
                &format!("DTSTART;TZID={tz}"),
                &start_local.format("%Y%m%dT%H%M%S").to_string(),
            );
            push_prop(
                &mut out,
                &format!("DTEND;TZID={tz}"),
                &end_local.format("%Y%m%dT%H%M%S").to_string(),
            );
        } else {
            push_prop(
                &mut out,
                "DTSTART",
                &event.starts_at.format("%Y%m%dT%H%M%SZ").to_string(),
            );
            push_prop(
                &mut out,
                "DTEND",
                &event.ends_at.format("%Y%m%dT%H%M%SZ").to_string(),
            );
        }

        if !event.title.is_empty() {
            push_prop(&mut out, "SUMMARY", &escape_text(&event.title));
        }
        if let Some(location) = &event.location {
            push_prop(&mut out, "LOCATION", &escape_text(location));
        }
        if let Some(notes) = &event.notes {
            push_prop(&mut out, "DESCRIPTION", &escape_text(notes));
        }
        if let Some(rrule) = &event.rrule {
            push_prop(&mut out, "RRULE", rrule);
        }
        if !event.exdates.is_empty() {
            let values = event
                .exdates
                .iter()
                .map(|d| d.format("%Y%m%d").to_string())
                .collect::<Vec<_>>()
                .join(",");
            push_prop(&mut out, "EXDATE", &values);
        }
        out.push_str("END:VEVENT\r\n");
    }
    out.push_str("END:VCALENDAR\r\n");
    Ok(out)
}

// ---------------------------------------------------------------------------
// Import internals
// ---------------------------------------------------------------------------

/// A parsed date value: the instant (UTC), whether it is date-only, and the
/// IANA `TZID` (when anchored to one).
type ParsedDateTime = (chrono::DateTime<Utc>, bool, Option<String>);

fn parse_vevent(vevent: &ical::parser::ical::component::IcalEvent) -> Result<Option<Event>> {
    let mut uid = None;
    let mut title = None;
    let mut location = None;
    let mut notes = None;
    let mut start = None;
    let mut end = None;
    let mut rrule = None;
    let mut exdates = Vec::new();
    let mut cancelled = false;

    for prop in &vevent.properties {
        match prop.name.to_ascii_uppercase().as_str() {
            "UID" => {
                uid = prop
                    .value
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            }
            "SUMMARY" => title = prop.value.as_deref().map(unescape_text),
            "DESCRIPTION" => notes = prop.value.as_deref().map(unescape_text),
            "LOCATION" => location = prop.value.as_deref().map(unescape_text),
            "DTSTART" => start = Some(parse_property_datetime(prop)?),
            "DTEND" => end = Some(parse_property_datetime(prop)?),
            "RRULE" => {
                // Unsupported rule values degrade to a single instance rather
                // than failing the whole import.
                if let Some(value) = prop
                    .value
                    .as_deref()
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                {
                    match crate::recurrence::validate_rrule(value) {
                        Ok(()) => rrule = Some(value.to_string()),
                        Err(_) => rrule = None,
                    }
                }
            }
            "EXDATE" => exdates.extend(parse_exdates(prop)?),
            "STATUS" => {
                cancelled = prop
                    .value
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case("cancelled"));
            }
            // Unknown properties are ignored without failing the import.
            _ => {}
        }
    }

    if cancelled {
        return Ok(None);
    }

    let (starts_at, all_day, tz) = start
        .clone()
        .ok_or_else(|| Error::Ical("VEVENT is missing DTSTART".into()))?;
    let ends_at = match end {
        Some((ends_at, _, _)) => ends_at,
        None if all_day => starts_at + Duration::days(1),
        None => starts_at + Duration::hours(1),
    };

    let event = Event {
        id: Uuid::new_v4(),
        calendar_id: Uuid::new_v4(), // assigned by the caller's store
        uid: uid.unwrap_or_else(|| format!("import-{}@almanac", Uuid::new_v4())),
        title: title.unwrap_or_default(),
        location,
        notes,
        starts_at,
        ends_at,
        all_day,
        tz,
        rrule,
        exdates,
        etag: None,
        updated_at: Utc::now(),
        created_at: Utc::now(),
        deleted_at: None,
    };
    event.validate().map_err(|e| Error::Ical(e.to_string()))?;
    Ok(Some(event))
}

fn parse_property_datetime(prop: &ical::property::Property) -> Result<ParsedDateTime> {
    let value = prop.value.as_deref().unwrap_or_default();
    let params = prop.params.as_deref().unwrap_or_default();
    let tzid = params
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("tzid"))
        .and_then(|(_, values)| values.first())
        .cloned();

    // Date-only values (no time part) are all-day events.
    if !value.contains('T') {
        let date = NaiveDate::parse_from_str(value, "%Y%m%d")
            .map_err(|_| Error::Ical(format!("invalid DATE value {value:?}")))?;
        let midnight = Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0).unwrap());
        return Ok((midnight, true, None));
    }

    let is_utc = value.ends_with('Z');
    let naive = NaiveDateTime::parse_from_str(value.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
        .map_err(|_| Error::Ical(format!("invalid DATE-TIME value {value:?}")))?;

    if !is_utc {
        if let Some(name) = &tzid {
            let tz = name
                .parse::<chrono_tz::Tz>()
                .map_err(|_| Error::Ical(format!("unknown TZID {name:?}")))?;
            let dt = tz
                .from_local_datetime(&naive)
                .earliest()
                .or_else(|| tz.from_local_datetime(&naive).latest())
                .ok_or_else(|| {
                    Error::Ical(format!("nonexistent local time {value:?} in {name}"))
                })?;
            return Ok((dt.with_timezone(&Utc), false, Some(name.clone())));
        }
    }
    Ok((Utc.from_utc_datetime(&naive), false, None))
}

fn parse_exdates(prop: &ical::property::Property) -> Result<Vec<NaiveDate>> {
    let value = prop.value.as_deref().unwrap_or_default();
    let mut out = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let date = if part.contains('T') {
            let naive = NaiveDateTime::parse_from_str(part.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
                .map_err(|_| Error::Ical(format!("invalid EXDATE {part:?}")))?;
            naive.date()
        } else {
            NaiveDate::parse_from_str(part, "%Y%m%d")
                .map_err(|_| Error::Ical(format!("invalid EXDATE {part:?}")))?
        };
        out.push(date);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Text escaping (RFC 5545 §3.3.11) and line folding (§3.1)
// ---------------------------------------------------------------------------

fn escape_text(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

fn unescape_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') | Some('N') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(';') => out.push(';'),
            Some(',') => out.push(','),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn push_prop(out: &mut String, name: &str, value: &str) {
    fold_line(out, &format!("{name}:{value}"));
}

/// Writes `line` folded at 75 octets; continuation lines start with a space.
fn fold_line(out: &mut String, line: &str) {
    const MAX: usize = 75;
    let mut first = true;
    let mut remaining = line;
    while !remaining.is_empty() {
        let prefix = if first { "" } else { " " };
        first = false;
        let budget = MAX.saturating_sub(prefix.len());
        let mut split = budget.min(remaining.len());
        while split > 0 && !remaining.is_char_boundary(split) {
            split -= 1;
        }
        out.push_str(prefix);
        out.push_str(&remaining[..split]);
        out.push_str("\r\n");
        remaining = &remaining[split..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, TimeZone};

    fn utc(s: &str) -> chrono::DateTime<Utc> {
        Utc.from_utc_datetime(&s.parse().unwrap())
    }

    fn sample_event() -> Event {
        Event {
            id: Uuid::new_v4(),
            calendar_id: Uuid::new_v4(),
            uid: "e1@example.com".into(),
            title: "Stats 101 lecture".into(),
            location: Some("Kane Hall 210".into()),
            notes: Some("Bring the problem set\nOffice hours after.".into()),
            starts_at: utc("2026-08-13T10:00:00"),
            ends_at: utc("2026-08-13T11:30:00"),
            all_day: false,
            tz: Some("America/New_York".into()),
            rrule: Some("FREQ=WEEKLY;BYDAY=MO,WE,TH".into()),
            exdates: vec![NaiveDate::from_ymd_opt(2026, 8, 20).unwrap()],
            etag: None,
            updated_at: utc("2026-08-13T10:00:00"),
            created_at: utc("2026-08-13T10:00:00"),
            deleted_at: None,
        }
    }

    #[test]
    fn round_trips_supported_fields() {
        let ics = write_ical(&[sample_event()]).unwrap();
        let imported = parse_ical(&ics).unwrap();
        assert_eq!(imported.len(), 1);

        let got = &imported[0];
        assert_eq!(got.uid, "e1@example.com");
        assert_eq!(got.title, "Stats 101 lecture");
        assert_eq!(got.location.as_deref(), Some("Kane Hall 210"));
        assert_eq!(
            got.notes.as_deref(),
            Some("Bring the problem set\nOffice hours after.")
        );
        assert_eq!(got.starts_at, utc("2026-08-13T10:00:00"));
        assert_eq!(got.ends_at, utc("2026-08-13T11:30:00"));
        assert!(!got.all_day);
        assert_eq!(got.tz.as_deref(), Some("America/New_York"));
        assert_eq!(got.rrule.as_deref(), Some("FREQ=WEEKLY;BYDAY=MO,WE,TH"));
        assert_eq!(
            got.exdates,
            vec![NaiveDate::from_ymd_opt(2026, 8, 20).unwrap()]
        );
    }

    #[test]
    fn round_trips_utc_timed_event() {
        let mut e = sample_event();
        e.tz = None;
        let ics = write_ical(&[e]).unwrap();
        let imported = parse_ical(&ics).unwrap();
        assert_eq!(imported[0].starts_at, utc("2026-08-13T10:00:00"));
        assert_eq!(imported[0].tz, None);
        assert!(ics.contains("DTSTART:20260813T100000Z"));
    }

    #[test]
    fn round_trips_all_day_event() {
        let mut e = sample_event();
        e.all_day = true;
        e.starts_at = utc("2026-08-13T00:00:00");
        e.ends_at = utc("2026-08-13T00:00:00"); // single-day: start == end
        e.rrule = None;
        e.tz = None;
        let ics = write_ical(&[e]).unwrap();
        assert!(ics.contains("DTSTART;VALUE=DATE:20260813"));
        assert!(
            ics.contains("DTEND;VALUE=DATE:20260814"),
            "end == start gets +1 day"
        );

        let imported = parse_ical(&ics).unwrap();
        assert_eq!(imported[0].starts_at, utc("2026-08-13T00:00:00"));
        assert!(imported[0].all_day);
    }

    #[test]
    fn export_escapes_and_folds_long_values() {
        let mut e = sample_event();
        e.title = "Café, \"x\"; 100,000 tasks".to_string();
        e.notes = None;
        let ics = write_ical(&[e]).unwrap();
        let imported = parse_ical(&ics).unwrap();
        assert_eq!(imported[0].title, "Café, \"x\"; 100,000 tasks");
        assert!(
            ics.lines().all(|l| l.len() <= 76),
            "folded to <= 75 octets + CRLF"
        );
    }

    #[test]
    fn import_ignores_unknown_properties_and_cancelled_events() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "PRODID:-//Test//EN\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:keep@example.com\r\n",
            "SUMMARY:Keep me\r\n",
            "DTSTART:20260813T100000Z\r\n",
            "DTEND:20260813T110000Z\r\n",
            "ATTENDEE:mailto:someone@example.com\r\n",
            "SEQUENCE:2\r\n",
            "X-CUSTOM-PROP:whatever\r\n",
            "END:VEVENT\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:drop@example.com\r\n",
            "SUMMARY:Cancelled thing\r\n",
            "DTSTART:20260814T100000Z\r\n",
            "DTEND:20260814T110000Z\r\n",
            "STATUS:CANCELLED\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR\r\n",
        );
        let events = parse_ical(ics).unwrap();
        assert_eq!(events.len(), 1, "cancelled event is dropped");
        assert_eq!(events[0].uid, "keep@example.com");
        assert_eq!(events[0].title, "Keep me");
    }

    #[test]
    fn import_defaults_missing_dtend() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n",
            "UID:a@example.com\r\nSUMMARY:No end\r\nDTSTART:20260813T100000Z\r\n",
            "END:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        let events = parse_ical(ics).unwrap();
        assert_eq!(
            events[0].ends_at,
            utc("2026-08-13T11:00:00"),
            "defaults to +1h"
        );
    }

    #[test]
    fn import_all_day_without_dtend() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n",
            "UID:a@example.com\r\nSUMMARY:All day\r\nDTSTART;VALUE=DATE:20260813\r\n",
            "END:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        let events = parse_ical(ics).unwrap();
        assert!(events[0].all_day);
        assert_eq!(
            events[0].ends_at,
            utc("2026-08-14T00:00:00"),
            "defaults to +1d"
        );
    }

    #[test]
    fn import_interprets_tzid_datetimes() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n",
            "UID:a@example.com\r\nSUMMARY:Tz bound\r\n",
            "DTSTART;TZID=America/New_York:20260813T090000\r\n",
            "DTEND;TZID=America/New_York:20260813T100000\r\n",
            "END:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        let events = parse_ical(ics).unwrap();
        assert_eq!(
            events[0].starts_at,
            utc("2026-08-13T13:00:00"),
            "EDT = UTC-4"
        );
        assert_eq!(events[0].tz.as_deref(), Some("America/New_York"));
    }

    #[test]
    fn import_degrades_unsupported_rrule_to_single_instance() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n",
            "UID:a@example.com\r\nSUMMARY:Ordinal\r\n",
            "DTSTART:20260813T100000Z\r\nDTEND:20260813T110000Z\r\n",
            "RRULE:FREQ=MONTHLY;BYDAY=2MO\r\n",
            "END:VEVENT\r\nEND:VCALENDAR\r\n",
        );
        let events = parse_ical(ics).unwrap();
        assert_eq!(events.len(), 1, "import still succeeds");
        assert!(
            events[0].rrule.is_none(),
            "unsupported rule is dropped, event kept"
        );
    }
}

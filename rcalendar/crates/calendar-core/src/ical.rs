//! RFC 5545 (iCalendar) import/export (S1.6).
//!
//! Import maps `VEVENT` properties onto [`Event`] and ignores unknown
//! properties without failing. Export writes a canonical `VCALENDAR` that
//! round-trips the supported fields losslessly. Text values are unescaped on
//! import and escaped on export; lines are folded at 75 octets per the spec.

use chrono::{Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::{Attendee, AttendeeRole, AttendeeStatus, Event};

/// Parses an RFC 5545 `.ics` string into events.
///
/// - Unknown properties are ignored (the import never fails on extra data).
/// - Events with `STATUS:CANCELLED` are dropped.
/// - A `DTEND` that is missing defaults to `DTSTART + 1h` (timed) or
///   `DTSTART + 1d` (all-day).
/// - Floating times (no `TZID`, no `Z`) are treated as UTC.
pub fn parse_ical(input: &str) -> Result<Vec<Event>> {
    parse_ical_inner(input, false)
}

/// Like [`parse_ical`], but keeps `STATUS:CANCELLED` events (marked with
/// `deleted_at`) so iTIP `METHOD:CANCEL` payloads can be applied.
pub fn parse_ical_including_cancelled(input: &str) -> Result<Vec<Event>> {
    parse_ical_inner(input, true)
}

fn parse_ical_inner(input: &str, include_cancelled: bool) -> Result<Vec<Event>> {
    let mut events = Vec::new();
    for calendar in ical::parser::ical::IcalParser::new(input.as_bytes()) {
        let calendar = calendar.map_err(|e| Error::Ical(e.to_string()))?;
        for vevent in &calendar.events {
            if let Some(event) = parse_vevent(vevent, include_cancelled)? {
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
        write_attendees(&mut out, &event.attendees);
        push_prop(
            &mut out,
            "TRANSP",
            if event.busy { "OPAQUE" } else { "TRANSPARENT" },
        );
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

/// A parsed person line (ATTENDEE or ORGANIZER) before it is bound to an event id.
struct RawAttendee {
    email: String,
    display_name: Option<String>,
    role: AttendeeRole,
    status: AttendeeStatus,
    rsvp: bool,
    is_organizer: bool,
}

fn parse_attendee_prop(prop: &ical::property::Property, is_organizer: bool) -> Result<RawAttendee> {
    let params = prop.params.as_deref().unwrap_or_default();
    let param = |key: &str| {
        params
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .and_then(|(_, v)| v.first())
            .cloned()
    };

    let email = prop
        .value
        .as_deref()
        .map(|v| v.trim().trim_start_matches("mailto:"))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .ok_or_else(|| Error::Ical("person line is missing an email".into()))?;

    let role = param("ROLE")
        .as_deref()
        .map(AttendeeRole::from_rfc)
        .unwrap_or(if is_organizer {
            AttendeeRole::Chair
        } else {
            AttendeeRole::Required
        });
    let status = param("PARTSTAT")
        .as_deref()
        .map(AttendeeStatus::from_rfc)
        .unwrap_or_default();
    let rsvp = param("RSVP").is_some_and(|v| v.eq_ignore_ascii_case("true"));

    Ok(RawAttendee {
        email,
        display_name: param("CN"),
        role,
        status,
        rsvp,
        is_organizer,
    })
}

/// Emits `ATTENDEE` / `ORGANIZER` lines for an event's people (RFC 5545 §3.8.4).
fn write_attendees(out: &mut String, attendees: &[Attendee]) {
    for a in attendees {
        if a.is_organizer {
            let mut line = String::from("ORGANIZER");
            if let Some(cn) = &a.display_name {
                line.push_str(&format!(";CN={}", escape_text(cn)));
            }
            line.push_str(&format!(":mailto:{}", a.email));
            fold_line(out, &line);
            continue;
        }
        let mut line = String::from("ATTENDEE");
        if let Some(cn) = &a.display_name {
            line.push_str(&format!(";CN={}", escape_text(cn)));
        }
        line.push_str(&format!(";ROLE={}", a.role.rfc_value()));
        line.push_str(&format!(";PARTSTAT={}", a.status.rfc_value()));
        if a.rsvp {
            line.push_str(";RSVP=TRUE");
        }
        line.push_str(&format!(":mailto:{}", a.email));
        fold_line(out, &line);
    }
}

fn parse_vevent(
    vevent: &ical::parser::ical::component::IcalEvent,
    include_cancelled: bool,
) -> Result<Option<Event>> {
    let mut uid = None;
    let mut title = None;
    let mut location = None;
    let mut notes = None;
    let mut start = None;
    let mut end = None;
    let mut rrule = None;
    let mut exdates = Vec::new();
    let mut cancelled = false;
    let mut busy = true;
    let mut people = Vec::new();

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
            "ATTENDEE" => people.push(parse_attendee_prop(prop, false)?),
            "ORGANIZER" => people.push(parse_attendee_prop(prop, true)?),
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
            "TRANSP" => {
                busy = !prop
                    .value
                    .as_deref()
                    .is_some_and(|v| v.eq_ignore_ascii_case("TRANSPARENT"));
            }
            // Unknown properties are ignored without failing the import.
            _ => {}
        }
    }

    if cancelled && !include_cancelled {
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

    let event_id = Uuid::new_v4();
    let event = Event {
        id: event_id,
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
        travel_time_minutes: None,
        color: None,
        etag: None,
        attendees: people
            .into_iter()
            .map(|p| Attendee {
                id: Uuid::new_v4(),
                event_id,
                email: p.email,
                display_name: p.display_name,
                role: p.role,
                status: p.status,
                rsvp: p.rsvp,
                is_organizer: p.is_organizer,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                deleted_at: None,
            })
            .collect(),
        busy,
        updated_at: Utc::now(),
        created_at: Utc::now(),
        deleted_at: if cancelled { Some(Utc::now()) } else { None },
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
            travel_time_minutes: None,
            color: None,
            etag: None,
            attendees: vec![],
            busy: true,
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

    // P1.6 hardening: malformed input and VALARM lines are tolerated.
    #[test]
    fn ignores_valarm_and_parses_attendee_lines() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:r@example.com\r\n",
            "DTSTART:20260813T100000Z\r\n",
            "DTEND:20260813T110000Z\r\n",
            "SUMMARY:Meet\r\n",
            "ATTENDEE;CN=Alice;PARTSTAT=ACCEPTED:mailto:alice@example.com\r\n",
            "BEGIN:VALARM\r\n",
            "TRIGGER:-PT15M\r\n",
            "ACTION:DISPLAY\r\n",
            "END:VALARM\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR",
        );
        let events = parse_ical(ics).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Meet");
        assert_eq!(events[0].tz, None, "floating times parse as UTC");
        assert_eq!(events[0].attendees.len(), 1);
        assert_eq!(events[0].attendees[0].email, "alice@example.com");
        assert_eq!(events[0].attendees[0].status, AttendeeStatus::Accepted);
    }

    #[test]
    fn transparency_round_trips() {
        let mut e = sample_event();
        e.busy = false;
        let ics = write_ical(&[e]).unwrap();
        assert!(ics.contains("TRANSP:TRANSPARENT"));
        let imported = parse_ical(&ics).unwrap();
        assert!(!imported[0].busy);

        let mut opaque = sample_event();
        opaque.busy = true;
        let ics = write_ical(&[opaque]).unwrap();
        assert!(ics.contains("TRANSP:OPAQUE"));
        assert!(parse_ical(&ics).unwrap()[0].busy);
    }

    #[test]
    fn attendee_round_trips_through_ical() {
        use crate::model::Attendee;
        let mut e = sample_event();
        e.attendees = vec![
            Attendee {
                id: Uuid::new_v4(),
                event_id: e.id,
                email: "alice@example.com".into(),
                display_name: Some("Alice".into()),
                role: crate::model::AttendeeRole::Required,
                status: crate::model::AttendeeStatus::Accepted,
                rsvp: true,
                is_organizer: false,
                created_at: utc("2026-08-01T00:00:00"),
                updated_at: utc("2026-08-01T00:00:00"),
                deleted_at: None,
            },
            Attendee {
                id: Uuid::new_v4(),
                event_id: e.id,
                email: "carol@example.com".into(),
                display_name: None,
                role: crate::model::AttendeeRole::Optional,
                status: crate::model::AttendeeStatus::Tentative,
                rsvp: false,
                is_organizer: false,
                created_at: utc("2026-08-01T00:00:00"),
                updated_at: utc("2026-08-01T00:00:00"),
                deleted_at: None,
            },
            Attendee {
                id: Uuid::new_v4(),
                event_id: e.id,
                email: "boss@example.com".into(),
                display_name: Some("Boss".into()),
                role: crate::model::AttendeeRole::Chair,
                status: crate::model::AttendeeStatus::NeedsAction,
                rsvp: false,
                is_organizer: true,
                created_at: utc("2026-08-01T00:00:00"),
                updated_at: utc("2026-08-01T00:00:00"),
                deleted_at: None,
            },
        ];

        let ics = write_ical(&[e.clone()]).unwrap();
        assert!(
            ics.contains("ORGANIZER;CN=Boss:mailto:boss@example.com"),
            "organizer emitted: {ics}"
        );
        assert!(
            ics.contains("ATTENDEE;CN=Alice;ROLE=REQ-PARTICIPANT"),
            "attendee params emitted: {ics}"
        );
        assert!(
            ics.contains("PARTSTAT=ACCEPTED;RSVP=TRUE"),
            "attendee status/rsvp emitted: {ics}"
        );

        let imported = parse_ical(&ics).unwrap();
        assert_eq!(imported.len(), 1);
        let got = &imported[0];
        assert_eq!(got.attendees.len(), 3, "all people round-trip");

        let alice = got
            .attendees
            .iter()
            .find(|a| a.email == "alice@example.com")
            .unwrap();
        assert_eq!(alice.display_name.as_deref(), Some("Alice"));
        assert_eq!(alice.role, crate::model::AttendeeRole::Required);
        assert_eq!(alice.status, crate::model::AttendeeStatus::Accepted);
        assert!(alice.rsvp);
        assert!(!alice.is_organizer);
        assert_eq!(
            alice.event_id, got.id,
            "attendee bound to its (imported) event"
        );

        let boss = got
            .attendees
            .iter()
            .find(|a| a.email == "boss@example.com")
            .unwrap();
        assert!(boss.is_organizer);
        assert_eq!(boss.role, crate::model::AttendeeRole::Chair);
    }

    #[test]
    fn import_rules_out_valarm_and_malformed_input() {
        let ics = concat!(
            "BEGIN:VCALENDAR\r\n",
            "VERSION:2.0\r\n",
            "BEGIN:VEVENT\r\n",
            "UID:r@example.com\r\n",
            "DTSTART:20260813T100000Z\r\n",
            "DTEND:20260813T110000Z\r\n",
            "SUMMARY:Meet\r\n",
            "BEGIN:VALARM\r\n",
            "TRIGGER:-PT15M\r\n",
            "ACTION:DISPLAY\r\n",
            "END:VALARM\r\n",
            "END:VEVENT\r\n",
            "END:VCALENDAR",
        );
        let events = parse_ical(ics).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Meet");
        assert_eq!(events[0].tz, None, "floating times parse as UTC");
    }

    #[test]
    fn malformed_input_does_not_panic() {
        let _ = parse_ical("not ical at all");
        let _ = parse_ical("BEGIN:VCALENDAR\r\nVERSION:2.0"); // no VEVENT
        let _ = parse_ical("BEGIN:VEVENT\r\nUID:x\r\n"); // unterminated
    }

    #[test]
    fn rrule_with_count_round_trips() {
        let mut e = sample_event();
        e.rrule = Some("FREQ=DAILY;COUNT=5".into());
        let ics = write_ical(&[e.clone()]).unwrap();
        let imported = parse_ical(&ics).unwrap();
        assert_eq!(imported[0].rrule, e.rrule);
    }
}

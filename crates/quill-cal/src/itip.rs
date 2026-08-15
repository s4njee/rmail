//! iTIP (RFC 5546) and iMIP (RFC 6047) protocol utilities for Quill.

use chrono::{DateTime, Local, Utc};
use chrono_tz::Tz;
use quill_store::{AttendeeInfo, CalendarInvite};

/// Parse an iCalendar invitation string into a structured [`CalendarInvite`].
pub fn parse_itip_invite(ics: &str, user_email: &str) -> Option<CalendarInvite> {
    let mut method = "REQUEST".to_string();
    let mut uid = String::new();
    let mut sequence: u32 = 0;
    let mut title = String::new();
    let mut location: Option<String> = None;
    let mut organizer_email = String::new();
    let mut organizer_name: Option<String> = None;
    let mut user_partstat = "NEEDS-ACTION".to_string();
    let mut attendees: Vec<AttendeeInfo> = Vec::new();
    let mut start_ms: i64 = 0;
    let mut end_ms: i64 = 0;
    let mut all_day = false;
    let mut timezone: Option<String> = None;

    let lines = unfold_lines(ics);

    for line in &lines {
        // Split at the first colon outside a quoted string: a quoted parameter
        // value may itself contain a colon (e.g. ATTENDEE;CN="Doe, John: Manager").
        let (key_part, val) = match split_unquoted_colon(line) {
            Some((k, v)) => (k.trim(), v.trim()),
            None => continue,
        };

        let key = key_part.split(';').next().unwrap_or("").to_uppercase();

        match key.as_str() {
            "METHOD" => {
                method = val.to_uppercase();
            }
            "UID" => {
                uid = val.to_string();
            }
            "SEQUENCE" => {
                sequence = val.parse::<u32>().unwrap_or(0);
            }
            "SUMMARY" => {
                title = unescape_text(val);
            }
            "LOCATION" => {
                location = Some(unescape_text(val));
            }
            "DTSTART" => {
                if let Some((day, ms, tz)) = resolve_datetime(key_part, val) {
                    all_day = day;
                    start_ms = ms;
                    if tz.is_some() {
                        timezone = tz;
                    }
                }
            }
            "DTEND" => {
                if let Some((_, ms, _)) = resolve_datetime(key_part, val) {
                    end_ms = ms;
                }
            }
            "ORGANIZER" => {
                let email = val.trim_start_matches("mailto:").trim_start_matches("MAILTO:").to_string();
                organizer_email = email;
                for param in key_part.split(';').skip(1) {
                    if let Some((pk, pv)) = param.split_once('=') {
                        if pk.eq_ignore_ascii_case("CN") {
                            organizer_name = Some(pv.trim_matches('"').to_string());
                        }
                    }
                }
            }
            "ATTENDEE" => {
                let email = val.trim_start_matches("mailto:").trim_start_matches("MAILTO:").to_string();
                let mut name: Option<String> = None;
                let mut partstat = "NEEDS-ACTION".to_string();
                let mut role: Option<String> = None;

                for param in key_part.split(';').skip(1) {
                    if let Some((pk, pv)) = param.split_once('=') {
                        let clean_val = pv.trim_matches('"');
                        if pk.eq_ignore_ascii_case("CN") {
                            name = Some(clean_val.to_string());
                        } else if pk.eq_ignore_ascii_case("PARTSTAT") {
                            partstat = clean_val.to_uppercase();
                        } else if pk.eq_ignore_ascii_case("ROLE") {
                            role = Some(clean_val.to_string());
                        }
                    }
                }

                if email.eq_ignore_ascii_case(user_email) {
                    user_partstat = partstat.clone();
                }

                attendees.push(AttendeeInfo {
                    name,
                    email,
                    partstat,
                    role,
                });
            }
            _ => {}
        }
    }

    if uid.is_empty() {
        return None;
    }

    // A real invite always has a start time; without one the invite would be
    // presented at the Unix epoch (the previous fallback for an unparseable
    // DTSTART).
    if start_ms == 0 {
        return None;
    }

    if end_ms == 0 {
        end_ms = if all_day {
            start_ms + 86_400_000
        } else {
            start_ms + 3_600_000
        };
    }

    Some(CalendarInvite {
        method,
        uid,
        sequence,
        title: if title.is_empty() { "Untitled Event".into() } else { title },
        start_ms,
        end_ms,
        all_day,
        location,
        organizer_name,
        organizer_email,
        user_partstat,
        attendees,
        raw_ics: ics.to_string(),
        timezone,
    })
}

/// Generates an RFC 6047 iMIP `METHOD:REPLY` payload responding to an invitation.
pub fn generate_imip_reply(
    invite: &CalendarInvite,
    user_email: &str,
    user_name: &str,
    partstat: &str, // "ACCEPTED", "TENTATIVE", "DECLINED"
    comment: Option<&str>,
) -> String {
    let now_str = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut out = String::new();
    out.push_str("BEGIN:VCALENDAR\r\n");
    out.push_str("PRODID:-//Quill//iMIP Client//EN\r\n");
    out.push_str("VERSION:2.0\r\n");
    out.push_str("CALSCALE:GREGORIAN\r\n");
    out.push_str("METHOD:REPLY\r\n");
    out.push_str("BEGIN:VEVENT\r\n");
    out.push_str(&format!("UID:{}\r\n", invite.uid));
    out.push_str(&format!("SEQUENCE:{}\r\n", invite.sequence));
    out.push_str(&format!("DTSTAMP:{}\r\n", now_str));
    out.push_str(&format!("SUMMARY:{}\r\n", escape_text(&invite.title)));

    let start_dt = DateTime::from_timestamp_millis(invite.start_ms).unwrap_or_default();
    let end_dt = DateTime::from_timestamp_millis(invite.end_ms).unwrap_or_default();

    if invite.all_day {
        out.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", start_dt.format("%Y%m%d")));
        out.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", end_dt.format("%Y%m%d")));
    } else {
        out.push_str(&format!("DTSTART:{}\r\n", start_dt.format("%Y%m%dT%H%M%SZ")));
        out.push_str(&format!("DTEND:{}\r\n", end_dt.format("%Y%m%dT%H%M%SZ")));
    }

    if !invite.organizer_email.is_empty() {
        if let Some(org_name) = &invite.organizer_name {
            out.push_str(&format!(
                "ORGANIZER;CN=\"{}\":mailto:{}\r\n",
                escape_text(org_name),
                invite.organizer_email
            ));
        } else {
            out.push_str(&format!("ORGANIZER:mailto:{}\r\n", invite.organizer_email));
        }
    }

    let cn_param = if user_name.is_empty() {
        String::new()
    } else {
        format!(";CN=\"{}\"", escape_text(user_name))
    };
    out.push_str(&format!(
        "ATTENDEE{};PARTSTAT={}:mailto:{}\r\n",
        cn_param,
        partstat.to_uppercase(),
        user_email
    ));

    if let Some(c) = comment {
        if !c.trim().is_empty() {
            out.push_str(&format!("COMMENT:{}\r\n", escape_text(c.trim())));
        }
    }

    out.push_str("END:VEVENT\r\n");
    out.push_str("END:VCALENDAR\r\n");
    out
}

fn unfold_lines(ics: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for line in ics.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.starts_with(' ') || trimmed.starts_with('\t') {
            current.push_str(&trimmed[1..]);
        } else {
            if !current.is_empty() {
                lines.push(current);
            }
            current = trimmed.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Split a property line at the first colon that is not inside a quoted string
/// (iCalendar parameter values can legally contain `:` inside `"…"`).
fn split_unquoted_colon(line: &str) -> Option<(&str, &str)> {
    let mut in_quote = false;
    for (idx, c) in line.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ':' if !in_quote => return Some((&line[..idx], &line[idx + 1..])),
            _ => {}
        }
    }
    None
}

/// Resolve a DTSTART/DTEND value into `(all_day, utc-millis)`, honoring
/// `VALUE=DATE` (all-day), the `TZID` parameter, and the `Z` UTC suffix.
/// A value with no TZID and no `Z` is a floating time — per RFC 5545 that is
/// local to the recipient, so it is interpreted in the machine's local zone.
/// Returns `None` for unparseable values.
fn resolve_datetime(key_part: &str, val: &str) -> Option<(bool, i64, Option<String>)> {
    let params = key_part.split(';').skip(1);
    // Match the exact `VALUE=DATE` token — a substring check also matches
    // `VALUE=DATE-TIME`, mis-flagging timed events as all-day.
    let is_all_day = params.clone().any(|p| p.eq_ignore_ascii_case("VALUE=DATE")) || val.len() == 8;
    if is_all_day {
        let d = chrono::NaiveDate::parse_from_str(val, "%Y%m%d").ok()?;
        let ms = d.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis();
        return Some((true, ms, None));
    }
    let tzid = params
        .filter_map(|p| p.split_once('='))
        .find(|(k, _)| k.eq_ignore_ascii_case("TZID"))
        .map(|(_, v)| v.trim_matches('"').to_string());
    let has_z = val.ends_with('Z');
    let naive = chrono::NaiveDateTime::parse_from_str(val.trim_end_matches('Z'), "%Y%m%dT%H%M%S")
        .ok()?;
    let dt = match tzid.as_deref() {
        Some(name) => naive
            .and_local_timezone(name.parse::<Tz>().ok()?)
            .earliest()?
            .with_timezone(&Utc),
        None if has_z => naive.and_utc(),
        // Floating time: local to the recipient.
        None => naive
            .and_local_timezone(Local)
            .earliest()?
            .with_timezone(&Utc),
    };
    Some((false, dt.timestamp_millis(), tzid))
}

/// Unescape RFC 5545 text in a single left-to-right pass. A global
/// `replace("\\n", …)` chain mis-decodes `\\n` (escaped backslash + literal
/// `n`): the `\\`→`\` pass would first turn it into `\n`, which the `\n` pass
/// then converts to a real newline. Scanning left-to-right keeps each escape
/// consuming exactly its own characters.
fn unescape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn escape_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(';', "\\;")
        .replace(',', "\\,")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_itip_request() {
        let ics = "BEGIN:VCALENDAR\r\n\
METHOD:REQUEST\r\n\
BEGIN:VEVENT\r\n\
UID:meet-12345\r\n\
SEQUENCE:1\r\n\
SUMMARY:Design Sync & Review\r\n\
LOCATION:Room 401\r\n\
DTSTART:20260901T150000Z\r\n\
DTEND:20260901T160000Z\r\n\
ORGANIZER;CN=\"Rosa Delgado\":mailto:rosa@meridianproperty.co\r\n\
ATTENDEE;CN=\"Me\";PARTSTAT=NEEDS-ACTION;ROLE=REQ-PARTICIPANT:mailto:work@quill.app\r\n\
END:VEVENT\r\n\
END:VCALENDAR";

        let invite = parse_itip_invite(ics, "work@quill.app").expect("valid invite");
        assert_eq!(invite.method, "REQUEST");
        assert_eq!(invite.uid, "meet-12345");
        assert_eq!(invite.sequence, 1);
        assert_eq!(invite.title, "Design Sync & Review");
        assert_eq!(invite.location.as_deref(), Some("Room 401"));
        assert_eq!(invite.organizer_email, "rosa@meridianproperty.co");
        assert_eq!(invite.user_partstat, "NEEDS-ACTION");
        assert_eq!(invite.attendees.len(), 1);
    }

    #[test]
    fn test_generate_imip_reply() {
        let invite = CalendarInvite {
            method: "REQUEST".into(),
            uid: "meet-12345".into(),
            sequence: 1,
            title: "Design Sync".into(),
            start_ms: 1700000000000,
            end_ms: 1700003600000,
            all_day: false,
            location: None,
            organizer_name: Some("Rosa".into()),
            organizer_email: "rosa@meridianproperty.co".into(),
            user_partstat: "NEEDS-ACTION".into(),
            attendees: Vec::new(),
            raw_ics: String::new(),
            timezone: None,
        };

        let reply = generate_imip_reply(&invite, "work@quill.app", "David", "ACCEPTED", Some("Looking forward to it"));
        assert!(reply.contains("METHOD:REPLY"));
        assert!(reply.contains("UID:meet-12345"));
        assert!(reply.contains("PARTSTAT=ACCEPTED"));
        assert!(reply.contains("COMMENT:Looking forward to it"));
    }

    #[test]
    fn test_parse_itip_tzid() {
        // A TZID-bound time must be converted through that zone, not read as UTC.
        let ics = "BEGIN:VCALENDAR\r\n\
METHOD:REQUEST\r\n\
BEGIN:VEVENT\r\n\
UID:tzid-meet\r\n\
SUMMARY:Standup\r\n\
DTSTART;TZID=America/New_York:20260901T150000\r\n\
DTEND;TZID=America/New_York:20260901T160000\r\n\
ORGANIZER:mailto:rosa@meridianproperty.co\r\n\
END:VEVENT\r\n\
END:VCALENDAR";
        let invite = parse_itip_invite(ics, "work@quill.app").expect("valid invite");
        // 15:00 America/New_York (EDT, UTC-4) == 19:00 UTC
        let expected = DateTime::parse_from_rfc3339("2026-09-01T19:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(invite.start_ms, expected);
        assert!(!invite.all_day);
    }

    #[test]
    fn test_parse_itip_value_date_time_is_timed() {
        // `VALUE=DATE-TIME` must not be treated as an all-day `VALUE=DATE`.
        let ics = "BEGIN:VCALENDAR\r\n\
METHOD:REQUEST\r\n\
BEGIN:VEVENT\r\n\
UID:dtime\r\n\
SUMMARY:Review\r\n\
DTSTART;VALUE=DATE-TIME:20260901T090000Z\r\n\
DTEND;VALUE=DATE-TIME:20260901T100000Z\r\n\
END:VEVENT\r\n\
END:VCALENDAR";
        let invite = parse_itip_invite(ics, "work@quill.app").expect("valid invite");
        assert!(!invite.all_day);
        let expected = DateTime::parse_from_rfc3339("2026-09-01T09:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(invite.start_ms, expected);
    }

    #[test]
    fn test_parse_itip_quoted_colon_in_param() {
        // A colon inside a quoted CN parameter must not split the property.
        let ics = "BEGIN:VCALENDAR\r\n\
METHOD:REQUEST\r\n\
BEGIN:VEVENT\r\n\
UID:quoted\r\n\
SUMMARY:Meet\r\n\
DTSTART:20260901T150000Z\r\n\
DTEND:20260901T160000Z\r\n\
ATTENDEE;CN=\"Doe, John: Manager\";PARTSTAT=NEEDS-ACTION:mailto:john@example.com\r\n\
END:VEVENT\r\n\
END:VCALENDAR";
        let invite = parse_itip_invite(ics, "work@quill.app").expect("valid invite");
        let attendee = &invite.attendees[0];
        assert_eq!(attendee.email, "john@example.com");
        assert_eq!(attendee.name.as_deref(), Some("Doe, John: Manager"));
    }

    #[test]
    fn test_unescape_text_escaped_backslash() {
        // `\\n` (escaped backslash + literal n) must decode to a backslash and
        // an `n`, not a newline — a global replace-chain would get this wrong.
        assert_eq!(unescape_text(r"C:\\new"), r"C:\new");
        assert_eq!(unescape_text(r"line1\nline2"), "line1\nline2");
    }
}

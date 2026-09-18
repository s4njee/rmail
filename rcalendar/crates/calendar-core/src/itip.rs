//! RFC 5546 (iTIP) and RFC 6047 (iMIP) protocol utilities (T1.10).
//!
//! Provides generation and parsing of iCalendar Transport-Independent Interoperability Protocol
//! messages (`METHOD:REQUEST`, `METHOD:REPLY`, `METHOD:CANCEL`) and email message generation
//! for mail transport integration.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::ical::{parse_ical_including_cancelled, write_ical};
use crate::model::{Attendee, Event};

/// The iTIP method defining the operation being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ItipMethod {
    Request,
    Reply,
    Cancel,
}

impl ItipMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            ItipMethod::Request => "REQUEST",
            ItipMethod::Reply => "REPLY",
            ItipMethod::Cancel => "CANCEL",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_uppercase().as_str() {
            "REPLY" => ItipMethod::Reply,
            "CANCEL" => ItipMethod::Cancel,
            _ => ItipMethod::Request,
        }
    }
}

/// A parsed structured iTIP message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItipMessage {
    pub method: ItipMethod,
    pub uid: String,
    pub sequence: u32,
    pub event: Event,
    pub organizer: Option<Attendee>,
    pub attendees: Vec<Attendee>,
}

/// An email payload formatted for sending an iTIP invitation or response over email (iMIP / RFC 6047).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImipEmailEnvelope {
    pub subject: String,
    pub text_body: String,
    pub ics_content: String,
    pub mime_type: String,
}

/// Generates an RFC 5546 `METHOD:REQUEST` invitation payload.
pub fn generate_itip_request(event: &Event) -> Result<String> {
    let raw_ics = write_ical(std::slice::from_ref(event))?;
    Ok(insert_method_header(&raw_ics, "REQUEST"))
}

/// Generates an RFC 5546 `METHOD:REPLY` RSVP reply payload for a specific attendee.
pub fn generate_itip_reply(event: &Event, responding_attendee: &Attendee) -> Result<String> {
    let mut reply_event = event.clone();
    // In a reply, the only attendee listed is the one responding
    reply_event.attendees = vec![responding_attendee.clone()];
    let raw_ics = write_ical(&[reply_event])?;
    Ok(insert_method_header(&raw_ics, "REPLY"))
}

/// Generates an RFC 5546 `METHOD:CANCEL` cancellation payload.
pub fn generate_itip_cancel(event: &Event) -> Result<String> {
    let raw_ics = write_ical(std::slice::from_ref(event))?;
    let mut out = insert_method_header(&raw_ics, "CANCEL");
    // Ensure STATUS:CANCELLED is in the VEVENT
    if !out.contains("STATUS:CANCELLED") {
        out = out.replace("BEGIN:VEVENT\r\n", "BEGIN:VEVENT\r\nSTATUS:CANCELLED\r\n");
    }
    Ok(out)
}

/// Parses an RFC 5546 iTIP message from an `.ics` string.
pub fn parse_itip_message(ics: &str) -> Result<ItipMessage> {
    let mut method = ItipMethod::Request;
    let mut sequence = 0u32;
    for line in ics.lines() {
        let trimmed = line.trim();
        let upper = trimmed.to_ascii_uppercase();
        if upper.starts_with("METHOD:") {
            if let Some((_, m)) = trimmed.split_once(':') {
                method = ItipMethod::parse(m);
            }
        } else if upper.starts_with("SEQUENCE:") {
            if let Some((_, s)) = trimmed.split_once(':') {
                sequence = s.trim().parse().unwrap_or(0);
            }
        }
    }

    let events = parse_ical_including_cancelled(ics)?;
    let event = events
        .into_iter()
        .next()
        .ok_or_else(|| Error::Ical("no VEVENT found in iTIP payload".into()))?;

    let organizer = event.attendees.iter().find(|a| a.is_organizer).cloned();
    let attendees = event
        .attendees
        .iter()
        .filter(|a| !a.is_organizer)
        .cloned()
        .collect();

    Ok(ItipMessage {
        method,
        uid: event.uid.clone(),
        sequence,
        event,
        organizer,
        attendees,
    })
}

/// Generates an iMIP email envelope (subject, human-readable body, and attached iTIP `.ics`).
pub fn generate_imip_email_invitation(
    event: &Event,
    organizer_name_or_email: &str,
) -> Result<ImipEmailEnvelope> {
    let ics = generate_itip_request(event)?;
    let when_str = if event.all_day {
        event
            .starts_at
            .format("%A, %B %d, %Y (All day)")
            .to_string()
    } else {
        format!(
            "{} - {}",
            event.starts_at.format("%A, %B %d, %Y from %H:%M"),
            event.ends_at.format("%H:%M UTC")
        )
    };

    let subject = format!("Invitation: {} @ {}", event.title, when_str);
    let mut body = format!(
        "You have been invited to \"{}\" by {}.\n\nWhen: {}\n",
        event.title, organizer_name_or_email, when_str
    );

    if let Some(loc) = &event.location {
        body.push_str(&format!("Where: {}\n", loc));
    }
    if let Some(notes) = &event.notes {
        body.push_str(&format!("\nDescription:\n{}\n", notes));
    }

    body.push_str("\n--\nSent with Almanac");

    Ok(ImipEmailEnvelope {
        subject,
        text_body: body,
        ics_content: ics,
        mime_type: "text/calendar; method=REQUEST; charset=UTF-8".into(),
    })
}

/// Generates an iMIP envelope for an RSVP reply.
pub fn generate_imip_email_reply(
    event: &Event,
    responding_attendee: &Attendee,
) -> Result<ImipEmailEnvelope> {
    let ics = generate_itip_reply(event, responding_attendee)?;
    let status = responding_attendee.status.rfc_value();
    let who = responding_attendee
        .display_name
        .as_deref()
        .unwrap_or(&responding_attendee.email);
    Ok(ImipEmailEnvelope {
        subject: format!("{status}: {}", event.title),
        text_body: format!("{who} responded {status} to \"{}\".\n", event.title),
        ics_content: ics,
        mime_type: "text/calendar; method=REPLY; charset=UTF-8".into(),
    })
}

/// Generates an iMIP envelope for a cancellation.
pub fn generate_imip_email_cancel(
    event: &Event,
    organizer_name_or_email: &str,
) -> Result<ImipEmailEnvelope> {
    let ics = generate_itip_cancel(event)?;
    Ok(ImipEmailEnvelope {
        subject: format!("Cancelled: {}", event.title),
        text_body: format!(
            "\"{}\" has been cancelled by {}.\n",
            event.title, organizer_name_or_email
        ),
        ics_content: ics,
        mime_type: "text/calendar; method=CANCEL; charset=UTF-8".into(),
    })
}

fn insert_method_header(raw_ics: &str, method: &str) -> String {
    if raw_ics.contains("METHOD:") {
        return raw_ics.to_string();
    }
    // Place METHOD: right after VERSION:2.0
    if let Some(pos) = raw_ics.find("VERSION:2.0\r\n") {
        let insert_at = pos + "VERSION:2.0\r\n".len();
        let mut result = String::with_capacity(raw_ics.len() + 32);
        result.push_str(&raw_ics[..insert_at]);
        result.push_str(&format!("METHOD:{method}\r\n"));
        result.push_str(&raw_ics[insert_at..]);
        result
    } else {
        raw_ics.replace(
            "BEGIN:VCALENDAR\r\n",
            &format!("BEGIN:VCALENDAR\r\nMETHOD:{method}\r\n"),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AttendeeRole, AttendeeStatus};
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn sample_event() -> Event {
        let s = Utc.from_utc_datetime(&"2026-08-25T14:00:00".parse().unwrap());
        let e = Utc.from_utc_datetime(&"2026-08-25T15:00:00".parse().unwrap());
        let event_id = Uuid::new_v4();
        Event {
            id: event_id,
            calendar_id: Uuid::new_v4(),
            uid: "itip-test-1@almanac.local".into(),
            title: "Roadmap Sync".into(),
            location: Some("Room 4B".into()),
            notes: Some("Quarterly review".into()),
            starts_at: s,
            ends_at: e,
            all_day: false,
            tz: None,
            rrule: None,
            exdates: vec![],
            travel_time_minutes: None,
            color: None,
            etag: None,
            busy: true,
            attendees: vec![
                Attendee {
                    id: Uuid::new_v4(),
                    event_id,
                    email: "organizer@example.com".into(),
                    display_name: Some("Organizer".into()),
                    role: AttendeeRole::Chair,
                    status: AttendeeStatus::Accepted,
                    rsvp: false,
                    is_organizer: true,
                    created_at: s,
                    updated_at: s,
                    deleted_at: None,
                },
                Attendee {
                    id: Uuid::new_v4(),
                    event_id,
                    email: "alice@example.com".into(),
                    display_name: Some("Alice".into()),
                    role: AttendeeRole::Required,
                    status: AttendeeStatus::NeedsAction,
                    rsvp: true,
                    is_organizer: false,
                    created_at: s,
                    updated_at: s,
                    deleted_at: None,
                },
            ],
            created_at: s,
            updated_at: s,
            deleted_at: None,
        }
    }

    #[test]
    fn generates_and_parses_itip_request() {
        let event = sample_event();
        let ics = generate_itip_request(&event).unwrap();
        assert!(ics.contains("METHOD:REQUEST"));
        assert!(ics.contains("ORGANIZER;CN=Organizer:mailto:organizer@example.com"));
        assert!(ics.contains("alice@example.com"));

        let parsed = parse_itip_message(&ics).unwrap();
        assert_eq!(parsed.method, ItipMethod::Request);
        assert_eq!(parsed.uid, "itip-test-1@almanac.local");
        assert_eq!(parsed.event.title, "Roadmap Sync");
        assert!(parsed.organizer.is_some());
        assert_eq!(parsed.organizer.unwrap().email, "organizer@example.com");
        assert_eq!(parsed.attendees.len(), 1);
        assert_eq!(parsed.attendees[0].email, "alice@example.com");
    }

    #[test]
    fn generates_and_parses_itip_reply() {
        let event = sample_event();
        let mut alice = event.attendees[1].clone();
        alice.status = AttendeeStatus::Accepted;

        let reply_ics = generate_itip_reply(&event, &alice).unwrap();
        assert!(reply_ics.contains("METHOD:REPLY"));
        assert!(reply_ics.contains("PARTSTAT=ACCEPTED"));

        let parsed = parse_itip_message(&reply_ics).unwrap();
        assert_eq!(parsed.method, ItipMethod::Reply);
        assert_eq!(parsed.attendees.len(), 1);
        assert_eq!(parsed.attendees[0].status, AttendeeStatus::Accepted);
    }

    #[test]
    fn generates_and_parses_itip_cancel() {
        let event = sample_event();
        let cancel_ics = generate_itip_cancel(&event).unwrap();
        assert!(cancel_ics.contains("METHOD:CANCEL"));
        assert!(cancel_ics.contains("STATUS:CANCELLED"));

        let parsed = parse_itip_message(&cancel_ics).unwrap();
        assert_eq!(parsed.method, ItipMethod::Cancel);
        assert_eq!(parsed.uid, "itip-test-1@almanac.local");
        assert!(parsed.event.deleted_at.is_some());
    }

    #[test]
    fn generates_imip_email_envelope() {
        let event = sample_event();
        let envelope = generate_imip_email_invitation(&event, "organizer@example.com").unwrap();
        assert!(envelope.subject.contains("Invitation: Roadmap Sync"));
        assert!(envelope.text_body.contains("Room 4B"));
        assert!(envelope.text_body.contains("Quarterly review"));
        assert!(envelope.ics_content.contains("METHOD:REQUEST"));
        assert_eq!(
            envelope.mime_type,
            "text/calendar; method=REQUEST; charset=UTF-8"
        );
    }
}

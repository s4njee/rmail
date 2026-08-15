//! `.eml` / mbox import (backlog.md P1.6).
//!
//! Parses external messages with `mail-parser` and inserts them through the
//! store (which dedups by Message-ID), splitting an mbox into individual raw
//! messages first.

use chrono::Utc;
use quill_store::sqlite::SqliteStore;
use quill_store::types::AccountId;

/// Split an mbox file into individual raw messages on the `From ` separator
/// lines that begin a message. mbox-escaped `>From ` lines are passed through.
pub fn parse_mbox(raw: &str) -> Vec<&str> {
    let bytes = raw.as_bytes();
    let mut messages = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < raw.len() {
        let line_start = i;
        let mut line_end = i;
        while line_end < raw.len() && bytes[line_end] != b'\n' {
            line_end += 1;
        }
        let is_from_line =
            line_end - line_start >= 5 && &raw[line_start..line_start + 5] == "From ";
        // A message boundary is a "From " line at the very start or right after
        // a newline (i.e. the start of a line).
        if is_from_line && (start == 0 || line_start == 0 || bytes[line_start - 1] == b'\n') {
            if line_start > start {
                messages.push(&raw[start..line_start]);
            }
            start = line_start;
        }
        i = if line_end < raw.len() { line_end + 1 } else { line_end };
    }
    if start < raw.len() {
        messages.push(&raw[start..]);
    }
    messages
}

/// Parse a single raw message and insert it into the store. Returns `Ok(true)`
/// when imported, `Ok(false)` when it was a Message-ID duplicate.
pub fn import_eml(
    store: &SqliteStore,
    account_id: AccountId,
    folder: &str,
    raw: &str,
) -> Result<bool, String> {
    let parsed = mail_parser::MessageParser::default()
        .parse(raw.as_bytes())
        .ok_or_else(|| "couldn't parse message".to_string())?;

    let sender = parsed
        .from()
        .and_then(|f| f.first())
        .and_then(|a| a.address().map(str::to_string))
        .unwrap_or_default();

    let mut recipients: Vec<(String, String)> = Vec::new();
    for (kind, list) in [
        ("to", parsed.to()),
        ("cc", parsed.cc()),
        ("bcc", parsed.bcc()),
    ] {
        if let Some(list) = list {
            for a in list.iter() {
                if let Some(addr) = a.address() {
                    recipients.push((kind.to_string(), addr.to_string()));
                }
            }
        }
    }

    let subject = parsed.subject().unwrap_or_default().to_string();
    let body = parsed.body_text(0).unwrap_or_default().to_string();
    let received_at_ms = parsed
        .date()
        .map(|d| d.to_timestamp() * 1000)
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    let message_id = parsed.message_id().map(str::to_string);

    store.import_message(
        account_id,
        folder,
        &sender,
        &recipients,
        &subject,
        &body,
        received_at_ms,
        message_id.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_mbox_on_from_lines() {
        let raw = "From a@x.com Thu Jan  1 00:00:00 2026\r\n\
                   Subject: one\r\n\r\nbody one\r\n\r\n\
                   From b@x.com Thu Jan  2 00:00:00 2026\r\n\
                   Subject: two\r\n\r\nbody two\r\n";
        let msgs = parse_mbox(raw);
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].contains("Subject: one"));
        assert!(msgs[1].contains("Subject: two"));
    }

    #[test]
    fn single_eml_is_one_message() {
        let raw = "Subject: single\r\n\r\njust one message";
        let msgs = parse_mbox(raw);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].contains("Subject: single"));
    }
}

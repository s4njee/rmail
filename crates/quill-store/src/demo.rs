//! The `--demo` seed (Epic 3.4): the exact mock content from README §Content
//! — 9 messages, 3 accounts, 5 folders — plus the small amount of filler
//! needed to reproduce the mock's folder badge numbers (Inbox 12, Starred 4,
//! Drafts 2). Everything here is placeholder fiction, as the handoff states.
//!
//! The 9 canonical messages are marked with a `// §` comment; the rest is
//! filler so the sidebar badges match the mock.

use crate::store::{Message, StoreData};
use crate::types::*;
use std::path::PathBuf;

/// 2026-08-13T00:00:00Z in Unix millis — the seed's "today", so the mock's
/// relative times (11:38, Yest, Tue, Mon) sort and display correctly.
const BASE_MS: i64 = 1_786_579_200_000;
const DAY_MS: i64 = 86_400_000;

fn ts(day_offset: i32, hour: u32, minute: u32) -> i64 {
    BASE_MS
        + (day_offset as i64 * DAY_MS)
        + (i64::from(hour) * 3600 + i64::from(minute) * 60) * 1000
}

fn recipient(name: &str, address: &str) -> Recipient {
    Recipient {
        name: name.to_string(),
        address: address.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn msg(
    id: MessageId,
    account_id: AccountId,
    folder: &str,
    from: (&str, &str),
    subject: &str,
    snippet: &str,
    day_offset: i32,
    hour: u32,
    minute: u32,
    unread: bool,
    flagged: bool,
    body: Vec<&str>,
    body_html: Option<&str>,
    to: Vec<Recipient>,
    attachments: Vec<Attachment>,
) -> Message {
    Message {
        id,
        account_id,
        folder: folder.to_string(),
        sender_name: from.0.to_string(),
        sender_address: from.1.to_string(),
        subject: subject.to_string(),
        snippet: snippet.to_string(),
        body: body.into_iter().map(str::to_string).collect(),
        body_html: body_html.map(str::to_string),
        to,
        cc: vec![],
        received_at_ms: ts(day_offset, hour, minute),
        unread,
        flagged,
        attachments,
    }
}

/// The demo accounts — the mock's three accounts, exact copy.
pub(crate) fn demo_accounts() -> Vec<Account> {
    vec![
        Account {
            id: 1,
            address: "work@quill.app".into(),
            protocol: "IMAP".into(),
            sync_mode: "every 2 min".into(),
            color: "#3b5bdb".into(),
            local_bytes: 218 * 1024 * 1024,
            connected: true,
            server: "imap.quill.app".into(),
            port: 993,
            tls: true,
            folder_count: 3, // only this account shows its count, per the mock
        },
        Account {
            id: 2,
            address: "rosa.personal@fastmail.com".into(),
            protocol: "IMAP".into(),
            sync_mode: "on open".into(),
            color: "#0f766e".into(),
            local_bytes: 141 * 1024 * 1024,
            connected: true,
            server: "imap.fastmail.com".into(),
            port: 993,
            tls: true,
            folder_count: 0,
        },
        Account {
            id: 3,
            address: "meridian.board@proton.me".into(),
            protocol: "Bridge".into(),
            sync_mode: "manual".into(),
            color: "#b4451f".into(),
            local_bytes: 53 * 1024 * 1024,
            connected: false,
            server: "bridge.proton.me".into(),
            port: 1143,
            tls: true,
            folder_count: 0,
        },
    ]
}

/// The demo messages (the mock's list + filler for the badge numbers) with
/// their bodies, recipients, and the HTML demo.
pub(crate) fn demo_messages() -> Vec<Message> {
    let lease = Attachment {
        id: 1,
        message_id: 1,
        filename: "meridian-lease-v4.pdf".into(),
        size_bytes: 253_952, // 248 KB, per the mock
        on_disk: true,
    };

    // § 1 doubles as the HTML-mail demo (Epic 7.3): a link and a remote
    // image exercise the sandboxed-iframe pipeline and the per-sender
    // "Load images" affordance.
    let demo_html = r#"<p>Hi both,</p>
<p>Attached is the <a href="https://example.com/meridian-lease">redlined lease</a> with the changes we discussed on Tuesday. The two open items are the escalation clause in 4.2 and the sublet language in 9. Everything else matches the term sheet.</p>
<p>I'd like to send this to their counsel Friday morning, so any comments by Thursday noon would be ideal.</p>
<p>Thanks,<br>Rosa</p>
<img src="https://picsum.photos/seed/quill/640/360" alt="floor plan">"#;

    vec![
        // § 1 — the open message, with the designed body and attachment.
        // Not starred: the mock stars 4 messages elsewhere (2, 4, 6, 12).
        msg(
            1,
            1,
            "Inbox",
            ("Rosa Delgado", "rosa@meridianproperty.co"),
            "Draft agreement for the Meridian lease",
            "Attached is the redlined lease with the changes we",
            0, 11, 38,
            true,
            false,
            vec![
                "Hi both,",
                "Attached is the redlined lease with the changes we discussed on Tuesday. The two open items are the escalation clause in 4.2 and the sublet language in 9. Everything else matches the term sheet.",
                "I'd like to send this to their counsel Friday morning, so any comments by Thursday noon would be ideal.",
                "Thanks,\nRosa",
            ],
            Some(demo_html),
            vec![
                recipient("me", "work@quill.app"),
                recipient("David Okoye", "david.okoye@okoyeassociates.com"),
            ],
            vec![lease],
        ),
        // § 2
        msg(
            2, 1, "Inbox", ("David Okoye", "david.okoye@okoyeassociates.com"),
            "Re: escalation clause in 4.2",
            "I think we can live with 3% if they drop the",
            0, 10, 52, true, true,
            vec!["I think we can live with 3% if they drop the hard cap in 9.", "Let's confirm with Rosa before Friday."],
            None,
            vec![recipient("me", "work@quill.app")],
            vec![],
        ),
        // § 3
        msg(
            3, 2, "Inbox", ("Fastmail", "security@fastmail.com"),
            "New sign-in from Lisbon",
            "A device signed in to your account at 09:14 UTC",
            0, 9, 14, false, false,
            vec!["A device signed in to your account at 09:14 UTC.", "If this was you, no action needed."],
            None,
            vec![recipient("me", "rosa.personal@fastmail.com")],
            vec![],
        ),
        // § 4
        msg(
            4, 3, "Inbox", ("Meridian Board", "board@meridianproperty.co"),
            "Agenda — September meeting",
            "Three items so far: budget, the lease, and the",
            -1, 17, 0, true, true,
            vec!["Three items so far: budget, the lease, and the sublet policy.", "Please add anything else by the 20th."],
            None,
            vec![recipient("me", "meridian.board@proton.me")],
            vec![],
        ),
        // § 5
        msg(
            5, 2, "Inbox", ("Priya Raman", "priya.raman@gmail.com"),
            "Photos from the weekend",
            "Sending the ones that came out well, the rest are",
            -1, 15, 30, false, false,
            vec!["Sending the ones that came out well, the rest are a bit blurry."],
            None,
            vec![recipient("me", "rosa.personal@fastmail.com")],
            vec![],
        ),
        // § 6
        msg(
            6, 1, "Inbox", ("Ledger", "invoices@ledger.app"),
            "Invoice 2841 paid",
            "€4,200.00 received from Meridian Property Co",
            -2, 14, 0, false, true,
            vec!["€4,200.00 received from Meridian Property Co."],
            None,
            vec![recipient("me", "work@quill.app")],
            vec![],
        ),
        // § 7
        msg(
            7, 1, "Inbox", ("Tomás Ferreira", "tomas.ferreira@northpoint.dev"),
            "Re: Thursday walkthrough",
            "11am works. I'll bring the survey and the older",
            -2, 11, 20, false, false,
            vec!["11am works. I'll bring the survey and the older floor plans."],
            None,
            vec![recipient("me", "work@quill.app")],
            vec![],
        ),
        // § 8
        msg(
            8, 1, "Inbox", ("Hannah Weiss", "hannah.weiss@sublethq.com"),
            "Quick question about the sublet language",
            "Section 9 reads like it forbids assignment",
            -3, 16, 45, false, false,
            vec!["Section 9 reads like it forbids assignment even with written consent — is that intended?"],
            None,
            vec![recipient("me", "work@quill.app")],
            vec![],
        ),
        // § 9
        msg(
            9, 3, "Inbox", ("Proton", "noreply@protonmail.com"),
            "Bridge update available",
            "Version 3.14 improves sync on slow connections",
            -3, 9, 30, false, false,
            vec!["Version 3.14 improves sync on slow connections."],
            None,
            vec![recipient("me", "meridian.board@proton.me")],
            vec![],
        ),
        // Filler — older Inbox messages so the Inbox badge (12) matches the
        // mock. Below the visible fold at 800px.
        msg(
            10, 1, "Inbox", ("David Okoye", "david.okoye@okoyeassociates.com"),
            "Re: draft agreement",
            "Rosa sent the latest over; the floor plan",
            -4, 13, 10, false, false,
            vec!["Rosa sent the latest over; the floor plan is attached."],
            None,
            vec![recipient("me", "work@quill.app")],
            vec![],
        ),
        msg(
            11, 1, "Inbox", ("Hannah Weiss", "hannah.weiss@sublethq.com"),
            "Section 9 — one more look",
            "Per our call, attached is the redline on",
            -6, 10, 0, false, false,
            vec!["Per our call, attached is the redline on assignment."],
            None,
            vec![recipient("me", "work@quill.app")],
            vec![],
        ),
        msg(
            12, 3, "Inbox", ("Meridian Board", "board@meridianproperty.co"),
            "Q3 budget review",
            "Draft budget for review ahead of the",
            -7, 9, 0, false, true,
            vec!["Draft budget for review ahead of the September meeting."],
            None,
            vec![recipient("me", "meridian.board@proton.me")],
            vec![],
        ),
        // Filler — two drafts so the Drafts badge (2) matches the mock.
        msg(
            13, 1, "Drafts", ("work@quill.app", "work@quill.app"),
            "Re: escalation clause in 4.2",
            "David, before we send — can you confirm",
            -1, 20, 5, false, false,
            vec!["David, before we send — can you confirm the 3% floor is final?"],
            None,
            vec![recipient("David Okoye", "david.okoye@okoyeassociates.com")],
            vec![],
        ),
        msg(
            14, 1, "Drafts", ("work@quill.app", "work@quill.app"),
            "Follow-up: Meridian lease terms",
            "Following up on the term sheet before Friday",
            -2, 18, 30, false, false,
            vec!["Following up on the term sheet before Friday's deadline."],
            None,
            vec![recipient("Meridian Board", "board@meridianproperty.co")],
            vec![],
        ),
    ]
}

/// A few demo calendar events around "today" (Epic 14) so the calendar views
/// have something to show.
pub(crate) fn demo_events() -> Vec<CalendarEvent> {
    vec![
        CalendarEvent {
            id: 1,
            account_id: 1,
            title: "Thursday walkthrough".into(),
            start_ms: ts(0, 11, 0),
            end_ms: ts(0, 12, 0),
            all_day: false,
            location: Some("Site — Meridian Plaza".into()),
            notes: None,
        },
        CalendarEvent {
            id: 2,
            account_id: 1,
            title: "Lease counsel call".into(),
            start_ms: ts(0, 14, 0),
            end_ms: ts(0, 14, 30),
            all_day: false,
            location: None,
            notes: None,
        },
        CalendarEvent {
            id: 3,
            account_id: 1,
            title: "September board meeting".into(),
            start_ms: ts(1, 10, 0),
            end_ms: ts(1, 11, 0),
            all_day: false,
            location: None,
            notes: None,
        },
        CalendarEvent {
            id: 4,
            account_id: 2,
            title: "Photos — print pick-up".into(),
            start_ms: ts(2, 15, 0),
            end_ms: ts(2, 16, 0),
            all_day: false,
            location: None,
            notes: None,
        },
    ]
}

/// Build the seeded store data for the in-memory store. Writes attachment
/// files under `attachments_root` so "cached locally" is real.
pub(crate) fn demo_data(attachments_root: PathBuf) -> StoreData {
    let accounts = demo_accounts();
    let messages = demo_messages();

    // Write the demo attachment so "cached locally" is backed by a real file.
    let lease_dir = attachments_root.join("1");
    std::fs::create_dir_all(&lease_dir).ok();
    std::fs::write(
        lease_dir.join("meridian-lease-v4.pdf"),
        crate::pdf::placeholder(253_952),
    )
    .ok();

    StoreData {
        accounts,
        messages,
        events: demo_events(),
        attachments_root: Some(attachments_root),
        next_account_id: 3, // the seed's three accounts take ids 1–3
        next_event_id: 4,
    }
}

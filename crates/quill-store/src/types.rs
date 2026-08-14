//! The IPC contract — every type that crosses the Tauri boundary.
//!
//! These are the single source of truth for what the frontend receives and
//! sends (Epic 3.1). They are `ts-rs`-annotated: `src/lib/ipc/` is generated
//! from them, so the TS types are never hand-duplicated. Run
//! `scripts/gen-ipc-types.sh` to regenerate.
//!
//! Payload discipline (Epic 3.3) lives in the shapes themselves:
//! [`MessageRow`] carries only what a list row renders — bodies are fetched
//! on selection as [`MessageDetail`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub type AccountId = u32;
pub type FolderId = u32;
pub type MessageId = u32;
pub type AttachmentId = u32;
pub type EventId = u32;

/// An email account (sidebar, Settings → Accounts).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct Account {
    pub id: AccountId,
    pub address: String,
    /// `"IMAP"` or `"Bridge"`.
    pub protocol: String,
    /// `"every 2 min"`, `"on open"`, or `"manual"`.
    pub sync_mode: String,
    /// Account dot color, hex (e.g. `"#3b5bdb"`).
    pub color: String,
    /// On-disk bytes for this account (footprint + Settings → Accounts).
    /// `number` in TS: crosses IPC as a JSON number.
    #[ts(type = "number")]
    pub local_bytes: u64,
    /// Auth state for the account row — a boolean, never a credential.
    pub connected: bool,
    /// IMAP server hostname.
    pub server: String,
    /// IMAP server port.
    pub port: u16,
    /// Use TLS on the connection.
    pub tls: bool,
    /// Number of folders the account has configured (0 = not shown in the
    /// Settings detail line).
    pub folder_count: u32,
}

/// The fields the add-account form collects (Epic 10.4). The password is a
/// separate argument to the command and goes straight to the OS keychain —
/// it is deliberately not part of this type, so it cannot cross IPC back out.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct NewAccount {
    pub address: String,
    /// `"IMAP"` or `"Bridge"`.
    pub protocol: String,
    pub server: String,
    pub port: u16,
    pub tls: bool,
    /// `"every 2 min"`, `"on open"`, or `"manual"`.
    pub sync_mode: String,
}

/// A sidebar folder (unified set) with its live counts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct Folder {
    pub id: FolderId,
    pub name: String,
    pub kind: FolderKind,
    pub unread_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FolderKind {
    Inbox,
    Starred,
    Drafts,
    Sent,
    Archive,
}

/// Everything a message-list row renders — and nothing else (Epic 3.3).
/// Bodies are fetched only on selection via [`MessageDetail`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct MessageRow {
    pub id: MessageId,
    pub account_id: AccountId,
    /// Folder the message currently lives in (`"Inbox"`, `"Drafts"`, …).
    pub folder: String,
    pub sender_name: String,
    pub sender_address: String,
    pub subject: String,
    pub snippet: String,
    /// Unix millis. `number` in TS (JSON number, not bigint).
    #[ts(type = "number")]
    pub received_at_ms: i64,
    pub unread: bool,
    pub flagged: bool,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct Recipient {
    pub name: String,
    pub address: String,
}

/// A message attachment card. The bytes travel over the asset protocol, not
/// through IPC (Epic 3.3); [`Attachment::on_disk`] drives the
/// "cached locally" label.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct Attachment {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub filename: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub on_disk: bool,
}

/// The full message, fetched on selection. The body is plain-text paragraphs
/// and/or sanitized HTML (Epic 7.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct MessageDetail {
    pub row: MessageRow,
    pub to: Vec<Recipient>,
    pub cc: Vec<Recipient>,
    pub body: Vec<String>,
    /// Sanitized HTML body, when the message arrived as HTML. Populated
    /// server-side of the IPC boundary (Epic 7.3): the frontend never sees
    /// raw mail HTML.
    pub body_html: Option<String>,
    /// Remote images parked in `body_html` — drives the "Load images"
    /// affordance (Epic 7.3).
    pub remote_image_count: u32,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct MessagePage {
    pub items: Vec<MessageRow>,
    pub total: u32,
}

/// List query. Filtering is by folder and/or account; the unified Inbox is a
/// query with `folder` unset.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct MessageQuery {
    pub folder: Option<String>,
    pub account_id: Option<AccountId>,
    pub offset: u32,
    pub limit: u32,
}

/// Outgoing mail. The transport lands in Epic 12; the contract is fixed here.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct OutgoingMessage {
    pub account_id: AccountId,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
}

/// A locally-stored draft (Epic 13.2). `id` is set when re-saving an existing
/// autosaved draft, so it updates in place in the Drafts folder.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct Draft {
    pub id: Option<MessageId>,
    pub account_id: AccountId,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
}

/// Calendar event. Recurrence is expanded in Rust (Epic 14) — the frontend
/// only ever sees resolved instances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CalendarEvent {
    pub id: EventId,
    pub account_id: AccountId,
    pub title: String,
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub end_ms: i64,
    pub all_day: bool,
    pub location: Option<String>,
    pub notes: Option<String>,
}

/// Push events the frontend subscribes to (Epic 3.2) — the frontend never
/// polls; sync/connectivity/footprint arrive here as deltas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum StoreEvent {
    Connectivity(ConnectivityUpdate),
    Footprint(FootprintUpdate),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ConnectivityUpdate {
    /// `"offline"` | `"syncing"` | `"synced"`.
    pub state: String,
    #[ts(type = "number | null")]
    pub last_synced_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct FootprintUpdate {
    #[ts(type = "number")]
    pub on_disk_bytes: u64,
}

/// App settings (Epic 3.1 / 10). Today: the global treatment (D4) and the
/// persisted pane widths (Epic 4.2). `None` widths mean "use the theme
/// default" (`--sidebar-w` / `--list-w`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppSettings {
    /// `"hairline"` | `"banded"`.
    pub theme: String,
    /// Persisted sidebar width in CSS px, if the user resized it.
    pub sidebar_width: Option<u32>,
    /// Persisted message-list width in CSS px, if the user resized it.
    pub list_width: Option<u32>,
    /// Sender addresses whose remote images load by default (Epic 7.3) — the
    /// per-sender "Load images" memory, never global-only.
    pub trusted_image_senders: Vec<String>,
}

impl Default for AppSettings {
    /// First launch defaults to Hairline (Epic 2.3); a corrupt or missing
    /// settings file falls back here silently.
    fn default() -> Self {
        Self {
            theme: "hairline".to_string(),
            sidebar_width: None,
            list_width: None,
            trusted_image_senders: Vec::new(),
        }
    }
}

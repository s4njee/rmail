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
    /// Last sync or connection error message, if any.
    pub last_error: Option<String>,
}

impl Account {
    /// Whether this account authenticates via OAuth2 (XOAUTH2) rather than a
    /// plain password. Detected from the protocol string set at creation
    /// ("Google (OAuth2)" / "Microsoft 365 (OAuth2)").
    pub fn is_oauth(&self) -> bool {
        self.protocol.contains("OAuth")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ActionType {
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
    Archive,
    Delete,
    Move,
    MarkJunk,
    MarkNotJunk,
    Send,
    MarkAnswered,
    MarkForwarded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct QueuedAction {
    #[ts(type = "number")]
    pub id: i64,
    pub account_id: AccountId,
    pub action_type: ActionType,
    pub folder: String,
    pub uid: Option<u32>,
    pub payload: Option<String>,
    #[ts(type = "number")]
    pub created_at_ms: i64,
    pub retries: u32,
    /// The last replay failure, when the action couldn't be applied (P0.3).
    /// `None` = pending or successful.
    pub last_error: Option<String>,
}

/// One triage action applied to many messages at once (P1.1). `Move` carries
/// its destination folder as the command's `destination` argument.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum BulkAction {
    MarkRead,
    MarkUnread,
    Star,
    Unstar,
    Archive,
    Delete,
    MarkJunk,
    MarkNotJunk,
    Move,
}

/// Result of a bulk action — counts, so the UI can report partial failures.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BulkActionResult {
    #[ts(type = "number")]
    pub ok: u32,
    #[ts(type = "number")]
    pub failed: u32,
    pub errors: Vec<String>,
}

/// A send-later message waiting in the durable Outbox (P1.1). The full
/// outgoing payload never crosses IPC — it lives in the store and is read only
/// by the flusher; the UI sees the display fields + the composer snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScheduledMessage {
    #[ts(type = "number")]
    pub id: i64,
    pub account_id: AccountId,
    #[ts(type = "number")]
    pub send_at_ms: i64,
    pub subject: String,
    pub to: Vec<String>,
    #[ts(type = "number")]
    pub created_at_ms: i64,
    /// Serialized composer snapshot so Edit can reopen the composer.
    pub draft: String,
}

/// A recipient suggestion for the composer (P1.2): deduplicated by address
/// from mail history, ranked by frequency and recency.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContactSuggestion {
    pub name: String,
    pub address: String,
    #[ts(type = "number")]
    pub use_count: u32,
    #[ts(type = "number")]
    pub last_used_at_ms: i64,
}

/// A contact group (P1.2): a named list of addresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ContactGroup {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
}

/// A saved search (P1.3) — a persistent virtual folder that re-runs a query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSearch {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub query: String,
    #[ts(type = "number")]
    pub created_at_ms: i64,
}

/// One rule that matched a message in a dry-run, with its actions (P1.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RulePreviewMatch {
    pub rule_name: String,
    /// Position in the rules list — the ordering explanation.
    #[ts(type = "number")]
    pub rule_index: u32,
    /// Human-readable action descriptions.
    pub actions: Vec<String>,
}

/// A message a dry-run says the rules would change, with its before-state so
/// an applied run can be reverted (P1.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RulePreview {
    #[ts(type = "number")]
    pub message_id: MessageId,
    pub subject: String,
    pub sender: String,
    pub folder_before: String,
    pub unread_before: bool,
    pub flagged_before: bool,
    pub matched: Vec<RulePreviewMatch>,
}

/// Result of a rule dry-run (P1.3): the affected count + per-message previews.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RulePreviewResult {
    #[ts(type = "number")]
    pub affected: u32,
    pub previews: Vec<RulePreview>,
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

/// The fields an existing account can be edited with (server/port/TLS/sync
/// cadence/color). The address and protocol identify the account and are
/// immutable; a password, if changed, is passed separately to the command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountEdit {
    pub id: AccountId,
    pub server: String,
    pub port: u16,
    pub tls: bool,
    /// `"every 2 min"`, `"on open"`, or `"manual"`.
    pub sync_mode: String,
    pub color: String,
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
    Junk,
    Trash,
    /// P1.1: the local-only Snoozed view (a message with a future
    /// `snoozed_until_ms`). Not a real mailbox — it never appears in server
    /// folder discovery.
    Snoozed,
}

impl FolderKind {
    /// Stable lowercase key stored in the `synced_folders.kind` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Starred => "starred",
            Self::Drafts => "drafts",
            Self::Sent => "sent",
            Self::Archive => "archive",
            Self::Junk => "junk",
            Self::Trash => "trash",
            Self::Snoozed => "snoozed",
        }
    }

    pub fn from_str(kind: &str) -> Self {
        match kind {
            "inbox" => Self::Inbox,
            "starred" => Self::Starred,
            "drafts" => Self::Drafts,
            "sent" => Self::Sent,
            "archive" => Self::Archive,
            "junk" => Self::Junk,
            "trash" => Self::Trash,
            "snoozed" => Self::Snoozed,
            _ => Self::Inbox,
        }
    }
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
    pub answered: bool,
    pub forwarded: bool,
    pub has_attachments: bool,
    pub thread_id: Option<String>,
    pub thread_count: u32,
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
    pub bcc: Vec<Recipient>,
    pub body: Vec<String>,
    /// Sanitized HTML body, when the message arrived as HTML. Populated
    /// server-side of the IPC boundary (Epic 7.3): the frontend never sees
    /// raw mail HTML.
    pub body_html: Option<String>,
    /// Remote images parked in `body_html` — drives the "Load images"
    /// affordance (Epic 7.3).
    pub remote_image_count: u32,
    pub attachments: Vec<Attachment>,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub thread_id: Option<String>,
    /// iTIP/iMIP calendar invitation parsed from `text/calendar` part (Roadmap 4.1).
    pub calendar_invite: Option<CalendarInvite>,
    /// RFC 8058 / RFC 2369 List-Unsubscribe header value (Roadmap 3.7).
    pub list_unsubscribe: Option<String>,
    /// RFC 8058 List-Unsubscribe-Post header value (e.g. "List-Unsubscribe=One-Click") (Roadmap 3.7).
    pub list_unsubscribe_post: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AttendeeInfo {
    pub name: Option<String>,
    pub email: String,
    pub partstat: String,
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarInvite {
    pub method: String,
    pub uid: String,
    pub sequence: u32,
    pub title: String,
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub end_ms: i64,
    pub all_day: bool,
    pub location: Option<String>,
    pub organizer_name: Option<String>,
    pub organizer_email: String,
    pub user_partstat: String,
    pub attendees: Vec<AttendeeInfo>,
    pub raw_ics: String,
    pub timezone: Option<String>,
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
    pub threaded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct OutgoingAttachment {
    pub filename: String,
    pub content_type: String,
    pub data_base64: String,
}

/// Outgoing mail. The transport lands in Epic 12/13; the contract is fixed here.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct OutgoingMessage {
    pub account_id: AccountId,
    pub from_name: Option<String>,
    pub from_address: Option<String>,
    pub reply_to: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub body_html: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
    pub attachments: Vec<OutgoingAttachment>,
    pub original_message_id: Option<MessageId>,
    pub is_forward: Option<bool>,
}

/// A signature configuration for an account / identity (Roadmap 3.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountSignature {
    pub plain_text: String,
    pub html: Option<String>,
    pub include_in_new_mail: bool,
    pub include_in_replies: bool,
    /// `"above_quote"` | `"bottom"`
    pub reply_placement: String,
}

impl Default for AccountSignature {
    fn default() -> Self {
        Self {
            plain_text: String::new(),
            html: None,
            include_in_new_mail: true,
            include_in_replies: false,
            reply_placement: "above_quote".to_string(),
        }
    }
}

/// A send-as identity or alias for an account (Roadmap 3.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountIdentity {
    pub id: String,
    pub account_id: AccountId,
    pub name: String,
    pub email: String,
    pub reply_to: Option<String>,
    pub signature: AccountSignature,
    pub is_default: bool,
}

/// Match logic for rule conditions (Roadmap 3.6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RuleMatchMode {
    All,
    Any,
}

/// Target field in message to evaluate (Roadmap 3.6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RuleField {
    From,
    To,
    Cc,
    Subject,
    ListId,
    HasAttachment,
    Body,
}

/// Comparison operator for rule conditions (Roadmap 3.6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RuleOperator {
    Contains,
    NotContains,
    Equals,
    NotEquals,
    StartsWith,
    EndsWith,
    Matches,
}

/// A condition predicate in a mail rule (Roadmap 3.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuleCondition {
    pub field: RuleField,
    pub operator: RuleOperator,
    pub value: String,
}

/// An action to perform when a mail rule matches (Roadmap 3.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum RuleAction {
    MoveToFolder {
        #[serde(rename = "folderName")]
        folder_name: String,
    },
    MarkRead,
    MarkUnread,
    MarkFlagged,
    MarkUnflagged,
    Delete,
    Archive,
}

/// A local filtering / routing rule for incoming mail (Roadmap 3.6).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub match_mode: RuleMatchMode,
    pub conditions: Vec<RuleCondition>,
    pub actions: Vec<RuleAction>,
    pub stop_processing: bool,
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
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub in_reply_to: Option<String>,
    pub references: Option<String>,
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
    pub alarm_minutes_before: Option<i32>,
    pub timezone: Option<String>,
    pub travel_time_minutes: Option<u32>,
    /// Source calendar key (e.g. a Google calendar id) — `None` for events
    /// with no external source. Lets the UI show each synced calendar as its
    /// own row instead of flattening them into the account (Roadmap 4.4).
    pub calendar_source: Option<String>,
    /// Display name of the source calendar, denormalized during sync.
    pub calendar_name: Option<String>,
    /// Color of the source calendar, denormalized during sync.
    pub calendar_color: Option<String>,
    /// Per-event color override (P1.4) — falls back to the calendar color when
    /// rendering. NULL = use the calendar color.
    pub color: Option<String>,
}

/// A distinct source calendar present in the local store (from sync). Drives
/// the calendar sidebar's per-calendar rows and show/hide toggles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSource {
    pub account_id: AccountId,
    pub source: String,
    pub name: String,
    pub color: String,
}

/// A to-do task / VTODO item (Roadmap 4.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarTask {
    pub id: u32,
    pub account_id: AccountId,
    pub title: String,
    #[ts(type = "number | null")]
    pub due_at_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub completed_at_ms: Option<i64>,
    pub priority: Option<u32>,
}

/// A free/busy interval for scheduling (Roadmap 4.5).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FreeBusySlot {
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub end_ms: i64,
    pub busy: bool,
    pub attendee: Option<String>,
}

/// Read-only external .ics / webcal calendar subscription (Roadmap 4.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CalendarSubscription {
    pub id: u32,
    pub name: String,
    pub url: String,
    pub color: String,
    pub refresh_interval_min: u32,
    #[ts(type = "number | null")]
    pub last_refreshed_at_ms: Option<i64>,
    pub enabled: bool,
}

/// Push events the frontend subscribes to (Epic 3.2) — the frontend never
/// polls; sync/connectivity/footprint arrive here as deltas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum StoreEvent {
    Connectivity(ConnectivityUpdate),
    Footprint(FootprintUpdate),
    /// New messages were written mid-sync (streamed) — the UI reloads the list
    /// so mail appears progressively instead of all at once when the sync ends.
    MailChanged(MailChangedUpdate),
    /// Body-download progress for the reading pane (Epic 7.2) — emitted while
    /// `get_message` fetches a body on demand, so the loading screen can show a
    /// real progress bar instead of a blind spinner.
    MessageProgress(MessageProgressUpdate),
    /// Search-index rebuild progress (P1.3): `"rebuilding"` carries
    /// indexed/total; `"fresh"` / `"idle"` mark completion (fresh = done,
    /// idle = cancelled/errored).
    SearchIndex(SearchIndexUpdate),
    /// A `mailto:` deep link (or the tray "New Message" action) arrived —
    /// the frontend opens the composer pre-filled (P1.5).
    Mailto(MailtoPayload),
}

/// A compose request from an OS integration (P1.5).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MailtoPayload {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Result of an `.eml`/mbox import (P1.6): counts + per-message errors.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ImportReport {
    #[ts(type = "number")]
    pub imported: u32,
    #[ts(type = "number")]
    pub duplicates: u32,
    pub errors: Vec<String>,
}

/// Search-index status/progress (P1.3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchIndexUpdate {
    /// "rebuilding" | "fresh" | "idle".
    pub state: String,
    #[ts(type = "number")]
    pub indexed: u32,
    #[ts(type = "number")]
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct MailChangedUpdate {
    pub account_id: AccountId,
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

/// Body-download progress for the reading pane. `phase` is one of
/// `"connecting"` | `"fetching"` | `"parsing"`; byte counters are non-zero only
/// during `"fetching"` (`total_bytes` is `0` until the server reports a size, so
/// the frontend shows an indeterminate bar until then).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct MessageProgressUpdate {
    pub message_id: MessageId,
    pub phase: String,
    #[ts(type = "number")]
    pub received_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountNotificationSetting {
    pub account_id: AccountId,
    pub enabled: bool,
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct NotificationSettings {
    pub enabled: bool,
    pub sound: bool,
    pub dock_badge: bool,
    pub quiet_hours_enabled: bool,
    pub quiet_hours_start: String,
    pub quiet_hours_end: String,
    pub known_contacts_only: bool,
    pub default_alarm_minutes: Option<i32>,
    pub per_account: Vec<AccountNotificationSetting>,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sound: true,
            dock_badge: true,
            quiet_hours_enabled: false,
            quiet_hours_start: "22:00".into(),
            quiet_hours_end: "08:00".into(),
            known_contacts_only: false,
            default_alarm_minutes: Some(15),
            per_account: Vec::new(),
        }
    }
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
    /// Enable JWZ-style conversation threading in message lists (Roadmap 3.2).
    pub conversation_threading: bool,
    /// Notification preferences and quiet hours (Roadmap 3.4).
    #[serde(default)]
    pub notifications: NotificationSettings,
    /// Configured send-as identities and aliases per account (Roadmap 3.5).
    #[serde(default)]
    pub identities: Vec<AccountIdentity>,
    /// Undo send delay in seconds (0 = disabled, 5..30 s) (Roadmap 3.5).
    #[serde(default = "default_undo_send_delay")]
    pub undo_send_delay_sec: u32,
    /// Local mail filtering and routing rules (Roadmap 3.6).
    #[serde(default)]
    pub rules: Vec<MailRule>,
    /// Whether remote images are blocked by default for untrusted senders (Roadmap 3.7).
    #[serde(default = "default_true")]
    pub block_remote_images: bool,
    /// Primary calendar timezone (IANA ID e.g. "America/New_York"), null = local system time (Roadmap 4.3).
    pub primary_timezone: Option<String>,
    /// Secondary calendar timezone for dual-gutter time display (Roadmap 4.3).
    pub secondary_timezone: Option<String>,
    /// Whether to display the secondary timezone column in week/day views (Roadmap 4.3).
    #[serde(default)]
    pub show_secondary_timezone: bool,
    /// Opt-in: upload scrubbed crash reports (Rust panics + JS errors) to the
    /// configured endpoint. Default off — reports are always written locally,
    /// transmission is the gated action (Roadmap 2.3).
    #[serde(default)]
    pub crash_reporting_enabled: bool,
    /// Opt-in: anonymous usage ping (app version/OS only) on launch. Default off
    /// (Roadmap 2.3).
    #[serde(default)]
    pub usage_ping_enabled: bool,
    /// Unified log level: "error" | "warn" | "info" | "debug" | "trace"
    /// (Roadmap 2.3 — unified logging).
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_true() -> bool {
    true
}

fn default_undo_send_delay() -> u32 {
    10
}

fn default_log_level() -> String {
    "info".to_string()
}

/// A search result item for messages or calendar events (Epic 15).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SearchMatch {
    /// `"message"` or `"event"`.
    pub kind: String,
    pub id: u32,
    pub account_id: AccountId,
    pub folder: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub snippet: String,
    #[ts(type = "number")]
    pub timestamp_ms: i64,
}

/// Search query payload with scoping and limit (Epic 15).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct SearchQuery {
    pub query: String,
    pub folder: Option<String>,
    pub account_id: Option<AccountId>,
    pub include_events: bool,
    pub limit: u32,
}

/// A discovered or configured CalDAV calendar collection (Roadmap 1.4).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct CalendarCollection {
    pub href: String,
    pub name: String,
    pub color: Option<String>,
    pub ctag: Option<String>,
    pub sync_token: Option<String>,
}

/// Initialization payload for browser-based OAuth2 sign-in (Roadmap 3.1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct OAuthInitPayload {
    pub auth_url: String,
    pub code_verifier: String,
    pub redirect_uri: String,
    pub state: String,
    /// The client ID actually used for the auth URL — surfaces the fallback
    /// placeholder so the UI can show which client is in play.
    pub client_id: String,
}

// --- P0.2: provider presets, autodiscovery, actionable connection errors,
// --- folder selection before sync, and account removal (backlog.md P0.2).

/// One mail/calendar endpoint (host, port, TLS) used by a preset or produced
/// by autodiscovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    pub tls: bool,
}

/// How a provider expects the user to authenticate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AuthKind {
    /// OAuth2 (Google / Microsoft 365).
    Oauth,
    /// Provider-issued app password (iCloud, Gmail app password, Fastmail).
    AppPassword,
    /// Ordinary account password.
    Password,
}

/// A known mail provider preset (Gmail, iCloud, Fastmail, …) used to prefill
/// and guide account setup.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ProviderPreset {
    /// Stable id ("gmail", "icloud", "fastmail", …).
    pub id: String,
    pub name: String,
    /// Email domains this preset matches (e.g. ["icloud.com", "me.com"]).
    pub domains: Vec<String>,
    pub imap: Endpoint,
    pub smtp: Endpoint,
    pub caldav: Option<Endpoint>,
    pub auth: AuthKind,
    /// `"google"` / `"microsoft365"` when `auth == Oauth`.
    pub oauth_provider: Option<String>,
    /// Provider-specific help shown at the point of failure (e.g. where to
    /// create an app-specific password).
    pub help: String,
}

/// One step of the autodiscovery pipeline with its outcome, so the UI can show
/// what was tried and why a fallback happened.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct DiscoveryStep {
    /// "preset" | "dns_srv" | "autoconfig" | "guess" | "manual".
    pub source: String,
    /// "ok" | "skip" | "error".
    pub status: String,
    pub detail: String,
}

/// Settings discovered for an email address, with provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct DiscoveredSettings {
    pub imap: Option<Endpoint>,
    pub smtp: Option<Endpoint>,
    pub caldav: Option<Endpoint>,
    pub provider: Option<ProviderPreset>,
    pub steps: Vec<DiscoveryStep>,
}

/// The network service a connection error concerns.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Service {
    Imap,
    Smtp,
    /// Renamed from the variant so the IPC string is `"caldav"`, not
    /// `"cal_dav"`.
    #[serde(rename = "caldav")]
    CalDav,
}

/// The class of a connection failure — what the UI distinguishes to offer the
/// right remedy (TLS vs auth vs network vs rate limit).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorKind {
    Dns,
    Connect,
    Tls,
    Auth,
    RateLimit,
    Timeout,
    Protocol,
    Offline,
}

/// A single actionable connection problem: which service+server, what kind of
/// failure, and provider-specific help when known.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ConnectionIssue {
    pub service: Service,
    pub server: String,
    pub kind: ErrorKind,
    pub detail: String,
    pub help: Option<String>,
}

/// Inputs for a full connection test (replaces the old raw-TCP probe).
///
/// The password, when one is supplied for auth testing, is a separate command
/// argument (exactly like `add_account`) so the credential never lives in the
/// IPC contract.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, TS)]
#[ts(export)]
pub struct TestConnectionSettings {
    pub email: String,
    /// "imap" | "smtp" | "caldav".
    pub protocol: String,
    pub server: String,
    pub port: u16,
    pub tls: bool,
}

/// Result of a connection test: reachability + auth per stage, with issues.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct ConnectionTestReport {
    pub ok: bool,
    pub authed: bool,
    pub issues: Vec<ConnectionIssue>,
    /// Human detail, e.g. "N folders" or the SMTP greeting.
    pub detail: String,
}

/// A mailbox as discovered on the server, before it is saved to the local
/// store — the unit of "choose which folders sync".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ServerFolder {
    /// Real IMAP mailbox name.
    pub server_name: String,
    /// Local display name (the folder storage key).
    pub local_name: String,
    pub kind: FolderKind,
}

/// A persisted folder-sync selection for an account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SyncedFolder {
    pub account_id: AccountId,
    pub server_name: String,
    pub local_name: String,
    pub kind: FolderKind,
    pub enabled: bool,
}

/// What removing an account will destroy — the removal confirm's content.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AccountRemovalInfo {
    pub account_id: AccountId,
    /// Queued offline actions that would be discarded.
    #[ts(type = "number")]
    pub queued_actions: u32,
    /// Locally saved but unsent drafts.
    #[ts(type = "number")]
    pub drafts: u32,
    #[ts(type = "number")]
    pub local_bytes: u64,
}

/// Result of waiting for the OAuth loopback redirect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(export)]
pub struct OAuthWaitResult {
    pub ok: bool,
    pub code: Option<String>,
    pub error: Option<String>,
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
            conversation_threading: true,
            notifications: NotificationSettings::default(),
            identities: Vec::new(),
            undo_send_delay_sec: 10,
            rules: Vec::new(),
            block_remote_images: true,
            primary_timezone: None,
            secondary_timezone: None,
            show_secondary_timezone: false,
            crash_reporting_enabled: false,
            usage_ping_enabled: false,
            log_level: default_log_level(),
        }
    }
}

/// Read-only diagnostics status for the Settings → Diagnostics section
/// (Roadmap 2.3). Never contains message content or account addresses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsInfo {
    /// Running app version.
    pub app_version: String,
    /// OS family (`std::env::consts::OS`): "macos" | "windows" | "linux".
    pub os: String,
    /// CPU architecture (`std::env::consts::ARCH`).
    pub arch: String,
    /// "stable" or the pre-release tag (e.g. "beta.1") of the app version.
    pub channel: String,
    /// Active unified log level.
    pub log_level: String,
    /// Whether the crash-reporting opt-in is currently on.
    pub crash_reporting_enabled: bool,
    /// Whether the usage-ping opt-in is currently on.
    pub usage_ping_enabled: bool,
    /// Count of crash reports queued locally, awaiting upload.
    pub pending_report_count: u32,
    /// Absolute path to the local log file, if the log target is active.
    pub log_file_path: Option<String>,
    /// Absolute path to the pending crash-reports directory.
    pub crash_reports_dir: String,
    /// Whether an upload endpoint is configured (build-time env vars).
    pub endpoint_configured: bool,
}

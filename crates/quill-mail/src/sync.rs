//! IMAP sync engine (Epic 12.2 / Roadmap E1.1).
//!
//! Connects to an account's IMAP server, syncs folders incrementally via
//! UIDVALIDITY/UIDNEXT/HIGHESTMODSEQ (full refetch when the validity changes),
//! and writes fetched envelopes and flags into the store.
//!
//! Features:
//! - Multi-folder sync (INBOX, Sent, Drafts, Archive, Trash, and custom folders)
//!   with folder-kind detection via RFC 6154 SPECIAL-USE attributes and name heuristics.
//! - Incremental sync per folder with flag synchronization (read/starred/expunged).
//! - On-demand MIME body and attachment fetching via `mail-parser`.
//! - Offline action replay engine (mark_read, star, archive, delete, send) with conflict policy.
//! - IMAP IDLE push loop with exponential backoff on reconnection.

use std::collections::{BTreeMap, HashSet};

use async_imap::types::{Fetch, Flag, Name, NameAttribute};
use futures::TryStreamExt;
use mail_parser::{Address, MessageParser, MimeHeaders};
use quill_store::sanitize::snippet_from_bodies;
use quill_store::sqlite::SqliteStore;
use quill_store::types::{
    Account, ActionType, Attachment, FolderKind, MessageId, MessageProgressUpdate, MessageRow,
    OutgoingMessage, Recipient,
};
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::auth::Credential;
use crate::oauth_store::get_valid_access_token;

/// The stream async-imap reads over: any duplex that can be boxed.
pub trait IoStream:
    futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + std::fmt::Debug
{
}
impl<T: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + std::fmt::Debug> IoStream
    for T
{
}

pub type Stream = Box<dyn IoStream>;

/// Retention window: keep only this many days of mail. Anything older is
/// pruned after each sync and skipped by the initial refetch (a busy Gmail
/// account can hold a million+ messages otherwise).
const RETAIN_DAYS: i64 = 7;
const RETAIN_DAYS_MS: i64 = RETAIN_DAYS * 24 * 3600 * 1000;

/// SASL XOAUTH2 authenticator for `async_imap` — produces
/// `user=…\x01auth=Bearer …\x01\x01`; async-imap base64-encodes it.
struct Xoauth2Authenticator {
    user: String,
    access_token: String,
}

impl async_imap::Authenticator for Xoauth2Authenticator {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct SyncOutcome {
    pub folders_synced: usize,
    pub messages_fetched: usize,
}

#[derive(Debug, Clone)]
pub struct DiscoveredFolder {
    pub server_name: String,
    pub local_name: String,
    pub kind: FolderKind,
}

/// Detect the folder kind from IMAP attributes and name heuristics.
pub fn detect_folder_kind(name: &str, attributes: &[NameAttribute]) -> FolderKind {
    for attr in attributes {
        let debug_str = format!("{attr:?}").to_lowercase();
        if debug_str.contains("inbox") {
            return FolderKind::Inbox;
        }
        if debug_str.contains("draft") {
            return FolderKind::Drafts;
        }
        if debug_str.contains("sent") {
            return FolderKind::Sent;
        }
        if debug_str.contains("junk") || debug_str.contains("spam") {
            return FolderKind::Junk;
        }
        if debug_str.contains("trash") || debug_str.contains("bin") || debug_str.contains("deleted") {
            return FolderKind::Trash;
        }
        if debug_str.contains("archive") || debug_str.contains("all") {
            return FolderKind::Archive;
        }
    }

    // Heuristics based on folder name
    let lower = name.to_lowercase();
    if lower == "inbox" {
        FolderKind::Inbox
    } else if lower.contains("draft") {
        FolderKind::Drafts
    } else if lower.contains("sent") {
        FolderKind::Sent
    } else if lower.contains("junk") || lower.contains("spam") || lower.contains("bulk") {
        FolderKind::Junk
    } else if lower.contains("trash") || lower.contains("bin") || lower.contains("deleted") {
        FolderKind::Trash
    } else if lower.contains("archive") || lower.contains("all mail") || lower.contains("all") {
        FolderKind::Archive
    } else {
        FolderKind::Inbox
    }
}

/// Map server folder name to canonical display name (e.g. INBOX -> "Inbox").
pub fn canonical_folder_name(_server_name: &str, kind: FolderKind) -> String {
    match kind {
        FolderKind::Inbox => "Inbox".to_string(),
        FolderKind::Drafts => "Drafts".to_string(),
        FolderKind::Sent => "Sent".to_string(),
        FolderKind::Archive => "Archive".to_string(),
        FolderKind::Starred => "Starred".to_string(),
        FolderKind::Junk => "Junk".to_string(),
        FolderKind::Trash => "Trash".to_string(),
        FolderKind::Snoozed => "Snoozed".to_string(),
    }
}

/// A one-shot signal that a message was written during a streaming sync; the
/// sync driver coalesces these into throttled UI push events so mail appears
/// progressively instead of all at once when the sync finishes.
pub type SyncProgress = tokio::sync::mpsc::Sender<()>;

/// Best-effort progress ping — never blocks the sync on a slow consumer.
fn ping_progress(tx: &Option<SyncProgress>) {
    if let Some(tx) = tx {
        let _ = tx.try_send(());
    }
}

/// Body-download progress sink for the reading pane. Each message carries the
/// phase and, during `"fetching"`, the bytes received so far; the Tauri shell
/// relays these to the frontend so the loading screen shows a real bar.
pub type BodyProgress = tokio::sync::mpsc::Sender<MessageProgressUpdate>;

/// Best-effort progress push — never blocks the body fetch on a slow consumer.
fn push_progress(
    tx: &Option<BodyProgress>,
    message_id: MessageId,
    phase: &str,
    received_bytes: u64,
    total_bytes: u64,
) {
    if let Some(tx) = tx {
        let _ = tx.try_send(MessageProgressUpdate {
            message_id,
            phase: phase.to_string(),
            received_bytes,
            total_bytes,
        });
    }
}

/// Write one fetched envelope to the store (streaming) and report progress.
/// `folder` is the local display key; `server_folder` is the actual IMAP
/// mailbox name, persisted so on-demand body fetches and offline-action
/// replay can re-`SELECT` the right mailbox.
fn write_envelope(
    store: &SqliteStore,
    account: &Account,
    folder: &str,
    server_folder: &str,
    uidvalidity: u32,
    fetch: &Fetch,
    progress: &Option<SyncProgress>,
) -> Result<Option<u32>, String> {
    let Some((row, uid, parsed)) = envelope_row(account, folder, uidvalidity, fetch) else {
        return Ok(None);
    };
    let message_id = store.upsert_fetched_message(
        account.id,
        folder,
        server_folder,
        uid,
        i64::from(uidvalidity),
        &row.sender_name,
        &row.sender_address,
        &row.subject,
        &row.snippet,
        row.received_at_ms,
        row.unread,
        row.flagged,
        row.answered,
        row.forwarded,
        row.has_attachments,
    )?;
    // The full body came down with the envelope — persist it (plus its
    // recipients/attachment metadata) so the reading pane, search index, and
    // attachment icons have it without a second fetch. Bodies land together
    // with the row that lists them.
    if let Some(p) = parsed {
        store.save_message_body_and_attachments(
            message_id,
            &p.plain_body,
            p.html_body.as_deref(),
            &p.to,
            &p.cc,
            &p.bcc,
            &p.attachments,
            p.message_id_header.as_deref(),
            p.in_reply_to.as_deref(),
            p.references.as_deref(),
            p.list_unsubscribe.as_deref(),
            p.list_unsubscribe_post.as_deref(),
        )?;
    }
    ping_progress(progress);
    Ok(Some(uid))
}

/// Connect to the IMAP server for an account. Password accounts use LOGIN;
/// OAuth accounts use SASL XOAUTH2 with a (lazily refreshed) access token.
pub async fn connect(
    account: &Account,
    credential: &Credential,
) -> Result<async_imap::Session<Stream>, String> {
    let addr = format!("{}:{}", account.server, account.port);
    let tcp = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("connect {addr}: {e}"))?;

    let stream: Stream = if account.tls {
        let tls = async_native_tls::TlsConnector::new();
        let tls_stream = tls
            .connect(&account.server, tcp.compat())
            .await
            .map_err(|e| format!("TLS to {}: {e}", account.server))?;
        Box::new(tls_stream)
    } else {
        Box::new(tcp.compat())
    };

    let mut client = async_imap::Client::new(stream);
    let _greeting = client
        .read_response()
        .await
        .map_err(|e| format!("greeting: {e}"))?
        .ok_or_else(|| "no greeting from server".to_string())?;

    match credential {
        Credential::Password(password) => client
            .login(&account.address, password)
            .await
            .map_err(|(e, _)| format!("login for {}: {e}", account.address)),
        Credential::OAuth { address, provider } => {
            let access_token = get_valid_access_token(address, *provider).await?;
            let auth = Xoauth2Authenticator {
                user: address.clone(),
                access_token,
            };
            client
                .authenticate("XOAUTH2", auth)
                .await
                .map_err(|(e, _)| format!("xoauth2 auth for {address}: {e}"))
        }
    }
}

/// Discover mailboxes available on the server.
pub async fn discover_folders(
    session: &mut async_imap::Session<Stream>,
) -> Result<Vec<DiscoveredFolder>, String> {
    let mailboxes = session
        .list(None, Some("*"))
        .await
        .map_err(|e| format!("list folders: {e}"))?;
    let names: Vec<Name> = mailboxes
        .try_collect()
        .await
        .map_err(|e| format!("collect folder list: {e}"))?;

    let mut folders = Vec::new();
    let mut seen_local = std::collections::HashSet::new();
    for name in names {
        let server_name = name.name().to_string();
        // Skip non-selectable folders (e.g. \Noselect)
        let is_noselect = name
            .attributes()
            .iter()
            .any(|a| matches!(a, NameAttribute::NoSelect));
        if is_noselect {
            continue;
        }
        // Gmail's [Gmail]/All Mail mirrors every message in the account;
        // syncing it would duplicate bodies and bloat the local DB (policy in
        // docs/provider-quirks.md).
        if server_name.eq_ignore_ascii_case("[Gmail]/All Mail") {
            continue;
        }
        let kind = detect_folder_kind(&server_name, name.attributes());
        let local_name = if server_name.eq_ignore_ascii_case("inbox") {
            "Inbox".to_string()
        } else {
            canonical_folder_name(&server_name, kind)
        };
        // The local name is the storage key for an account+folder. Two server
        // mailboxes must never share one key: the second mailbox's full
        // refetch would `delete_messages_not_in` the first mailbox's rows and
        // wipe it. Gmail's labels, [Gmail]/Starred and [Gmail]/Spam, plus any
        // custom folder, all collapsed to kind=Inbox and local_name="Inbox"
        // here, so each sync cycle thrashed and wiped the real inbox. Keep
        // only the first mailbox per local name, and drop anything that would
        // masquerade as the inbox (the real INBOX always maps to "Inbox").
        if kind == FolderKind::Inbox && !server_name.eq_ignore_ascii_case("inbox") {
            continue;
        }
        if !seen_local.insert(local_name.clone()) {
            continue;
        }
        folders.push(DiscoveredFolder {
            server_name,
            local_name,
            kind,
        });
    }

    if folders.is_empty() {
        // Fallback to standard set if LIST returns empty
        folders = vec![
            DiscoveredFolder {
                server_name: "INBOX".into(),
                local_name: "Inbox".into(),
                kind: FolderKind::Inbox,
            },
            DiscoveredFolder {
                server_name: "Drafts".into(),
                local_name: "Drafts".into(),
                kind: FolderKind::Drafts,
            },
            DiscoveredFolder {
                server_name: "Sent".into(),
                local_name: "Sent".into(),
                kind: FolderKind::Sent,
            },
            DiscoveredFolder {
                server_name: "Archive".into(),
                local_name: "Archive".into(),
                kind: FolderKind::Archive,
            },
        ];
    }

    Ok(folders)
}

/// UID range for a full refetch, bounded to the retention window via
/// `UID SEARCH SINCE`. `None` means the search succeeded but found nothing in
/// the window (the folder was already wiped — fetch nothing). A search failure
/// falls back to the full range so we never miss mail.
async fn bounded_fetch_range(session: &mut async_imap::Session<Stream>) -> Option<String> {
    let since = (chrono::Utc::now() - chrono::Duration::days(RETAIN_DAYS))
        .format("%d-%b-%Y")
        .to_string();
    match session.uid_search(&format!("SINCE {since}")).await {
        Ok(uids) if uids.is_empty() => None,
        Ok(uids) => {
            let mut sorted: Vec<u32> = uids.into_iter().collect();
            sorted.sort_unstable();
            Some(
                sorted
                    .iter()
                    .map(|u| u.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            )
        }
        Err(_) => Some("1:*".to_string()),
    }
}

/// Sync every tracked folder for one account. Replays pending offline actions
/// first, then incrementally synchronizes each folder.
///
/// `replay_actions` gates the offline-action replay. Only the periodic sync
/// path replays — the IDLE push worker must not, or the two (which run
/// concurrently for the same account) would both read a queued `Send` and
/// deliver it twice.
pub async fn sync_account(
    store: &SqliteStore,
    account: &Account,
    credential: &Credential,
    progress: Option<SyncProgress>,
    replay_actions: bool,
) -> Result<SyncOutcome, String> {
    let mut session = match connect(account, credential).await {
        Ok(s) => {
            let _ = store.set_account_connected(account.id, true, None);
            s
        }
        Err(e) => {
            let _ = store.set_account_connected(account.id, false, Some(&e));
            return Err(e);
        }
    };

    // 1. Replay any pending offline actions before syncing remote state.
    if replay_actions {
        if let Err(e) = replay_pending_actions(store, account, &mut session, credential).await {
            // Account id, never the address (never-log-PII rule, plan2 §10.4).
            log::warn!("action replay for account {}: {e}", account.id);
        }
    }

    // 2. Discover server folders.
    let discovered = match discover_folders(&mut session).await {
        Ok(f) => f,
        Err(_) => vec![
            DiscoveredFolder {
                server_name: "INBOX".into(),
                local_name: "Inbox".into(),
                kind: FolderKind::Inbox,
            },
            DiscoveredFolder {
                server_name: "Drafts".into(),
                local_name: "Drafts".into(),
                kind: FolderKind::Drafts,
            },
            DiscoveredFolder {
                server_name: "Sent".into(),
                local_name: "Sent".into(),
                kind: FolderKind::Sent,
            },
            DiscoveredFolder {
                server_name: "Archive".into(),
                local_name: "Archive".into(),
                kind: FolderKind::Archive,
            },
        ],
    };

    let _ = store.set_account_folder_count(account.id, discovered.len() as u32);

    // P0.2 folder selection: when the account has a configured selection, sync
    // only the enabled server mailboxes; an empty selection (pre-P0.2 and
    // never-chosen accounts) syncs everything discovered. Newly discovered
    // mailboxes are added to the selection enabled by default so the user
    // doesn't silently miss mail they never opted out of.
    let selection = store.synced_folders(account.id);
    let folders_to_sync: Vec<DiscoveredFolder> = if selection.is_empty() {
        discovered.clone()
    } else {
        let known: HashSet<&str> = selection.iter().map(|s| s.server_name.as_str()).collect();
        let missing: Vec<quill_store::types::SyncedFolder> = discovered
            .iter()
            .filter(|f| !known.contains(f.server_name.as_str()))
            .map(|f| quill_store::types::SyncedFolder {
                account_id: account.id,
                server_name: f.server_name.clone(),
                local_name: f.local_name.clone(),
                kind: f.kind,
                enabled: true,
            })
            .collect();
        if !missing.is_empty() {
            let _ = store.upsert_synced_folders(account.id, &missing);
        }
        let enabled = store.enabled_folder_set(account.id).unwrap_or_default();
        discovered
            .into_iter()
            .filter(|f| enabled.contains(&f.server_name))
            .collect()
    };

    let mut outcome = SyncOutcome::default();

    for folder in &folders_to_sync {
        match sync_folder(
            store,
            account,
            &mut session,
            &folder.server_name,
            &folder.local_name,
            &progress,
        )
        .await
        {
            Ok(fetched) => {
                outcome.folders_synced += 1;
                outcome.messages_fetched += fetched;
            }
            Err(e) => {
                log::warn!("sync account {} {}: {e}", account.id, folder.server_name);
            }
        }
    }

    // Retention: keep only the last RETAIN_DAYS of mail.
    if let Err(e) = store.prune_messages_before(now_ms() - RETAIN_DAYS_MS) {
        log::warn!("prune account {}: {e}", account.id);
    }

    let _ = session.logout().await;
    Ok(outcome)
}

/// Synchronize a single folder incrementally.
pub async fn sync_folder(
    store: &SqliteStore,
    account: &Account,
    session: &mut async_imap::Session<Stream>,
    server_folder: &str,
    local_folder: &str,
    progress: &Option<SyncProgress>,
) -> Result<usize, String> {
    let mailbox = session
        .select(server_folder)
        .await
        .map_err(|e| format!("select {server_folder}: {e}"))?;
    let uidvalidity = mailbox.uid_validity.unwrap_or(0);
    let uidnext = mailbox.uid_next.unwrap_or(0);
    let highest_modseq = mailbox.highest_modseq.unwrap_or(0);

    let (last_validity, last_next, _last_modseq) = store.get_sync_state(account.id, local_folder);
    let full = last_validity != i64::from(uidvalidity);

    let mut fetched_count = 0;
    // Tracks whether every fetch stream for this folder completed cleanly. On
    // a mid-stream error we keep what we already have locally and keep the old
    // sync watermark, so the next sync re-fetches from the old position instead
    // of skipping the messages the interrupted stream never delivered.
    let mut complete = true;

    if full {
        // UIDVALIDITY changed (or no watermark). The folder's local rows are
        // reconciled against the refetched set below; no upfront wipe, so an
        // interrupted refetch leaves the previous rows in place.
        if mailbox.exists > 0 {
            // Bound the refetch to the retention window so a huge mailbox
            // isn't downloaded in full (older rows are pruned anyway).
            match bounded_fetch_range(session).await {
                Some(range) => {
                    // 1. Reconcile flags and collect the server UID set across
                    //    the whole window — a light (flags-only) fetch.
                    let mut server_uids = Vec::new();
                    match session.uid_fetch(&range, "(UID FLAGS)").await {
                        Ok(mut fetches) => loop {
                            match fetches.try_next().await {
                                Ok(Some(fetch)) => {
                                    if let Some(uid) = fetch.uid {
                                        server_uids.push(uid);
                                        let unread =
                                            !fetch.flags().any(|f| matches!(f, Flag::Seen));
                                        let flagged =
                                            fetch.flags().any(|f| matches!(f, Flag::Flagged));
                                        let _ = store.update_message_flags_by_uid(
                                            account.id,
                                            local_folder,
                                            uid,
                                            unread,
                                            flagged,
                                            false,
                                            false,
                                        );
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => {
                                    complete = false;
                                    break;
                                }
                            }
                        },
                        Err(_) => complete = false,
                    }

                    // 2. Fetch bodies only for messages we don't already have.
                    //    Re-downloading every existing body on each catch-up
                    //    cycle is what stalled a busy Inbox before reaching new
                    //    mail (and blocked the store lock the whole time).
                    if complete {
                        let existing = store.folder_uids(account.id, local_folder);
                        let new_uids: Vec<u32> = server_uids
                            .iter()
                            .copied()
                            .filter(|u| !existing.contains(u))
                            .collect();
                        if !new_uids.is_empty() {
                            let new_range = new_uids
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(",");
                            let body_query = "(UID FLAGS INTERNALDATE ENVELOPE BODY.PEEK[])";
                            match session.uid_fetch(&new_range, body_query).await {
                                Ok(mut fetches) => loop {
                                    match fetches.try_next().await {
                                        Ok(Some(fetch)) => {
                                            if let Some(_uid) = write_envelope(
                                                store,
                                                account,
                                                local_folder,
                                                server_folder,
                                                uidvalidity,
                                                &fetch,
                                                progress,
                                            )? {
                                                fetched_count += 1;
                                            }
                                        }
                                        Ok(None) => break,
                                        Err(_) => {
                                            complete = false;
                                            break;
                                        }
                                    }
                                },
                                Err(_) => complete = false,
                            }
                        }
                    }

                    // Only prune locally-stored messages when the whole refetch
                    // completed cleanly; on a mid-stream error keep what we have.
                    if complete {
                        store.delete_messages_not_in(account.id, local_folder, &server_uids)?;
                    }
                }
                None => {
                    // The search found nothing in the retention window: the
                    // folder is effectively empty, so drop stale local rows.
                    store.delete_messages_not_in(account.id, local_folder, &[])?;
                }
            }
        } else {
            store.delete_messages_not_in(account.id, local_folder, &[])?;
        }
    } else {
        // Incremental:
        // 1. Fetch newly arrived messages since last_next.
        if uidnext > last_next as u32 && last_next > 0 {
            let start = last_next as u32;
            let range = format!("{start}:*");
            let query = "(UID FLAGS INTERNALDATE ENVELOPE BODY.PEEK[])";
            match session.uid_fetch(&range, query).await {
                Ok(mut fetches) => loop {
                    match fetches.try_next().await {
                        Ok(Some(fetch)) => {
                            if let Some(_uid) = write_envelope(
                                store,
                                account,
                                local_folder,
                                server_folder,
                                uidvalidity,
                                &fetch,
                                progress,
                            )? {
                                fetched_count += 1;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            complete = false;
                            break;
                        }
                    }
                },
                Err(_) => complete = false,
            }
        }

        // 2. Reconcile flags on existing messages and detect expunges.
        if mailbox.exists > 0 {
            let flag_query = "(UID FLAGS)";
            match session.uid_fetch("1:*", flag_query).await {
                Ok(mut fetches) => {
                    let mut server_uids = Vec::new();
                    loop {
                        match fetches.try_next().await {
                            Ok(Some(fetch)) => {
                                if let Some(uid) = fetch.uid {
                                    server_uids.push(uid);
                                    let unread = !fetch.flags().any(|f| matches!(f, Flag::Seen));
                                    let flagged = fetch.flags().any(|f| matches!(f, Flag::Flagged));
                                    let answered = fetch.flags().any(|f| match f {
                                        Flag::Answered => true,
                                        Flag::Custom(s) => s.eq_ignore_ascii_case("$answered"),
                                        _ => false,
                                    });
                                    let forwarded = fetch.flags().any(|f| match f {
                                        Flag::Custom(s) => {
                                            s.eq_ignore_ascii_case("$forwarded")
                                                || s.eq_ignore_ascii_case("forwarded")
                                                || s.eq_ignore_ascii_case("$passed")
                                        }
                                        _ => false,
                                    });
                                    let _ = store.update_message_flags_by_uid(
                                        account.id,
                                        local_folder,
                                        uid,
                                        unread,
                                        flagged,
                                        answered,
                                        forwarded,
                                    );
                                }
                            }
                            Ok(None) => break,
                            Err(_) => {
                                complete = false;
                                break;
                            }
                        }
                    }
                    // Delete expunged rows only when the whole flag stream was
                    // read; a partial stream would delete every message whose
                    // UID hadn't been seen yet.
                    if complete {
                        let _ = store.delete_messages_not_in(account.id, local_folder, &server_uids);
                    }
                }
                Err(_) => complete = false,
            }
        } else {
            let _ = store.delete_messages_not_in(account.id, local_folder, &[]);
        }
    }

    // Only advance the sync watermark when every stream for this folder
    // completed; an interrupted fetch must retry from the old watermark.
    if complete {
        store.set_sync_state(
            account.id,
            local_folder,
            i64::from(uidvalidity),
            i64::from(uidnext),
            highest_modseq as i64,
        )?;
    }

    Ok(fetched_count)
}

/// Fetch full message body and attachments on demand, parsing MIME parts.
/// `progress` streams byte counts to the reading pane's loading screen; pass
/// `&None` to fetch silently (the sync engine never uses this path).
pub async fn fetch_message_body_full(
    store: &SqliteStore,
    account: &Account,
    credential: &Credential,
    folder: &str,
    uid: u32,
    message_id: MessageId,
    progress: &Option<BodyProgress>,
) -> Result<(), String> {
    push_progress(progress, message_id, "connecting", 0, 0);
    let mut session = connect(account, credential).await?;
    let _ = session
        .select(folder)
        .await
        .map_err(|e| format!("select {folder}: {e}"))?;

    let raw_body = fetch_body_chunked(&mut session, uid, message_id, progress).await?;
    push_progress(progress, message_id, "parsing", 0, 0);

    let Some(parsed) = parse_full_message(&raw_body) else {
        return Err("failed to parse MIME message".into());
    };

    // Refresh the list-row snippet from the real body — a message synced
    // before the sync fetched bodies may carry a raw-MIME/HTML snippet that
    // this parse can now correct.
    persist_parsed_message(store, message_id, &parsed)?;

    let _ = session.logout().await;
    Ok(())
}

/// One body fetch window. Reading the whole message in one `BODY.PEEK[]`
/// round-trip gives the frontend nothing to draw, so pull it in fixed-size
/// partial reads and report bytes as they land.
const BODY_CHUNK_BYTES: usize = 64 * 1024;

/// Ceiling for a single fetched body — guards against a misbehaving server
/// that never returns a short final chunk (so the loop always terminates).
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

/// Stream a message body in `BODY_PEEK[]<origin.chunk>` partial fetches,
/// reporting byte progress after each window. `RFC822.SIZE` supplies the total
/// for a determinate bar; until it arrives (and for servers that ignore the
/// range and return the whole body at once) the loop still terminates via a
/// short final chunk, so the frontend just shows an indeterminate bar.
async fn fetch_body_chunked(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
    message_id: MessageId,
    progress: &Option<BodyProgress>,
) -> Result<Vec<u8>, String> {
    let mut body: Vec<u8> = Vec::new();
    let mut origin: usize = 0;
    let mut total: Option<usize> = None;

    loop {
        let query = format!("(RFC822.SIZE BODY.PEEK[]<{origin}.{BODY_CHUNK_BYTES}>)");
        let mut fetches = session
            .uid_fetch(format!("{uid}"), &query)
            .await
            .map_err(|e| format!("fetch body: {e}"))?;
        let Some(fetch) = fetches
            .try_next()
            .await
            .map_err(|e| format!("collect body: {e}"))?
        else {
            return Err("message not found on server".into());
        };
        if total.is_none() {
            total = fetch.size.map(|size| size as usize);
        }
        let Some(chunk) = fetch.body() else {
            return Err("no body content returned".into());
        };
        if chunk.is_empty() || body.len() >= MAX_BODY_BYTES {
            break;
        }
        body.extend_from_slice(chunk);
        let received = body.len();
        push_progress(
            progress,
            message_id,
            "fetching",
            received as u64,
            total.unwrap_or(0) as u64,
        );

        if let Some(total) = total {
            if received >= total {
                break;
            }
        } else if chunk.len() < BODY_CHUNK_BYTES {
            // No size reported — a short final chunk is the only end signal.
            break;
        }
        origin += chunk.len();
    }

    Ok(body)
}

/// Backfill stored bodies — and therefore real snippets — for messages that
/// were synced before the sync fetched full bodies. Runs once per account at
/// startup; idempotent (only messages with no stored body are fetched, so an
/// interrupted run resumes where it left off).
pub async fn backfill_account_bodies(
    store: &SqliteStore,
    account: &Account,
    credential: &Credential,
    progress: &Option<SyncProgress>,
) -> Result<usize, String> {
    let pending = store.list_messages_missing_bodies(account.id)?;
    if pending.is_empty() {
        return Ok(0);
    }

    // Group by (display folder, server mailbox, uidvalidity) so each mailbox
    // is selected once and its pending UIDs fetched in a single streamed
    // command rather than one round-trip per message.
    let mut by_folder: BTreeMap<(String, String, u32), Vec<(u32, MessageId)>> = BTreeMap::new();
    for p in &pending {
        by_folder
            .entry((p.folder.clone(), p.server_folder.clone(), p.uidvalidity))
            .or_default()
            .push((p.uid, p.message_id));
    }

    let mut session = connect(account, credential).await?;
    let mut filled = 0;
    for ((_folder, server_folder, uidvalidity), uid_ids) in by_folder {
        let mailbox = session
            .select(&server_folder)
            .await
            .map_err(|e| format!("select {server_folder}: {e}"))?;
        // UIDVALIDITY changed since these rows were stored — the folder was
        // re-synced and the uids no longer map to them; skip rather than
        // writing under a stale identity.
        if mailbox.uid_validity.unwrap_or(0) != uidvalidity {
            continue;
        }
        let uids: Vec<u32> = uid_ids.iter().map(|(u, _)| *u).collect();
        let uid_to_message: BTreeMap<u32, MessageId> = uid_ids.into_iter().collect();
        let range = uids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let query = "(UID FLAGS INTERNALDATE ENVELOPE BODY.PEEK[])";
        let mut fetches = session
            .uid_fetch(range, &query)
            .await
            .map_err(|e| format!("backfill fetch {server_folder}: {e}"))?;
        while let Some(fetch) = fetches
            .try_next()
            .await
            .map_err(|e| format!("backfill stream {server_folder}: {e}"))?
        {
            let Some(message_id) = fetch.uid.and_then(|u| uid_to_message.get(&u).copied()) else {
                continue;
            };
            let Some(raw_body) = fetch.body() else {
                continue;
            };
            if let Some(parsed) = parse_full_message(raw_body) {
                persist_parsed_message(store, message_id, &parsed)?;
                filled += 1;
                ping_progress(progress);
            }
        }
    }
    let _ = session.logout().await;
    Ok(filled)
}

/// Replay queued offline actions against the server.
///
/// Conflict Policy:
/// - If a message UID does not exist on the server (expunged/deleted), the action
///   is dropped cleanly.
/// - Transient network errors increment retry counter and suspend replay until reconnect.
pub async fn replay_pending_actions(
    store: &SqliteStore,
    account: &Account,
    session: &mut async_imap::Session<Stream>,
    credential: &Credential,
) -> Result<(), String> {
    let actions = store.peek_pending_actions(account.id);
    let mut current_folder = String::new();

    for action in actions {
        // SMTP sends don't operate on a mailbox, and the folder recorded for
        // them ("Outbox", or "Sent" for RSVP replies) is often not a real IMAP
        // mailbox — SELECTing it would fail and block every queued send
        // forever. Only re-select a mailbox for IMAP actions.
        let needs_mailbox = !matches!(action.action_type, ActionType::Send);
        if needs_mailbox && !action.folder.is_empty() && action.folder != current_folder {
            if let Err(e) = session.select(&action.folder).await {
                log::warn!("select for replay {}: {e}", action.folder);
                let _ = store.increment_action_retry(action.id);
                continue;
            }
            current_folder = action.folder.clone();
        }

        let result = match action.action_type {
            ActionType::MarkRead => {
                if let Some(uid) = action.uid {
                    set_seen(session, uid, true).await
                } else {
                    Ok(())
                }
            }
            ActionType::MarkUnread => {
                if let Some(uid) = action.uid {
                    set_seen(session, uid, false).await
                } else {
                    Ok(())
                }
            }
            ActionType::Star => {
                if let Some(uid) = action.uid {
                    set_flagged(session, uid, true).await
                } else {
                    Ok(())
                }
            }
            ActionType::Unstar => {
                if let Some(uid) = action.uid {
                    set_flagged(session, uid, false).await
                } else {
                    Ok(())
                }
            }
            ActionType::Archive => {
                if let Some(uid) = action.uid {
                    // Copy to the account's Archive mailbox first, and only
                    // delete the source if the copy succeeded. Gmail has no
                    // mailbox literally named "Archive" (its archive is
                    // [Gmail]/All Mail, skipped in discovery), so the previous
                    // copy-then-delete-anyway silently deleted the message
                    // instead of archiving it.
                    let archive_folder = store
                        .archive_folder_name(account.id)
                        .unwrap_or_else(|| "Archive".to_string());
                    match session.uid_copy(format!("{uid}"), &archive_folder).await {
                        Ok(_) => set_deleted(session, uid).await,
                        Err(e) => Err(format!("archive copy to {archive_folder}: {e}")),
                    }
                } else {
                    Ok(())
                }
            }
            ActionType::Delete => {
                if let Some(uid) = action.uid {
                    set_deleted(session, uid).await
                } else {
                    Ok(())
                }
            }
            ActionType::Move => {
                if let (Some(uid), Some(ref dest)) = (action.uid, &action.payload) {
                    match session.uid_copy(format!("{uid}"), dest).await {
                        Ok(_) => set_deleted(session, uid).await,
                        Err(e) => Err(format!("move copy to {dest}: {e}")),
                    }
                } else {
                    Ok(())
                }
            }
            ActionType::MarkAnswered => {
                if let Some(uid) = action.uid {
                    set_answered(session, uid, true).await
                } else {
                    Ok(())
                }
            }
            ActionType::MarkForwarded => {
                if let Some(uid) = action.uid {
                    set_forwarded(session, uid, true).await
                } else {
                    Ok(())
                }
            }
            ActionType::MarkJunk => {
                if let Some(uid) = action.uid {
                    let _ = session
                        .uid_store(format!("{uid}"), "+FLAGS.SILENT ($Junk)")
                        .await;
                    let junk_folder = "Junk";
                    match session.uid_copy(format!("{uid}"), junk_folder).await {
                        Ok(_) => set_deleted(session, uid).await,
                        Err(_) => Ok(()),
                    }
                } else {
                    Ok(())
                }
            }
            ActionType::MarkNotJunk => {
                if let Some(uid) = action.uid {
                    let _ = session
                        .uid_store(format!("{uid}"), "-FLAGS.SILENT ($Junk)")
                        .await;
                    let _ = session
                        .uid_store(format!("{uid}"), "+FLAGS.SILENT ($NotJunk)")
                        .await;
                    let inbox_folder = "INBOX";
                    match session.uid_copy(format!("{uid}"), inbox_folder).await {
                        Ok(_) => set_deleted(session, uid).await,
                        Err(_) => Ok(()),
                    }
                } else {
                    Ok(())
                }
            }
            ActionType::Send => {
                if let Some(ref payload) = action.payload {
                    if let Ok(outgoing) = serde_json::from_str::<OutgoingMessage>(payload) {
                        crate::smtp::send_email(account, &outgoing, credential).await
                    } else {
                        Ok(()) // invalid payload, drop
                    }
                } else {
                    Ok(())
                }
            }
        };

        match result {
            Ok(()) => {
                let _ = store.remove_action(action.id);
            }
            Err(e) => {
                log::warn!("failed action replay {}: {e}", action.id);
                // If the error indicates missing message / conflict, remove it so it doesn't block queue
                if e.contains("no such message") || e.contains("not found") {
                    let _ = store.remove_action(action.id);
                } else {
                    let _ = store.increment_action_retry(action.id);
                }
            }
        }
    }

    Ok(())
}

/// Everything the store needs from a parsed full message. Extracted once and
/// shared by the sync-time body store and the on-demand reading-pane fetch.
struct ParsedMessage {
    /// RFC 2047-decoded subject, as the list row should show it.
    subject: String,
    plain_body: String,
    html_body: Option<String>,
    to: Vec<Recipient>,
    cc: Vec<Recipient>,
    bcc: Vec<Recipient>,
    message_id_header: Option<String>,
    in_reply_to: Option<String>,
    references: Option<String>,
    list_unsubscribe: Option<String>,
    list_unsubscribe_post: Option<String>,
    attachments: Vec<Attachment>,
}

/// Parse a full RFC 5322 message and pull out everything the store keeps.
fn parse_full_message(raw_body: &[u8]) -> Option<ParsedMessage> {
    let parsed = MessageParser::default().parse(raw_body)?;
    let plain_body = parsed
        .body_text(0)
        .map(|t| t.to_string())
        .unwrap_or_default();
    let html_body = parsed.body_html(0).map(|h| h.to_string());
    let message_id_header = parsed.message_id().map(ToString::to_string);
    let in_reply_to = parsed.in_reply_to().as_text().map(ToString::to_string);
    let references = parsed.references().as_text().map(ToString::to_string);
    let list_unsubscribe = parsed
        .headers()
        .iter()
        .find(|h| h.name().eq_ignore_ascii_case("List-Unsubscribe"))
        .and_then(|h| h.value().as_text())
        .map(ToString::to_string);
    let list_unsubscribe_post = parsed
        .headers()
        .iter()
        .find(|h| h.name().eq_ignore_ascii_case("List-Unsubscribe-Post"))
        .and_then(|h| h.value().as_text())
        .map(ToString::to_string);
    let attachments = parsed
        .attachments()
        .enumerate()
        .map(|(i, att)| Attachment {
            id: (i + 1) as u32,
            message_id: 0, // reassigned by the caller's insert
            filename: att
                .attachment_name()
                .unwrap_or(&format!("attachment_{i}"))
                .to_string(),
            size_bytes: att.contents().len() as u64,
            on_disk: false,
        })
        .collect();
    Some(ParsedMessage {
        // mail-parser decodes RFC 2047 encoded-words in headers, so this is
        // the subject as a human should read it (e.g. "🚘 …" rather than
        // "=?UTF-8?Q?=F0=9F=9A=98…?=").
        subject: parsed.subject().map(ToString::to_string).unwrap_or_default(),
        plain_body,
        html_body,
        to: parsed.to().map(address_list).unwrap_or_default(),
        cc: parsed.cc().map(address_list).unwrap_or_default(),
        bcc: parsed.bcc().map(address_list).unwrap_or_default(),
        message_id_header,
        in_reply_to,
        references,
        list_unsubscribe,
        list_unsubscribe_post,
        attachments,
    })
}

/// Flatten an RFC 5322 address header (a list, or groups of mailboxes) into
/// the store's recipient shape.
fn address_list(addrs: &Address) -> Vec<Recipient> {
    let mut out = Vec::new();
    match addrs {
        Address::List(addrs) => {
            for addr in addrs {
                out.push(Recipient {
                    name: addr.name.as_deref().unwrap_or_default().to_string(),
                    address: addr.address.as_deref().unwrap_or_default().to_string(),
                });
            }
        }
        Address::Group(groups) => {
            for group in groups {
                for addr in &group.addresses {
                    out.push(Recipient {
                        name: addr.name.as_deref().unwrap_or_default().to_string(),
                        address: addr.address.as_deref().unwrap_or_default().to_string(),
                    });
                }
            }
        }
    }
    out
}

/// Store a parsed message's body, metadata, and refreshed snippet for an
/// existing row. Shared by the on-demand reading-pane fetch and the startup
/// backfill.
fn persist_parsed_message(
    store: &SqliteStore,
    message_id: MessageId,
    parsed: &ParsedMessage,
) -> Result<(), String> {
    let snippet = snippet_from_bodies(&parsed.plain_body, parsed.html_body.as_deref());
    store.update_snippet(message_id, &snippet)?;
    store.save_message_body_and_attachments(
        message_id,
        &parsed.plain_body,
        parsed.html_body.as_deref(),
        &parsed.to,
        &parsed.cc,
        &parsed.bcc,
        &parsed.attachments,
        parsed.message_id_header.as_deref(),
        parsed.in_reply_to.as_deref(),
        parsed.references.as_deref(),
        parsed.list_unsubscribe.as_deref(),
        parsed.list_unsubscribe_post.as_deref(),
    )
}

fn envelope_row(
    account: &Account,
    folder: &str,
    _uidvalidity: u32,
    fetch: &Fetch,
) -> Option<(MessageRow, u32, Option<ParsedMessage>)> {
    let uid = fetch.uid?;
    let envelope = fetch.envelope()?;
    let parsed = fetch.body().and_then(parse_full_message);

    let from = envelope.from.as_ref().and_then(|v| v.first());
    // Display names can be RFC 2047 encoded-words too; decode them the same
    // way as the subject (plain text passes through unchanged).
    let sender_name = from
        .and_then(|a| a.name.as_ref())
        .map(|n| quill_store::sanitize::decode_rfc2047(&String::from_utf8_lossy(n)))
        .unwrap_or_default();
    let sender_address = match from.and_then(|a| a.mailbox.as_ref()) {
        Some(mailbox) => {
            let local = String::from_utf8_lossy(mailbox).into_owned();
            match from.and_then(|a| a.host.as_ref()) {
                Some(host) => format!("{local}@{}", String::from_utf8_lossy(host)),
                None => local,
            }
        }
        None => String::new(),
    };
    // Prefer the decoded subject from the parsed message (mail-parser handles
    // RFC 2047 encoded-words); fall back to decoding the raw ENVELOPE subject,
    // which IMAP serves exactly as stored — encoded-words and all.
    let subject = parsed
        .as_ref()
        .map(|p| p.subject.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            envelope.subject.as_ref().map(|s| {
                quill_store::sanitize::decode_rfc2047(&String::from_utf8_lossy(s))
            })
        })
        .unwrap_or_default();
    let received_at_ms = fetch
        .internal_date()
        .map(|d| d.timestamp_millis())
        .unwrap_or_else(now_ms);
    let unread = !fetch.flags().any(|f| matches!(f, Flag::Seen));
    let flagged = fetch.flags().any(|f| matches!(f, Flag::Flagged));
    let answered = fetch.flags().any(|f| match f {
        Flag::Answered => true,
        Flag::Custom(s) => s.eq_ignore_ascii_case("$answered"),
        _ => false,
    });
    let forwarded = fetch.flags().any(|f| match f {
        Flag::Custom(s) => {
            s.eq_ignore_ascii_case("$forwarded")
                || s.eq_ignore_ascii_case("forwarded")
                || s.eq_ignore_ascii_case("$passed")
        }
        _ => false,
    });
    // The sync now fetches the full message (`BODY.PEEK[]`), so the snippet
    // comes from the parsed plain-text body — not a raw MIME/HTML fragment —
    // and the parsed body itself is persisted in `write_envelope`.
    let snippet = match &parsed {
        Some(p) => snippet_from_bodies(&p.plain_body, p.html_body.as_deref()),
        None => fetch
            .text()
            .map(|t| {
                String::from_utf8_lossy(t)
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default(),
    };

    let row = MessageRow {
        id: 0,
        account_id: account.id,
        folder: folder.to_string(),
        sender_name,
        sender_address,
        subject,
        snippet,
        received_at_ms,
        unread,
        flagged,
        answered,
        forwarded,
        has_attachments: parsed.as_ref().map_or(false, |p| !p.attachments.is_empty()),
        thread_id: None,
        thread_count: 1,
    };
    Some((row, uid, parsed))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub async fn set_seen(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
    seen: bool,
) -> Result<(), String> {
    let cmd = if seen {
        "+FLAGS.SILENT (\\Seen)"
    } else {
        "-FLAGS.SILENT (\\Seen)"
    };
    session
        .uid_store(uid.to_string(), cmd)
        .await
        .map_err(|e| format!("store: {e}"))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("store: {e}"))?;
    Ok(())
}

pub async fn set_flagged(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
    flagged: bool,
) -> Result<(), String> {
    let cmd = if flagged {
        "+FLAGS.SILENT (\\Flagged)"
    } else {
        "-FLAGS.SILENT (\\Flagged)"
    };
    session
        .uid_store(uid.to_string(), cmd)
        .await
        .map_err(|e| format!("store: {e}"))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("store: {e}"))?;
    Ok(())
}

pub async fn set_answered(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
    answered: bool,
) -> Result<(), String> {
    let cmd = if answered {
        "+FLAGS.SILENT (\\Answered)"
    } else {
        "-FLAGS.SILENT (\\Answered)"
    };
    session
        .uid_store(uid.to_string(), cmd)
        .await
        .map_err(|e| format!("store: {e}"))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("store: {e}"))?;
    Ok(())
}

pub async fn set_forwarded(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
    forwarded: bool,
) -> Result<(), String> {
    let cmd = if forwarded {
        "+FLAGS.SILENT ($Forwarded)"
    } else {
        "-FLAGS.SILENT ($Forwarded)"
    };
    session
        .uid_store(uid.to_string(), cmd)
        .await
        .map_err(|e| format!("store: {e}"))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("store: {e}"))?;
    Ok(())
}

pub async fn set_deleted(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
) -> Result<(), String> {
    session
        .uid_store(uid.to_string(), "+FLAGS.SILENT (\\Deleted)")
        .await
        .map_err(|e| format!("store deleted: {e}"))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("store deleted: {e}"))?;
    let _ = session.expunge().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use quill_store::sanitize::SNIPPET_MAX;

    #[test]
    fn test_detect_folder_kind_heuristics() {
        assert_eq!(detect_folder_kind("INBOX", &[]), FolderKind::Inbox);
        assert_eq!(detect_folder_kind("Drafts", &[]), FolderKind::Drafts);
        assert_eq!(detect_folder_kind("Sent Messages", &[]), FolderKind::Sent);
        assert_eq!(detect_folder_kind("Archive", &[]), FolderKind::Archive);
        assert_eq!(detect_folder_kind("Trash", &[]), FolderKind::Trash);
        assert_eq!(detect_folder_kind("Junk Mail", &[]), FolderKind::Junk);
        assert_eq!(detect_folder_kind("Spam", &[]), FolderKind::Junk);
        assert_eq!(detect_folder_kind("Receipts", &[]), FolderKind::Inbox);
    }

    #[test]
    fn test_canonical_folder_name() {
        assert_eq!(canonical_folder_name("INBOX", FolderKind::Inbox), "Inbox");
        assert_eq!(
            canonical_folder_name("Drafts", FolderKind::Drafts),
            "Drafts"
        );
        assert_eq!(canonical_folder_name("Sent", FolderKind::Sent), "Sent");
        assert_eq!(
            canonical_folder_name("Archive", FolderKind::Archive),
            "Archive"
        );
        assert_eq!(canonical_folder_name("Junk", FolderKind::Junk), "Junk");
        assert_eq!(canonical_folder_name("Trash", FolderKind::Trash), "Trash");
    }

    #[test]
    fn test_mime_body_parsing() {
        let raw = concat!(
            "From: Sender <sender@example.com>\r\n",
            "To: Recipient <rec@example.com>\r\n",
            "Cc: Carbon <cc@example.com>\r\n",
            "Subject: Test Subject\r\n",
            "Content-Type: multipart/alternative; boundary=\"boundary123\"\r\n",
            "\r\n",
            "--boundary123\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "\r\n",
            "Hello plain world!\r\n",
            "--boundary123\r\n",
            "Content-Type: text/html; charset=utf-8\r\n",
            "\r\n",
            "<p>Hello <b>HTML</b> world!</p>\r\n",
            "--boundary123--\r\n"
        );

        let parsed = MessageParser::default().parse(raw.as_bytes()).unwrap();
        assert_eq!(parsed.body_text(0).unwrap(), "Hello plain world!");
        assert_eq!(
            parsed.body_html(0).unwrap(),
            "<p>Hello <b>HTML</b> world!</p>"
        );
    }

    #[test]
    fn test_snippet_from_bodies_prefers_plain_text() {
        let s = snippet_from_bodies(
            "Hello world, this is a plain body.",
            Some("<p>ignored html</p>"),
        );
        assert_eq!(s, "Hello world, this is a plain body.");
    }

    #[test]
    fn test_snippet_html_only_strips_markup() {
        // The exact shape that was leaking into the list rows: an HTML-only
        // message whose body starts with a doctype and markup.
        let html = concat!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>x</title></head>",
            "<body><p>Hi both, attached is the redlined lease.</p></body></html>"
        );
        let s = snippet_from_bodies("", Some(html));
        assert!(s.contains("Hi both"), "snippet should carry the body text: {s:?}");
        assert!(!s.contains("<!DOCTYPE"), "markup must not leak into the snippet");
        assert!(!s.contains("<p>"), "markup must not leak into the snippet");
    }

    #[test]
    fn test_snippet_capped_and_collapsed() {
        let long = "word ".repeat(500);
        let s = snippet_from_bodies(&long, None);
        assert!(s.len() <= SNIPPET_MAX);
        assert!(!s.contains("  "), "whitespace should be collapsed");
    }

    #[test]
    fn test_parse_full_message_extracts_recipients_and_body() {
        let raw = concat!(
            "From: Sender <sender@example.com>\r\n",
            "To: One <one@example.com>, Two <two@example.com>\r\n",
            "Cc: Carbon <cc@example.com>\r\n",
            "Subject: Test\r\n",
            "Content-Type: text/plain; charset=utf-8\r\n",
            "\r\n",
            "Body text here.\r\n"
        );
        let parsed = parse_full_message(raw.as_bytes()).unwrap();
        assert_eq!(parsed.plain_body.trim(), "Body text here.");
        assert_eq!(parsed.to.len(), 2);
        assert_eq!(parsed.cc.len(), 1);
        assert_eq!(parsed.bcc.len(), 0);
        assert_eq!(parsed.attachments.len(), 0);
    }
}

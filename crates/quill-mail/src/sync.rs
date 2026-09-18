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
use mail_parser::{Address, MessageParser, MimeHeaders, PartType};
use quill_store::folders::{
    classify_folder_kind, display_name_of, infer_namespace, local_name_for,
};
use quill_store::sanitize::snippet_from_bodies;
use quill_store::sqlite::SqliteStore;
use quill_store::types::{
    Account, ActionType, AttachmentData, DiscoveredMailbox, FolderKind, MessageId,
    MessageProgressUpdate, MessageRow, OutgoingMessage, Recipient,
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
    pub delimiter: String,
    pub subscribed: bool,
    pub selectable: bool,
    pub namespace: String,
}

impl DiscoveredFolder {
    pub fn to_mailbox(&self) -> DiscoveredMailbox {
        DiscoveredMailbox {
            server_name: self.server_name.clone(),
            local_name: self.local_name.clone(),
            display_name: display_name_of(&self.server_name, &self.delimiter),
            kind: self.kind,
            delimiter: self.delimiter.clone(),
            subscribed: self.subscribed,
            selectable: self.selectable,
            namespace: self.namespace.clone(),
        }
    }
}

/// Detect the folder kind from IMAP attributes and name heuristics.
pub fn detect_folder_kind(name: &str, attributes: &[NameAttribute]) -> FolderKind {
    classify_folder_kind(name, attributes.iter().map(|a| format!("{a:?}")))
}

/// Map server folder name to the local storage key (e.g. INBOX -> "Inbox",
/// custom mailboxes keep their server name).
pub fn canonical_folder_name(server_name: &str, kind: FolderKind) -> String {
    local_name_for(server_name, kind)
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
    let envelope_message_id = fetch
        .envelope()
        .and_then(|envelope| envelope.message_id.as_ref())
        .map(|value| String::from_utf8_lossy(value).into_owned());
    let message_id = store.upsert_fetched_message_with_message_id(
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
        envelope_message_id.as_deref(),
    )?;
    // When a body was requested with this fetch, persist it (plus its
    // recipients/attachment metadata). Header-only fetches still create or
    // refresh the row without disturbing a body already cached locally.
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

    let subscribed: HashSet<String> = match session.lsub(None, Some("*")).await {
        Ok(stream) => stream
            .try_collect::<Vec<Name>>()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|n| n.name().to_string())
            .collect(),
        Err(_) => HashSet::new(),
    };
    let lsub_ok = !subscribed.is_empty();

    let mut folders = Vec::new();
    let mut seen_local = std::collections::HashSet::new();
    for name in names {
        let server_name = name.name().to_string();
        let is_noselect = name
            .attributes()
            .iter()
            .any(|a| matches!(a, NameAttribute::NoSelect));
        let kind = detect_folder_kind(&server_name, name.attributes());
        // A second mailbox classified as Inbox would share the local key and
        // wipe the real inbox on refetch. Custom kinds keep their server name,
        // so only a mis-classified special can collide.
        if kind == FolderKind::Inbox && !server_name.eq_ignore_ascii_case("inbox") {
            continue;
        }
        let delimiter = name
            .delimiter()
            .filter(|d| !d.is_empty())
            .unwrap_or("/")
            .to_string();
        // Persist Gmail/Google Mail All Mail for provider actions, but keep it
        // out of the normal sync set so it does not duplicate every message.
        let local_name = if is_all_mail_mailbox(&server_name) {
            server_name.clone()
        } else {
            canonical_folder_name(&server_name, kind)
        };
        if !seen_local.insert(local_name.clone()) {
            continue;
        }
        let is_subscribed = if lsub_ok {
            subscribed.contains(&server_name)
        } else {
            true
        };
        folders.push(DiscoveredFolder {
            server_name: server_name.clone(),
            local_name,
            kind,
            delimiter: delimiter.clone(),
            subscribed: is_subscribed,
            selectable: !is_noselect,
            namespace: infer_namespace(&server_name, &delimiter),
        });
    }

    if folders.is_empty() {
        // Fallback to standard set if LIST returns empty
        folders = vec![
            fallback_folder("INBOX", FolderKind::Inbox),
            fallback_folder("Drafts", FolderKind::Drafts),
            fallback_folder("Sent", FolderKind::Sent),
            fallback_folder("Archive", FolderKind::Archive),
            fallback_folder("Trash", FolderKind::Trash),
        ];
    }

    Ok(folders)
}

fn fallback_folder(server_name: &str, kind: FolderKind) -> DiscoveredFolder {
    DiscoveredFolder {
        server_name: server_name.to_string(),
        local_name: canonical_folder_name(server_name, kind),
        kind,
        delimiter: "/".into(),
        subscribed: true,
        selectable: true,
        namespace: String::new(),
    }
}

fn is_all_mail_mailbox(server_name: &str) -> bool {
    let lower = server_name.to_ascii_lowercase();
    lower == "all"
        || lower == "all mail"
        || lower == "[gmail]/all mail"
        || lower == "[google mail]/all mail"
        || lower.ends_with("/all mail")
        || lower.ends_with(".all mail")
}

/// Sync every tracked folder for one account. Replays pending offline actions
/// first, then incrementally synchronizes each folder.
///
/// `replay_actions` gates the offline-action replay. Periodic, on-open, and
/// explicit refresh syncs replay; the concurrent IDLE push worker does not.
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

    // A server MOVE assigns a destination UID. Resolve optimistic rows by
    // Message-ID before normal folder sync, including Gmail All Mail (which is
    // intentionally excluded from full synchronization to avoid duplicates).
    resolve_pending_locations(store, account.id, &mut session).await;

    // 2. Discover server folders.
    let discovered = match discover_folders(&mut session).await {
        Ok(f) => f,
        Err(_) => vec![
            fallback_folder("INBOX", FolderKind::Inbox),
            fallback_folder("Drafts", FolderKind::Drafts),
            fallback_folder("Sent", FolderKind::Sent),
            fallback_folder("Archive", FolderKind::Archive),
            fallback_folder("Trash", FolderKind::Trash),
        ],
    };

    let mailboxes: Vec<DiscoveredMailbox> = discovered.iter().map(|d| d.to_mailbox()).collect();
    let _ = store.reconcile_folders(account.id, &mailboxes);
    let _ = store.set_account_folder_count(account.id, discovered.len() as u32);

    // P0.2 folder selection: when the account has a configured selection, sync
    // only the enabled server mailboxes; an empty selection (pre-P0.2 and
    // never-chosen accounts) syncs everything discovered. Newly discovered
    // mailboxes are added to the selection — subscribed ones enabled so the
    // user doesn't silently miss mail, unsubscribed ones visible but not
    // synced (T0.1). \Noselect parents are never fetched.
    let selection = store.synced_folders(account.id);
    let folders_to_sync: Vec<DiscoveredFolder> = if selection.is_empty() {
        discovered
            .iter()
            .filter(|f| f.selectable && !is_all_mail_mailbox(&f.server_name))
            .cloned()
            .collect()
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
                enabled: f.subscribed && f.selectable,
            })
            .collect();
        if !missing.is_empty() {
            let _ = store.upsert_synced_folders(account.id, &missing);
        }
        let enabled = store.enabled_folder_set(account.id).unwrap_or_default();
        discovered
            .into_iter()
            .filter(|f| {
                f.selectable
                    && !is_all_mail_mailbox(&f.server_name)
                    && enabled.contains(&f.server_name)
            })
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

    // Headers are retained for the entire mailbox. Only cached bodies and
    // attachment files are evicted according to the account's cache window.
    let cutoff = store.body_cache_cutoff_ms(account.id, now_ms());
    if let Err(e) = store.evict_cached_bodies_before(account.id, cutoff) {
        log::warn!("evict cached bodies for account {}: {e}", account.id);
    }

    let _ = session.logout().await;
    Ok(outcome)
}

async fn resolve_pending_locations(
    store: &SqliteStore,
    account_id: u32,
    session: &mut async_imap::Session<Stream>,
) {
    let Ok(pending) = store.pending_message_locations(account_id) else {
        return;
    };
    for (message_id, server_folder, message_id_header) in pending {
        let Ok(mailbox) = session.select(&server_folder).await else {
            continue;
        };
        let Some(uidvalidity) = mailbox.uid_validity else {
            continue;
        };
        let query = format!(
            "HEADER Message-ID \"{}\"",
            message_id_header.replace('"', "")
        );
        let Ok(matches) = session.uid_search(query).await else {
            continue;
        };
        if let Some(uid) = matches.iter().next().copied() {
            let _ = store.resolve_pending_message_location(
                message_id,
                &server_folder,
                uid,
                uidvalidity,
            );
        }
    }
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
            // Reconcile the complete mailbox using headers first. Body bytes
            // are fetched only for new/missing messages inside the cache
            // window, so old mail remains immediately usable as headers and is
            // fetched on demand when opened.
            let existing: HashSet<u32> = store
                .folder_uids(account.id, local_folder)
                .into_iter()
                .collect();
            let missing: HashSet<u32> = store
                .list_messages_missing_bodies_since(
                    account.id,
                    store.body_cache_cutoff_ms(account.id, now_ms()),
                )?
                .into_iter()
                .filter(|p| p.folder == local_folder)
                .map(|p| p.uid)
                .collect();
            let cutoff = store.body_cache_cutoff_ms(account.id, now_ms());
            let mut server_uids = Vec::new();
            let mut body_uids = missing;
            // SEARCH gives us a descending UID order so the first header
            // batches are the newest mail. A search failure falls back to the
            // standard full range and still retains every header.
            let mut all_uids: Vec<u32> = session
                .uid_search("ALL")
                .await
                .unwrap_or_default()
                .into_iter()
                .collect();
            all_uids.sort_unstable_by(|a, b| b.cmp(a));
            let header_ranges: Vec<String> = if all_uids.is_empty() {
                vec!["1:*".to_string()]
            } else {
                all_uids
                    .chunks(200)
                    .map(|chunk| {
                        chunk
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .collect()
            };
            for range in header_ranges {
                match session
                    .uid_fetch(&range, "(UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE)")
                    .await
                {
                    Ok(mut fetches) => loop {
                        match fetches.try_next().await {
                            Ok(Some(fetch)) => {
                                if let Some(uid) = fetch.uid {
                                    server_uids.push(uid);
                                    if !existing.contains(&uid)
                                        && (cutoff == i64::MIN
                                            || fetch
                                                .internal_date()
                                                .map(|d| d.timestamp_millis() >= cutoff)
                                                .unwrap_or(true))
                                    {
                                        body_uids.insert(uid);
                                    }
                                    if write_envelope(
                                        store,
                                        account,
                                        local_folder,
                                        server_folder,
                                        uidvalidity,
                                        &fetch,
                                        progress,
                                    )?
                                    .is_some()
                                    {
                                        fetched_count += 1;
                                    }
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
                if !complete {
                    break;
                }
            }

            if complete && !body_uids.is_empty() {
                let body_query = "(UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])";
                let mut body_uids: Vec<u32> = body_uids.into_iter().collect();
                body_uids.sort_unstable_by(|a, b| b.cmp(a));
                for chunk in body_uids.chunks(200) {
                    let body_range = chunk
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",");
                    match session.uid_fetch(&body_range, body_query).await {
                        Ok(mut fetches) => loop {
                            match fetches.try_next().await {
                                Ok(Some(fetch)) => {
                                    if write_envelope(
                                        store,
                                        account,
                                        local_folder,
                                        server_folder,
                                        uidvalidity,
                                        &fetch,
                                        progress,
                                    )?
                                    .is_some()
                                    {
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
                    if !complete {
                        break;
                    }
                }
            }

            // Only reconcile expunges when the complete header stream arrived;
            // a partial stream must never delete unseen rows.
            if complete {
                store.delete_messages_not_in(account.id, local_folder, &server_uids)?;
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
            let query = "(UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])";
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
                        let _ =
                            store.delete_messages_not_in(account.id, local_folder, &server_uids);
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

/// Backfill stored bodies — and therefore real snippets — for recent messages
/// that were synced before the sync fetched full bodies. Runs once per account
/// at startup; older bodies remain on-demand and an interrupted run resumes.
pub async fn backfill_account_bodies(
    store: &SqliteStore,
    account: &Account,
    credential: &Credential,
    progress: &Option<SyncProgress>,
) -> Result<usize, String> {
    let pending = store.list_messages_missing_bodies_since(
        account.id,
        store.body_cache_cutoff_ms(account.id, now_ms()),
    )?;
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
        let query = "(UID FLAGS INTERNALDATE ENVELOPE BODYSTRUCTURE BODY.PEEK[])";
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
    let actions = store.peek_pending_actions(account.id)?;
    let mut current_folder = String::new();
    let mut current_uidvalidity = None;

    for action in actions {
        if !store.claim_action(action.id)? {
            continue;
        }
        // SMTP sends don't operate on a mailbox, and the folder recorded for
        // them ("Outbox", or "Sent" for RSVP replies) is often not a real IMAP
        // mailbox — SELECTing it would fail and block every queued send
        // forever. Only re-select a mailbox for IMAP actions.
        let needs_mailbox = !matches!(
            action.action_type,
            ActionType::Send
                | ActionType::CreateFolder
                | ActionType::RenameFolder
                | ActionType::DeleteFolder
                | ActionType::SubscribeFolder
                | ActionType::UnsubscribeFolder
        );
        if needs_mailbox && !action.folder.is_empty() && action.folder != current_folder {
            match session.select(&action.folder).await {
                Ok(mailbox) => current_uidvalidity = mailbox.uid_validity,
                Err(e) => {
                    let error = format!("select for replay {}: {e}", action.folder);
                    log::warn!("{error}");
                    let _ = store.rollback_action_local(
                        account.id,
                        action.action_type,
                        &action.folder,
                        action.uid,
                        action.uidvalidity,
                        action.message_id_header.as_deref(),
                    );
                    let _ = store.record_action_failure(action.id, &error);
                    continue;
                }
            }
            current_folder = action.folder.clone();
        }

        let mut effective_uidvalidity = action.uidvalidity;
        let resolved_uid = if action.action_type.requires_message_uid() {
            match (action.uid, action.uidvalidity, current_uidvalidity) {
                (Some(uid), Some(expected), Some(actual)) if expected == actual => Some(uid),
                (Some(_), _, _) => {
                    let message_id = action.message_id_header.as_deref().ok_or_else(|| {
                        "UIDVALIDITY changed and the action has no Message-ID to re-resolve"
                            .to_string()
                    });
                    match message_id {
                        Ok(message_id) => {
                            let query =
                                format!("HEADER Message-ID \"{}\"", message_id.replace('"', ""));
                            match session.uid_search(query).await {
                                Ok(mut matches) => {
                                    let resolved = matches.drain().next();
                                    if let (Some(uid), Some(uidvalidity)) =
                                        (resolved, current_uidvalidity)
                                    {
                                        effective_uidvalidity = Some(uidvalidity);
                                        let _ = store.update_action_identity(
                                            action.id,
                                            uid,
                                            uidvalidity,
                                        );
                                    }
                                    resolved
                                }
                                Err(e) => {
                                    let error = format!("re-resolve after UIDVALIDITY change: {e}");
                                    let _ = store.rollback_action_local(
                                        account.id,
                                        action.action_type,
                                        &action.folder,
                                        None,
                                        None,
                                        action.message_id_header.as_deref(),
                                    );
                                    let _ = store.record_action_failure(action.id, &error);
                                    continue;
                                }
                            }
                        }
                        Err(error) => {
                            let _ = store.rollback_action_local(
                                account.id,
                                action.action_type,
                                &action.folder,
                                None,
                                None,
                                action.message_id_header.as_deref(),
                            );
                            let _ = store.record_action_failure(action.id, &error);
                            continue;
                        }
                    }
                }
                (None, _, _) if action.action_type == ActionType::Move => None,
                _ => None,
            }
        } else {
            action.uid
        };

        let restore_move = action.action_type == ActionType::Move
            && action.uid.is_none()
            && action.payload.as_deref().is_some_and(|payload| {
                serde_json::from_str::<serde_json::Value>(payload)
                    .ok()
                    .and_then(|value| value.get("restore_to").cloned())
                    .is_some()
            });
        if action.action_type.requires_message_uid() && resolved_uid.is_none() && !restore_move {
            let error = "message UID is missing or could not be re-resolved";
            let _ = store.rollback_action_local(
                account.id,
                action.action_type,
                &action.folder,
                None,
                None,
                action.message_id_header.as_deref(),
            );
            let _ = store.record_action_failure(action.id, error);
            continue;
        }

        let result: Result<(), String> = async {
            match action.action_type {
                ActionType::MarkRead => {
                    if let Some(uid) = resolved_uid {
                        set_seen(session, uid, true).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::MarkUnread => {
                    if let Some(uid) = resolved_uid {
                        set_seen(session, uid, false).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::Star => {
                    if let Some(uid) = resolved_uid {
                        set_flagged(session, uid, true).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::Unstar => {
                    if let Some(uid) = resolved_uid {
                        set_flagged(session, uid, false).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::Archive => {
                    if let Some(uid) = resolved_uid {
                        // Copy to the account's Archive mailbox first, and only
                        // delete the source if the copy succeeded. Gmail has no
                        // mailbox literally named "Archive" (Gmail's archive is
                        // [Gmail]/All Mail, persisted during discovery), so a
                        // provider-aware target is required before moving.
                        let archive_folder =
                            store.archive_folder_name(account.id).ok_or_else(|| {
                                "no Archive or All Mail mailbox configured for this account"
                                    .to_string()
                            })?;
                        move_uid_to_mailbox(session, uid, &archive_folder).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::Delete => {
                    if let Some(uid) = resolved_uid {
                        let trash_folder = store
                            .trash_folder_name(account.id)
                            .unwrap_or_else(|| "Trash".to_string());
                        if action.folder == trash_folder || is_spam_mailbox(&action.folder) {
                            // Deleting from Trash is the explicit permanent-delete
                            // path; scope UID EXPUNGE to this one UID.
                            expunge_uid(session, uid).await
                        } else {
                            move_uid_to_mailbox(session, uid, &trash_folder).await
                        }
                    } else {
                        Ok(())
                    }
                }
                ActionType::Move => {
                    if let Some(ref payload) = action.payload {
                        let (dest, uid) = if let Ok(restore) =
                            serde_json::from_str::<serde_json::Value>(payload)
                        {
                            let Some(dest) = restore.get("restore_to").and_then(|v| v.as_str())
                            else {
                                return Err("restore action missing destination".into());
                            };
                            let message_id = restore
                                .get("message_id")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| "restore action missing Message-ID".to_string())?;
                            let query =
                                format!("HEADER Message-ID \"{}\"", message_id.replace('"', ""));
                            let mut matches = session
                                .uid_search(query)
                                .await
                                .map_err(|e| format!("find message in Trash: {e}"))?;
                            let uid = matches
                                .drain()
                                .next()
                                .ok_or_else(|| "message no longer exists in Trash".to_string())?;
                            (dest.to_string(), uid)
                        } else if let Some(uid) = resolved_uid {
                            (payload.clone(), uid)
                        } else {
                            return Ok(());
                        };
                        move_uid_to_mailbox(session, uid, &dest).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::MarkAnswered => {
                    if let Some(uid) = resolved_uid {
                        set_answered(session, uid, true).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::MarkForwarded => {
                    if let Some(uid) = resolved_uid {
                        set_forwarded(session, uid, true).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::MarkJunk => {
                    if let Some(uid) = resolved_uid {
                        let junk_folder = store
                            .junk_folder_name(account.id)
                            .ok_or_else(|| "no Junk/Spam mailbox configured".to_string())?;
                        move_uid_to_mailbox(session, uid, &junk_folder).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::MarkNotJunk => {
                    if let Some(uid) = resolved_uid {
                        let _ = session
                            .uid_store(format!("{uid}"), "-FLAGS.SILENT ($Junk)")
                            .await;
                        let _ = session
                            .uid_store(format!("{uid}"), "+FLAGS.SILENT ($NotJunk)")
                            .await;
                        let inbox_folder = store
                            .inbox_folder_name(account.id)
                            .unwrap_or_else(|| "INBOX".to_string());
                        move_uid_to_mailbox(session, uid, &inbox_folder).await
                    } else {
                        Ok(())
                    }
                }
                ActionType::Send => {
                    if let Some(ref payload) = action.payload {
                        let outgoing = serde_json::from_str::<OutgoingMessage>(payload)
                            .map_err(|e| format!("invalid queued send payload: {e}"))?;
                        crate::smtp::send_email(account, &outgoing, credential).await
                    } else {
                        Err("queued send is missing its payload".into())
                    }
                }
                ActionType::CreateFolder => match session.create(&action.folder).await {
                    Ok(()) => {
                        let _ = session.subscribe(&action.folder).await;
                        Ok(())
                    }
                    Err(e) => Err(format!("create folder {}: {e}", action.folder)),
                },
                ActionType::RenameFolder => {
                    let dest = action
                        .payload
                        .as_deref()
                        .ok_or_else(|| "rename folder missing destination".to_string())?;
                    session
                        .rename(&action.folder, dest)
                        .await
                        .map_err(|e| format!("rename folder {} → {dest}: {e}", action.folder))
                }
                ActionType::DeleteFolder => session
                    .delete(&action.folder)
                    .await
                    .map_err(|e| format!("delete folder {}: {e}", action.folder)),
                ActionType::SubscribeFolder => session
                    .subscribe(&action.folder)
                    .await
                    .map_err(|e| format!("subscribe {}: {e}", action.folder)),
                ActionType::UnsubscribeFolder => session
                    .unsubscribe(&action.folder)
                    .await
                    .map_err(|e| format!("unsubscribe {}: {e}", action.folder)),
            }
        }
        .await;

        match result {
            Ok(()) => {
                let _ = store.remove_action(action.id);
            }
            Err(e) => {
                log::warn!("failed action replay {}: {e}", action.id);
                let _ = store.rollback_action_local(
                    account.id,
                    action.action_type,
                    &action.folder,
                    resolved_uid,
                    effective_uidvalidity,
                    action.message_id_header.as_deref(),
                );
                // Bounded exponential backoff. At the cap the row becomes
                // `failed`, stays visible, and is replayed only after Retry.
                let _ = store.record_action_failure(action.id, &e);
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
    attachments: Vec<AttachmentData>,
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
        .map(|(i, att)| {
            let content_type = att
                .content_type()
                .map(|ct| {
                    format!(
                        "{}/{}",
                        ct.c_type,
                        ct.c_subtype.as_deref().unwrap_or("octet-stream")
                    )
                })
                .unwrap_or_else(|| "application/octet-stream".into());
            // Some servers include a filename on an inline part, which this
            // parser classifies as Binary. A Content-ID is the authoritative
            // signal for body-addressable inline content in that case.
            let is_inline =
                matches!(&att.body, PartType::InlineBinary(_)) || att.content_id().is_some();
            let generated_name = if is_inline {
                let subtype = att
                    .content_type()
                    .and_then(|ct| ct.c_subtype.as_deref())
                    .unwrap_or("bin");
                format!("inline_{}.{}", i + 1, subtype)
            } else {
                format!("attachment_{}", i + 1)
            };
            AttachmentData {
                filename: att.attachment_name().unwrap_or(&generated_name).to_string(),
                content_type,
                content_id: att.content_id().and_then(|cid| {
                    let cid = cid
                        .trim()
                        .trim_start_matches('<')
                        .trim_end_matches('>')
                        .trim();
                    (!cid.is_empty()).then(|| cid.to_string())
                }),
                is_inline,
                bytes: att.contents().to_vec(),
            }
        })
        .collect();
    Some(ParsedMessage {
        // mail-parser decodes RFC 2047 encoded-words in headers, so this is
        // the subject as a human should read it (e.g. "🚘 …" rather than
        // "=?UTF-8?Q?=F0=9F=9A=98…?=").
        subject: parsed
            .subject()
            .map(ToString::to_string)
            .unwrap_or_default(),
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
            envelope
                .subject
                .as_ref()
                .map(|s| quill_store::sanitize::decode_rfc2047(&String::from_utf8_lossy(s)))
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
    // Body fetches use the parsed plain-text body; header-only fetches leave
    // the snippet empty so the store preserves any previously cached snippet.
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
        has_attachments: parsed.as_ref().is_some_and(|p| !p.attachments.is_empty()),
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

/// Move a UID to a destination mailbox. RFC 6851 MOVE is preferred; when the
/// server lacks it, COPY + UID EXPUNGE provides the same no-bare-expunge
/// semantics while scoping removal to this UID.
pub async fn move_uid_to_mailbox(
    session: &mut async_imap::Session<Stream>,
    uid: u32,
    destination: &str,
) -> Result<(), String> {
    match session.uid_mv(uid.to_string(), destination).await {
        Ok(()) => Ok(()),
        Err(move_error) => {
            session
                .uid_copy(uid.to_string(), destination)
                .await
                .map_err(|copy_error| {
                    format!(
                        "MOVE to {destination} failed ({move_error}); COPY fallback failed: {copy_error}"
                    )
                })?;
            expunge_uid(session, uid).await
        }
    }
}

fn is_spam_mailbox(folder: &str) -> bool {
    let lower = folder.to_ascii_lowercase();
    lower.contains("spam") || lower.contains("junk")
}

/// Permanently remove exactly one UID. This is only used for messages already
/// in Trash (or for COPY fallbacks), never as a mailbox-wide EXPUNGE.
pub async fn expunge_uid(
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
    session
        .uid_expunge(uid.to_string())
        .await
        .map_err(|e| format!("UID EXPUNGE {uid}: {e}"))?
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("UID EXPUNGE {uid}: {e}"))?;
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
        assert_eq!(detect_folder_kind("Junk Email", &[]), FolderKind::Junk);
        assert_eq!(detect_folder_kind("Spam", &[]), FolderKind::Junk);
        assert_eq!(detect_folder_kind("Receipts", &[]), FolderKind::Custom);
        assert_eq!(detect_folder_kind("Work/Projects", &[]), FolderKind::Custom);
        assert_eq!(
            canonical_folder_name("Work/Projects", FolderKind::Custom),
            "Work/Projects"
        );
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
        assert!(
            s.contains("Hi both"),
            "snippet should carry the body text: {s:?}"
        );
        assert!(
            !s.contains("<!DOCTYPE"),
            "markup must not leak into the snippet"
        );
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

    #[test]
    fn test_parse_full_message_keeps_attachment_bytes_and_cid() {
        let raw = concat!(
            "From: sender@example.com\r\n",
            "To: one@example.com\r\n",
            "Subject: MIME\r\n",
            "Content-Type: multipart/related; boundary=x\r\n\r\n",
            "--x\r\nContent-Type: text/html\r\n\r\n<img src=\"cid:logo\">\r\n",
            "--x\r\nContent-Type: image/png\r\n",
            "Content-Disposition: inline; filename=\"logo.png\"\r\n",
            "Content-ID: <logo>\r\n",
            "Content-Transfer-Encoding: base64\r\n\r\n",
            "AQID\r\n--x--\r\n"
        );
        let parsed = parse_full_message(raw.as_bytes()).unwrap();
        assert_eq!(parsed.attachments.len(), 1);
        assert_eq!(parsed.attachments[0].filename, "logo.png");
        assert_eq!(parsed.attachments[0].content_id.as_deref(), Some("logo"));
        assert!(parsed.attachments[0].is_inline);
        assert_eq!(parsed.attachments[0].bytes, vec![1, 2, 3]);
    }
}

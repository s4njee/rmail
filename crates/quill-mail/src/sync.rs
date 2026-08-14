//! IMAP sync engine (Epic 12.2).
//!
//! Connects to an account's IMAP server, syncs folders incrementally via
//! UIDVALIDITY/UIDNEXT (full refetch only when the validity changes), and
//! writes fetched envelopes into the store. A failing folder never stalls the
//! others — each is isolated and reported. Bodies/attachments are fetched
//! lazily on selection (Epic 12.2), so sync only pulls envelope rows plus a
//! short body snippet for the list.

use async_imap::types::{Fetch, Flag};
use futures::TryStreamExt;
use quill_store::sqlite::SqliteStore;
use quill_store::types::{Account, MessageRow};
use tokio_util::compat::TokioAsyncReadCompatExt;

/// The stream async-imap reads over: any duplex that can be boxed.
pub(crate) trait IoStream:
    futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + std::fmt::Debug
{
}
impl<T: futures::io::AsyncRead + futures::io::AsyncWrite + Unpin + Send + std::fmt::Debug> IoStream
    for T
{
}

pub(crate) type Stream = Box<dyn IoStream>;

pub struct SyncOutcome {
    pub folders_synced: usize,
    pub messages_fetched: usize,
}

/// Sync every tracked folder for one account. Errors on a folder are
/// isolated — reported, not fatal — so one failing folder never stalls the
/// rest (Epic 12.2).
pub async fn sync_account(
    store: &SqliteStore,
    account: &Account,
    password: &str,
) -> Result<SyncOutcome, String> {
    let mut session = connect(account, password).await?;
    let mut outcome = SyncOutcome {
        folders_synced: 0,
        messages_fetched: 0,
    };
    for folder in ["INBOX", "Drafts", "Sent", "Archive"] {
        match sync_folder(store, account, &mut session, folder).await {
            Ok(fetched) => {
                outcome.folders_synced += 1;
                outcome.messages_fetched += fetched;
            }
            Err(e) => {
                // Isolated: report and continue. The shell surfaces this on
                // the account row's connectivity state.
                eprintln!("sync {} {folder}: {e}", account.address);
            }
        }
    }
    let _ = session.logout().await;
    Ok(outcome)
}

async fn connect(account: &Account, password: &str) -> Result<async_imap::Session<Stream>, String> {
    let addr = format!("{}:{}", account.server, account.port);
    let tcp = tokio::net::TcpStream::connect(&addr)
        .await
        .map_err(|e| format!("connect {addr}: {e}"))?;

    let stream: Stream = if account.tls {
        // async-native-tls is futures-io (async-imap's Session is too); the
        // tokio socket bridges over with compat.
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
    let session = client
        .login(&account.address, password)
        .await
        .map_err(|(e, _)| format!("login for {}: {e}", account.address))?;
    Ok(session)
}

async fn sync_folder(
    store: &SqliteStore,
    account: &Account,
    session: &mut async_imap::Session<Stream>,
    folder_name: &str,
) -> Result<usize, String> {
    let mailbox = session
        .select(folder_name)
        .await
        .map_err(|e| format!("select {folder_name}: {e}"))?;
    let uidvalidity = mailbox.uid_validity.unwrap_or(0);
    let uidnext = mailbox.uid_next.unwrap_or(0);
    let folder = folder_name.to_lowercase();

    // Incremental: only new messages since the last watermark; a changed
    // UIDVALIDITY means the mailbox was re-created — full refetch.
    let (last_validity, last_next) = store.get_sync_state(account.id, &folder);
    let full = last_validity != i64::from(uidvalidity);
    let start = if full {
        1
    } else {
        std::cmp::max(last_next, 1).max(1) as u32
    };

    let range = format!("{start}:*");
    let query = "(UID FLAGS INTERNALDATE ENVELOPE BODY.PEEK[TEXT]<0.300>)";
    let fetched = session
        .uid_fetch(&range, query)
        .await
        .map_err(|e| format!("fetch: {e}"))?;
    let fetched: Vec<Fetch> = fetched
        .try_collect()
        .await
        .map_err(|e| format!("fetch: {e}"))?;

    let mut count = 0;
    let server_uids: Vec<u32> = fetched.iter().filter_map(|f| f.uid).collect();
    for fetch in &fetched {
        let Some((row, uid)) = envelope_row(account, &folder, uidvalidity, fetch) else {
            continue;
        };
        store.upsert_fetched_message(
            account.id,
            &folder,
            uid,
            i64::from(uidvalidity),
            &row.sender_name,
            &row.sender_address,
            &row.subject,
            &row.snippet,
            row.received_at_ms,
            row.unread,
            row.flagged,
            row.has_attachments,
        )?;
        count += 1;
    }

    if full {
        store.delete_messages_not_in(account.id, &folder, &server_uids)?;
    }
    store.set_sync_state(
        account.id,
        &folder,
        i64::from(uidvalidity),
        i64::from(uidnext),
    )?;
    Ok(count)
}

/// Map one fetched message to a local row (snippet from the body head, which
/// keeps the list populated without pulling full bodies).
fn envelope_row(
    account: &Account,
    folder: &str,
    _uidvalidity: u32,
    fetch: &Fetch,
) -> Option<(MessageRow, u32)> {
    let uid = fetch.uid?;
    let envelope = fetch.envelope()?;

    let from = envelope.from.as_ref().and_then(|v| v.first());
    let sender_name = from
        .and_then(|a| a.name.as_ref())
        .map(|n| String::from_utf8_lossy(n).into_owned())
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
    let subject = envelope
        .subject
        .as_ref()
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .unwrap_or_default();
    let received_at_ms = fetch
        .internal_date()
        .map(|d| d.timestamp_millis())
        .unwrap_or_else(now_ms);
    let unread = !fetch.flags().any(|f| matches!(f, Flag::Seen));
    let flagged = fetch.flags().any(|f| matches!(f, Flag::Flagged));
    let snippet = fetch
        .text()
        .map(|t| {
            String::from_utf8_lossy(t)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default();

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
        has_attachments: false, // derived when the body is fetched
    };
    Some((row, uid))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Message actions propagating to the server (Epic 12.4). The frontend applies
/// them optimistically; this makes the server match, returning an error the
/// caller can roll back with. Wired to the store commands once a sync session
/// is addressable per account.
#[allow(dead_code)]
pub(crate) async fn set_seen(
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

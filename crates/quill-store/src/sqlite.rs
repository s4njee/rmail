//! SQLite-backed store (Epic 12.1).
//!
//! Replaces the in-memory store behind the same operations surface, so the
//! Tauri shell and the frontend are untouched. SQLite is the single source of
//! truth the UI reads from; the sync engine (Epic 12.2) writes to it. The
//! schema is indexed for the list query (folder + receivedAt DESC) and for
//! search, and migrations are forward-only, run on launch.

use crate::demo::{demo_accounts, demo_events, demo_messages};
use crate::types::*;
use rusqlite::types::Value;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PARAGRAPH_SEP: &str = "\n\n";

/// Forward-only migrations, indexed by target `user_version`.
const MIGRATIONS: [&str; 1] = [r#"
CREATE TABLE accounts (
  id INTEGER PRIMARY KEY,
  address TEXT NOT NULL UNIQUE,
  protocol TEXT NOT NULL,
  sync_mode TEXT NOT NULL,
  color TEXT NOT NULL,
  local_bytes INTEGER NOT NULL DEFAULT 0,
  connected INTEGER NOT NULL DEFAULT 0,
  server TEXT NOT NULL DEFAULT '',
  port INTEGER NOT NULL DEFAULT 993,
  tls INTEGER NOT NULL DEFAULT 1,
  folder_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE messages (
  id INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  folder TEXT NOT NULL,
  sender_name TEXT NOT NULL,
  sender_address TEXT NOT NULL,
  subject TEXT NOT NULL,
  snippet TEXT NOT NULL,
  received_at_ms INTEGER NOT NULL,
  unread INTEGER NOT NULL DEFAULT 1,
  flagged INTEGER NOT NULL DEFAULT 0,
  uid INTEGER,
  uidvalidity INTEGER,
  has_attachments INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_messages_folder_received ON messages(folder, received_at_ms DESC);
CREATE INDEX idx_messages_account ON messages(account_id);

CREATE TABLE bodies (
  message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
  plain TEXT NOT NULL DEFAULT '',
  html TEXT
);

CREATE TABLE recipients (
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  name TEXT NOT NULL,
  address TEXT NOT NULL,
  position INTEGER NOT NULL
);

CREATE TABLE attachments (
  id INTEGER PRIMARY KEY,
  message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
  filename TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  on_disk INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE events (
  id INTEGER PRIMARY KEY,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  start_ms INTEGER NOT NULL,
  end_ms INTEGER NOT NULL,
  all_day INTEGER NOT NULL DEFAULT 0,
  location TEXT,
  notes TEXT
);

CREATE TABLE sync_state (
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  folder TEXT NOT NULL,
  uidvalidity INTEGER,
  uidnext INTEGER,
  last_synced_at_ms INTEGER,
  PRIMARY KEY (account_id, folder)
);
"#];

pub struct SqliteStore {
    conn: Mutex<Connection>,
    attachments_root: PathBuf,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| e.to_string())?;
        let store = Self {
            conn: Mutex::new(conn),
            attachments_root: PathBuf::from("attachments"),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn set_attachments_root(&mut self, root: PathBuf) {
        self.attachments_root = root;
    }

    /// Forward-only migrations, run on launch.
    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| e.to_string())?;
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let target = i as i64 + 1;
            if target > version {
                conn.execute_batch(sql).map_err(|e| e.to_string())?;
                conn.pragma_update(None, "user_version", target)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    fn read_account(row: &rusqlite::Row<'_>) -> rusqlite::Result<Account> {
        Ok(Account {
            id: row.get(0)?,
            address: row.get(1)?,
            protocol: row.get(2)?,
            sync_mode: row.get(3)?,
            color: row.get(4)?,
            local_bytes: row.get::<_, i64>(5)? as u64,
            connected: row.get::<_, i64>(6)? != 0,
            server: row.get(7)?,
            port: row.get(8)?,
            tls: row.get::<_, i64>(9)? != 0,
            folder_count: row.get(10)?,
        })
    }

    fn read_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MessageRow> {
        Ok(MessageRow {
            id: row.get(0)?,
            account_id: row.get(1)?,
            folder: row.get(2)?,
            sender_name: row.get(3)?,
            sender_address: row.get(4)?,
            subject: row.get(5)?,
            snippet: row.get(6)?,
            received_at_ms: row.get(7)?,
            unread: row.get::<_, i64>(8)? != 0,
            flagged: row.get::<_, i64>(9)? != 0,
            has_attachments: row.get::<_, i64>(10)? != 0,
        })
    }

    pub fn accounts(&self) -> Vec<Account> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT * FROM accounts ORDER BY id")
            .expect("accounts query");
        stmt.query_map([], Self::read_account)
            .expect("accounts rows")
            .map(|r| r.expect("account row"))
            .collect()
    }

    pub fn folders(&self) -> Vec<Folder> {
        let conn = self.conn.lock().unwrap();
        const KINDS: [(FolderKind, &str); 5] = [
            (FolderKind::Inbox, "Inbox"),
            (FolderKind::Starred, "Starred"),
            (FolderKind::Drafts, "Drafts"),
            (FolderKind::Sent, "Sent"),
            (FolderKind::Archive, "Archive"),
        ];
        KINDS
            .into_iter()
            .enumerate()
            .map(|(i, (kind, name))| {
                let (sql, arg): (&str, Option<&str>) = match kind {
                    FolderKind::Starred => (
                        "SELECT COUNT(*), COALESCE(SUM(unread), 0) FROM messages WHERE flagged = 1",
                        None,
                    ),
                    _ => (
                        "SELECT COUNT(*), COALESCE(SUM(unread), 0) FROM messages WHERE folder = ?1",
                        Some(name),
                    ),
                };
                let (total, unread): (i64, i64) = match arg {
                    Some(a) => conn
                        .query_row(sql, params![a], |r| Ok((r.get(0)?, r.get(1)?)))
                        .expect("folder count"),
                    None => conn
                        .query_row(sql, [], |r| Ok((r.get(0)?, r.get(1)?)))
                        .expect("folder count"),
                };
                Folder {
                    id: (i + 1) as FolderId,
                    name: name.to_string(),
                    kind,
                    total_count: total as u32,
                    unread_count: unread as u32,
                }
            })
            .collect()
    }

    pub fn page_messages(&self, query: &MessageQuery) -> MessagePage {
        let conn = self.conn.lock().unwrap();
        let folder = query.folder.as_deref();
        use rusqlite::types::Value;

        let (where_sql, mut params): (String, Vec<Value>) = match folder {
            Some("Starred") => {
                let sql = if let Some(account) = query.account_id {
                    format!("flagged = 1 AND account_id = {account}")
                } else {
                    "flagged = 1".to_string()
                };
                (sql, vec![])
            }
            Some(f) => {
                let mut sql = String::from("folder = ?1");
                let p: Vec<Value> = vec![Value::Text(f.to_string())];
                if let Some(account) = query.account_id {
                    sql.push_str(&format!(" AND account_id = {account}"));
                }
                (sql, p)
            }
            None => {
                let mut sql = String::from("1 = 1");
                let p: Vec<Value> = vec![];
                if let Some(account) = query.account_id {
                    sql.push_str(&format!(" AND account_id = {account}"));
                }
                (sql, p)
            }
        };

        let total: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM messages WHERE {where_sql}"),
                rusqlite::params_from_iter(params.iter()),
                |r| r.get(0),
            )
            .expect("count query");

        let mut stmt = conn
            .prepare(&format!(
                "SELECT id, account_id, folder, sender_name, sender_address, subject, snippet, \
                 received_at_ms, unread, flagged, has_attachments FROM messages \
                 WHERE {where_sql} ORDER BY received_at_ms DESC LIMIT ?2 OFFSET ?3"
            ))
            .expect("page query");
        params.push(Value::Integer(i64::from(query.limit)));
        params.push(Value::Integer(i64::from(query.offset)));
        let rows: Vec<MessageRow> = stmt
            .query_map(rusqlite::params_from_iter(params.iter()), Self::read_row)
            .expect("page rows")
            .map(|r| r.expect("page row"))
            .collect();

        MessagePage {
            items: rows,
            total: total as u32,
        }
    }

    pub fn get_message(&self, id: MessageId) -> Option<MessageDetail> {
        let conn = self.conn.lock().unwrap();
        let row: Option<MessageRow> = conn
            .query_row(
                "SELECT id, account_id, folder, sender_name, sender_address, subject, snippet, \
                 received_at_ms, unread, flagged, has_attachments FROM messages WHERE id = ?1",
                params![id],
                Self::read_row,
            )
            .optional()
            .ok()
            .flatten();

        let row = row?;
        let mut body_plain = String::new();
        let mut body_html = None;
        if let Ok(mut stmt) = conn.prepare("SELECT plain, html FROM bodies WHERE message_id = ?1") {
            if let Ok(Some((plain, html))) = stmt
                .query_row(params![id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
                })
                .optional()
            {
                body_plain = plain;
                body_html = html;
            }
        }

        let mut to = Vec::new();
        let mut cc = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT kind, name, address FROM recipients WHERE message_id = ?1 ORDER BY position",
        ) {
            if let Ok(iter) = stmt.query_map(params![id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            }) {
                for rec in iter.flatten() {
                    let recipient = Recipient {
                        name: rec.1,
                        address: rec.2,
                    };
                    if rec.0 == "cc" {
                        cc.push(recipient);
                    } else {
                        to.push(recipient);
                    }
                }
            }
        }

        let mut attachments = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT id, message_id, filename, size_bytes, on_disk FROM attachments WHERE message_id = ?1 ORDER BY id",
        ) {
            if let Ok(iter) = stmt.query_map(params![id], |r| {
                Ok(Attachment {
                    id: r.get(0)?,
                    message_id: r.get(1)?,
                    filename: r.get(2)?,
                    size_bytes: r.get::<_, i64>(3)? as u64,
                    on_disk: r.get::<_, i64>(4)? != 0,
                })
            }) {
                attachments = iter.map(|a| a.expect("attachment")).collect();
            }
        }

        let body: Vec<String> = body_plain
            .split(PARAGRAPH_SEP)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();

        Some(MessageDetail {
            row,
            to,
            cc,
            body,
            body_html,
            remote_image_count: 0, // set by the command after sanitizing
            attachments,
        })
    }

    pub fn set_read(&self, id: MessageId, unread: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET unread = ?1 WHERE id = ?2",
            params![unread as i64, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_flagged(&self, id: MessageId, flagged: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET flagged = ?1 WHERE id = ?2",
            params![flagged as i64, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn archive(&self, id: MessageId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET folder = 'Archive', unread = 0 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete(&self, id: MessageId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM messages WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn attachment(&self, id: AttachmentId) -> Option<Attachment> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, message_id, filename, size_bytes, on_disk FROM attachments WHERE id = ?1",
            params![id],
            |r| {
                Ok(Attachment {
                    id: r.get(0)?,
                    message_id: r.get(1)?,
                    filename: r.get(2)?,
                    size_bytes: r.get::<_, i64>(3)? as u64,
                    on_disk: r.get::<_, i64>(4)? != 0,
                })
            },
        )
        .optional()
        .ok()
        .flatten()
    }

    pub fn attachment_path(&self, id: AttachmentId) -> Option<PathBuf> {
        let att = self.attachment(id)?;
        Some(
            self.attachments_root
                .join(id.to_string())
                .join(&att.filename),
        )
    }

    pub fn send(&self, _outgoing: &OutgoingMessage) -> Result<(), String> {
        // SMTP transport lands in Epic 12.3; the contract is fixed here.
        Err("outgoing mail (SMTP) lands in Epic 12".to_string())
    }

    pub fn list_events(&self, start_ms: i64, end_ms: i64) -> Vec<CalendarEvent> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, account_id, title, start_ms, end_ms, all_day, location, notes \
                 FROM events WHERE end_ms >= ?1 AND start_ms <= ?2 ORDER BY start_ms",
            )
            .expect("events query");
        stmt.query_map(params![start_ms, end_ms], |r| {
            Ok(CalendarEvent {
                id: r.get(0)?,
                account_id: r.get(1)?,
                title: r.get(2)?,
                start_ms: r.get(3)?,
                end_ms: r.get(4)?,
                all_day: r.get::<_, i64>(5)? != 0,
                location: r.get(6)?,
                notes: r.get(7)?,
            })
        })
        .expect("event rows")
        .map(|r| r.expect("event"))
        .collect()
    }

    pub fn create_event(&self, mut event: CalendarEvent) -> CalendarEvent {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (account_id, title, start_ms, end_ms, all_day, location, notes) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.account_id,
                event.title,
                event.start_ms,
                event.end_ms,
                event.all_day as i64,
                event.location,
                event.notes
            ],
        )
        .expect("insert event");
        event.id = conn.last_insert_rowid() as EventId;
        event
    }

    pub fn update_event(&self, event: CalendarEvent) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE events SET account_id = ?1, title = ?2, start_ms = ?3, end_ms = ?4, \
             all_day = ?5, location = ?6, notes = ?7 WHERE id = ?8",
            params![
                event.account_id,
                event.title,
                event.start_ms,
                event.end_ms,
                event.all_day as i64,
                event.location,
                event.notes,
                event.id
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn delete_event(&self, id: EventId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM events WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn total_disk_bytes(&self) -> u64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(SUM(local_bytes), 0) FROM accounts",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|b| b as u64)
        .unwrap_or(0)
    }

    pub fn create_account(&self, info: &NewAccount, color: String) -> Account {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO accounts (address, protocol, sync_mode, color, server, port, tls, folder_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                info.address,
                info.protocol,
                info.sync_mode,
                color,
                info.server,
                info.port,
                info.tls as i64
            ],
        )
        .expect("insert account");
        let id = conn.last_insert_rowid() as AccountId;
        Account {
            id,
            address: info.address.clone(),
            protocol: info.protocol.clone(),
            sync_mode: info.sync_mode.clone(),
            color,
            local_bytes: 0,
            connected: false,
            server: info.server.clone(),
            port: info.port,
            tls: info.tls,
            folder_count: 0,
        }
    }

    pub fn remove_account(&self, id: AccountId) -> Result<String, String> {
        let conn = self.conn.lock().unwrap();
        let address: Option<String> = conn
            .query_row(
                "SELECT address FROM accounts WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(address) = address else {
            return Err("no such account".into());
        };
        conn.execute("DELETE FROM accounts WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(address)
    }

    // -- Sync writes (Epic 12.2) ------------------------------------------

    /// The last sync watermark for an account+folder: `(uidvalidity, uidnext)`.
    pub fn get_sync_state(&self, account_id: AccountId, folder: &str) -> (i64, i64) {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(uidvalidity, 0), COALESCE(uidnext, 0) FROM sync_state \
             WHERE account_id = ?1 AND folder = ?2",
            params![account_id, folder],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or((0, 0))
    }

    pub fn set_sync_state(
        &self,
        account_id: AccountId,
        folder: &str,
        uidvalidity: i64,
        uidnext: i64,
    ) -> Result<(), String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_state (account_id, folder, uidvalidity, uidnext, last_synced_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(account_id, folder) DO UPDATE SET uidvalidity = ?3, uidnext = ?4, \
             last_synced_at_ms = ?5",
            params![account_id, folder, uidvalidity, uidnext, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// The IMAP uids currently stored for an account+folder (for the
    /// incremental fetch comparison).
    pub fn message_uids(&self, account_id: AccountId, folder: &str) -> Vec<u32> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT uid FROM messages WHERE account_id = ?1 AND folder = ?2 AND uid IS NOT NULL",
            )
            .expect("uids query");
        stmt.query_map(params![account_id, folder], |r| r.get::<_, i64>(0))
            .expect("uid rows")
            .map(|r| r.expect("uid") as u32)
            .collect()
    }

    /// Insert or update a message fetched from the server, keyed by
    /// (account, folder, uid, uidvalidity). Returns the local message id.
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_fetched_message(
        &self,
        account_id: AccountId,
        folder: &str,
        uid: u32,
        uidvalidity: i64,
        sender_name: &str,
        sender_address: &str,
        subject: &str,
        snippet: &str,
        received_at_ms: i64,
        unread: bool,
        flagged: bool,
        has_attachments: bool,
    ) -> Result<MessageId, String> {
        let conn = self.conn.lock().unwrap();
        let existing: Option<MessageId> = conn
            .query_row(
                "SELECT id FROM messages WHERE account_id = ?1 AND folder = ?2 \
                 AND uid = ?3 AND uidvalidity = ?4",
                params![account_id, folder, uid, uidvalidity],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(id) = existing {
            conn.execute(
                "UPDATE messages SET sender_name = ?1, sender_address = ?2, subject = ?3, \
                 snippet = ?4, received_at_ms = ?5, unread = ?6, flagged = ?7, \
                 has_attachments = ?8 WHERE id = ?9",
                params![
                    sender_name,
                    sender_address,
                    subject,
                    snippet,
                    received_at_ms,
                    unread as i64,
                    flagged as i64,
                    has_attachments as i64,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(id)
        } else {
            conn.execute(
                "INSERT INTO messages (account_id, folder, sender_name, sender_address, subject, \
                 snippet, received_at_ms, unread, flagged, uid, uidvalidity, has_attachments) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    account_id,
                    folder,
                    sender_name,
                    sender_address,
                    subject,
                    snippet,
                    received_at_ms,
                    unread as i64,
                    flagged as i64,
                    uid,
                    uidvalidity,
                    has_attachments as i64
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(conn.last_insert_rowid() as MessageId)
        }
    }

    /// Remove local messages for an account+folder whose uid is not in
    /// `keep_uids` (used on a full refetch after the uidvalidity changed).
    pub fn delete_messages_not_in(
        &self,
        account_id: AccountId,
        folder: &str,
        keep_uids: &[u32],
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        if keep_uids.is_empty() {
            conn.execute(
                "DELETE FROM messages WHERE account_id = ?1 AND folder = ?2",
                params![account_id, folder],
            )
            .map_err(|e| e.to_string())?;
        } else {
            let placeholders = keep_uids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
            let sql = format!(
                "DELETE FROM messages WHERE account_id = ?1 AND folder = ?2 AND uid IS NOT NULL \
                 AND uid NOT IN ({placeholders})"
            );
            let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
            let mut params: Vec<rusqlite::types::Value> = vec![
                Value::Integer(i64::from(account_id)),
                Value::Text(folder.to_string()),
            ];
            for uid in keep_uids {
                params.push((*uid as i64).into());
            }
            stmt.execute(rusqlite::params_from_iter(params.iter()))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // -- Drafts (Epic 13.2) ------------------------------------------------

    /// Insert or update a draft as a message in the Drafts folder (sender =
    /// the account address). Returns the draft's message id so re-saves can
    /// update it in place.
    pub fn save_draft(&self, draft: &Draft) -> Result<MessageId, String> {
        let conn = self.conn.lock().unwrap();
        let sender: Option<String> = conn
            .query_row(
                "SELECT address FROM accounts WHERE id = ?1",
                params![draft.account_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(sender) = sender else {
            return Err("no such account".into());
        };
        let snippet: String = draft.body.chars().take(120).collect();
        let received_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let id = if let Some(existing) = draft.id {
            conn.execute(
                "UPDATE messages SET sender_name = ?1, sender_address = ?2, subject = ?3, \
                 snippet = ?4, received_at_ms = ?5 WHERE id = ?6",
                params![
                    sender,
                    sender,
                    draft.subject,
                    snippet,
                    received_at_ms,
                    existing
                ],
            )
            .map_err(|e| e.to_string())?;
            existing
        } else {
            conn.execute(
                "INSERT INTO messages (account_id, folder, sender_name, sender_address, subject, \
                 snippet, received_at_ms, unread, flagged, has_attachments) \
                 VALUES (?1, 'Drafts', ?2, ?3, ?4, ?5, ?6, 0, 0, 0)",
                params![
                    draft.account_id,
                    sender,
                    sender,
                    draft.subject,
                    snippet,
                    received_at_ms
                ],
            )
            .map_err(|e| e.to_string())?;
            conn.last_insert_rowid() as MessageId
        };

        // Replace the body + recipients wholesale.
        conn.execute("DELETE FROM bodies WHERE message_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute("DELETE FROM recipients WHERE message_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO bodies (message_id, plain) VALUES (?1, ?2)",
            params![id, draft.body],
        )
        .map_err(|e| e.to_string())?;
        for (position, to) in draft.to.iter().enumerate() {
            conn.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, 'to', ?2, ?2, ?3)",
                params![id, to, position as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        for (position, cc) in draft.cc.iter().enumerate() {
            conn.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, 'cc', ?2, ?2, ?3)",
                params![id, cc, position as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(id)
    }

    /// Seed the demo content (Epic 3.4) into SQLite.
    pub fn seed_demo(&self, attachments_root: &Path) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        for account in demo_accounts() {
            tx.execute(
                "INSERT INTO accounts (id, address, protocol, sync_mode, color, local_bytes, \
                 connected, server, port, tls, folder_count) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    account.id,
                    account.address,
                    account.protocol,
                    account.sync_mode,
                    account.color,
                    account.local_bytes as i64,
                    account.connected as i64,
                    account.server,
                    account.port,
                    account.tls as i64,
                    account.folder_count
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        for message in demo_messages() {
            tx.execute(
                "INSERT INTO messages (id, account_id, folder, sender_name, sender_address, \
                 subject, snippet, received_at_ms, unread, flagged, has_attachments) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    message.id,
                    message.account_id,
                    message.folder,
                    message.sender_name,
                    message.sender_address,
                    message.subject,
                    message.snippet,
                    message.received_at_ms,
                    message.unread as i64,
                    message.flagged as i64,
                    !message.attachments.is_empty() as i64
                ],
            )
            .map_err(|e| e.to_string())?;

            tx.execute(
                "INSERT INTO bodies (message_id, plain, html) VALUES (?1, ?2, ?3)",
                params![
                    message.id,
                    message.body.join(PARAGRAPH_SEP),
                    message.body_html,
                ],
            )
            .map_err(|e| e.to_string())?;

            for (position, recipient) in message.to.iter().enumerate() {
                tx.execute(
                    "INSERT INTO recipients (message_id, kind, name, address, position) \
                     VALUES (?1, 'to', ?2, ?3, ?4)",
                    params![
                        message.id,
                        recipient.name,
                        recipient.address,
                        position as i64
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
            for (position, recipient) in message.cc.iter().enumerate() {
                tx.execute(
                    "INSERT INTO recipients (message_id, kind, name, address, position) \
                     VALUES (?1, 'cc', ?2, ?3, ?4)",
                    params![
                        message.id,
                        recipient.name,
                        recipient.address,
                        position as i64
                    ],
                )
                .map_err(|e| e.to_string())?;
            }

            for attachment in &message.attachments {
                tx.execute(
                    "INSERT INTO attachments (id, message_id, filename, size_bytes, on_disk) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        attachment.id,
                        attachment.message_id,
                        attachment.filename,
                        attachment.size_bytes as i64,
                        attachment.on_disk as i64
                    ],
                )
                .map_err(|e| e.to_string())?;
            }
        }

        for event in demo_events() {
            tx.execute(
                "INSERT INTO events (id, account_id, title, start_ms, end_ms, all_day, \
                 location, notes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event.id,
                    event.account_id,
                    event.title,
                    event.start_ms,
                    event.end_ms,
                    event.all_day as i64,
                    event.location,
                    event.notes
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;

        // Write the demo attachment so "cached locally" is backed by a real file.
        let lease_dir = attachments_root.join("1");
        std::fs::create_dir_all(&lease_dir).map_err(|e| e.to_string())?;
        std::fs::write(
            lease_dir.join("meridian-lease-v4.pdf"),
            crate::pdf::placeholder(253_952),
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded() -> SqliteStore {
        let store = SqliteStore::open_in_memory().unwrap();
        let root = std::env::temp_dir().join(format!("quill-sqlite-{}", std::process::id()));
        store.seed_demo(&root).unwrap();
        store
    }

    #[test]
    fn migrations_run_forward_only() {
        let store = SqliteStore::open_in_memory().unwrap();
        let version: i64 = store
            .conn
            .lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        // Re-running is a no-op (the version is already current).
        store.migrate().unwrap();
    }

    #[test]
    fn seed_matches_mock_content() {
        let store = seeded();
        assert_eq!(store.accounts().len(), 3);

        let folders = store.folders();
        let by = |n: &str| folders.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by("Inbox").total_count, 12);
        assert_eq!(by("Starred").total_count, 4);
        assert_eq!(by("Drafts").total_count, 2);

        let page = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit: 12,
        });
        assert_eq!(page.total, 12);
        assert_eq!(
            page.items[0].subject,
            "Draft agreement for the Meridian lease"
        );
        assert!(page.items[0].unread);

        let detail = store.get_message(page.items[0].id).unwrap();
        assert_eq!(detail.body.len(), 4);
        assert!(detail.body[1].contains("redlined lease"));
        assert_eq!(detail.to.len(), 2);
        assert!(detail.body_html.is_some());
        assert_eq!(detail.attachments.len(), 1);
    }

    #[test]
    fn actions_apply() {
        let store = seeded();
        let id = store
            .page_messages(&MessageQuery {
                folder: Some("Inbox".into()),
                account_id: None,
                offset: 0,
                limit: 1,
            })
            .items[0]
            .id;

        store.set_read(id, false).unwrap();
        assert!(!store.get_message(id).unwrap().row.unread);
        store.archive(id).unwrap();
        assert_eq!(store.get_message(id).unwrap().row.folder, "Archive");
        store.delete(id).unwrap();
        assert!(store.get_message(id).is_none());
    }

    #[test]
    fn account_add_remove_and_footprint() {
        let store = seeded();
        let before = store.total_disk_bytes();
        let info = NewAccount {
            address: "n@example.com".into(),
            protocol: "IMAP".into(),
            server: "imap.example.com".into(),
            port: 993,
            tls: true,
            sync_mode: "on open".into(),
        };
        let account = store.create_account(&info, "#3b5bdb".into());
        assert_eq!(store.accounts().len(), 4);
        assert_eq!(store.total_disk_bytes(), before); // new account is 0 bytes

        let address = store.remove_account(account.id).unwrap();
        assert_eq!(address, "n@example.com");
        assert_eq!(store.accounts().len(), 3);
        assert_eq!(store.total_disk_bytes(), before); // footprint restored
    }

    #[test]
    fn events_crud() {
        let store = seeded();
        let event = store.create_event(CalendarEvent {
            id: 0,
            account_id: 1,
            title: "Meet".into(),
            start_ms: 1000,
            end_ms: 2000,
            all_day: false,
            location: None,
            notes: None,
        });
        assert!(event.id > 0);
        assert_eq!(store.list_events(0, 3000).len(), 1);

        store
            .update_event(CalendarEvent {
                id: event.id,
                account_id: 1,
                title: "Meet updated".into(),
                start_ms: 1000,
                end_ms: 2000,
                all_day: false,
                location: None,
                notes: None,
            })
            .unwrap();
        assert_eq!(store.list_events(0, 3000)[0].title, "Meet updated");

        store.delete_event(event.id).unwrap();
        assert!(store.list_events(0, 3000).is_empty());
    }

    #[test]
    fn sync_state_and_fetched_messages() {
        let store = seeded();

        // Incremental watermark round-trips.
        store.set_sync_state(1, "Inbox", 12345, 101).unwrap();
        assert_eq!(store.get_sync_state(1, "Inbox"), (12345, 101));

        // Fetched envelopes upsert keyed by (account, folder, uid, uidvalidity).
        store
            .upsert_fetched_message(
                1,
                "Inbox",
                100,
                12345,
                "New Sender",
                "n@example.com",
                "New subject",
                "snippet",
                1_700_000_000_000,
                true,
                false,
                false,
            )
            .unwrap();
        store
            .upsert_fetched_message(
                1,
                "Inbox",
                101,
                12345,
                "Two",
                "two@example.com",
                "Subject 2",
                "s",
                1_700_000_000_001,
                false,
                true,
                false,
            )
            .unwrap();
        assert_eq!(store.message_uids(1, "Inbox").len(), 2);

        // A full refetch (uidvalidity changed) keeps only the server's uids.
        store.delete_messages_not_in(1, "Inbox", &[101]).unwrap();
        assert_eq!(store.message_uids(1, "Inbox"), vec![101]);

        let page = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: Some(1),
            offset: 0,
            limit: 50,
        });
        assert!(page.items.iter().any(|r| r.subject == "Subject 2"));
    }

    #[test]
    fn adding_an_account_never_carries_credentials() {
        let store = seeded();
        let info = NewAccount {
            address: "n@example.com".into(),
            protocol: "IMAP".into(),
            server: "imap.example.com".into(),
            port: 993,
            tls: true,
            sync_mode: "manual".into(),
        };
        store.create_account(&info, "#0f766e".into());
        let json = serde_json::to_string(&store.accounts()).unwrap();
        for word in [
            "password",
            "passwd",
            "secret",
            "credential",
            "api_key",
            "access_token",
        ] {
            assert!(!json.contains(word), "account JSON leaks {word}");
        }
    }
}

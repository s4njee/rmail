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
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PARAGRAPH_SEP: &str = "\n\n";

// -- P1.3 search-operator parsing ---------------------------------------
//
// `search` splits the raw query into full-text terms (FTS MATCH) and
// `op:value` operators that become SQL WHERE clauses. Operator tokens are
// stripped from the full-text match; everything else is matched against the
// index. Values may be quoted (`from:"John Smith"`).

/// Split a query into tokens, honoring `"quoted values"` (so a value with
/// spaces stays one token).
fn search_tokens(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in raw.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quote {
            let t = current.trim();
            if !t.is_empty() {
                out.push(t.to_string());
            }
            current.clear();
        } else {
            current.push(ch);
        }
    }
    let t = current.trim();
    if !t.is_empty() {
        out.push(t.to_string());
    }
    out
}

/// The recognized search operators (P1.3).
const SEARCH_OPERATORS: &[&str] = &[
    "from", "to", "cc", "subject", "has", "is", "before", "after", "in",
    "account", "calendar",
];

/// Split `op:value` — returns `(op, unquoted_value)` when the op is known.
fn split_operator(token: &str) -> Option<(String, String)> {
    let idx = token.find(':')?;
    let op = token[..idx].to_lowercase();
    if !SEARCH_OPERATORS.contains(&op.as_str()) {
        return None;
    }
    let mut value = token[idx + 1..].to_string();
    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        value = value[1..value.len() - 1].to_string();
    }
    if value.is_empty() {
        return None;
    }
    Some((op, value))
}

/// Parse a raw search query into full-text terms and operator pairs.
fn parse_search_query(raw: &str) -> (Vec<String>, Vec<(String, String)>) {
    let mut terms = Vec::new();
    let mut ops = Vec::new();
    for token in search_tokens(raw) {
        match split_operator(&token) {
            Some(op) => ops.push(op),
            None => terms.push(token),
        }
    }
    (terms, ops)
}

/// Tokenize full-text terms into an FTS5 prefix-match query
/// (`"meeting"* "design"*`). Sanitizes each term to matchable characters.
fn fts_match_query(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| {
            let clean: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '@' || *c == '.' || *c == '_')
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{clean}\"*")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a search date — ISO `YYYY-MM-DD` or today/yesterday/tomorrow — into
/// epoch ms at (UTC) midnight of that day. `None` for anything else.
fn parse_search_date(s: &str) -> Option<i64> {
    let day_ms = 86_400_000;
    let today_start = (now_ms() / day_ms) * day_ms;
    match s.to_lowercase().as_str() {
        "today" => return Some(today_start),
        "yesterday" => return Some(today_start - day_ms),
        "tomorrow" => return Some(today_start + day_ms),
        _ => {}
    }
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1970 {
        return None;
    }
    // Days since epoch for a Gregorian date (civil-from-days inverse).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * day_ms)
}

/// A human-readable description of a rule action, for the dry-run preview
/// (P1.3).
fn describe_rule_action(action: &RuleAction) -> String {
    match action {
        RuleAction::MoveToFolder { folder_name } => format!("Move to {folder_name}"),
        RuleAction::MarkRead => "Mark read".into(),
        RuleAction::MarkUnread => "Mark unread".into(),
        RuleAction::MarkFlagged => "Star".into(),
        RuleAction::MarkUnflagged => "Unstar".into(),
        RuleAction::Delete => "Delete".into(),
        RuleAction::Archive => "Archive".into(),
    }
}

/// The subject + recipient list from a serialized OutgoingMessage for the
/// Scheduled view — display fields only; the body stays in the payload.
fn scheduled_display(payload: &str) -> (String, Vec<String>) {
    let v: serde_json::Value =
        serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
    let subject = v
        .get("subject")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();
    let to = v
        .get("to")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    (subject, to)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Forward-only migrations, indexed by target `user_version`.
const MIGRATIONS: [&str; 23] = [
    r#"
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
"#,
    r#"
ALTER TABLE accounts ADD COLUMN last_error TEXT;
ALTER TABLE sync_state ADD COLUMN highestmodseq INTEGER NOT NULL DEFAULT 0;

CREATE TABLE action_queue (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  action_type TEXT NOT NULL,
  folder TEXT NOT NULL,
  uid INTEGER,
  payload TEXT,
  created_at_ms INTEGER NOT NULL,
  retries INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_action_queue_account ON action_queue(account_id, id);
"#,
    r#"
ALTER TABLE messages ADD COLUMN message_id_header TEXT;
ALTER TABLE messages ADD COLUMN in_reply_to TEXT;
ALTER TABLE messages ADD COLUMN references_header TEXT;
"#,
    r#"
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
  message_id UNINDEXED,
  subject,
  sender,
  recipients,
  body,
  tokenize='porter unicode61'
);

CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(
  event_id UNINDEXED,
  title,
  location,
  notes,
  tokenize='porter unicode61'
);

CREATE TRIGGER IF NOT EXISTS trg_messages_ai AFTER INSERT ON messages BEGIN
  INSERT INTO messages_fts(message_id, subject, sender, recipients, body)
  VALUES (new.id, new.subject, new.sender_name || ' ' || new.sender_address, '', new.snippet);
END;

CREATE TRIGGER IF NOT EXISTS trg_messages_ad AFTER DELETE ON messages BEGIN
  DELETE FROM messages_fts WHERE message_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_messages_au AFTER UPDATE ON messages BEGIN
  DELETE FROM messages_fts WHERE message_id = old.id;
  INSERT INTO messages_fts(message_id, subject, sender, recipients, body)
  SELECT new.id, new.subject, new.sender_name || ' ' || new.sender_address,
         COALESCE((SELECT GROUP_CONCAT(name || ' ' || address, ' ') FROM recipients WHERE message_id = new.id), ''),
         COALESCE((SELECT plain FROM bodies WHERE message_id = new.id), new.snippet);
END;

CREATE TRIGGER IF NOT EXISTS trg_bodies_ai AFTER INSERT ON bodies BEGIN
  DELETE FROM messages_fts WHERE message_id = new.message_id;
  INSERT INTO messages_fts(message_id, subject, sender, recipients, body)
  SELECT m.id, m.subject, m.sender_name || ' ' || m.sender_address,
         COALESCE((SELECT GROUP_CONCAT(name || ' ' || address, ' ') FROM recipients WHERE message_id = m.id), ''),
         new.plain
  FROM messages m WHERE m.id = new.message_id;
END;

CREATE TRIGGER IF NOT EXISTS trg_events_ai AFTER INSERT ON events BEGIN
  INSERT INTO events_fts(event_id, title, location, notes)
  VALUES (new.id, new.title, COALESCE(new.location, ''), COALESCE(new.notes, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_events_ad AFTER DELETE ON events BEGIN
  DELETE FROM events_fts WHERE event_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_events_au AFTER UPDATE ON events BEGIN
  DELETE FROM events_fts WHERE event_id = old.id;
  INSERT INTO events_fts(event_id, title, location, notes)
  VALUES (new.id, new.title, COALESCE(new.location, ''), COALESCE(new.notes, ''));
END;
"#,
    r#"
ALTER TABLE messages ADD COLUMN thread_id TEXT;
CREATE INDEX IF NOT EXISTS idx_messages_thread_id ON messages(account_id, thread_id);
UPDATE messages SET thread_id = 'th_subj_' || LOWER(TRIM(subject)) WHERE thread_id IS NULL;
"#,
    r#"
ALTER TABLE events ADD COLUMN alarm_minutes_before INTEGER;
"#,
    r#"
CREATE TABLE calendar_subscriptions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  color TEXT NOT NULL,
  refresh_interval_min INTEGER NOT NULL DEFAULT 60,
  last_refreshed_at_ms INTEGER,
  enabled INTEGER NOT NULL DEFAULT 1
);
"#,
    r#"
CREATE INDEX IF NOT EXISTS idx_messages_account_received ON messages(account_id, received_at_ms DESC);
"#,
    r#"
ALTER TABLE messages ADD COLUMN server_folder TEXT;
"#,
    r#"
ALTER TABLE messages ADD COLUMN answered INTEGER NOT NULL DEFAULT 0;
ALTER TABLE messages ADD COLUMN forwarded INTEGER NOT NULL DEFAULT 0;
"#,
    r#"
ALTER TABLE bodies ADD COLUMN list_unsubscribe TEXT;
ALTER TABLE bodies ADD COLUMN list_unsubscribe_post TEXT;
"#,
    r#"
ALTER TABLE events ADD COLUMN timezone TEXT;
"#,
    r#"
ALTER TABLE events ADD COLUMN travel_time_minutes INTEGER;
CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  due_at_ms INTEGER,
  completed_at_ms INTEGER,
  priority INTEGER
);
"#,
    r#"
ALTER TABLE events ADD COLUMN calendar_source TEXT;
ALTER TABLE events ADD COLUMN calendar_name TEXT;
ALTER TABLE events ADD COLUMN calendar_color TEXT;
"#,
    r#"
CREATE TABLE IF NOT EXISTS removed_calendar_sources (
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  source TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  color TEXT NOT NULL DEFAULT '',
  PRIMARY KEY (account_id, source)
);
"#,
    // P0.2 folder selection: which server mailboxes an account syncs. An empty
    // set (no rows) means "sync everything discovered" — the pre-P0.2
    // behavior — so existing accounts are unaffected.
    r#"
CREATE TABLE IF NOT EXISTS synced_folders (
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  server_name TEXT NOT NULL,
  local_name TEXT NOT NULL,
  kind TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  PRIMARY KEY (account_id, server_name)
);
"#,
    // P1.1: soft delete so Delete can be undone (restore + cancel the queued
    // server action). NULL = not deleted; a timestamp hides the row from every
    // view and marks it for the server Delete action.
    r#"
ALTER TABLE messages ADD COLUMN deleted_at_ms INTEGER;
"#,
    // P1.1: snooze. A snoozed message keeps its real folder locally but is
    // hidden from views until `snoozed_until_ms` passes; the scheduler then
    // clears it so it returns to its folder. NULL = not snoozed.
    r#"
ALTER TABLE messages ADD COLUMN snoozed_until_ms INTEGER;
"#,
    // P1.1: durable send-later. The scheduler flushes due rows through the
    // SMTP path; the draft snapshot lets the user re-open the composer to edit.
    r#"
CREATE TABLE IF NOT EXISTS scheduled_messages (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  send_at_ms INTEGER NOT NULL,
  payload TEXT NOT NULL,
  draft TEXT NOT NULL DEFAULT '',
  created_at_ms INTEGER NOT NULL
);
"#,
    // P1.2: recipient autocomplete. Suggestions are computed live from the
    // recipients + messages tables (dedup by address); this table suppresses a
    // bad suggestion the user dismissed, and the address index keeps the
    // per-keystroke query fast.
    r#"
CREATE TABLE IF NOT EXISTS hidden_recipients (
  address TEXT PRIMARY KEY
);
CREATE INDEX IF NOT EXISTS idx_recipients_address ON recipients(address);
"#,
    // P1.2: contact groups — named address lists (matched by lower(address)).
    r#"
CREATE TABLE IF NOT EXISTS contact_groups (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);
CREATE TABLE IF NOT EXISTS contact_group_members (
  group_id INTEGER NOT NULL REFERENCES contact_groups(id) ON DELETE CASCADE,
  address TEXT NOT NULL,
  PRIMARY KEY (group_id, address)
);
"#,
    // P1.3: saved searches — persistent virtual folders.
    r#"
CREATE TABLE IF NOT EXISTS saved_searches (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL UNIQUE,
  query TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL
);
"#,
    // P1.4: per-event color override (falls back to the calendar color).
    r#"
ALTER TABLE events ADD COLUMN color TEXT;
"#,
];

pub struct SqliteStore {
    conn: Mutex<Connection>,
    attachments_root: PathBuf,
    /// The on-disk database path, if any (None for in-memory). Used to take a
    /// pre-migration backup (E2.2 — downgrade safety).
    db_path: Option<PathBuf>,
}

/// A message that has no stored body yet — a candidate for the startup
/// backfill, which re-fetches and parses it so it gets a real snippet.
pub struct PendingBody {
    pub message_id: MessageId,
    pub folder: String,
    pub server_folder: String,
    pub uid: u32,
    pub uidvalidity: u32,
}

impl SqliteStore {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        Self::init(conn, Some(path.to_path_buf()))
    }

    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
        Self::init(conn, None)
    }

    fn init(conn: Connection, db_path: Option<PathBuf>) -> Result<Self, String> {
        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| e.to_string())?;
        let store = Self {
            conn: Mutex::new(conn),
            attachments_root: PathBuf::from("attachments"),
            db_path,
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn set_attachments_root(&mut self, root: PathBuf) {
        self.attachments_root = root;
    }

    /// Forward-only migrations, run on launch.
    ///
    /// `user_version` tracks the SQL migrations (1..N) only. Older builds used
    /// to stamp the thread_id code migration into the same space (`N+1`), so
    /// databases migrated by those builds sit at a version past a later SQL
    /// migration and skip it — the events source-calendar columns (migration
    /// 14) were missed this way. The repair below reconciles the stamp and the
    /// code migration is now tracked in a `meta` table so it can never consume
    /// a SQL slot again.
    /// Take a pre-migration backup of the on-disk database (E2.2 — downgrade
    /// safety / recovery): a copy written next to the live DB so a failed
    /// migration or a bad upgrade can be rolled back. No-op for in-memory.
    fn backup_db(&self, conn: &Connection) -> Result<(), String> {
        let Some(path) = &self.db_path else {
            return Ok(());
        };
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = path.with_extension(format!("sqlite.bak-{stamp}"));
        if backup.exists() {
            let _ = std::fs::remove_file(&backup);
        }
        // `VACUUM INTO` writes a fresh DB file from the current committed state;
        // the target must not exist, hence the remove above.
        let target = backup.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{target}'"))
            .map_err(|e| e.to_string())
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let mut version: i64 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| e.to_string())?;

        // Downgrade safety: a database stamped by a NEWER app must not be
        // opened by an older one — writing could corrupt the newer schema.
        // Refuse with a clear error instead of silently re-stamping the version.
        if version > MIGRATIONS.len() as i64 {
            return Err(format!(
                "database schema v{version} is newer than this app supports (v{}); update Quill",
                MIGRATIONS.len()
            ));
        }

        // Backup before the first migration write, so an interrupted or failed
        // migration (or a bad update) can be rolled back from the copy.
        if version < MIGRATIONS.len() as i64 {
            self.backup_db(&conn)?;
        }

        // SQL migrations (forward-only, version-gated).
        for (i, sql) in MIGRATIONS.iter().enumerate() {
            let target = i as i64 + 1;
            if target > version {
                conn.execute_batch(sql).map_err(|e| e.to_string())?;
                version = target;
                conn.pragma_update(None, "user_version", target)
                    .map_err(|e| e.to_string())?;
            }
        }

        // Defensive schema repair (idempotent): a migration can be skipped when
        // the stamp says it already ran (see the version-collision note above).
        // Ensure the events source-calendar columns actually exist.
        let event_cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('events')")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut repaired = false;
        for col in ["calendar_source", "calendar_name", "calendar_color", "color"] {
            if !event_cols.iter().any(|c| c == col) {
                conn.execute_batch(&format!("ALTER TABLE events ADD COLUMN {col} TEXT;"))
                    .map_err(|e| e.to_string())?;
                repaired = true;
            }
        }
        // Migrations 17 & 18 (P1.1): the messages soft-delete + snooze columns.
        // A database stamped past them (the old code-migration collision) would
        // otherwise be missing them — same defensive repair as the events cols.
        let msg_cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('messages')")
            .map_err(|e| e.to_string())?
            .query_map([], |r| r.get(0))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for col in ["deleted_at_ms", "snoozed_until_ms"] {
            if !msg_cols.iter().any(|c| c == col) {
                conn.execute_batch(&format!("ALTER TABLE messages ADD COLUMN {col} INTEGER;"))
                    .map_err(|e| e.to_string())?;
                repaired = true;
            }
        }
        // Same for migration 15's table — a database stamped past it (the old
        // code-migration collision) would otherwise be missing it.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS removed_calendar_sources (
              account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
              source TEXT NOT NULL,
              name TEXT NOT NULL DEFAULT '',
              color TEXT NOT NULL DEFAULT '',
              PRIMARY KEY (account_id, source)
            );",
        )
        .map_err(|e| e.to_string())?;
        // And migration 16's — a database stamped past it misses synced_folders.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS synced_folders (
              account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
              server_name TEXT NOT NULL,
              local_name TEXT NOT NULL,
              kind TEXT NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1,
              PRIMARY KEY (account_id, server_name)
            );",
        )
        .map_err(|e| e.to_string())?;
        // And migration 19's — a database stamped past it misses the durable
        // send-later table.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scheduled_messages (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
              send_at_ms INTEGER NOT NULL,
              payload TEXT NOT NULL,
              draft TEXT NOT NULL DEFAULT '',
              created_at_ms INTEGER NOT NULL
            );",
        )
        .map_err(|e| e.to_string())?;
        // Migrations 20–21 (P1.2) — recipient autocomplete + contact groups.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS hidden_recipients (address TEXT PRIMARY KEY);
             CREATE INDEX IF NOT EXISTS idx_recipients_address ON recipients(address);
             CREATE TABLE IF NOT EXISTS contact_groups (
               id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
             CREATE TABLE IF NOT EXISTS contact_group_members (
               group_id INTEGER NOT NULL REFERENCES contact_groups(id) ON DELETE CASCADE,
               address TEXT NOT NULL, PRIMARY KEY (group_id, address));
             CREATE TABLE IF NOT EXISTS saved_searches (
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               name TEXT NOT NULL UNIQUE,
               query TEXT NOT NULL,
               created_at_ms INTEGER NOT NULL);",
        )
        .map_err(|e| e.to_string())?;

        // Reconcile the stamp: SQL migrations 1..N are all applied (the repair
        // above covers any that were skipped), so the version should be N.
        let sql_version = MIGRATIONS.len() as i64;
        if version != sql_version || repaired {
            conn.pragma_update(None, "user_version", sql_version)
                .map_err(|e| e.to_string())?;
        }

        // Code migration (thread_id recompute), tracked in a `meta` table so it
        // no longer consumes a user_version slot and collides with SQL
        // migrations. Idempotent: recomputes every subject-derived id with the
        // same `compute_thread_id` new writes use; reference ids are untouched.
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .map_err(|e| e.to_string())?;
        let code_done: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key = 'code_migration_thread_id'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        if !code_done {
            let rows: Vec<(i64, String)> = conn
                .prepare("SELECT id, subject FROM messages WHERE thread_id LIKE 'th_subj_%'")
                .map_err(|e| e.to_string())?
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let mut update = conn
                .prepare("UPDATE messages SET thread_id = ?1 WHERE id = ?2")
                .map_err(|e| e.to_string())?;
            for (id, subject) in rows {
                let tid = crate::threading::compute_thread_id(None, None, &subject);
                update.execute(params![tid, id]).map_err(|e| e.to_string())?;
            }
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES ('code_migration_thread_id', '1')",
                [],
            )
            .map_err(|e| e.to_string())?;
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
            last_error: row.get(11).ok(),
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
            answered: row.get::<_, i64>(10)? != 0,
            forwarded: row.get::<_, i64>(11)? != 0,
            has_attachments: row.get::<_, i64>(12)? != 0,
            thread_id: row.get(13).ok(),
            thread_count: row.get::<_, u32>(14).unwrap_or(1),
        })
    }

    pub fn accounts(&self) -> Vec<Account> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                // `local_bytes` is the account's on-disk cache: stored message
                // bodies plus attachment files. It isn't maintained on every
                // write, so compute it from the actual data.
                "SELECT a.id, a.address, a.protocol, a.sync_mode, a.color, \
                 COALESCE((SELECT SUM(LENGTH(CAST(b.plain AS BLOB)) + COALESCE(LENGTH(CAST(b.html AS BLOB)), 0)) \
                           FROM bodies b JOIN messages m ON m.id = b.message_id \
                           WHERE m.account_id = a.id), 0) \
                 + COALESCE((SELECT SUM(at.size_bytes) \
                             FROM attachments at JOIN messages m ON m.id = at.message_id \
                             WHERE m.account_id = a.id AND at.on_disk = 1), 0) AS local_bytes, \
                 a.connected, a.server, a.port, a.tls, a.folder_count, a.last_error \
                 FROM accounts a ORDER BY a.id",
            )
            .expect("accounts query");
        stmt.query_map([], Self::read_account)
            .expect("accounts rows")
            .map(|r| r.expect("account row"))
            .collect()
    }

    pub fn folders(&self) -> Vec<Folder> {
        let conn = self.conn.lock().unwrap();
        const KINDS: [(FolderKind, &str); 8] = [
            (FolderKind::Inbox, "Inbox"),
            (FolderKind::Starred, "Starred"),
            (FolderKind::Drafts, "Drafts"),
            (FolderKind::Sent, "Sent"),
            (FolderKind::Archive, "Archive"),
            (FolderKind::Junk, "Junk"),
            (FolderKind::Trash, "Trash"),
            (FolderKind::Snoozed, "Snoozed"),
        ];
        KINDS
            .into_iter()
            .enumerate()
            .map(|(i, (kind, name))| {
                let now = now_ms();
                let (sql, arg): (&str, Option<&str>) = match kind {
                    FolderKind::Starred => (
                        "SELECT COUNT(*), COALESCE(SUM(unread), 0) FROM messages \
                         WHERE flagged = 1 AND deleted_at_ms IS NULL \
                           AND (snoozed_until_ms IS NULL OR snoozed_until_ms <= ?1)",
                        None,
                    ),
                    // P1.1: the local-only Snoozed view.
                    FolderKind::Snoozed => (
                        "SELECT COUNT(*), COALESCE(SUM(unread), 0) FROM messages \
                         WHERE snoozed_until_ms > ?1 AND deleted_at_ms IS NULL",
                        None,
                    ),
                    _ => (
                        "SELECT COUNT(*), COALESCE(SUM(unread), 0) FROM messages \
                         WHERE folder = ?1 AND deleted_at_ms IS NULL \
                           AND (snoozed_until_ms IS NULL OR snoozed_until_ms <= ?2)",
                        Some(name),
                    ),
                };
                let (total, unread): (i64, i64) = match arg {
                    Some(a) => conn
                        .query_row(sql, params![a, now], |r| Ok((r.get(0)?, r.get(1)?)))
                        .expect("folder count"),
                    None => conn
                        .query_row(sql, params![now], |r| Ok((r.get(0)?, r.get(1)?)))
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

        let (mut where_sql, mut outer_where_sql, mut params): (String, String, Vec<Value>) =
            match folder {
            Some("Starred") => {
                if let Some(account) = query.account_id {
                    (
                        format!("flagged = 1 AND account_id = {account}"),
                        format!("m.flagged = 1 AND m.account_id = {account}"),
                        vec![],
                    )
                } else {
                    (
                        "flagged = 1".to_string(),
                        "m.flagged = 1".to_string(),
                        vec![],
                    )
                }
            }
            // P1.1 Snoozed: the inverse of the hidden filter — show rows whose
            // snooze has not yet elapsed. The `now` bound param is appended
            // below with the deleted filter, sharing the same `?1` slot.
            Some("Snoozed") => {
                if let Some(account) = query.account_id {
                    (
                        format!("snoozed_until_ms > ?1 AND account_id = {account}"),
                        format!("m.snoozed_until_ms > ?1 AND m.account_id = {account}"),
                        vec![],
                    )
                } else {
                    (
                        "snoozed_until_ms > ?1".to_string(),
                        "m.snoozed_until_ms > ?1".to_string(),
                        vec![],
                    )
                }
            }
            Some(f) => {
                let mut inner = String::from("folder = ?1");
                let mut outer = String::from("m.folder = ?1");
                let p: Vec<Value> = vec![Value::Text(f.to_string())];
                if let Some(account) = query.account_id {
                    inner.push_str(&format!(" AND account_id = {account}"));
                    outer.push_str(&format!(" AND m.account_id = {account}"));
                }
                (inner, outer, p)
            }
            None => {
                let mut inner = String::from("1 = 1");
                let mut outer = String::from("1 = 1");
                let p: Vec<Value> = vec![];
                if let Some(account) = query.account_id {
                    inner.push_str(&format!(" AND account_id = {account}"));
                    outer.push_str(&format!(" AND m.account_id = {account}"));
                }
                (inner, outer, p)
            }
        };

        // P1.1 hidden-message filter: soft-deleted rows are gone from every
        // view, and a snoozed row stays out of its folder until its time
        // passes. The Snoozed view is the inverse. `now` is a single bound
        // param appended after the folder/account params above (the threaded
        // query duplicates them, which the `all_params` reuse below already
        // accounts for).
        {
            let idx = params.len() + 1;
            if folder == Some("Snoozed") {
                where_sql.push_str(&format!(
                    " AND deleted_at_ms IS NULL AND snoozed_until_ms > ?{idx}"
                ));
                outer_where_sql.push_str(&format!(
                    " AND m.deleted_at_ms IS NULL AND m.snoozed_until_ms > ?{idx}"
                ));
            } else {
                where_sql.push_str(&format!(
                    " AND deleted_at_ms IS NULL AND (snoozed_until_ms IS NULL OR snoozed_until_ms <= ?{idx})"
                ));
                outer_where_sql.push_str(&format!(
                    " AND m.deleted_at_ms IS NULL AND (m.snoozed_until_ms IS NULL OR m.snoozed_until_ms <= ?{idx})"
                ));
            }
            params.push(Value::Integer(now_ms()));
        }

        if query.threaded {
            let count_sql = format!(
                "SELECT COUNT(*) FROM (SELECT 1 FROM messages WHERE {where_sql} GROUP BY account_id, thread_id)"
            );
            let total: i64 = conn
                .query_row(&count_sql, rusqlite::params_from_iter(params.iter()), |r| {
                    r.get(0)
                })
                .unwrap_or(0);

            // The displayed row is the thread's newest message, while the
            // unread/flagged/attachment badges aggregate over the whole thread.
            // A bare `GROUP BY account_id, thread_id` would return an arbitrary
            // (nondeterministic) member's sender/subject/snippet, so the row is
            // joined to the max-received message per group instead. `where_sql`
            // is applied both inside the group and on the joined row, so a
            // message outside the folder filter can't win the "newest" slot.
            // The where clause contributes 0 or 1 bound params; they're
            // duplicated for the inner and outer application, then the
            // LIMIT/OFFSET placeholders are numbered after them.
            let mut all_params = params.clone();
            all_params.extend(params.clone());
            let limit_idx = all_params.len() + 1;
            let offset_idx = limit_idx + 1;
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT m.id, m.account_id, m.folder, m.sender_name, m.sender_address, \
                     m.subject, m.snippet, m.received_at_ms, \
                     g.unread, g.flagged, g.answered, g.forwarded, g.has_attachments, \
                     m.thread_id, g.thread_count \
                     FROM messages m JOIN ( \
                       SELECT account_id, thread_id, MAX(received_at_ms) AS received_at_ms, \
                              MAX(unread) AS unread, MAX(flagged) AS flagged, \
                              MAX(answered) AS answered, MAX(forwarded) AS forwarded, \
                              MAX(has_attachments) AS has_attachments, COUNT(*) AS thread_count \
                       FROM messages \
                       WHERE {where_sql} \
                       GROUP BY account_id, thread_id \
                     ) g ON m.account_id = g.account_id AND m.thread_id = g.thread_id \
                        AND m.received_at_ms = g.received_at_ms \
                     WHERE {outer_where_sql} \
                     ORDER BY m.received_at_ms DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
                ))
                .expect("page query");
            all_params.push(Value::Integer(i64::from(query.limit)));
            all_params.push(Value::Integer(i64::from(query.offset)));
            let rows: Vec<MessageRow> = stmt
                .query_map(rusqlite::params_from_iter(all_params.iter()), Self::read_row)
                .expect("page rows")
                .map(|r| r.expect("page row"))
                .collect();

            MessagePage {
                items: rows,
                total: total as u32,
            }
        } else {
            let total: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM messages WHERE {where_sql}"),
                    rusqlite::params_from_iter(params.iter()),
                    |r| r.get(0),
                )
                .expect("count query");

            let limit_idx = params.len() + 1;
            let offset_idx = limit_idx + 1;
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT id, account_id, folder, sender_name, sender_address, subject, snippet, \
                     received_at_ms, unread, flagged, answered, forwarded, has_attachments, thread_id, 1 as thread_count FROM messages \
                     WHERE {where_sql} ORDER BY received_at_ms DESC LIMIT ?{limit_idx} OFFSET ?{offset_idx}"
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
    }

    pub fn get_message(&self, id: MessageId) -> Option<MessageDetail> {
        let conn = self.conn.lock().unwrap();
        let query_res: Option<(MessageRow, Option<String>, Option<String>, Option<String>)> = conn
            .query_row(
                "SELECT id, account_id, folder, sender_name, sender_address, subject, snippet, \
                 received_at_ms, unread, flagged, answered, forwarded, has_attachments, thread_id, 1 as thread_count, \
                 message_id_header, in_reply_to, references_header \
                 FROM messages WHERE id = ?1",
                params![id],
                |r| {
                    Ok((
                        Self::read_row(r)?,
                        r.get::<_, Option<String>>(15)?,
                        r.get::<_, Option<String>>(16)?,
                        r.get::<_, Option<String>>(17)?,
                    ))
                },
            )
            .optional()
            .ok()
            .flatten();

        let (row, message_id_header, in_reply_to, references) = query_res?;
        let mut body_plain = String::new();
        let mut body_html = None;
        let mut list_unsubscribe = None;
        let mut list_unsubscribe_post = None;
        if let Ok(mut stmt) = conn.prepare(
            "SELECT plain, html, list_unsubscribe, list_unsubscribe_post FROM bodies WHERE message_id = ?1",
        ) {
            if let Ok(Some((plain, html, unsub, unsub_post))) = stmt
                .query_row(params![id], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                    ))
                })
                .optional()
            {
                body_plain = plain;
                body_html = html;
                list_unsubscribe = unsub;
                list_unsubscribe_post = unsub_post;
            }
        }

        let mut to = Vec::new();
        let mut cc = Vec::new();
        let mut bcc = Vec::new();
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
                    } else if rec.0 == "bcc" {
                        bcc.push(recipient);
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

        let thread_id = row.thread_id.clone();

        Some(MessageDetail {
            row,
            to,
            cc,
            bcc,
            body,
            body_html,
            remote_image_count: 0, // set by the command after sanitizing
            attachments,
            message_id_header,
            in_reply_to,
            references,
            thread_id,
            calendar_invite: None,
            list_unsubscribe,
            list_unsubscribe_post,
        })
    }

    /// Returns all messages in a thread in chronological order (Roadmap 3.2).
    pub fn get_thread_messages(
        &self,
        account_id: AccountId,
        thread_id: &str,
    ) -> Vec<MessageDetail> {
        let conn = self.conn.lock().unwrap();
        let ids: Vec<MessageId> = if let Ok(mut stmt) = conn.prepare(
            "SELECT id FROM messages WHERE account_id = ?1 AND thread_id = ?2 ORDER BY received_at_ms ASC",
        ) {
            stmt.query_map(params![account_id, thread_id], |r| r.get(0))
                .map(|iter| iter.flatten().collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        drop(conn);

        ids.into_iter()
            .filter_map(|id| self.get_message(id))
            .collect()
    }

    /// Apply a bulk action across all messages in a conversation thread (Roadmap 3.2).
    pub fn apply_thread_action(
        &self,
        account_id: AccountId,
        thread_id: &str,
        action: ActionType,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        match action {
            ActionType::MarkRead => {
                conn.execute(
                    "UPDATE messages SET unread = 0 WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::MarkUnread => {
                conn.execute(
                    "UPDATE messages SET unread = 1 WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::Star => {
                conn.execute(
                    "UPDATE messages SET flagged = 1 WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::Unstar => {
                conn.execute(
                    "UPDATE messages SET flagged = 0 WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::Archive => {
                conn.execute(
                    "UPDATE messages SET folder = 'Archive' WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::Delete => {
                conn.execute(
                    "UPDATE messages SET folder = 'Trash' WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::MarkAnswered => {
                conn.execute(
                    "UPDATE messages SET answered = 1 WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::MarkForwarded => {
                conn.execute(
                    "UPDATE messages SET forwarded = 1 WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::MarkJunk => {
                conn.execute(
                    "UPDATE messages SET folder = 'Junk' WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::MarkNotJunk => {
                conn.execute(
                    "UPDATE messages SET folder = 'Inbox' WHERE account_id = ?1 AND thread_id = ?2",
                    params![account_id, thread_id],
                )
                .map_err(|e| e.to_string())?;
            }
            ActionType::Move => {}
            ActionType::Send => {}
        }
        Ok(())
    }

    pub fn set_read(&self, id: MessageId, unread: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET unread = ?1 WHERE id = ?2",
                params![unread as i64, id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }
        Ok(())
    }

    pub fn set_flagged(&self, id: MessageId, flagged: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET flagged = ?1 WHERE id = ?2",
                params![flagged as i64, id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }
        Ok(())
    }

    pub fn set_answered(&self, id: MessageId, answered: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET answered = ?1 WHERE id = ?2",
                params![answered as i64, id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }
        Ok(())
    }

    pub fn set_forwarded(&self, id: MessageId, forwarded: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET forwarded = ?1 WHERE id = ?2",
                params![forwarded as i64, id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }
        Ok(())
    }

    pub fn archive(&self, id: MessageId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET folder = 'Archive', unread = 0 WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }
        Ok(())
    }

    /// Soft-delete (P1.1): hide the row from every view but keep it so Delete
    /// can be undone and the queued server Delete can replay. Hard cleanup
    /// happens once the server delete lands (the sync reconcile removes rows
    /// whose UID is gone) or via retention pruning.
    pub fn delete(&self, id: MessageId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET deleted_at_ms = ?1 WHERE id = ?2 AND deleted_at_ms IS NULL",
                params![now_ms(), id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }
        Ok(())
    }

    /// Undo a soft delete: clear the tombstone so the message reappears. The
    /// queued server Delete must be cancelled separately via
    /// [`Self::cancel_pending_actions`] (or the server copy is lost on the
    /// next replay).
    pub fn restore_message(&self, id: MessageId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET deleted_at_ms = NULL WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Remove queued sync actions for a message's (account, folder, uid) —
    /// used by undo so a reverted local change isn't then applied to the
    /// server. Returns the number of actions cancelled.
    pub fn cancel_pending_actions(
        &self,
        account_id: AccountId,
        folder: &str,
        uid: Option<u32>,
    ) -> Result<u32, String> {
        if let Some(uid) = uid {
            let conn = self.conn.lock().unwrap();
            let deleted = conn
                .execute(
                    "DELETE FROM action_queue WHERE account_id = ?1 AND folder = ?2 AND uid = ?3",
                    params![account_id, folder, uid],
                )
                .map_err(|e| e.to_string())?;
            Ok(deleted as u32)
        } else {
            Ok(0)
        }
    }

    pub fn move_message(&self, id: MessageId, destination_folder: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let (account_id, current_folder, uid): (AccountId, String, Option<u32>) = conn
            .query_row(
                "SELECT account_id, folder, uid FROM messages WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;

        let affected = conn
            .execute(
                "UPDATE messages SET folder = ?1 WHERE id = ?2",
                params![destination_folder, id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }

        let now = now_ms();
        let _ = conn.execute(
            "INSERT INTO action_queue (account_id, action_type, folder, uid, payload, created_at_ms, retries) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![account_id, "move", current_folder, uid, destination_folder, now],
        );

        Ok(())
    }

    pub fn mark_junk(&self, id: MessageId, junk: bool) -> Result<(), String> {
        let destination_folder = if junk { "Junk" } else { "Inbox" };
        let action_name = if junk { "markJunk" } else { "markNotJunk" };

        let conn = self.conn.lock().unwrap();
        let (account_id, current_folder, uid): (AccountId, String, Option<u32>) = conn
            .query_row(
                "SELECT account_id, folder, uid FROM messages WHERE id = ?1",
                params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(|e| e.to_string())?;

        let affected = conn
            .execute(
                "UPDATE messages SET folder = ?1 WHERE id = ?2",
                params![destination_folder, id],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such message".into());
        }

        let now = now_ms();
        let _ = conn.execute(
            "INSERT INTO action_queue (account_id, action_type, folder, uid, payload, created_at_ms, retries) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![account_id, action_name, current_folder, uid, destination_folder, now],
        );

        Ok(())
    }

    // -- P1.1 bulk triage ------------------------------------------------
    //
    // Each bulk method mirrors its single-message counterpart: update the rows
    // AND enqueue the per-message server action (via the server mailbox, which
    // the replay engine selects), returning `(ok, errors)` so the UI can
    // report partial failures. Soft-deleted rows are skipped as "no such
    // message".

    /// (account_id, server_mailbox, uid) for a live (non-soft-deleted) row.
    fn message_location(&self, id: MessageId) -> Option<(AccountId, String, Option<u32>)> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT account_id, COALESCE(server_folder, folder), uid FROM messages \
             WHERE id = ?1 AND deleted_at_ms IS NULL",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok()
    }

    fn enqueue_action_str(
        &self,
        account_id: AccountId,
        action_type: &str,
        folder: &str,
        uid: Option<u32>,
        payload: Option<&str>,
    ) {
        let conn = self.conn.lock().unwrap();
        let _ = conn.execute(
            "INSERT INTO action_queue (account_id, action_type, folder, uid, payload, created_at_ms, retries) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![account_id, action_type, folder, uid, payload, now_ms()],
        );
    }

    fn apply_each<F>(ids: &[MessageId], mut apply: F) -> (u32, Vec<String>)
    where
        F: FnMut(MessageId) -> Result<(), String>,
    {
        let mut ok = 0;
        let mut errors = Vec::new();
        for &id in ids {
            match apply(id) {
                Ok(()) => ok += 1,
                Err(e) => errors.push(format!("message {id}: {e}")),
            }
        }
        (ok, errors)
    }

    pub fn bulk_set_read(&self, ids: &[MessageId], unread: bool) -> (u32, Vec<String>) {
        let action = if unread { "mark_unread" } else { "mark_read" };
        Self::apply_each(ids, |id| {
            let Some((account_id, server_folder, uid)) = self.message_location(id) else {
                return Err("no such message".into());
            };
            self.enqueue_action_str(account_id, action, &server_folder, uid, None);
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET unread = ?1 WHERE id = ?2",
                params![unread as i64, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn bulk_set_flagged(&self, ids: &[MessageId], flagged: bool) -> (u32, Vec<String>) {
        let action = if flagged { "star" } else { "unstar" };
        Self::apply_each(ids, |id| {
            let Some((account_id, server_folder, uid)) = self.message_location(id) else {
                return Err("no such message".into());
            };
            self.enqueue_action_str(account_id, action, &server_folder, uid, None);
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET flagged = ?1 WHERE id = ?2",
                params![flagged as i64, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn bulk_archive(&self, ids: &[MessageId]) -> (u32, Vec<String>) {
        Self::apply_each(ids, |id| {
            let Some((account_id, server_folder, uid)) = self.message_location(id) else {
                return Err("no such message".into());
            };
            self.enqueue_action_str(account_id, "archive", &server_folder, uid, None);
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET folder = 'Archive', unread = 0 WHERE id = ?1",
                params![id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn bulk_delete(&self, ids: &[MessageId]) -> (u32, Vec<String>) {
        Self::apply_each(ids, |id| {
            let Some((account_id, server_folder, uid)) = self.message_location(id) else {
                return Err("no such message".into());
            };
            self.enqueue_action_str(account_id, "delete", &server_folder, uid, None);
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET deleted_at_ms = ?1 WHERE id = ?2",
                params![now_ms(), id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn bulk_move(&self, ids: &[MessageId], destination: &str) -> (u32, Vec<String>) {
        Self::apply_each(ids, |id| {
            let Some((account_id, server_folder, uid)) = self.message_location(id) else {
                return Err("no such message".into());
            };
            self.enqueue_action_str(account_id, "move", &server_folder, uid, Some(destination));
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET folder = ?1 WHERE id = ?2",
                params![destination, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    pub fn bulk_mark_junk(&self, ids: &[MessageId], junk: bool) -> (u32, Vec<String>) {
        let (dest, action) = if junk {
            ("Junk", "mark_junk")
        } else {
            ("Inbox", "mark_not_junk")
        };
        Self::apply_each(ids, |id| {
            let Some((account_id, server_folder, uid)) = self.message_location(id) else {
                return Err("no such message".into());
            };
            self.enqueue_action_str(account_id, action, &server_folder, uid, None);
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "UPDATE messages SET folder = ?1 WHERE id = ?2",
                params![dest, id],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })
    }

    // -- P1.1 snooze ----------------------------------------------
    //
    // Snooze is local-only: the row keeps its real folder (so the server copy
    // is untouched), `snoozed_until_ms` hides it from views, and the scheduler
    // clears it when the time passes so the message "returns to inbox".

    /// Set a future wake time for messages, hiding them from their folders
    /// until then.
    pub fn set_snoozed(&self, ids: &[MessageId], until_ms: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "UPDATE messages SET snoozed_until_ms = ?1 WHERE id = ?2 AND deleted_at_ms IS NULL",
            )
            .map_err(|e| e.to_string())?;
        for &id in ids {
            stmt.execute(params![until_ms, id]).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Clear snoozed rows whose wake time has passed — the "return to inbox"
    /// step. Returns the number returned.
    pub fn clear_due_snoozes(&self, now: i64) -> Result<u32, String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE messages SET snoozed_until_ms = NULL \
                 WHERE snoozed_until_ms IS NOT NULL AND snoozed_until_ms <= ?1",
                params![now],
            )
            .map_err(|e| e.to_string())?;
        Ok(affected as u32)
    }

    // -- P1.1 durable send-later (Outbox) ---------------------------------

    /// Queue a message to send at `send_at_ms`. `payload` is the serialized
    /// OutgoingMessage (only the flusher reads it back); `draft` is the
    /// composer snapshot for Edit. Returns the row id.
    pub fn schedule_message(
        &self,
        account_id: AccountId,
        send_at_ms: i64,
        payload: &str,
        draft: &str,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO scheduled_messages (account_id, send_at_ms, payload, draft, created_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![account_id, send_at_ms, payload, draft, now_ms()],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    /// All scheduled messages, soonest first — the Scheduled view's rows.
    /// The payload body is deliberately not returned across IPC.
    pub fn list_scheduled(&self) -> Vec<ScheduledMessage> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, account_id, send_at_ms, payload, draft, created_at_ms \
             FROM scheduled_messages ORDER BY send_at_ms ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| {
                let (subject, to) = scheduled_display(&r.get::<_, String>(3)?);
                Ok(ScheduledMessage {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    send_at_ms: r.get(2)?,
                    subject,
                    to,
                    created_at_ms: r.get(5)?,
                    draft: r.get(4)?,
                })
            })
            .map_err(|_| ());
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// Due scheduled messages as raw (id, account_id, send_at_ms, payload)
    /// rows for the flusher — the payload stays inside this crate.
    pub fn due_scheduled(&self, now: i64) -> Vec<(i64, AccountId, i64, String)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, account_id, send_at_ms, payload FROM scheduled_messages \
             WHERE send_at_ms <= ?1 ORDER BY send_at_ms ASC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![now], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)));
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn cancel_scheduled(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM scheduled_messages WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // -- P1.2 recipient suggestions + contact groups ---------------------
    //
    // Suggestions are computed live from mail history (recipients to/cc/bcc +
    // message senders), deduplicated by lower-cased address — that GROUP BY
    // *is* the merge/dedup a CardDAV import would feed. `hidden_recipients`
    // suppresses a dismissed suggestion.

    /// Escape `%`, `_` and the LIKE escape char so a typed query can't
    /// wildcard unexpectedly.
    fn escape_like(s: &str) -> String {
        s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
    }

    /// The history rows a suggestion is built from: every recipient we've seen
    /// (to/cc/bcc) plus every sender. Soft-deleted messages are excluded.
    const HISTORY_UNION: &'static str = " \
        SELECT r.name AS name, r.address AS address, m.received_at_ms AS received_at_ms \
        FROM recipients r JOIN messages m ON m.id = r.message_id \
        WHERE m.deleted_at_ms IS NULL \
        UNION ALL \
        SELECT m.sender_name AS name, m.sender_address AS address, m.received_at_ms AS received_at_ms \
        FROM messages m WHERE m.deleted_at_ms IS NULL AND m.sender_address != ''";

    /// Recipient suggestions matching `query`, ranked by frequency then
    /// recency (P1.2). Empty query returns nothing — use [`Self::recent_recipients`].
    pub fn suggest_recipients(&self, query: &str, limit: u32) -> Vec<ContactSuggestion> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let pattern = format!("%{}%", Self::escape_like(query.trim()));
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT MIN(name) AS name, MIN(address) AS address, COUNT(*) AS n, \
                    MAX(received_at_ms) AS last \
             FROM ({}) \
             WHERE lower(address) NOT IN (SELECT lower(address) FROM hidden_recipients) \
               AND lower(address) NOT IN (SELECT lower(address) FROM accounts) \
               AND (lower(address) LIKE ?1 ESCAPE '\\' OR name LIKE ?1 ESCAPE '\\') \
             GROUP BY lower(address) ORDER BY n DESC, last DESC LIMIT ?2",
            Self::HISTORY_UNION
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![pattern, limit], Self::read_suggestion);
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The most recently used recipients (P1.2), for the composer's empty-field
    /// dropdown.
    pub fn recent_recipients(&self, limit: u32) -> Vec<ContactSuggestion> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT MIN(name) AS name, MIN(address) AS address, COUNT(*) AS n, \
                    MAX(received_at_ms) AS last \
             FROM ({}) \
             WHERE lower(address) NOT IN (SELECT lower(address) FROM hidden_recipients) \
               AND lower(address) NOT IN (SELECT lower(address) FROM accounts) \
             GROUP BY lower(address) ORDER BY last DESC, n DESC LIMIT ?1",
            Self::HISTORY_UNION
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![limit], Self::read_suggestion);
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    fn read_suggestion(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContactSuggestion> {
        Ok(ContactSuggestion {
            name: row.get(0)?,
            address: row.get(1)?,
            use_count: row.get::<_, i64>(2)? as u32,
            last_used_at_ms: row.get(3)?,
        })
    }

    /// Dismiss a suggestion so it stops appearing (P1.2).
    pub fn hide_recipient(&self, address: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO hidden_recipients (address) VALUES (?1)",
            params![address],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_contact_groups(&self) -> Vec<ContactGroup> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare("SELECT id, name FROM contact_groups ORDER BY name") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| Ok(ContactGroup { id: r.get(0)?, name: r.get(1)? }))
            .map_err(|_| ());
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn create_contact_group(&self, name: &str) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO contact_groups (name) VALUES (?1)",
            params![name.trim()],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "a group with that name already exists".to_string()
            } else {
                e.to_string()
            }
        })?;
        Ok(conn.last_insert_rowid())
    }

    /// Contact groups whose name matches `query` — for expanding a group into
    /// its recipients in the composer (P2).
    pub fn suggest_groups(&self, query: &str, limit: u32) -> Vec<ContactGroup> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        let pattern = format!("%{}%", Self::escape_like(query.trim()));
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, name FROM contact_groups WHERE lower(name) LIKE ?1 ESCAPE '\\' \
             ORDER BY name LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(params![pattern, limit], |r| Ok(ContactGroup { id: r.get(0)?, name: r.get(1)? }))
            .map_err(|_| ());
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn delete_contact_group(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM contact_groups WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_contact_to_group(&self, group_id: i64, address: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO contact_group_members (group_id, address) VALUES (?1, ?2)",
            params![group_id, address.trim()],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn remove_contact_from_group(&self, group_id: i64, address: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM contact_group_members WHERE group_id = ?1 AND address = ?2",
            params![group_id, address],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// A group's members as suggestions (names/counts joined from history when
    /// known).
    pub fn contact_group_members(&self, group_id: i64) -> Vec<ContactSuggestion> {
        let conn = self.conn.lock().unwrap();
        let sql = format!(
            "SELECT COALESCE(h.name, ''), m.address, COALESCE(h.n, 0), COALESCE(h.last, 0) \
             FROM contact_group_members m \
             LEFT JOIN ( \
               SELECT lower(address) AS addr, MAX(name) AS name, COUNT(*) AS n, \
                      MAX(received_at_ms) AS last \
               FROM ({}) GROUP BY lower(address) \
             ) h ON h.addr = lower(m.address) \
             WHERE m.group_id = ?1 ORDER BY m.address",
            Self::HISTORY_UNION
        );
        let mut stmt = match conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![group_id], Self::read_suggestion);
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    // -- P1.3 saved searches ---------------------------------------------

    pub fn list_saved_searches(&self) -> Vec<SavedSearch> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, name, query, created_at_ms FROM saved_searches ORDER BY created_at_ms",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| {
                Ok(SavedSearch {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    query: r.get(2)?,
                    created_at_ms: r.get(3)?,
                })
            })
            .map_err(|_| ());
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    pub fn save_search(&self, name: &str, query: &str) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO saved_searches (name, query, created_at_ms) VALUES (?1, ?2, ?3)",
            params![name.trim(), query.trim(), now_ms()],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                "a saved search with that name already exists".to_string()
            } else {
                e.to_string()
            }
        })?;
        Ok(conn.last_insert_rowid())
    }

    pub fn delete_saved_search(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM saved_searches WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn apply_rules_to_folder(
        &self,
        account_id: AccountId,
        folder: &str,
        rules: &[MailRule],
    ) -> Result<u32, String> {
        if rules.is_empty() {
            return Ok(0);
        }

        let messages = self.messages_in_folder(account_id, folder);

        let mut count = 0;
        for msg in messages {
            let detail = self.get_message(msg.id);
            let actions = crate::rules::evaluate_rules(rules, &msg, detail.as_ref());
            if actions.is_empty() {
                continue;
            }

            for action in actions {
                match action {
                    RuleAction::MoveToFolder { folder_name } => {
                        let _ = self.move_message(msg.id, &folder_name);
                    }
                    RuleAction::MarkRead => {
                        let _ = self.set_read(msg.id, false);
                    }
                    RuleAction::MarkUnread => {
                        let _ = self.set_read(msg.id, true);
                    }
                    RuleAction::MarkFlagged => {
                        let _ = self.set_flagged(msg.id, true);
                    }
                    RuleAction::MarkUnflagged => {
                        let _ = self.set_flagged(msg.id, false);
                    }
                    RuleAction::Delete => {
                        let _ = self.delete(msg.id);
                    }
                    RuleAction::Archive => {
                        let _ = self.archive(msg.id);
                    }
                }
            }
            count += 1;
        }

        Ok(count)
    }

    /// The message rows in an account+folder — shared by rule apply and preview.
    fn messages_in_folder(&self, account_id: AccountId, folder: &str) -> Vec<MessageRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT m.id, m.account_id, m.folder, m.sender_name, m.sender_address, \
             m.subject, m.snippet, m.received_at_ms, m.unread, m.flagged, \
             m.answered, m.forwarded, m.has_attachments, m.thread_id, 1 as thread_count \
             FROM messages m WHERE m.account_id = ?1 AND m.folder = ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(params![account_id, folder], Self::read_row)
            .map_err(|_| ());
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// P1.3 rule dry-run: evaluate rules against a folder's messages WITHOUT
    /// applying them, returning the affected count + per-message previews with
    /// the matching-rule order (the ordering explanation) and before-state
    /// (so an applied run can be reverted).
    pub fn preview_rules(
        &self,
        account_id: AccountId,
        folder: &str,
        rules: &[MailRule],
    ) -> RulePreviewResult {
        if rules.is_empty() {
            return RulePreviewResult {
                affected: 0,
                previews: Vec::new(),
            };
        }
        let messages = self.messages_in_folder(account_id, folder);
        let mut previews = Vec::new();
        for msg in messages {
            let detail = self.get_message(msg.id);
            let matched_idx = crate::rules::matching_rules(rules, &msg, detail.as_ref());
            if matched_idx.is_empty() {
                continue;
            }
            let matched = matched_idx
                .into_iter()
                .map(|i| RulePreviewMatch {
                    rule_name: rules[i].name.clone(),
                    rule_index: i as u32,
                    actions: rules[i].actions.iter().map(describe_rule_action).collect(),
                })
                .collect();
            previews.push(RulePreview {
                message_id: msg.id,
                subject: msg.subject.clone(),
                sender: if msg.sender_name.is_empty() {
                    msg.sender_address.clone()
                } else {
                    format!("{} <{}>", msg.sender_name, msg.sender_address)
                },
                folder_before: msg.folder.clone(),
                unread_before: msg.unread,
                flagged_before: msg.flagged,
                matched,
            });
        }
        RulePreviewResult {
            affected: previews.len() as u32,
            previews,
        }
    }

    /// P1.3: undo an applied rule run — restore each previewed message to its
    /// before-state (folder/read/star) and cancel the queued server action for
    /// moved messages. Returns how many messages were reverted.
    pub fn revert_rules(
        &self,
        account_id: AccountId,
        previews: &[RulePreview],
    ) -> Result<u32, String> {
        let mut reverted = 0;
        for p in previews {
            let Some(detail) = self.get_message(p.message_id) else {
                continue;
            };
            let row = detail.row;
            if row.folder != p.folder_before {
                if let Some((acct, _local, server_folder, uid)) =
                    self.get_message_location(p.message_id)
                {
                    if let Some(f) = server_folder {
                        let _ = self.cancel_pending_actions(acct, &f, uid);
                    }
                }
                let _ = self.move_message(p.message_id, &p.folder_before);
                reverted += 1;
            }
            if row.unread != p.unread_before {
                let _ = self.set_read(p.message_id, p.unread_before);
            }
            if row.flagged != p.flagged_before {
                let _ = self.set_flagged(p.message_id, p.flagged_before);
            }
        }
        let _ = account_id;
        Ok(reverted)
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

    /// Search across messages and calendar events using SQLite FTS5 (Epic 15).
    pub fn search(&self, query: &SearchQuery) -> Vec<SearchMatch> {
        let conn = self.conn.lock().unwrap();
        let raw = query.query.trim();
        if raw.is_empty() {
            return Vec::new();
        }

        // P1.3: parse the query into full-text terms + operators.
        let (terms, ops) = parse_search_query(raw);
        let fts_query = fts_match_query(&terms);
        let has_fts = !fts_query.is_empty();
        let mut results = Vec::new();
        let limit = query.limit.max(1).min(100);

        // Build the message-side WHERE clauses from operators. `m.`-prefixed
        // because the FTS join aliases messages as `m` (the operators-only
        // scan below uses the same alias).
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();
        for (op, value) in &ops {
            match op.as_str() {
                "from" => {
                    let idx = params.len() + 1;
                    clauses.push(format!(
                        "(lower(m.sender_address) LIKE ?{idx} ESCAPE '\\' \
                         OR lower(m.sender_name) LIKE ?{idx} ESCAPE '\\')"
                    ));
                    params.push(Value::Text(format!("%{}%", Self::escape_like(value))));
                }
                "to" | "cc" => {
                    let kind_idx = params.len() + 1;
                    let val_idx = params.len() + 2;
                    clauses.push(format!(
                        "EXISTS(SELECT 1 FROM recipients r WHERE r.message_id = m.id \
                         AND r.kind = ?{kind_idx} \
                         AND (lower(r.address) LIKE ?{val_idx} ESCAPE '\\' \
                              OR lower(r.name) LIKE ?{val_idx} ESCAPE '\\'))"
                    ));
                    params.push(Value::Text((if op == "to" { "to" } else { "cc" }).into()));
                    params.push(Value::Text(format!("%{}%", Self::escape_like(value))));
                }
                "subject" => {
                    let idx = params.len() + 1;
                    clauses.push(format!("lower(m.subject) LIKE ?{idx} ESCAPE '\\'"));
                    params.push(Value::Text(format!("%{}%", Self::escape_like(value))));
                }
                "has" => {
                    if value.eq_ignore_ascii_case("attachment") {
                        clauses.push("m.has_attachments = 1".into());
                    }
                }
                "is" => match value.to_lowercase().as_str() {
                    "unread" => clauses.push("m.unread = 1".into()),
                    "read" => clauses.push("m.unread = 0".into()),
                    "starred" => clauses.push("m.flagged = 1".into()),
                    "unstarred" => clauses.push("m.flagged = 0".into()),
                    _ => {}
                },
                "before" | "after" => {
                    if let Some(ms) = parse_search_date(value) {
                        let idx = params.len() + 1;
                        let cmp = if op == "before" { "<" } else { ">" };
                        clauses.push(format!("m.received_at_ms {cmp} ?{idx}"));
                        params.push(Value::Integer(ms));
                    }
                }
                "in" => {
                    let idx = params.len() + 1;
                    clauses.push(format!("lower(m.folder) = lower(?{idx})"));
                    params.push(Value::Text(value.clone()));
                }
                "account" => {
                    let matches: Vec<i64> = conn
                        .prepare(
                            "SELECT id FROM accounts WHERE lower(address) = lower(?1) OR id = ?2",
                        )
                        .ok()
                        .map(|mut stmt| {
                            let id: i64 = value.parse().unwrap_or(-1);
                            stmt.query_map(params![value, id], |r| r.get(0))
                                .map(|iter| iter.flatten().collect())
                                .unwrap_or_default()
                        })
                        .unwrap_or_default();
                    match matches.first() {
                        Some(id) => clauses.push(format!("m.account_id = {id}")),
                        None => clauses.push("0 = 1".into()), // unknown account → no hits
                    }
                }
                "calendar" => {} // event side only
                _ => {}
            }
        }

        // Full-text clause (when there are plain terms) + scope (folder /
        // account / P1.1 hidden+deleted).
        if has_fts {
            let idx = params.len() + 1;
            clauses.push(format!("messages_fts MATCH ?{idx}"));
            params.push(Value::Text(fts_query.clone()));
        }
        if let Some(ref folder) = query.folder {
            let idx = params.len() + 1;
            clauses.push(format!("m.folder = ?{idx}"));
            params.push(Value::Text(folder.clone()));
        }
        if let Some(account_id) = query.account_id {
            clauses.push(format!("m.account_id = {account_id}"));
        }
        {
            let idx = params.len() + 1;
            clauses.push(format!(
                "m.deleted_at_ms IS NULL AND (m.snoozed_until_ms IS NULL OR m.snoozed_until_ms <= ?{idx})"
            ));
            params.push(Value::Integer(now_ms()));
        }

        // 1. Messages — join messages_fts when there's a full-text term (for
        // the snippet + rank), else a plain scan with the row's snippet.
        if !clauses.is_empty() {
            let where_sql = clauses.join(" AND ");
            let sql = if has_fts {
                format!(
                    "SELECT m.id, m.account_id, m.folder, m.subject, m.sender_name, \
                     m.sender_address, m.received_at_ms, \
                     snippet(messages_fts, -1, '<mark>', '</mark>', '…', 15) \
                     FROM messages_fts JOIN messages m ON m.id = messages_fts.message_id \
                     WHERE {where_sql} ORDER BY rank LIMIT {limit}"
                )
            } else {
                format!(
                    "SELECT m.id, m.account_id, m.folder, m.subject, m.sender_name, \
                     m.sender_address, m.received_at_ms, m.snippet \
                     FROM messages m WHERE {where_sql} \
                     ORDER BY m.received_at_ms DESC LIMIT {limit}"
                )
            };
            if let Ok(mut stmt) = conn.prepare(&sql) {
                if let Ok(iter) = stmt.query_map(rusqlite::params_from_iter(params.iter()), |r| {
                    let sender_name: String = r.get(4)?;
                    let sender_addr: String = r.get(5)?;
                    let subtitle = if sender_name.is_empty() {
                        sender_addr
                    } else {
                        format!("{sender_name} <{sender_addr}>")
                    };
                    Ok(SearchMatch {
                        kind: "message".into(),
                        id: r.get(0)?,
                        account_id: r.get(1)?,
                        folder: Some(r.get(2)?),
                        title: r.get(3)?,
                        subtitle,
                        snippet: r.get(7)?,
                        timestamp_ms: r.get(6)?,
                    })
                }) {
                    for m in iter.flatten() {
                        results.push(m);
                    }
                }
            }
        }

        // 2. Calendar events — only in a global (non-folder-scoped) search and
        // only when there's a full-text term or an event-relevant operator
        // (`calendar:`, `before:`, `after:`), so `is:unread` alone doesn't
        // surface events.
        let event_relevant = has_fts
            || ops
                .iter()
                .any(|(op, _)| op == "calendar" || op == "before" || op == "after");
        if query.folder.is_none() && event_relevant {
            let mut e_clauses: Vec<String> = Vec::new();
            let mut e_params: Vec<Value> = Vec::new();
            if has_fts {
                let idx = e_params.len() + 1;
                e_clauses.push(format!("events_fts MATCH ?{idx}"));
                e_params.push(Value::Text(fts_query.clone()));
            }
            for (op, value) in &ops {
                match op.as_str() {
                    "calendar" => {
                        let idx = e_params.len() + 1;
                        e_clauses.push(format!(
                            "lower(COALESCE(e.calendar_name, '')) LIKE ?{idx} ESCAPE '\\'"
                        ));
                        e_params.push(Value::Text(format!("%{}%", Self::escape_like(value))));
                    }
                    "before" | "after" => {
                        if let Some(ms) = parse_search_date(value) {
                            let idx = e_params.len() + 1;
                            let cmp = if op == "before" { "<" } else { ">" };
                            e_clauses.push(format!("e.start_ms {cmp} ?{idx}"));
                            e_params.push(Value::Integer(ms));
                        }
                    }
                    _ => {}
                }
            }
            if let Some(account_id) = query.account_id {
                e_clauses.push(format!("e.account_id = {account_id}"));
            }
            if !e_clauses.is_empty() {
                let event_sql = if has_fts {
                    format!(
                        "SELECT e.id, e.account_id, e.title, COALESCE(e.location, ''), e.start_ms, \
                         snippet(events_fts, -1, '<mark>', '</mark>', '…', 15) \
                         FROM events_fts JOIN events e ON e.id = events_fts.event_id \
                         WHERE {} ORDER BY rank LIMIT {limit}",
                        e_clauses.join(" AND ")
                    )
                } else {
                    format!(
                        "SELECT e.id, e.account_id, e.title, COALESCE(e.location, ''), e.start_ms, \
                         COALESCE(e.notes, '') \
                         FROM events e WHERE {} ORDER BY e.start_ms DESC LIMIT {limit}",
                        e_clauses.join(" AND ")
                    )
                };
                if let Ok(mut stmt) = conn.prepare(&event_sql) {
                    if let Ok(iter) =
                        stmt.query_map(rusqlite::params_from_iter(e_params.iter()), |r| {
                            Ok(SearchMatch {
                                kind: "event".into(),
                                id: r.get(0)?,
                                account_id: r.get(1)?,
                                folder: None,
                                title: r.get(2)?,
                                subtitle: r.get(3)?,
                                snippet: r.get(5)?,
                                timestamp_ms: r.get(4)?,
                            })
                        })
                    {
                        for e in iter.flatten() {
                            results.push(e);
                        }
                    }
                }
            }
        }

        results
    }

    /// Rebuild SQLite FTS5 search index tables from base tables.
    pub fn rebuild_search_index(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "DELETE FROM messages_fts; \
             INSERT INTO messages_fts(message_id, subject, sender, recipients, body) \
             SELECT m.id, m.subject, m.sender_name || ' ' || m.sender_address, \
                    COALESCE((SELECT GROUP_CONCAT(name || ' ' || address, ' ') FROM recipients WHERE message_id = m.id), ''), \
                    COALESCE((SELECT plain FROM bodies WHERE message_id = m.id), m.snippet) \
             FROM messages m; \
             DELETE FROM events_fts; \
             INSERT INTO events_fts(event_id, title, location, notes) \
             SELECT id, title, COALESCE(location, ''), COALESCE(notes, '') FROM events;",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// P1.3: messages vs indexed rows — fresh when equal. The triggers keep the
    /// index live day-to-day, so a rebuild is the repair path.
    pub fn search_index_status(&self) -> (u64, u64) {
        let conn = self.conn.lock().unwrap();
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap_or(0);
        let indexed: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages_fts", [], |r| r.get(0))
            .unwrap_or(0);
        (total.max(0) as u64, indexed.max(0) as u64)
    }

    /// P1.3: rebuild the FTS index in batches, checking `cancel` between
    /// batches and reporting progress. Returns `Err("cancelled")` when the flag
    /// is set mid-way — the partially rebuilt index stays consistent (the
    /// triggers keep it live), so a later run simply continues.
    pub fn rebuild_search_index_cancellable(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
        on_progress: impl Fn(usize, usize),
    ) -> Result<(), String> {
        use std::sync::atomic::Ordering;
        let conn = self.conn.lock().unwrap();
        conn.execute_batch("DELETE FROM messages_fts;")
            .map_err(|e| e.to_string())?;
        let total: usize = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize;
        let mut indexed = 0usize;
        let mut last_id: i64 = 0;
        const BATCH: i64 = 200;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let batch_ids: Vec<i64> = {
                let mut stmt = conn
                    .prepare("SELECT id FROM messages WHERE id > ?1 ORDER BY id LIMIT ?2")
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(params![last_id, BATCH], |r| r.get(0))
                    .map_err(|e| e.to_string())?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?
            };
            if batch_ids.is_empty() {
                break;
            }
            let max_id = *batch_ids.last().unwrap();
            let placeholders = batch_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let inserted = conn
                .execute(
                    &format!(
                        "INSERT INTO messages_fts(message_id, subject, sender, recipients, body) \
                         SELECT m.id, m.subject, m.sender_name || ' ' || m.sender_address, \
                                COALESCE((SELECT GROUP_CONCAT(name || ' ' || address, ' ') FROM recipients WHERE message_id = m.id), ''), \
                                COALESCE((SELECT plain FROM bodies WHERE message_id = m.id), m.snippet) \
                         FROM messages m WHERE m.id IN ({placeholders})"
                    ),
                    rusqlite::params_from_iter(batch_ids.iter()),
                )
                .map_err(|e| e.to_string())?;
            indexed += inserted as usize;
            last_id = max_id;
            on_progress(indexed, total);
        }
        conn.execute_batch(
            "DELETE FROM events_fts; \
             INSERT INTO events_fts(event_id, title, location, notes) \
             SELECT id, title, COALESCE(location, ''), COALESCE(notes, '') FROM events;",
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_events(&self, start_ms: i64, end_ms: i64) -> Vec<CalendarEvent> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, account_id, title, start_ms, end_ms, all_day, location, notes, alarm_minutes_before, timezone, travel_time_minutes, \
                 calendar_source, calendar_name, calendar_color, color \
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
                alarm_minutes_before: r.get(8)?,
                timezone: r.get(9)?,
                travel_time_minutes: r.get(10)?,
                calendar_source: r.get(11)?,
                calendar_name: r.get(12)?,
                calendar_color: r.get(13)?,
                color: r.get(14)?,
            })
        })
        .expect("event rows")
        .map(|r| r.expect("event"))
        .collect()
    }

    pub fn create_event(&self, mut event: CalendarEvent) -> Result<CalendarEvent, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (account_id, title, start_ms, end_ms, all_day, location, notes, alarm_minutes_before, timezone, travel_time_minutes, \
             calendar_source, calendar_name, calendar_color, color) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                event.account_id,
                event.title,
                event.start_ms,
                event.end_ms,
                event.all_day as i64,
                event.location,
                event.notes,
                event.alarm_minutes_before,
                event.timezone,
                event.travel_time_minutes,
                event.calendar_source,
                event.calendar_name,
                event.calendar_color,
                event.color,
            ],
        )
        .map_err(|e| format!("create event failed: {e}"))?;
        event.id = conn.last_insert_rowid() as EventId;
        Ok(event)
    }

    pub fn update_event(&self, event: CalendarEvent) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE events SET account_id = ?1, title = ?2, start_ms = ?3, end_ms = ?4, \
                 all_day = ?5, location = ?6, notes = ?7, alarm_minutes_before = ?8, timezone = ?9, travel_time_minutes = ?10, \
                 calendar_source = ?11, calendar_name = ?12, calendar_color = ?13, color = ?14 WHERE id = ?15",
                params![
                    event.account_id,
                    event.title,
                    event.start_ms,
                    event.end_ms,
                    event.all_day as i64,
                    event.location,
                    event.notes,
                    event.alarm_minutes_before,
                    event.timezone,
                    event.travel_time_minutes,
                    event.calendar_source,
                    event.calendar_name,
                    event.calendar_color,
                    event.color,
                    event.id
                ],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such event".into());
        }
        Ok(())
    }

    /// Distinct source calendars present in the store (Roadmap 4.4) — the rows
    /// the calendar sidebar shows for synced Google calendars.
    pub fn list_calendar_sources(&self) -> Vec<CalendarSource> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT account_id, calendar_source, calendar_name, calendar_color \
                 FROM events \
                 WHERE calendar_source IS NOT NULL AND calendar_source != '' \
                 ORDER BY calendar_name",
            )
            .expect("calendar sources query");
        stmt.query_map([], |r| {
            Ok(CalendarSource {
                account_id: r.get(0)?,
                source: r.get(1)?,
                name: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                color: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })
        .expect("calendar source rows")
        .map(|r| r.expect("calendar source"))
        .collect()
    }

    pub fn delete_event(&self, id: EventId) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute("DELETE FROM events WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such event".into());
        }
        Ok(())
    }

    /// P1.4 undo: restore an event exactly as captured (re-create a deleted
    /// event or overwrite an edited one). INSERT-OR-REPLACE by id.
    pub fn restore_event(&self, event: CalendarEvent) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO events (id, account_id, title, start_ms, end_ms, all_day, \
             location, notes, alarm_minutes_before, timezone, travel_time_minutes, \
             calendar_source, calendar_name, calendar_color, color) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                event.id,
                event.account_id,
                event.title,
                event.start_ms,
                event.end_ms,
                event.all_day as i64,
                event.location,
                event.notes,
                event.alarm_minutes_before,
                event.timezone,
                event.travel_time_minutes,
                event.calendar_source,
                event.calendar_name,
                event.calendar_color,
                event.color,
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// P1.4: clone an event into the same calendar with a fresh id and a
    /// "(copy)" title. Returns the new event.
    pub fn duplicate_event(&self, id: EventId) -> Result<CalendarEvent, String> {
        let events = self.list_events(0, i64::MAX / 2);
        let src = events
            .into_iter()
            .find(|e| e.id == id)
            .ok_or("no such event")?;
        let mut clone = CalendarEvent {
            id: 0,
            account_id: src.account_id,
            title: format!("{} (copy)", src.title),
            start_ms: src.start_ms,
            end_ms: src.end_ms,
            all_day: src.all_day,
            location: src.location,
            notes: src.notes,
            alarm_minutes_before: src.alarm_minutes_before,
            timezone: src.timezone,
            travel_time_minutes: src.travel_time_minutes,
            calendar_source: src.calendar_source,
            calendar_name: src.calendar_name,
            calendar_color: src.calendar_color,
            color: src.color,
        };
        self.create_event(clone)
    }

    /// Delete every event belonging to a source calendar (e.g. one Google
    /// calendar). FTS rows are cleaned up by the existing `trg_events_ad`
    /// trigger.
    pub fn delete_events_by_source(
        &self,
        account_id: AccountId,
        source: &str,
    ) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "DELETE FROM events WHERE account_id = ?1 AND calendar_source = ?2",
                params![account_id, source],
            )
            .map_err(|e| e.to_string())?;
        Ok(affected)
    }

    /// Record a source calendar as removed (so the sync skips it and it stays
    /// gone), keeping its name/color so the Settings "Removed" list can show
    /// it after its events are deleted.
    pub fn mark_calendar_source_removed(
        &self,
        account_id: AccountId,
        source: &str,
        name: &str,
        color: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO removed_calendar_sources (account_id, source, name, color) \
             VALUES (?1, ?2, ?3, ?4)",
            params![account_id, source, name, color],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// All source calendars the user has removed (excluded from sync).
    pub fn removed_calendar_sources(&self) -> Vec<CalendarSource> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT account_id, source, name, color FROM removed_calendar_sources \
                 ORDER BY name",
            )
            .expect("removed calendar sources query");
        stmt.query_map([], |r| {
            Ok(CalendarSource {
                account_id: r.get(0)?,
                source: r.get(1)?,
                name: r.get(2)?,
                color: r.get(3)?,
            })
        })
        .expect("removed calendar source rows")
        .map(|r| r.expect("removed calendar source"))
        .collect()
    }

    /// Re-allow a source calendar on the next sync (undo a removal).
    pub fn clear_calendar_source_removed(
        &self,
        account_id: AccountId,
        source: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM removed_calendar_sources WHERE account_id = ?1 AND source = ?2",
            params![account_id, source],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_tasks(&self, account_id: Option<AccountId>) -> Vec<CalendarTask> {
        let conn = self.conn.lock().unwrap();
        let mut query = "SELECT id, account_id, title, due_at_ms, completed_at_ms, priority FROM tasks".to_string();
        if account_id.is_some() {
            query.push_str(" WHERE account_id = ?1");
        }
        query.push_str(" ORDER BY completed_at_ms ASC, due_at_ms ASC, id ASC");

        let mut stmt = conn.prepare(&query).expect("tasks query");
        let mapper = |r: &rusqlite::Row| {
            Ok(CalendarTask {
                id: r.get(0)?,
                account_id: r.get(1)?,
                title: r.get(2)?,
                due_at_ms: r.get(3)?,
                completed_at_ms: r.get(4)?,
                priority: r.get(5)?,
            })
        };

        if let Some(acc) = account_id {
            stmt.query_map(params![acc], mapper)
                .expect("task rows")
                .map(|r| r.expect("task"))
                .collect()
        } else {
            stmt.query_map([], mapper)
                .expect("task rows")
                .map(|r| r.expect("task"))
                .collect()
        }
    }

    pub fn create_task(&self, mut task: CalendarTask) -> Result<CalendarTask, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO tasks (account_id, title, due_at_ms, completed_at_ms, priority) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                task.account_id,
                task.title,
                task.due_at_ms,
                task.completed_at_ms,
                task.priority,
            ],
        )
        .map_err(|e| format!("create task failed: {e}"))?;
        task.id = conn.last_insert_rowid() as u32;
        Ok(task)
    }

    pub fn update_task(&self, task: CalendarTask) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE tasks SET account_id = ?1, title = ?2, due_at_ms = ?3, completed_at_ms = ?4, priority = ?5 WHERE id = ?6",
                params![
                    task.account_id,
                    task.title,
                    task.due_at_ms,
                    task.completed_at_ms,
                    task.priority,
                    task.id
                ],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such task".into());
        }
        Ok(())
    }

    pub fn toggle_task(&self, id: u32) -> Result<CalendarTask, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, account_id, title, due_at_ms, completed_at_ms, priority FROM tasks WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        let mut task = stmt
            .query_row(params![id], |r| {
                Ok(CalendarTask {
                    id: r.get(0)?,
                    account_id: r.get(1)?,
                    title: r.get(2)?,
                    due_at_ms: r.get(3)?,
                    completed_at_ms: r.get(4)?,
                    priority: r.get(5)?,
                })
            })
            .map_err(|_| "no such task".to_string())?;

        let new_completed = if task.completed_at_ms.is_some() {
            None
        } else {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Some(now_ms)
        };
        conn.execute(
            "UPDATE tasks SET completed_at_ms = ?1 WHERE id = ?2",
            params![new_completed, id],
        )
        .map_err(|e| e.to_string())?;

        task.completed_at_ms = new_completed;
        Ok(task)
    }

    pub fn delete_task(&self, id: u32) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such task".into());
        }
        Ok(())
    }

    pub fn list_subscriptions(&self) -> Vec<CalendarSubscription> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, name, url, color, refresh_interval_min, last_refreshed_at_ms, enabled \
                 FROM calendar_subscriptions ORDER BY id ASC",
            )
            .expect("subscriptions query");
        stmt.query_map([], |r| {
            Ok(CalendarSubscription {
                id: r.get(0)?,
                name: r.get(1)?,
                url: r.get(2)?,
                color: r.get(3)?,
                refresh_interval_min: r.get(4)?,
                last_refreshed_at_ms: r.get(5)?,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        })
        .expect("subscription rows")
        .map(|r| r.expect("subscription"))
        .collect()
    }

    pub fn create_subscription(
        &self,
        name: &str,
        url: &str,
        color: &str,
        refresh_interval_min: u32,
    ) -> Result<CalendarSubscription, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO calendar_subscriptions (name, url, color, refresh_interval_min, enabled) \
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![name, url, color, refresh_interval_min],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid() as u32;
        Ok(CalendarSubscription {
            id,
            name: name.to_string(),
            url: url.to_string(),
            color: color.to_string(),
            refresh_interval_min,
            last_refreshed_at_ms: None,
            enabled: true,
        })
    }

    pub fn delete_subscription(&self, id: u32) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM calendar_subscriptions WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_subscription_refreshed(&self, id: u32, timestamp_ms: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE calendar_subscriptions SET last_refreshed_at_ms = ?1 WHERE id = ?2",
            params![timestamp_ms, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// On-disk cache bytes: stored message bodies plus attachment files. (The
    /// `accounts.local_bytes` column is never maintained, so this computes from
    /// the actual data.)
    pub fn total_disk_bytes(&self) -> u64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE((SELECT SUM(LENGTH(CAST(plain AS BLOB)) + COALESCE(LENGTH(CAST(html AS BLOB)), 0)) FROM bodies), 0) \
             + COALESCE((SELECT SUM(size_bytes) FROM attachments WHERE on_disk = 1), 0)",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|b| b as u64)
        .unwrap_or(0)
    }

    pub fn create_account(&self, info: &NewAccount, color: String) -> Result<Account, String> {
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
        .map_err(|e| format!("could not create account: {e}"))?;
        let id = conn.last_insert_rowid() as AccountId;
        Ok(Account {
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
            last_error: None,
        })
    }

    pub fn set_account_connected(
        &self,
        id: AccountId,
        connected: bool,
        last_error: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE accounts SET connected = ?1, last_error = ?2 WHERE id = ?3",
            params![connected as i64, last_error, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_account_folder_count(&self, id: AccountId, count: u32) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE accounts SET folder_count = ?1 WHERE id = ?2",
            params![count, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    // -- Folder sync selection (P0.2) --------------------------------------

    /// Replace an account's folder-sync selection wholesale. Rows for folders
    /// that disappeared server-side are dropped; new selections upsert.
    pub fn set_synced_folders(
        &self,
        account_id: AccountId,
        folders: &[SyncedFolder],
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM synced_folders WHERE account_id = ?1",
            params![account_id],
        )
        .map_err(|e| e.to_string())?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT OR REPLACE INTO synced_folders \
                     (account_id, server_name, local_name, kind, enabled) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| e.to_string())?;
            for f in folders {
                stmt.execute(params![
                    account_id,
                    f.server_name,
                    f.local_name,
                    f.kind.as_str(),
                    f.enabled as i64,
                ])
                .map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())
    }

    /// Add folders missing from the account's selection, enabled by default.
    /// Existing rows (and their enabled flags) are untouched — this lets the
    /// sync engine surface a newly created server mailbox without re-enabling
    /// folders the user deliberately turned off.
    pub fn upsert_synced_folders(
        &self,
        account_id: AccountId,
        folders: &[SyncedFolder],
    ) -> Result<(), String> {
        if folders.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "INSERT OR IGNORE INTO synced_folders \
                 (account_id, server_name, local_name, kind, enabled) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .map_err(|e| e.to_string())?;
        for f in folders {
            stmt.execute(params![
                account_id,
                f.server_name,
                f.local_name,
                f.kind.as_str(),
                f.enabled as i64,
            ])
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// The account's folder-sync selection. Empty = not configured yet (the
    /// sync engine falls back to syncing every discovered folder).
    pub fn synced_folders(&self, account_id: AccountId) -> Vec<SyncedFolder> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT account_id, server_name, local_name, kind, enabled \
             FROM synced_folders WHERE account_id = ?1 ORDER BY local_name",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map(params![account_id], |r| {
                Ok(SyncedFolder {
                    account_id: r.get(0)?,
                    server_name: r.get(1)?,
                    local_name: r.get(2)?,
                    kind: FolderKind::from_str(&r.get::<_, String>(3)?),
                    enabled: r.get::<_, i64>(4)? != 0,
                })
            })
            .map_err(|_| ());
        match rows {
            Ok(rows) => rows.flatten().collect(),
            Err(_) => Vec::new(),
        }
    }

    /// The set of enabled server mailboxes, when the account has a configured
    /// folder selection. `None` means no selection exists → sync everything.
    pub fn enabled_folder_set(&self, account_id: AccountId) -> Option<HashSet<String>> {
        let all = self.synced_folders(account_id);
        if all.is_empty() {
            return None;
        }
        Some(
            all.into_iter()
                .filter(|f| f.enabled)
                .map(|f| f.server_name)
                .collect(),
        )
    }

    /// Update the editable fields of an existing account (server/port/TLS/sync
    /// mode/color). The address and protocol are the account's identity and are
    /// not changed by an edit.
    pub fn update_account(&self, edit: &AccountEdit) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let affected = conn
            .execute(
                "UPDATE accounts SET server = ?1, port = ?2, tls = ?3, sync_mode = ?4, color = ?5 \
                 WHERE id = ?6",
                params![
                    edit.server,
                    edit.port,
                    edit.tls as i64,
                    edit.sync_mode,
                    edit.color,
                    edit.id
                ],
            )
            .map_err(|e| e.to_string())?;
        if affected == 0 {
            return Err("no such account".into());
        }
        Ok(())
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

    /// The account, local folder, server mailbox name, and UID for a message.
    /// The server mailbox is what on-demand body fetches and offline-action
    /// replay re-`SELECT` on the IMAP connection — the local display name
    /// ("Sent", "Archive", …) is not a real mailbox on Gmail/Outlook.
    pub fn get_message_location(
        &self,
        id: MessageId,
    ) -> Option<(AccountId, String, Option<String>, Option<u32>)> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT account_id, folder, server_folder, uid FROM messages WHERE id = ?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// The server mailbox name used for archiving an account's mail, derived
    /// from the first message mapped to the display Archive folder. Callers
    /// fall back to "Archive" when nothing is known.
    pub fn archive_folder_name(&self, account_id: AccountId) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT server_folder FROM messages WHERE account_id = ?1 AND folder = 'Archive' \
             AND server_folder IS NOT NULL AND server_folder != '' LIMIT 1",
            params![account_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    // -- Sync writes (Epic 12.2) ------------------------------------------

    /// The last sync watermark for an account+folder: `(uidvalidity, uidnext, highestmodseq)`.
    /// All UIDs currently stored for an account+folder. Used by the sync's
    /// full-refetch path to avoid re-downloading bodies for messages it already
    /// has (a busy Inbox would otherwise refetch the whole folder each cycle).
    pub fn folder_uids(&self, account_id: AccountId, folder: &str) -> Vec<u32> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT uid FROM messages WHERE account_id = ?1 AND folder = ?2 AND uid IS NOT NULL")
            .expect("folder uids query");
        stmt.query_map(params![account_id, folder], |r| r.get::<_, i64>(0))
            .expect("folder uid rows")
            .map(|r| r.expect("folder uid") as u32)
            .collect()
    }

    pub fn get_sync_state(&self, account_id: AccountId, folder: &str) -> (i64, i64, i64) {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(uidvalidity, 0), COALESCE(uidnext, 0), COALESCE(highestmodseq, 0) FROM sync_state \
             WHERE account_id = ?1 AND folder = ?2",
            params![account_id, folder],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or((0, 0, 0))
    }

    pub fn set_sync_state(
        &self,
        account_id: AccountId,
        folder: &str,
        uidvalidity: i64,
        uidnext: i64,
        highestmodseq: i64,
    ) -> Result<(), String> {
        let now = now_ms();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sync_state (account_id, folder, uidvalidity, uidnext, highestmodseq, last_synced_at_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(account_id, folder) DO UPDATE SET uidvalidity = ?3, uidnext = ?4, \
             highestmodseq = ?5, last_synced_at_ms = ?6",
            params![account_id, folder, uidvalidity, uidnext, highestmodseq, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_message_flags_by_uid(
        &self,
        account_id: AccountId,
        folder: &str,
        uid: u32,
        unread: bool,
        flagged: bool,
        answered: bool,
        forwarded: bool,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET unread = ?1, flagged = ?2, answered = ?3, forwarded = ?4 \
             WHERE account_id = ?5 AND folder = ?6 AND uid = ?7",
            params![
                unread as i64,
                flagged as i64,
                answered as i64,
                forwarded as i64,
                account_id,
                folder,
                uid
            ],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn save_message_body_and_attachments(
        &self,
        id: MessageId,
        plain: &str,
        html: Option<&str>,
        to: &[Recipient],
        cc: &[Recipient],
        bcc: &[Recipient],
        attachments: &[Attachment],
        message_id_header: Option<&str>,
        in_reply_to: Option<&str>,
        references: Option<&str>,
        list_unsubscribe: Option<&str>,
        list_unsubscribe_post: Option<&str>,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        tx.execute(
            "INSERT INTO bodies (message_id, plain, html, list_unsubscribe, list_unsubscribe_post) VALUES (?1, ?2, ?3, ?4, ?5) \
             ON CONFLICT(message_id) DO UPDATE SET plain = ?2, html = ?3, list_unsubscribe = ?4, list_unsubscribe_post = ?5",
            params![id, plain, html, list_unsubscribe, list_unsubscribe_post],
        )
        .map_err(|e| e.to_string())?;

        tx.execute("DELETE FROM recipients WHERE message_id = ?1", params![id])
            .map_err(|e| e.to_string())?;

        for (i, r) in to.iter().enumerate() {
            tx.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, 'to', ?2, ?3, ?4)",
                params![id, r.name, r.address, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }

        for (i, r) in cc.iter().enumerate() {
            tx.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, 'cc', ?2, ?3, ?4)",
                params![id, r.name, r.address, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }

        for (i, r) in bcc.iter().enumerate() {
            tx.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, 'bcc', ?2, ?3, ?4)",
                params![id, r.name, r.address, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.execute("DELETE FROM attachments WHERE message_id = ?1", params![id])
            .map_err(|e| e.to_string())?;

        for a in attachments {
            tx.execute(
                "INSERT INTO attachments (message_id, filename, size_bytes, on_disk) \
                 VALUES (?1, ?2, ?3, ?4)",
                params![id, a.filename, a.size_bytes as i64, a.on_disk as i64],
            )
            .map_err(|e| e.to_string())?;
        }

        let has_attachments = !attachments.is_empty();
        tx.execute(
            "UPDATE messages SET has_attachments = ?1, \
             message_id_header = COALESCE(?2, message_id_header), \
             in_reply_to = COALESCE(?3, in_reply_to), \
             references_header = COALESCE(?4, references_header) \
             WHERE id = ?5",
            params![
                has_attachments as i64,
                message_id_header,
                in_reply_to,
                references,
                id
            ],
        )
        .map_err(|e| e.to_string())?;

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn enqueue_action(
        &self,
        account_id: AccountId,
        action_type: ActionType,
        folder: &str,
        uid: Option<u32>,
        payload: Option<&str>,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        let now = now_ms();
        let type_str = match action_type {
            ActionType::MarkRead => "mark_read",
            ActionType::MarkUnread => "mark_unread",
            ActionType::Star => "star",
            ActionType::Unstar => "unstar",
            ActionType::Archive => "archive",
            ActionType::Delete => "delete",
            ActionType::Move => "move",
            ActionType::MarkJunk => "mark_junk",
            ActionType::MarkNotJunk => "mark_not_junk",
            ActionType::MarkAnswered => "mark_answered",
            ActionType::MarkForwarded => "mark_forwarded",
            ActionType::Send => "send",
        };
        conn.execute(
            "INSERT INTO action_queue (account_id, action_type, folder, uid, payload, created_at_ms, retries) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0)",
            params![account_id, type_str, folder, uid, payload, now],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }

    pub fn peek_pending_actions(&self, account_id: AccountId) -> Vec<QueuedAction> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, account_id, action_type, folder, uid, payload, created_at_ms, retries \
             FROM action_queue WHERE account_id = ?1 ORDER BY id ASC LIMIT 50",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt.query_map(params![account_id], |r| {
            let type_str: String = r.get(2)?;
            let action_type = match type_str.as_str() {
                "mark_read" => ActionType::MarkRead,
                "mark_unread" => ActionType::MarkUnread,
                "star" => ActionType::Star,
                "unstar" => ActionType::Unstar,
                "archive" => ActionType::Archive,
                "delete" => ActionType::Delete,
                "move" => ActionType::Move,
                "mark_junk" => ActionType::MarkJunk,
                "mark_not_junk" => ActionType::MarkNotJunk,
                "mark_answered" => ActionType::MarkAnswered,
                "mark_forwarded" => ActionType::MarkForwarded,
                "send" => ActionType::Send,
                _ => ActionType::MarkRead,
            };
            Ok(QueuedAction {
                id: r.get(0)?,
                account_id: r.get(1)?,
                action_type,
                folder: r.get(3)?,
                uid: r.get(4)?,
                payload: r.get(5)?,
                created_at_ms: r.get(6)?,
                retries: r.get::<_, i64>(7)? as u32,
            })
        });
        rows.map(|iter| iter.flatten().collect())
            .unwrap_or_default()
    }

    pub fn remove_action(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM action_queue WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn increment_action_retry(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE action_queue SET retries = retries + 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Number of queued (unsent/unapplied) actions for an account — the
    /// removal confirm warns when this is non-zero.
    pub fn pending_action_count(&self, account_id: AccountId) -> u32 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM action_queue WHERE account_id = ?1",
            params![account_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Number of locally saved drafts for an account.
    pub fn draft_count(&self, account_id: AccountId) -> u32 {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE account_id = ?1 AND folder = 'Drafts'",
            params![account_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Delete the on-disk attachment files belonging to an account's messages
    /// (the rows themselves cascade on account removal, but the files don't).
    /// Best-effort, matching `prune_messages_before`. Returns the file count.
    pub fn delete_attachments_for_account(&self, account_id: AccountId) -> usize {
        let ids: Vec<i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = match conn.prepare(
                "SELECT a.id FROM attachments a \
                 JOIN messages m ON m.id = a.message_id \
                 WHERE m.account_id = ?1 AND a.on_disk = 1",
            ) {
                Ok(s) => s,
                Err(_) => return 0,
            };
            let rows = stmt
                .query_map(params![account_id], |r| r.get(0))
                .map_err(|_| ());
            match rows {
                Ok(rows) => rows.flatten().collect(),
                Err(_) => Vec::new(),
            }
        };
        let mut deleted = 0;
        for id in ids {
            if std::fs::remove_dir_all(self.attachments_root.join(id.to_string())).is_ok() {
                deleted += 1;
            }
        }
        deleted
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
        server_folder: &str,
        uid: u32,
        uidvalidity: i64,
        sender_name: &str,
        sender_address: &str,
        subject: &str,
        snippet: &str,
        received_at_ms: i64,
        unread: bool,
        flagged: bool,
        answered: bool,
        forwarded: bool,
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
        let thread_id = crate::threading::compute_thread_id(None, None, subject);
        if let Some(id) = existing {
            conn.execute(
                "UPDATE messages SET sender_name = ?1, sender_address = ?2, subject = ?3, \
                 snippet = ?4, received_at_ms = ?5, unread = ?6, flagged = ?7, \
                 answered = ?8, forwarded = ?9, has_attachments = ?10, server_folder = ?11, \
                 thread_id = COALESCE(thread_id, ?12) WHERE id = ?13",
                params![
                    sender_name,
                    sender_address,
                    subject,
                    snippet,
                    received_at_ms,
                    unread as i64,
                    flagged as i64,
                    answered as i64,
                    forwarded as i64,
                    has_attachments as i64,
                    server_folder,
                    thread_id,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(id)
        } else {
            conn.execute(
                "INSERT INTO messages (account_id, folder, server_folder, sender_name, sender_address, subject, \
                 snippet, received_at_ms, unread, flagged, answered, forwarded, uid, uidvalidity, has_attachments, thread_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    account_id,
                    folder,
                    server_folder,
                    sender_name,
                    sender_address,
                    subject,
                    snippet,
                    received_at_ms,
                    unread as i64,
                    flagged as i64,
                    answered as i64,
                    forwarded as i64,
                    uid,
                    uidvalidity,
                    has_attachments as i64,
                    thread_id
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(conn.last_insert_rowid() as MessageId)
        }
    }

    /// Refresh a row's list snippet from a parsed body. Used when the full
    /// body arrives after the row was first listed (on-demand fetch repairs a
    /// snippet that an older sync stored as raw MIME/HTML).
    pub fn update_snippet(&self, id: MessageId, snippet: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE messages SET snippet = ?1 WHERE id = ?2",
            params![snippet, id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Recompute list snippets from stored bodies. Rows synced before the sync
    /// fetched full bodies carry raw MIME/HTML fragments in `snippet`; once a
    /// body is stored (an on-demand fetch, or a re-sync) this makes the
    /// preview match the content. Runs at startup; only rows with a stored
    /// body are touched, and unchanged snippets are left alone.
    pub fn repair_snippets_from_bodies(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT m.id, b.plain, b.html FROM messages m \
                 JOIN bodies b ON b.message_id = m.id \
                 WHERE b.plain != '' OR b.html != ''",
            )
            .map_err(|e| e.to_string())?;
        let rows: Vec<(MessageId, String, Option<String>)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        let mut updated = 0;
        for (id, plain, html) in rows {
            let snippet = crate::sanitize::snippet_from_bodies(&plain, html.as_deref());
            updated += conn
                .execute(
                    "UPDATE messages SET snippet = ?1 WHERE id = ?2 AND snippet != ?1",
                    params![snippet, id],
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(updated)
    }

    /// Decode RFC 2047 encoded-words in subjects stored before the sync
    /// started decoding them. Idempotent and cheap — only rows whose subject
    /// actually changes are written.
    pub fn repair_encoded_subjects(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, subject FROM messages WHERE subject LIKE '%=?%'")
            .map_err(|e| e.to_string())?;
        let rows: Vec<(MessageId, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        let mut updated = 0;
        for (id, subject) in rows {
            let decoded = crate::sanitize::decode_rfc2047(&subject);
            if decoded != subject {
                updated += conn
                    .execute(
                        "UPDATE messages SET subject = ?1 WHERE id = ?2",
                        params![decoded, id],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(updated)
    }

    /// Messages in an account that have never had a body fetched. These are
    /// the rows synced before the sync fetched full bodies; a backfill pass
    /// targets exactly them.
    pub fn list_messages_missing_bodies(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<PendingBody>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, folder, server_folder, uid, uidvalidity FROM messages \
                 WHERE account_id = ?1 AND uid IS NOT NULL \
                 AND NOT EXISTS (SELECT 1 FROM bodies WHERE bodies.message_id = messages.id) \
                 ORDER BY received_at_ms DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![account_id], |r| {
                Ok(PendingBody {
                    message_id: r.get(0)?,
                    folder: r.get(1)?,
                    server_folder: r.get(2)?,
                    uid: r.get(3)?,
                    uidvalidity: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
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

    /// Retention policy: delete every message older than `before_ms` along with
    /// its body, recipients, and attachments (the schema cascades), and
    /// best-effort remove the on-disk attachment files. Returns the number of
    /// messages deleted.
    pub fn prune_messages_before(&self, before_ms: i64) -> Result<u64, String> {
        let on_disk_ids: Vec<i64> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT a.id FROM attachments a \
                     JOIN messages m ON m.id = a.message_id \
                     WHERE m.received_at_ms < ?1 AND a.on_disk = 1",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![before_ms], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?
        };

        // Remove attachment files outside the DB lock.
        for id in on_disk_ids {
            let _ = std::fs::remove_dir_all(self.attachments_root.join(id.to_string()));
        }

        let conn = self.conn.lock().unwrap();
        let deleted = conn
            .execute(
                "DELETE FROM messages WHERE received_at_ms < ?1",
                params![before_ms],
            )
            .map_err(|e| e.to_string())?;
        Ok(deleted as u64)
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

        let mut thread_id = crate::threading::compute_thread_id(
            draft.in_reply_to.as_deref(),
            draft.references.as_deref(),
            &draft.subject,
        );
        // Join the reply to its parent's thread when the parent is local: the
        // draft's In-Reply-To is the parent Message-ID, and adopting the
        // parent's thread_id keeps the draft grouped with the conversation it
        // answers. The sync path computes thread ids from subject headers, so
        // a pure reference-derived id here would otherwise split the reply
        // into its own one-message thread.
        if let Some(reply_to) = draft.in_reply_to.as_deref() {
            let trimmed = reply_to.trim();
            let clean = trimmed.trim_matches(|c| c == '<' || c == '>');
            let parent_tid: Option<String> = conn
                .query_row(
                    "SELECT thread_id FROM messages WHERE message_id_header IN (?1, ?2) LIMIT 1",
                    params![trimmed, clean],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if let Some(tid) = parent_tid {
                thread_id = tid;
            }
        }
        let id = if let Some(existing) = draft.id {
            conn.execute(
                "UPDATE messages SET sender_name = ?1, sender_address = ?2, subject = ?3, \
                 snippet = ?4, received_at_ms = ?5, in_reply_to = ?6, references_header = ?7, \
                 thread_id = ?8 WHERE id = ?9",
                params![
                    sender,
                    sender,
                    draft.subject,
                    snippet,
                    received_at_ms,
                    draft.in_reply_to,
                    draft.references,
                    thread_id,
                    existing
                ],
            )
            .map_err(|e| e.to_string())?;
            existing
        } else {
            conn.execute(
                "INSERT INTO messages (account_id, folder, sender_name, sender_address, subject, \
                 snippet, received_at_ms, unread, flagged, has_attachments, in_reply_to, references_header, thread_id) \
                 VALUES (?1, 'Drafts', ?2, ?3, ?4, ?5, ?6, 0, 0, 0, ?7, ?8, ?9)",
                params![
                    draft.account_id,
                    sender,
                    sender,
                    draft.subject,
                    snippet,
                    received_at_ms,
                    draft.in_reply_to,
                    draft.references,
                    thread_id
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
        for (position, bcc) in draft.bcc.iter().enumerate() {
            conn.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, 'bcc', ?2, ?2, ?3)",
                params![id, bcc, position as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(id)
    }

    /// The most recent non-deleted draft, so an unfinished composer can be
    /// resumed after a restart (P1.5). None when there are no drafts.
    pub fn latest_draft(&self) -> Option<Draft> {
        let conn = self.conn.lock().unwrap();
        let (id, account_id, subject, in_reply_to, references): (
            MessageId,
            AccountId,
            String,
            Option<String>,
            Option<String>,
        ) = conn
            .query_row(
                "SELECT id, account_id, subject, in_reply_to, references_header \
                 FROM messages WHERE folder = 'Drafts' AND deleted_at_ms IS NULL \
                 ORDER BY received_at_ms DESC, id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .ok()?;
        let body: String = conn
            .query_row(
                "SELECT COALESCE(plain, '') FROM bodies WHERE message_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or_default();
        let mut to = Vec::new();
        let mut cc = Vec::new();
        let mut bcc = Vec::new();
        let mut stmt = conn
            .prepare(
                "SELECT kind, address FROM recipients \
                 WHERE message_id = ?1 ORDER BY position",
            )
            .ok()?;
        let rows = stmt.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).ok()?;
        for row in rows.flatten() {
            match row.0.as_str() {
                "to" => to.push(row.1),
                "cc" => cc.push(row.1),
                "bcc" => bcc.push(row.1),
                _ => {}
            }
        }
        Some(Draft {
            id: Some(id),
            account_id,
            to,
            cc,
            bcc,
            subject,
            body,
            in_reply_to,
            references,
        })
    }

    /// P1.6: assemble an RFC-5322 `.eml` from a stored message, for export
    /// without touching the SQLite schema. Rebuilds the headers the DB keeps
    /// (raw headers like the original Date are not retained) + a plain/HTML
    /// body; attachment bytes are noted, not embedded.
    pub fn eml_for_message(&self, id: MessageId) -> Option<String> {
        let detail = self.get_message(id)?;
        let row = detail.row;
        let addr = |name: &str, address: &str| -> String {
            if name.trim().is_empty() {
                address.to_string()
            } else {
                format!("{name} <{address}>")
            }
        };
        let date = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(row.received_at_ms)
            .map(|d| d.to_rfc2822())
            .unwrap_or_default();

        let mut out = String::new();
        out.push_str(&format!(
            "From: {}\r\n",
            addr(&row.sender_name, &row.sender_address)
        ));
        for r in &detail.to {
            out.push_str(&format!("To: {}\r\n", addr(&r.name, &r.address)));
        }
        for r in &detail.cc {
            out.push_str(&format!("Cc: {}\r\n", addr(&r.name, &r.address)));
        }
        for r in &detail.bcc {
            out.push_str(&format!("Bcc: {}\r\n", addr(&r.name, &r.address)));
        }
        out.push_str(&format!("Date: {date}\r\n"));
        if let Some(mid) = detail.message_id_header.as_deref() {
            out.push_str(&format!("Message-ID: {mid}\r\n"));
        }
        if let Some(ir) = detail.in_reply_to.as_deref() {
            out.push_str(&format!("In-Reply-To: {ir}\r\n"));
        }
        if let Some(rf) = detail.references.as_deref() {
            out.push_str(&format!("References: {rf}\r\n"));
        }
        out.push_str(&format!("Subject: {}\r\n", row.subject));
        out.push_str("MIME-Version: 1.0\r\n");

        let plain = detail.body.join("\n");
        let has_html = detail
            .body_html
            .as_deref()
            .map(|h| !h.is_empty())
            .unwrap_or(false);
        if has_html {
            out.push_str(
                "Content-Type: multipart/alternative; boundary=\"qeml\"\r\n\r\n\
                 --qeml\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n",
            );
            out.push_str(&plain);
            out.push_str("\r\n--qeml\r\nContent-Type: text/html; charset=utf-8\r\n\r\n");
            out.push_str(detail.body_html.as_deref().unwrap_or(""));
            out.push_str("\r\n--qeml--\r\n");
        } else {
            out.push_str("Content-Type: text/plain; charset=utf-8\r\n\r\n");
            out.push_str(&plain);
        }
        for a in &detail.attachments {
            out.push_str(&format!("\r\n(attachment: {})", a.filename));
        }
        Some(out)
    }

    /// P1.6: import an external message (from `.eml`/mbox) into a folder.
    /// Dedups by Message-ID — returns `Ok(false)` when the account already has
    /// that header. The sender is mirrored to both name+address columns (as
    /// `save_draft` does) and the thread id is derived from the subject.
    pub fn import_message(
        &self,
        account_id: AccountId,
        folder: &str,
        sender: &str,
        recipients: &[(String, String)], // (kind, address)
        subject: &str,
        body: &str,
        received_at_ms: i64,
        message_id_header: Option<&str>,
    ) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();
        if let Some(mid) = message_id_header {
            let existing: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM messages \
                     WHERE account_id = ?1 AND message_id_header = ?2",
                    params![account_id, mid],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if existing > 0 {
                return Ok(false);
            }
        }
        let snippet: String = body.chars().take(120).collect();
        let thread_id = crate::threading::compute_thread_id(None, None, subject);
        conn.execute(
            "INSERT INTO messages (account_id, folder, sender_name, sender_address, subject, \
             snippet, received_at_ms, unread, flagged, has_attachments, message_id_header, thread_id) \
             VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, 1, 0, 0, ?7, ?8)",
            params![
                account_id,
                folder,
                sender,
                subject,
                snippet,
                received_at_ms,
                message_id_header,
                thread_id
            ],
        )
        .map_err(|e| e.to_string())?;
        let id = conn.last_insert_rowid() as MessageId;
        conn.execute(
            "INSERT INTO bodies (message_id, plain) VALUES (?1, ?2)",
            params![id, body],
        )
        .map_err(|e| e.to_string())?;
        for (i, (kind, address)) in recipients.iter().enumerate() {
            conn.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (?1, ?2, ?3, ?3, ?4)",
                params![id, kind, address, i as i64],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(true)
    }

    /// All non-deleted drafts — a backup must include the user's unsent mail.
    pub fn list_drafts(&self) -> Vec<Draft> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, account_id, subject, in_reply_to, references_header \
             FROM messages WHERE folder = 'Drafts' AND deleted_at_ms IS NULL \
             ORDER BY received_at_ms DESC, id DESC",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?)))
            .map_err(|_| ());
        let rows: Vec<(i64, i64, String, Option<String>, Option<String>)> =
            match rows { Ok(r) => r.flatten().collect(), Err(_) => return Vec::new() };
        rows.into_iter()
            .filter_map(|(id, account_id, subject, in_reply_to, references)| {
                let body: String = conn
                    .query_row(
                        "SELECT COALESCE(plain, '') FROM bodies WHERE message_id = ?1",
                        params![id as MessageId],
                        |r| r.get(0),
                    )
                    .unwrap_or_default();
                let mut to = Vec::new();
                let mut cc = Vec::new();
                let mut bcc = Vec::new();
                let mut stmt = conn
                    .prepare(
                        "SELECT kind, address FROM recipients WHERE message_id = ?1 ORDER BY position",
                    )
                    .ok()?;
                let recs = stmt
                    .query_map(params![id as MessageId], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                    .ok()?;
                for row in recs.flatten() {
                    match row.0.as_str() {
                        "to" => to.push(row.1),
                        "cc" => cc.push(row.1),
                        "bcc" => bcc.push(row.1),
                        _ => {}
                    }
                }
                Some(Draft {
                    id: Some(id as MessageId),
                    account_id: account_id as AccountId,
                    to, cc, bcc,
                    subject,
                    body,
                    in_reply_to,
                    references,
                })
            })
            .collect()
    }

    /// The scheduled sends with their full payloads — send-later mail is
    /// local-only and would be lost without them (P1.6 backup).
    pub fn scheduled_for_backup(&self) -> Vec<(i64, AccountId, i64, String, String, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, account_id, send_at_ms, payload, draft, created_at_ms \
             FROM scheduled_messages ORDER BY id",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
            .map_err(|_| ());
        match rows { Ok(r) => r.flatten().collect(), Err(_) => Vec::new() }
    }

    /// P1.6 backup: a JSON bundle of the LOCAL-ONLY data (never OS-keychain
    /// secrets). Server-authoritative mail is excluded — it re-syncs.
    pub fn backup_local_data(&self) -> Result<serde_json::Value, String> {
        let events: Vec<CalendarEvent> = self
            .list_events(0, i64::MAX / 2)
            .into_iter()
            .filter(|e| e.calendar_source.is_none())
            .collect();
        let groups = self.list_contact_groups();
        let members: Vec<(i64, Vec<ContactSuggestion>)> = groups
            .iter()
            .map(|g| (g.id, self.contact_group_members(g.id)))
            .collect();
        let hidden: Vec<String> = {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare("SELECT address FROM hidden_recipients")
                .map_err(|e| e.to_string())?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| e.to_string())?;
            rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())?
        };
        Ok(serde_json::json!({
            "version": 1,
            "events": events,
            "tasks": self.list_tasks(None),
            "saved_searches": self.list_saved_searches(),
            "contact_groups": groups,
            "contact_group_members": members,
            "hidden_recipients": hidden,
            "subscriptions": self.list_subscriptions(),
            "drafts": self.list_drafts(),
            "scheduled": self.scheduled_for_backup(),
        }))
    }

    /// P1.6 restore: re-apply a backup's local-only rows (best-effort per
    /// table; ids are re-assigned where the source table auto-increments).
    pub fn restore_local_data(&self, value: &serde_json::Value) -> Result<(), String> {
        let v = value;
        // Local events (INSERT-OR-REPLACE by id).
        if let Some(events) = v.get("events").and_then(|x| x.as_array()) {
            for e in events {
                if let Ok(ev) = serde_json::from_value::<CalendarEvent>(e.clone()) {
                    let _ = self.restore_event(ev);
                }
            }
        }
        if let Some(tasks) = v.get("tasks").and_then(|x| x.as_array()) {
            for t in tasks {
                if let Ok(task) = serde_json::from_value::<CalendarTask>(t.clone()) {
                    let _ = self.create_task(task);
                }
            }
        }
        if let Some(searches) = v.get("saved_searches").and_then(|x| x.as_array()) {
            for s in searches {
                if let Ok(ss) = serde_json::from_value::<SavedSearch>(s.clone()) {
                    let _ = self.save_search(&ss.name, &ss.query);
                }
            }
        }
        if let Some(groups) = v.get("contact_groups").and_then(|x| x.as_array()) {
            for g in groups {
                if let Ok(cg) = serde_json::from_value::<ContactGroup>(g.clone()) {
                    if let Ok(gid) = self.create_contact_group(&cg.name) {
                        if let Some(members) = v.get("contact_group_members").and_then(|x| x.as_array()) {
                            for m in members {
                                if let Ok((old_id, suggestions)) = serde_json::from_value::<(i64, Vec<ContactSuggestion>)>(m.clone()) {
                                    if old_id == cg.id {
                                        for s in suggestions {
                                            let _ = self.add_contact_to_group(gid, &s.address);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(hidden) = v.get("hidden_recipients").and_then(|x| x.as_array()) {
            for h in hidden {
                if let Some(addr) = h.as_str() {
                    let _ = self.hide_recipient(addr);
                }
            }
        }
        if let Some(subs) = v.get("subscriptions").and_then(|x| x.as_array()) {
            for s in subs {
                if let Ok(sub) = serde_json::from_value::<CalendarSubscription>(s.clone()) {
                    let _ = self.create_subscription(&sub.name, &sub.url, &sub.color, sub.refresh_interval_min);
                }
            }
        }
        if let Some(drafts) = v.get("drafts").and_then(|x| x.as_array()) {
            for d in drafts {
                if let Ok(draft) = serde_json::from_value::<Draft>(d.clone()) {
                    let _ = self.save_draft(&draft);
                }
            }
        }
        if let Some(scheduled) = v.get("scheduled").and_then(|x| x.as_array()) {
            for s in scheduled {
                if let Ok((_id, account_id, send_at_ms, payload, draft, _created)) =
                    serde_json::from_value::<(i64, AccountId, i64, String, String, i64)>(s.clone())
                {
                    let _ = self.schedule_message(account_id, send_at_ms, &payload, &draft);
                }
            }
        }
        Ok(())
    }

    /// Seed the demo content (Epic 3.4) into SQLite.
    pub fn seed_demo(&self, attachments_root: &Path) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        for account in demo_accounts() {
            tx.execute(
                "INSERT INTO accounts (id, address, protocol, sync_mode, color, local_bytes, \
                 connected, server, port, tls, folder_count, last_error) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                    account.folder_count,
                    account.last_error
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        for message in demo_messages() {
            let thread_id = crate::threading::compute_thread_id(None, None, &message.subject);
            tx.execute(
                "INSERT INTO messages (id, account_id, folder, sender_name, sender_address, \
                 subject, snippet, received_at_ms, unread, flagged, answered, forwarded, has_attachments, thread_id) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
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
                    message.answered as i64,
                    message.forwarded as i64,
                    !message.attachments.is_empty() as i64,
                    thread_id
                ],
            )
            .map_err(|e| e.to_string())?;

            tx.execute(
                "INSERT INTO bodies (message_id, plain, html, list_unsubscribe, list_unsubscribe_post) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    message.id,
                    message.body.join(PARAGRAPH_SEP),
                    message.body_html,
                    message.list_unsubscribe,
                    message.list_unsubscribe_post,
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
                 location, notes, alarm_minutes_before, timezone, travel_time_minutes) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    event.id,
                    event.account_id,
                    event.title,
                    event.start_ms,
                    event.end_ms,
                    event.all_day as i64,
                    event.location,
                    event.notes,
                    event.alarm_minutes_before,
                    event.timezone,
                    event.travel_time_minutes,
                ],
            )
            .map_err(|e| e.to_string())?;
        }

        for task in crate::demo::demo_tasks() {
            tx.execute(
                "INSERT INTO tasks (id, account_id, title, due_at_ms, completed_at_ms, priority) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    task.id,
                    task.account_id,
                    task.title,
                    task.due_at_ms,
                    task.completed_at_ms,
                    task.priority,
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
        let conn = store.conn.lock().unwrap();
        // user_version tracks the SQL migrations only (1..N); the thread_id
        // code migration is tracked separately in `meta`.
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        let code_done: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key = 'code_migration_thread_id'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap();
        assert!(code_done);
        drop(conn);
        // Re-running is a no-op (the version is already current).
        store.migrate().unwrap();
    }

    /// Simulate the version-collision bug: a database stamped past SQL migration
    /// 14 by an older code migration (user_version=15) but missing the events
    /// source-calendar columns. Reopening must repair the columns and reconcile
    /// the version so future migrations apply.
    #[test]
    fn migrate_repairs_skipped_events_columns() {
        let dir = std::env::temp_dir().join(format!("quill-mig-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stuck.sqlite");
        let _ = std::fs::remove_file(&path);

        {
            // Build the exact schema an older build left behind: apply SQL
            // migrations 1..N-5 (the pre-calendar-source set) directly, then
            // stamp user_version past the calendar-source migration (15, so
            // migrations 14+ look "already run") the way the old thread_id
            // code migration did — the events source columns were never added.
            let conn = rusqlite::Connection::open(&path).unwrap();
            for sql in MIGRATIONS.iter().take(MIGRATIONS.len() - 5) {
                conn.execute_batch(sql).expect("apply old migrations");
            }
            conn.pragma_update(None, "user_version", (MIGRATIONS.len() - 3) as i64)
                .unwrap();
        }

        let store = SqliteStore::open(&path).unwrap();
        let conn = store.conn.lock().unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('events')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for col in ["calendar_source", "calendar_name", "calendar_color"] {
            assert!(cols.contains(&col.to_string()), "missing repaired column {col}");
        }
        drop(conn);
        // list_events now works against the repaired schema (this re-locks the
        // store's connection, so the assertion must run without `conn` held).
        assert!(store.list_events(0, i64::MAX / 2).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Same version-collision repair, for migration 16's `synced_folders`
    /// table: a database stamped past it must still get the table on reopen.
    #[test]
    fn migrate_repairs_skipped_synced_folders() {
        let dir = std::env::temp_dir().join(format!("quill-mig-sf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stuck.sqlite");
        let _ = std::fs::remove_file(&path);

        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            for sql in MIGRATIONS.iter().take(MIGRATIONS.len() - 1) {
                conn.execute_batch(sql).expect("apply old migrations");
            }
            conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64)
                .unwrap();
        }

        let store = SqliteStore::open(&path).unwrap();
        let conn = store.conn.lock().unwrap();
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM synced_folders", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Migrations 17–18 (P1.1) add the messages soft-delete and snooze
    /// columns; a database stamped past them gets them repaired on reopen.
    #[test]
    fn migrate_repairs_skipped_message_columns() {
        let dir = std::env::temp_dir().join(format!("quill-mig-msg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stuck.sqlite");
        let _ = std::fs::remove_file(&path);

        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            for sql in MIGRATIONS.iter().take(MIGRATIONS.len() - 2) {
                conn.execute_batch(sql).expect("apply old migrations");
            }
            conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64)
                .unwrap();
        }

        let store = SqliteStore::open(&path).unwrap();
        let conn = store.conn.lock().unwrap();
        let cols: Vec<String> = conn
            .prepare("SELECT name FROM pragma_table_info('messages')")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for col in ["deleted_at_ms", "snoozed_until_ms"] {
            assert!(
                cols.contains(&col.to_string()),
                "missing repaired message column {col}"
            );
        }
        let version: i64 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Folder-sync selection CRUD (P0.2): unconfigured → sync everything; the
    /// enabled set drives the sync engine; upsert never re-enables a folder
    /// the user turned off.
    #[test]
    fn synced_folders_crud_and_filter() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();

        // Unconfigured → sync everything.
        assert_eq!(store.enabled_folder_set(acc.id), None);

        let folders = vec![
            SyncedFolder {
                account_id: acc.id,
                server_name: "INBOX".into(),
                local_name: "Inbox".into(),
                kind: FolderKind::Inbox,
                enabled: true,
            },
            SyncedFolder {
                account_id: acc.id,
                server_name: "Drafts".into(),
                local_name: "Drafts".into(),
                kind: FolderKind::Drafts,
                enabled: false,
            },
            SyncedFolder {
                account_id: acc.id,
                server_name: "Sent".into(),
                local_name: "Sent".into(),
                kind: FolderKind::Sent,
                enabled: true,
            },
        ];
        store.set_synced_folders(acc.id, &folders).unwrap();
        let enabled = store.enabled_folder_set(acc.id).unwrap();
        assert!(enabled.contains("INBOX"));
        assert!(!enabled.contains("Drafts"));
        assert!(enabled.contains("Sent"));
        assert_eq!(store.synced_folders(acc.id).len(), 3);

        // Replacing with an empty set clears the configuration → sync all again.
        store.set_synced_folders(acc.id, &[]).unwrap();
        assert_eq!(store.synced_folders(acc.id).len(), 0);
        assert_eq!(store.enabled_folder_set(acc.id), None);

        // Upsert adds only missing rows and never flips enabled back on.
        store
            .set_synced_folders(
                acc.id,
                &[SyncedFolder {
                    account_id: acc.id,
                    server_name: "INBOX".into(),
                    local_name: "Inbox".into(),
                    kind: FolderKind::Inbox,
                    enabled: false,
                }],
            )
            .unwrap();
        store
            .upsert_synced_folders(
                acc.id,
                &[
                    SyncedFolder {
                        account_id: acc.id,
                        server_name: "INBOX".into(),
                        local_name: "Inbox".into(),
                        kind: FolderKind::Inbox,
                        enabled: true, // existing row — must NOT re-enable
                    },
                    SyncedFolder {
                        account_id: acc.id,
                        server_name: "Archive".into(),
                        local_name: "Archive".into(),
                        kind: FolderKind::Archive,
                        enabled: true, // new row — added
                    },
                ],
            )
            .unwrap();
        let rows = store.synced_folders(acc.id);
        assert_eq!(rows.len(), 2);
        let inbox = rows.iter().find(|f| f.server_name == "INBOX").unwrap();
        assert!(!inbox.enabled, "upsert must not flip enabled=false back on");
        let archive = rows.iter().find(|f| f.server_name == "Archive").unwrap();
        assert!(archive.enabled);
    }

    /// Queued-action and draft counts feed the account-removal confirm.
    #[test]
    fn pending_action_and_draft_counts() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        assert_eq!(store.pending_action_count(acc.id), 0);
        assert_eq!(store.draft_count(acc.id), 0);

        store
            .enqueue_action(acc.id, ActionType::MarkRead, "Inbox", Some(1), None)
            .unwrap();
        store
            .enqueue_action(acc.id, ActionType::Send, "Outbox", None, Some("{}"))
            .unwrap();
        assert_eq!(store.pending_action_count(acc.id), 2);

        store
            .save_draft(&Draft {
                id: None,
                account_id: acc.id,
                to: vec![],
                cc: vec![],
                bcc: vec![],
                subject: "Draft".into(),
                body: "body".into(),
                in_reply_to: None,
                references: None,
            })
            .unwrap();
        assert_eq!(store.draft_count(acc.id), 1);
    }

    /// Removing an account must also remove its on-disk attachment files —
    /// the DB rows cascade, the files don't.
    #[test]
    fn delete_attachments_removes_on_disk_files() {
        let mut store = SqliteStore::open_in_memory().unwrap();
        let dir = std::env::temp_dir().join(format!("quill-att-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        store.set_attachments_root(dir.clone());

        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO messages (id, account_id, folder, sender_name, sender_address, subject, snippet, received_at_ms, unread, flagged, has_attachments) \
                 VALUES (1, ?1, 'Inbox', 'S', 's@e.com', 'Subj', 'snip', 0, 1, 0, 1)",
                params![acc.id],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO attachments (id, message_id, filename, size_bytes, on_disk) \
                 VALUES (1, 1, 'a.pdf', 10, 1)",
                [],
            )
            .unwrap();
        }
        let att_dir = dir.join("1");
        std::fs::create_dir_all(&att_dir).unwrap();
        std::fs::write(att_dir.join("a.pdf"), b"x").unwrap();
        assert!(att_dir.exists());

        store.delete_attachments_for_account(acc.id);
        assert!(!att_dir.exists(), "attachment files must be deleted on removal");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// P1.1: soft delete hides a message from the list but keeps it resolvable
    /// by id (the reading pane doesn't blank), and restore brings it back.
    #[test]
    fn soft_delete_hides_and_restore_shows() {
        let store = seeded();
        let query = MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit: 500,
            threaded: false,
        };
        let inbox_ids: Vec<u32> = store
            .page_messages(&query)
            .items
            .iter()
            .map(|r| r.id)
            .collect();
        assert!(!inbox_ids.is_empty());
        let target = inbox_ids[0];

        store.delete(target).unwrap();
        assert!(
            !store
                .page_messages(&query)
                .items
                .iter()
                .any(|r| r.id == target),
            "soft-deleted message must leave the list"
        );
        assert!(store.get_message(target).is_some());

        store.restore_message(target).unwrap();
        assert!(
            store.page_messages(&query).items.iter().any(|r| r.id == target),
            "restored message must return to the list"
        );
    }

    /// P1.1: bulk actions update rows AND enqueue server actions, and undo's
    /// cancel removes the queued action.
    #[test]
    fn bulk_delete_enqueues_and_cancel_removes() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO messages (id, account_id, folder, server_folder, sender_name, \
                 sender_address, subject, snippet, received_at_ms, unread, flagged, uid, has_attachments) \
                 VALUES (1, ?1, 'Inbox', 'INBOX', 'S', 's@e.com', 'Subj', 'snip', 0, 1, 0, 1001, 0)",
                params![acc.id],
            )
            .unwrap();
        }

        let (ok, errors) = store.bulk_delete(&[1]);
        assert_eq!(ok, 1);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            store.pending_action_count(acc.id),
            1,
            "bulk delete must enqueue the server Delete"
        );

        let cancelled = store
            .cancel_pending_actions(acc.id, "INBOX", Some(1001))
            .unwrap();
        assert_eq!(cancelled, 1);
        assert_eq!(store.pending_action_count(acc.id), 0);

        // A second delete of the (now soft-deleted) message is a partial failure.
        let (ok, errors) = store.bulk_delete(&[1]);
        assert_eq!(ok, 0);
        assert_eq!(errors.len(), 1);
    }

    /// P1.1: bulk read/star/archive/move report per-id results and skip
    /// already-hidden rows.
    #[test]
    fn bulk_triage_actions() {
        let store = seeded();
        let query = MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit: 500,
            threaded: false,
        };
        let ids: Vec<u32> = store
            .page_messages(&query)
            .items
            .iter()
            .take(3)
            .map(|r| r.id)
            .collect();
        assert_eq!(ids.len(), 3);

        let (ok, errors) = store.bulk_set_read(&ids, false);
        assert_eq!(ok, 3);
        assert!(errors.is_empty(), "{errors:?}");

        let (ok, _) = store.bulk_set_flagged(&ids, true);
        assert_eq!(ok, 3);

        let (ok, _) = store.bulk_archive(&ids);
        assert_eq!(ok, 3);

        let (ok, _) = store.bulk_move(&ids, "Archive");
        assert_eq!(ok, 3);

        let (ok, _) = store.bulk_mark_junk(&ids, true);
        assert_eq!(ok, 3);

        // A bogus id is reported, not panicked on.
        let (ok, errors) = store.bulk_archive(&[99999]);
        assert_eq!(ok, 0);
        assert_eq!(errors.len(), 1);
    }

    /// P1.1 snooze: a snoozed message leaves its folder, shows in the Snoozed
    /// view, and returns to its folder once the wake time passes.
    #[test]
    fn snooze_hides_then_returns() {
        let store = seeded();
        let inbox = MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit: 500,
            threaded: false,
        };
        let snoozed = MessageQuery {
            folder: Some("Snoozed".into()),
            account_id: None,
            offset: 0,
            limit: 500,
            threaded: false,
        };
        let target = store.page_messages(&inbox).items[0].id;

        let wake = now_ms() + 3600_000;
        store.set_snoozed(&[target], wake).unwrap();
        assert!(
            !store.page_messages(&inbox).items.iter().any(|r| r.id == target),
            "snoozed message must leave the inbox"
        );
        assert!(
            store
                .page_messages(&snoozed)
                .items
                .iter()
                .any(|r| r.id == target),
            "snoozed message must appear in the Snoozed view"
        );

        // Before the wake time it stays put.
        assert_eq!(store.clear_due_snoozes(wake - 1).unwrap(), 0);
        // At/after the wake time it returns.
        let returned = store.clear_due_snoozes(wake).unwrap();
        assert!(returned >= 1);
        assert!(
            store.page_messages(&inbox).items.iter().any(|r| r.id == target),
            "returned message must reappear in the inbox"
        );
        assert!(
            !store
                .page_messages(&snoozed)
                .items
                .iter()
                .any(|r| r.id == target),
            "returned message must leave the Snoozed view"
        );
    }

    /// P1.1 send-later: schedule, list (display fields only), due rows for the
    /// flusher, and cancel.
    #[test]
    fn scheduled_send_crud() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();

        let payload = r#"{"account_id":1,"to":["b@example.com"],"subject":"Later"}"#;
        let id = store
            .schedule_message(acc.id, now_ms() + 60000, payload, r#"{"draft":true}"#)
            .unwrap();
        assert!(id > 0);

        let listed = store.list_scheduled();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].subject, "Later");
        assert_eq!(listed[0].to, vec!["b@example.com"]);
        assert!(listed[0].draft.contains("draft"));

        // Not due yet.
        assert!(store.due_scheduled(now_ms()).is_empty());
        // Due once the time passes.
        let due = store.due_scheduled(now_ms() + 60001);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].1, acc.id);
        assert!(due[0].3.contains("Later"), "flusher must read the payload");

        store.cancel_scheduled(id).unwrap();
        assert!(store.list_scheduled().is_empty());
    }

    /// Insert a message + recipient row for the contact tests.
    fn insert_recipient(
        store: &SqliteStore,
        account_id: AccountId,
        msg_id: MessageId,
        recv_ms: i64,
        name: &str,
        address: &str,
    ) {
        // The sender is the account's own address — suggestions must exclude
        // it (the account-address filter below keeps the test's history clean).
        let my_address = store
            .accounts()
            .iter()
            .find(|a| a.id == account_id)
            .map(|a| a.address.clone())
            .unwrap_or_default();
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (id, account_id, folder, sender_name, sender_address, subject, \
             snippet, received_at_ms, unread, flagged, has_attachments) \
             VALUES (?1, ?2, 'Inbox', 'Me', ?4, 'S', 'snip', ?3, 0, 0, 0)",
            params![msg_id, account_id, recv_ms, my_address],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO recipients (message_id, kind, name, address, position) \
             VALUES (?1, 'to', ?2, ?3, 0)",
            params![msg_id, name, address],
        )
        .unwrap();
    }

    /// P1.2: suggestions rank by frequency then recency, dedup by
    /// lower(address), and a hidden address is suppressed.
    #[test]
    fn suggestion_ranking_dedup_and_hide() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        insert_recipient(&store, acc.id, 1, 1000, "Bob", "bob@example.com");
        insert_recipient(&store, acc.id, 2, 2000, "Bob", "bob@example.com");
        insert_recipient(&store, acc.id, 3, 3000, "Bob", "bob@example.com");
        insert_recipient(&store, acc.id, 4, 5000, "Alice", "alice@example.com");
        insert_recipient(&store, acc.id, 5, 4000, "Bob", "BOB@example.com"); // case-duplicate

        // Case-insensitive dedup: bob@example.com + BOB@example.com → one row.
        let sugs = store.suggest_recipients("bob", 10);
        assert_eq!(sugs.len(), 1, "suggestions dedup by lower(address)");
        assert_eq!(sugs[0].address.to_lowercase(), "bob@example.com");
        assert_eq!(sugs[0].use_count, 4);
        assert_eq!(sugs[0].last_used_at_ms, 4000);

        // Recency ranks Alice above Bob in "recent".
        let recents = store.recent_recipients(10);
        assert_eq!(recents[0].address, "alice@example.com");
        assert_eq!(recents[1].address.to_lowercase(), "bob@example.com");

        // Hiding one casing suppresses every casing of the address.
        store.hide_recipient("BOB@example.com").unwrap();
        assert!(
            store.suggest_recipients("bob", 10).is_empty(),
            "hidden recipient must not be suggested"
        );
        assert!(
            store
                .recent_recipients(10)
                .iter()
                .all(|s| s.address.to_lowercase() != "bob@example.com")
        );

        // LIKE wildcards in the query are literal.
        let sugs = store.suggest_recipients("%", 10);
        assert!(
            sugs.iter().all(|s| s.address != "alice@example.com"),
            "a literal % must not wildcard"
        );
        assert!(store.suggest_recipients("_", 10).is_empty());
    }

    /// P1.2: contact groups CRUD, cascade on delete, members joined with names.
    #[test]
    fn contact_group_crud() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        insert_recipient(&store, acc.id, 1, 1000, "Alice", "alice@example.com");
        insert_recipient(&store, acc.id, 2, 2000, "Bob", "bob@example.com");

        let gid = store.create_contact_group("Team").unwrap();
        assert!(gid > 0);
        store.add_contact_to_group(gid, "alice@example.com").unwrap();
        store.add_contact_to_group(gid, "bob@example.com").unwrap();
        store.add_contact_to_group(gid, "alice@example.com").unwrap(); // dedup

        let members = store.contact_group_members(gid);
        assert_eq!(members.len(), 2);
        let alice = members.iter().find(|m| m.address == "alice@example.com").unwrap();
        assert_eq!(alice.name, "Alice", "member names join from history");

        let groups = store.list_contact_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Team");

        store.remove_contact_from_group(gid, "bob@example.com").unwrap();
        assert_eq!(store.contact_group_members(gid).len(), 1);

        store.delete_contact_group(gid).unwrap();
        assert!(store.list_contact_groups().is_empty());
        assert!(
            store.contact_group_members(gid).is_empty(),
            "members cascade on group delete"
        );

        // Duplicate group names are rejected.
        store.create_contact_group("Team").unwrap();
        assert!(store.create_contact_group("Team").is_err());
    }

    /// P1.5: `latest_draft` returns the most recent draft for resuming an
    /// unfinished composer.
    #[test]
    fn latest_draft_resumes_most_recent() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.latest_draft().is_none());

        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        let first = store
            .save_draft(&Draft {
                id: None,
                account_id: acc.id,
                to: vec!["b@example.com".into()],
                cc: vec![],
                bcc: vec![],
                subject: "Older".into(),
                body: "one".into(),
                in_reply_to: None,
                references: None,
            })
            .unwrap();
        store
            .save_draft(&Draft {
                id: None,
                account_id: acc.id,
                to: vec!["c@example.com".into()],
                cc: vec![],
                bcc: vec![],
                subject: "Newer".into(),
                body: "two".into(),
                in_reply_to: None,
                references: None,
            })
            .unwrap();

        let d = store.latest_draft().unwrap();
        assert_eq!(d.subject, "Newer");
        assert_eq!(d.to, vec!["c@example.com"]);
        assert_eq!(d.body, "two");
        assert_ne!(d.id, Some(first));

        // A soft-deleted draft is not resumed.
        store.delete(d.id.unwrap()).unwrap();
        assert_eq!(store.latest_draft().unwrap().subject, "Older");
    }

    /// P1.6: an `.eml` export round-trips the stored headers, and import dedups
    /// by Message-ID.
    #[test]
    fn eml_export_and_import_dedup() {
        let store = seeded();
        let eml = store.eml_for_message(1).unwrap();
        assert!(eml.contains("Subject:"));
        assert!(eml.contains("From:"));
        assert!(eml.contains("Date:"));
        assert!(eml.contains("MIME-Version: 1.0"));

        let imported = store
            .import_message(
                1,
                "Inbox",
                "Importer <imp@example.com>",
                &[("to".into(), "me@example.com".into())],
                "Imported message",
                "hello",
                1000,
                Some("import-1@example.com"),
            )
            .unwrap();
        assert!(imported);
        let dup = store
            .import_message(
                1,
                "Inbox",
                "Importer",
                &[],
                "Imported message",
                "hello",
                1000,
                Some("import-1@example.com"),
            )
            .unwrap();
        assert!(!dup, "Message-ID dedup skips a re-import");
    }

    /// P1.6: a backup captures local-only data (a local event + a saved
    /// search) and restore re-applies it.
    #[test]
    fn backup_restore_local_data() {
        let store = seeded();
        let local_event = store
            .create_event(CalendarEvent {
                id: 0,
                account_id: 1,
                title: "Local only".into(),
                start_ms: 5000,
                end_ms: 6000,
                all_day: false,
                location: None,
                notes: None,
                alarm_minutes_before: None,
                timezone: None,
                travel_time_minutes: None,
                calendar_source: None,
                calendar_name: None,
                calendar_color: None,
                color: None,
            })
            .unwrap();
        store.save_search("Unread", "is:unread").unwrap();

        let backup = store.backup_local_data().unwrap();
        assert!(backup.get("events").is_some());
        assert!(backup.get("saved_searches").is_some());

        // Wipe the local-only rows, then restore.
        store.delete_event(local_event.id).unwrap();
        let search_id = store.list_saved_searches()[0].id;
        store.delete_saved_search(search_id).unwrap();
        assert!(!store.list_events(0, i64::MAX / 2).iter().any(|e| e.id == local_event.id));

        store.restore_local_data(&backup).unwrap();
        assert!(store
            .list_events(0, i64::MAX / 2)
            .iter()
            .any(|e| e.id == local_event.id));
        assert!(store.list_saved_searches().iter().any(|s| s.name == "Unread"));
    }

    /// Downgrade safety (E2.2): a database stamped by a newer app version must
    /// refuse to open, so an older app never writes into a schema it doesn't
    /// understand.
    #[test]
    fn migrate_refuses_newer_database() {
        let dir = std::env::temp_dir().join(format!("quill-newdb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("new.sqlite");
        let _ = std::fs::remove_file(&path);
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64 + 1)
                .unwrap();
        }
        match SqliteStore::open(&path) {
            Ok(_) => panic!("expected a newer-database error"),
            Err(err) => assert!(
                err.contains("newer than this app supports"),
                "unexpected error: {err}"
            ),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// E2.2 — a pre-migration backup is written next to the DB before the first
    /// migration runs, so a failed upgrade can be rolled back.
    #[test]
    fn migrate_takes_backup_before_upgrading() {
        let dir = std::env::temp_dir().join(format!("quill-bak-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bak.sqlite");
        let _ = std::fs::remove_file(&path);

        // A brand-new DB (version 0) opens by running all migrations; migrate()
        // backs it up first.
        drop(SqliteStore::open(&path).unwrap());
        let backups: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".bak-"))
            .collect();
        assert!(!backups.is_empty(), "expected a pre-migration backup, got {backups:?}");
        let _ = std::fs::remove_dir_all(&dir);
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
            threaded: false,
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
                threaded: false,
            })
            .items[0]
            .id;

        store.set_read(id, false).unwrap();
        assert!(!store.get_message(id).unwrap().row.unread);
        store.archive(id).unwrap();
        assert_eq!(store.get_message(id).unwrap().row.folder, "Archive");
        store.delete(id).unwrap();
        // P1.1: delete is soft — hidden from the list, still resolvable by id
        // so the action can be undone.
        assert!(store.get_message(id).is_some());
        assert!(
            !store
                .page_messages(&MessageQuery {
                    folder: Some("Inbox".into()),
                    account_id: None,
                    offset: 0,
                    limit: 500,
                    threaded: false,
                })
                .items
                .iter()
                .any(|r| r.id == id)
        );
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
        let account = store.create_account(&info, "#3b5bdb".into()).unwrap();
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
        let event = store
            .create_event(CalendarEvent {
                id: 0,
                account_id: 1,
                title: "Meet".into(),
                start_ms: 1000,
                end_ms: 2000,
                all_day: false,
                location: None,
                notes: None,
                alarm_minutes_before: Some(15),
                timezone: Some("America/New_York".into()),
                travel_time_minutes: Some(30),
                calendar_source: Some("personal@example.com".into()),
                calendar_name: Some("Personal".into()),
                calendar_color: Some("#1F6FEB".into()),
                color: Some("#e8590c".into()),
            })
            .unwrap();
        assert!(event.id > 0);
        assert_eq!(store.list_events(0, 3000).len(), 1);
        assert_eq!(store.list_events(0, 3000)[0].alarm_minutes_before, Some(15));
        assert_eq!(store.list_events(0, 3000)[0].timezone.as_deref(), Some("America/New_York"));
        assert_eq!(store.list_events(0, 3000)[0].travel_time_minutes, Some(30));
        assert_eq!(
            store.list_events(0, 3000)[0].color.as_deref(),
            Some("#e8590c"),
            "per-event color round-trips"
        );
        assert_eq!(
            store.list_events(0, 3000)[0].calendar_source.as_deref(),
            Some("personal@example.com")
        );

        // The source calendar is exposed via list_calendar_sources.
        let sources = store.list_calendar_sources();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source, "personal@example.com");
        assert_eq!(sources[0].name, "Personal");
        assert_eq!(sources[0].color, "#1F6FEB");

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
                alarm_minutes_before: Some(30),
                timezone: Some("Europe/London".into()),
                travel_time_minutes: Some(45),
                calendar_source: None,
                calendar_name: None,
                calendar_color: None,
                color: Some("#0f766e".into()),
            })
            .unwrap();
        assert_eq!(store.list_events(0, 3000)[0].title, "Meet updated");
        assert_eq!(
            store.list_events(0, 3000)[0].color.as_deref(),
            Some("#0f766e"),
            "color updates too"
        );
        assert_eq!(store.list_events(0, 3000)[0].alarm_minutes_before, Some(30));
        assert_eq!(store.list_events(0, 3000)[0].timezone.as_deref(), Some("Europe/London"));
        assert_eq!(store.list_events(0, 3000)[0].travel_time_minutes, Some(45));

        store.delete_event(event.id).unwrap();
        assert!(store.list_events(0, 3000).is_empty());
    }

    /// P1.4: restore_event re-creates a deleted event and overwrites an edited
    /// one; duplicate_event clones with a fresh id + "(copy)" title.
    #[test]
    fn event_restore_and_duplicate() {
        let store = seeded();
        // The demo has events; use the first as the subject.
        let events = store.list_events(0, i64::MAX / 2);
        assert!(!events.is_empty());
        let original = events[0].clone();

        // Undo-delete: delete, then restore the captured event.
        store.delete_event(original.id).unwrap();
        assert!(
            store
                .list_events(0, i64::MAX / 2)
                .iter()
                .all(|e| e.id != original.id)
        );
        store.restore_event(original.clone()).unwrap();
        assert!(
            store
                .list_events(0, i64::MAX / 2)
                .iter()
                .any(|e| e.id == original.id && e.title == original.title)
        );

        // Undo-edit: overwrite with the pre-edit snapshot.
        let edited = CalendarEvent {
            title: "Moved elsewhere".into(),
            start_ms: original.start_ms + 60000,
            ..original.clone()
        };
        store.update_event(edited).unwrap();
        store.restore_event(original.clone()).unwrap();
        let back = store
            .list_events(0, i64::MAX / 2)
            .into_iter()
            .find(|e| e.id == original.id)
            .unwrap();
        assert_eq!(back.title, original.title);
        assert_eq!(back.start_ms, original.start_ms);

        // Duplicate: a fresh id, same times, "(copy)" title.
        let dup = store.duplicate_event(original.id).unwrap();
        assert!(dup.id != original.id);
        assert_eq!(dup.start_ms, original.start_ms);
        assert!(dup.title.contains("(copy)"));
        assert_eq!(dup.calendar_source, original.calendar_source);
        assert_eq!(dup.color, original.color);
    }

    #[test]
    fn tasks_crud() {
        let store = seeded();
        let tasks = store.list_tasks(None);
        assert_eq!(tasks.len(), 3);

        let created = store
            .create_task(CalendarTask {
                id: 0,
                account_id: 1,
                title: "Follow up with client".into(),
                due_at_ms: Some(50000),
                completed_at_ms: None,
                priority: Some(1),
            })
            .unwrap();
        assert!(created.id > 0);
        assert_eq!(store.list_tasks(Some(1)).len(), 3);

        // Toggle task completion
        let toggled = store.toggle_task(created.id).unwrap();
        assert!(toggled.completed_at_ms.is_some());

        // Delete task
        store.delete_task(created.id).unwrap();
        assert_eq!(store.list_tasks(Some(1)).len(), 2);
    }

    #[test]
    fn sync_state_and_fetched_messages() {
        let store = seeded();

        // Incremental watermark round-trips with highestmodseq.
        store.set_sync_state(1, "Inbox", 12345, 101, 500).unwrap();
        assert_eq!(store.get_sync_state(1, "Inbox"), (12345, 101, 500));

        // Fetched envelopes upsert keyed by (account, folder, uid, uidvalidity).
        store
            .upsert_fetched_message(
                1,
                "Inbox",
                "INBOX",
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
                false,
                false,
            )
            .unwrap();
        store
            .upsert_fetched_message(
                1,
                "Inbox",
                "INBOX",
                101,
                12345,
                "Two",
                "two@example.com",
                "Subject 2",
                "s",
                1_700_000_000_001,
                false,
                true,
                true,
                false,
                false,
            )
            .unwrap();
        assert_eq!(store.message_uids(1, "Inbox").len(), 2);

        // Update flags by uid
        store
            .update_message_flags_by_uid(1, "Inbox", 100, false, true, true, false)
            .unwrap();
        let msg = store
            .page_messages(&MessageQuery {
                folder: Some("Inbox".into()),
                account_id: Some(1),
                offset: 0,
                limit: 50,
                threaded: false,
            })
            .items
            .into_iter()
            .find(|m| m.subject == "New subject")
            .unwrap();
        assert!(!msg.unread);
        assert!(msg.flagged);
        assert!(msg.answered);
        assert!(!msg.forwarded);

        // A full refetch (uidvalidity changed) keeps only the server's uids.
        store.delete_messages_not_in(1, "Inbox", &[101]).unwrap();
        assert_eq!(store.message_uids(1, "Inbox"), vec![101]);

        let page = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: Some(1),
            offset: 0,
            limit: 50,
            threaded: false,
        });
        assert!(page.items.iter().any(|r| r.subject == "Subject 2"));
    }

    #[test]
    fn action_queue_operations() {
        let store = seeded();
        let action_id = store
            .enqueue_action(
                1,
                ActionType::MarkRead,
                "Inbox",
                Some(101),
                Some("{\"seen\":true}"),
            )
            .unwrap();
        assert!(action_id > 0);

        let pending = store.peek_pending_actions(1);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].action_type, ActionType::MarkRead);
        assert_eq!(pending[0].uid, Some(101));
        assert_eq!(pending[0].retries, 0);

        store.increment_action_retry(action_id).unwrap();
        let pending = store.peek_pending_actions(1);
        assert_eq!(pending[0].retries, 1);

        store.remove_action(action_id).unwrap();
        assert!(store.peek_pending_actions(1).is_empty());
    }

    #[test]
    fn account_error_and_connection_state() {
        let store = seeded();
        store
            .set_account_connected(1, false, Some("Connection timed out"))
            .unwrap();
        let accounts = store.accounts();
        let acct = accounts.iter().find(|a| a.id == 1).unwrap();
        assert!(!acct.connected);
        assert_eq!(acct.last_error.as_deref(), Some("Connection timed out"));

        store.set_account_connected(1, true, None).unwrap();
        let accounts = store.accounts();
        let acct = accounts.iter().find(|a| a.id == 1).unwrap();
        assert!(acct.connected);
        assert!(acct.last_error.is_none());
    }

    #[test]
    fn save_body_and_attachments_on_demand() {
        let store = seeded();
        let page = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit: 1,
            threaded: false,
        });
        let msg_id = page.items[0].id;

        store
            .save_message_body_and_attachments(
                msg_id,
                "Updated body text",
                Some("<p>Updated body html</p>"),
                &[Recipient {
                    name: "Recipient".into(),
                    address: "rec@example.com".into(),
                }],
                &[],
                &[Recipient {
                    name: "Hidden".into(),
                    address: "bcc@example.com".into(),
                }],
                &[Attachment {
                    id: 999,
                    message_id: msg_id,
                    filename: "doc.pdf".into(),
                    size_bytes: 1024,
                    on_disk: true,
                }],
                Some("<msg-123@example.com>"),
                Some("<in-reply-to@example.com>"),
                Some("<ref1@example.com> <ref2@example.com>"),
                Some("<https://example.com/unsub>"),
                Some("List-Unsubscribe=One-Click"),
            )
            .unwrap();

        let detail = store.get_message(msg_id).unwrap();
        assert_eq!(detail.body, vec!["Updated body text"]);
        assert_eq!(
            detail.body_html.as_deref(),
            Some("<p>Updated body html</p>")
        );
        assert_eq!(
            detail.list_unsubscribe.as_deref(),
            Some("<https://example.com/unsub>")
        );
        assert_eq!(
            detail.list_unsubscribe_post.as_deref(),
            Some("List-Unsubscribe=One-Click")
        );
        assert_eq!(detail.to.len(), 1);
        assert_eq!(detail.bcc.len(), 1);
        assert_eq!(detail.bcc[0].address, "bcc@example.com");
        assert_eq!(
            detail.message_id_header.as_deref(),
            Some("<msg-123@example.com>")
        );
        assert_eq!(
            detail.in_reply_to.as_deref(),
            Some("<in-reply-to@example.com>")
        );
        assert_eq!(
            detail.references.as_deref(),
            Some("<ref1@example.com> <ref2@example.com>")
        );
        assert_eq!(detail.attachments.len(), 1);
        assert_eq!(detail.attachments[0].filename, "doc.pdf");
    }

    #[test]
    fn test_draft_threading_and_bcc_persistence() {
        let store = seeded();
        let draft = Draft {
            id: None,
            account_id: 1,
            to: vec!["to@example.com".into()],
            cc: vec!["cc@example.com".into()],
            bcc: vec!["bcc@example.com".into()],
            subject: "Re: Threaded Subject".into(),
            body: "Replying to threaded email".into(),
            in_reply_to: Some("<orig@example.com>".into()),
            references: Some("<thread-root@example.com> <orig@example.com>".into()),
        };

        let draft_id = store.save_draft(&draft).unwrap();
        let detail = store.get_message(draft_id).unwrap();
        assert_eq!(detail.to.len(), 1);
        assert_eq!(detail.cc.len(), 1);
        assert_eq!(detail.bcc.len(), 1);
        assert_eq!(detail.bcc[0].address, "bcc@example.com");
        assert_eq!(detail.in_reply_to.as_deref(), Some("<orig@example.com>"));
        assert_eq!(
            detail.references.as_deref(),
            Some("<thread-root@example.com> <orig@example.com>")
        );
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
        store.create_account(&info, "#0f766e".into()).unwrap();
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

    #[test]
    fn test_fts5_search_and_index_rebuild() {
        let store = seeded();
        store.rebuild_search_index().unwrap();

        // Search in messages
        let query = SearchQuery {
            query: "lease".into(),
            folder: None,
            account_id: None,
            include_events: true,
            limit: 20,
        };
        let matches = store.search(&query);
        assert!(!matches.is_empty(), "expected match for lease");
        assert_eq!(matches[0].kind, "message");
        assert!(matches[0].snippet.contains("<mark>"));

        // Search with scoping
        let scoped_query = SearchQuery {
            query: "lease".into(),
            folder: Some("Sent".into()),
            account_id: None,
            include_events: false,
            limit: 20,
        };
        let scoped_matches = store.search(&scoped_query);
        for m in scoped_matches {
            assert_eq!(m.folder.as_deref(), Some("Sent"));
        }

        // Search events
        let event_query = SearchQuery {
            query: "board".into(),
            folder: None,
            account_id: None,
            include_events: true,
            limit: 20,
        };
        let event_matches = store.search(&event_query);
        assert!(
            event_matches.iter().any(|m| m.kind == "event"),
            "expected at least one matching calendar event"
        );
    }

    /// P1.3: search operators (`from:`, `to:`, `cc:`, `subject:`,
    /// `has:attachment`, `is:unread`, `is:starred`, `before:`, `in:`,
    /// `account:`) narrow results; unknown tokens fall through to full text.
    #[test]
    fn search_operators() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "me@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        let insert = |id: MessageId,
                      folder: &str,
                      name: &str,
                      address: &str,
                      subject: &str,
                      unread: i64,
                      flagged: i64,
                      has_att: i64| {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO messages (id, account_id, folder, sender_name, sender_address, \
                 subject, snippet, received_at_ms, unread, flagged, has_attachments) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, '', ?7, ?8, ?9, ?10)",
                params![id, acc.id, folder, name, address, subject, id as i64, unread, flagged, has_att],
            )
            .unwrap();
        };
        insert(1, "Inbox", "Alice Adams", "alice@example.com", "Quarterly report", 1, 0, 1);
        insert(2, "Sent", "Bob Brown", "bob@example.com", "Weekly status", 0, 1, 0);
        insert(3, "Inbox", "Alice Adams", "alice@example.com", "Hello world", 1, 0, 0);
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (2, 'to', 'Carol', 'carol@example.com', 0)",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO recipients (message_id, kind, name, address, position) \
                 VALUES (1, 'cc', 'Dave', 'dave@example.com', 0)",
                [],
            )
            .unwrap();
        }
        store.rebuild_search_index().unwrap();

        let run = |q: &str| {
            store
                .search(&SearchQuery {
                    query: q.into(),
                    folder: None,
                    account_id: None,
                    include_events: false,
                    limit: 20,
                })
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>()
        };

        assert_eq!(run("from:alice"), vec![3, 1], "from matches the address");
        assert_eq!(run("from:\"Alice Adams\""), vec![3, 1], "quoted name match");
        assert_eq!(run("to:carol"), vec![2], "to matches a recipient");
        assert_eq!(run("cc:dave"), vec![1], "cc matches a recipient");
        assert_eq!(run("subject:report"), vec![1], "subject operator");
        assert_eq!(run("has:attachment"), vec![1]);
        assert_eq!(run("is:unread"), vec![3, 1], "unread filter");
        assert_eq!(run("is:starred"), vec![2], "starred filter");
        assert_eq!(run("in:sent"), vec![2], "in is case-insensitive");
        assert_eq!(run("before:2030-01-01").len(), 3, "before a future date");
        assert_eq!(run("after:2030-01-01"), Vec::<u32>::new(), "after a future date");
        assert_eq!(run("account:me@example.com").len(), 3, "account by address");
        assert_eq!(run("account:unknown@example.com"), Vec::<u32>::new());

        // Operators + a plain term together: subject:report narrows the FTS.
        assert_eq!(run("report"), vec![1]);
        assert_eq!(run("status"), vec![2]);
        assert_eq!(run("xyzzy_nomatch"), Vec::<u32>::new());
    }

    /// P1.3: parse_search_date understands ISO dates + relative words.
    #[test]
    fn search_date_parsing() {
        let before = parse_search_date("2030-06-15").unwrap();
        assert_eq!(before % 86_400_000, 0, "midnight of the day");
        assert!(parse_search_date("today").unwrap() <= now_ms());
        assert!(parse_search_date("yesterday").unwrap() < parse_search_date("today").unwrap());
        assert!(parse_search_date("tomorrow").unwrap() > parse_search_date("today").unwrap());
        assert!(parse_search_date("not-a-date").is_none());
        assert!(parse_search_date("2030-13-01").is_none());
    }

    /// P1.3: saved searches CRUD + duplicate-name handling.
    #[test]
    fn saved_searches_crud() {
        let store = seeded();
        assert!(store.list_saved_searches().is_empty());
        let id = store.save_search("Unread", "is:unread").unwrap();
        assert!(id > 0);
        store.save_search("From Alice", "from:alice").unwrap();
        let list = store.list_saved_searches();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Unread");
        assert_eq!(list[0].query, "is:unread");
        assert!(store.save_search("Unread", "x").is_err(), "names are unique");
        store.delete_saved_search(id).unwrap();
        assert_eq!(store.list_saved_searches().len(), 1);
    }

    /// P1.3: the cancellable rebuild produces a fresh index and honours the
    /// cancel flag.
    #[test]
    fn cancellable_rebuild_honours_flag() {
        let store = seeded();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        store
            .rebuild_search_index_cancellable(&cancel, |_, _| {})
            .unwrap();
        let (total, indexed) = store.search_index_status();
        assert_eq!(total, indexed, "index fresh after rebuild");
        assert!(total > 0);

        // A pre-set cancel flag aborts before any work.
        let cancel2 = std::sync::atomic::AtomicBool::new(true);
        assert!(store
            .rebuild_search_index_cancellable(&cancel2, |_, _| {})
            .is_err());
    }

    /// P1.3: the rule dry-run previews what apply would change (with rule
    /// order), and revert restores the before-state.
    #[test]
    fn rule_preview_and_revert() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acc = store
            .create_account(
                &NewAccount {
                    address: "a@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#3b5bdb".into(),
            )
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO messages (id, account_id, folder, sender_name, sender_address, \
                 subject, snippet, received_at_ms, unread, flagged, has_attachments) \
                 VALUES (1, ?1, 'Inbox', 'Alice', 'alice@example.com', 'Hi', '', 1000, 1, 0, 0)",
                params![acc.id],
            )
            .unwrap();
        }
        let rules = vec![MailRule {
            id: "r1".into(),
            name: "Alice mail".into(),
            enabled: true,
            match_mode: RuleMatchMode::All,
            conditions: vec![RuleCondition {
                field: RuleField::From,
                operator: RuleOperator::Contains,
                value: "alice@example.com".into(),
            }],
            actions: vec![RuleAction::MoveToFolder {
                folder_name: "Archive".into(),
            }],
            stop_processing: true,
        }];

        let preview = store.preview_rules(acc.id, "Inbox", &rules);
        assert_eq!(preview.affected, 1);
        assert_eq!(preview.previews[0].message_id, 1);
        assert_eq!(preview.previews[0].folder_before, "Inbox");
        assert_eq!(preview.previews[0].unread_before, true);
        assert_eq!(preview.previews[0].matched[0].rule_name, "Alice mail");
        assert_eq!(
            preview.previews[0].matched[0].actions,
            vec!["Move to Archive".to_string()]
        );

        // The preview did NOT apply anything.
        assert_eq!(store.get_message(1).unwrap().row.folder, "Inbox");

        // Apply, then revert.
        let applied = store.apply_rules_to_folder(acc.id, "Inbox", &rules).unwrap();
        assert_eq!(applied, 1);
        assert_eq!(store.get_message(1).unwrap().row.folder, "Archive");

        let reverted = store.revert_rules(acc.id, &preview.previews).unwrap();
        assert_eq!(reverted, 1);
        assert_eq!(store.get_message(1).unwrap().row.folder, "Inbox");
        assert_eq!(store.get_message(1).unwrap().row.unread, true);
    }

    #[test]
    fn test_conversation_threading_and_thread_actions() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acct = store.create_account(
            &NewAccount {
                address: "thread@example.com".into(),
                protocol: "IMAP".into(),
                server: "imap.example.com".into(),
                port: 993,
                tls: true,
                sync_mode: "every 2 min".into(),
            },
            "#3b5bdb".into(),
        )
        .unwrap();

        // Insert two messages with same subject / thread root
        let msg1_id = store
            .upsert_fetched_message(
                acct.id,
                "Inbox",
                "INBOX",
                101,
                1,
                "Alice",
                "alice@example.com",
                "Project Plan",
                "First message in thread",
                1000,
                true,
                false,
                false,
                false,
                false,
            )
            .unwrap();

        let msg2_id = store
            .upsert_fetched_message(
                acct.id,
                "Inbox",
                "INBOX",
                102,
                1,
                "Bob",
                "bob@example.com",
                "Re: Project Plan",
                "Reply message in thread",
                2000,
                true,
                false,
                false,
                false,
                false,
            )
            .unwrap();

        // When threaded = true, grouping groups them into 1 thread row with thread_count = 2
        let page_threaded = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: Some(acct.id),
            offset: 0,
            limit: 10,
            threaded: true,
        });
        assert_eq!(page_threaded.total, 1);
        assert_eq!(page_threaded.items.len(), 1);
        assert_eq!(page_threaded.items[0].thread_count, 2);

        let thread_id = page_threaded.items[0].thread_id.as_ref().unwrap();

        // Get all messages in the thread in chronological order
        let thread_msgs = store.get_thread_messages(acct.id, thread_id);
        assert_eq!(thread_msgs.len(), 2);
        assert_eq!(thread_msgs[0].row.id, msg1_id);
        assert_eq!(thread_msgs[1].row.id, msg2_id);

        // Apply thread action (MarkRead)
        store
            .apply_thread_action(acct.id, thread_id, ActionType::MarkRead)
            .unwrap();

        let d1 = store.get_message(msg1_id).unwrap();
        let d2 = store.get_message(msg2_id).unwrap();
        assert!(!d1.row.unread);
        assert!(!d2.row.unread);

        // When threaded = false, flat listing returns 2 separate rows
        let page_flat = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: Some(acct.id),
            offset: 0,
            limit: 10,
            threaded: false,
        });
        assert_eq!(page_flat.total, 2);
        assert_eq!(page_flat.items.len(), 2);
    }

    #[test]
    fn test_page_messages_without_folder_filter() {
        // Regression: the "1 = 1" where clause (no folder filter, no search)
        // contributed zero bound params while the SQL hardcoded "LIMIT ?2
        // OFFSET ?3", panicking with rusqlite InvalidParameterCount.
        let store = SqliteStore::open_in_memory().unwrap();
        let acct = store.create_account(
            &NewAccount {
                address: "no-folder@example.com".into(),
                protocol: "IMAP".into(),
                server: "imap.example.com".into(),
                port: 993,
                tls: true,
                sync_mode: "every 2 min".into(),
            },
            "#0f766e".into(),
        )
        .unwrap();

        for uid in 1..=3u32 {
            store
                .upsert_fetched_message(
                    acct.id,
                    "Inbox",
                    "INBOX",
                    uid,
                    1,
                    "Sender",
                    "sender@example.com",
                    "Shared Subject",
                    "snippet",
                    1000 + i64::from(uid),
                    true,
                    false,
                    false,
                    false,
                    false,
                )
                .unwrap();
        }

        let flat = store.page_messages(&MessageQuery {
            folder: None,
            account_id: Some(acct.id),
            offset: 0,
            limit: 10,
            threaded: false,
        });
        assert_eq!(flat.total, 3);
        assert_eq!(flat.items.len(), 3);

        let threaded = store.page_messages(&MessageQuery {
            folder: None,
            account_id: Some(acct.id),
            offset: 0,
            limit: 10,
            threaded: true,
        });
        assert_eq!(threaded.total, 1);
        assert_eq!(threaded.items.len(), 1);
        assert_eq!(threaded.items[0].thread_count, 3);
    }

    #[test]
    fn test_prune_messages_before() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acct = store.create_account(
            &NewAccount {
                address: "prune@example.com".into(),
                protocol: "IMAP".into(),
                server: "imap.example.com".into(),
                port: 993,
                tls: true,
                sync_mode: "every 2 min".into(),
            },
            "#0f766e".into(),
        )
        .unwrap();

        store
            .upsert_fetched_message(
                acct.id, "Inbox", "INBOX", 1, 1, "Old", "old@example.com", "Old Mail", "", 1_000_000,
                true, false, false, false, false,
            )
            .unwrap();
        store
            .upsert_fetched_message(
                acct.id, "Inbox", "INBOX", 2, 1, "Mid", "mid@example.com", "Mid Mail", "", 2_000_000,
                true, false, false, false, false,
            )
            .unwrap();
        store
            .upsert_fetched_message(
                acct.id, "Inbox", "INBOX", 3, 1, "New", "new@example.com", "New Mail", "", 3_000_000,
                true, false, false, false, false,
            )
            .unwrap();

        let deleted = store.prune_messages_before(2_500_000).unwrap();
        assert_eq!(deleted, 2);

        let page = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: Some(acct.id),
            offset: 0,
            limit: 10,
            threaded: false,
        });
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].subject, "New Mail");
    }

    #[test]
    fn test_answered_forwarded_flags_and_roundtrip() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acct = store.create_account(
            &NewAccount {
                address: "flags@example.com".into(),
                protocol: "IMAP".into(),
                server: "imap.example.com".into(),
                port: 993,
                tls: true,
                sync_mode: "every 2 min".into(),
            },
            "#3b5bdb".into(),
        )
        .unwrap();

        let msg_id = store
            .upsert_fetched_message(
                acct.id,
                "Inbox",
                "INBOX",
                1,
                1,
                "Sender",
                "sender@example.com",
                "Hello",
                "Snippet",
                1000,
                true,
                false,
                false,
                false,
                false,
            )
            .unwrap();

        let msg = store.get_message(msg_id).unwrap();
        assert!(!msg.row.answered);
        assert!(!msg.row.forwarded);

        store.set_answered(msg_id, true).unwrap();
        let msg = store.get_message(msg_id).unwrap();
        assert!(msg.row.answered);

        store.set_forwarded(msg_id, true).unwrap();
        let msg = store.get_message(msg_id).unwrap();
        assert!(msg.row.answered);
        assert!(msg.row.forwarded);
    }

    #[test]
    fn repair_snippets_from_stored_bodies() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acct = store
            .create_account(
                &NewAccount {
                    address: "repair@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#0f766e".into(),
            )
            .unwrap();

        // A row synced before the sync fetched full bodies: its snippet is raw
        // HTML markup rather than the message text.
        let id = store
            .upsert_fetched_message(
                acct.id,
                "Inbox",
                "INBOX",
                1,
                1,
                "Sender",
                "sender@example.com",
                "Subject",
                "<!DOCTYPE html><html><head></head><body><p>Real text</p></body></html>",
                1000,
                false,
                false,
                false,
                false,
                false,
            )
            .unwrap();

        // No stored body yet → nothing to derive from; repair is a no-op.
        assert_eq!(store.repair_snippets_from_bodies().unwrap(), 0);

        // An on-demand body fetch later stores the parsed plain body…
        store
            .save_message_body_and_attachments(
                id,
                "Real text from the plain body",
                None,
                &[],
                &[],
                &[],
                &[],
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();

        // …and the next startup repair fixes the list snippet from it.
        assert_eq!(store.repair_snippets_from_bodies().unwrap(), 1);
        let page = store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit: 10,
            threaded: false,
        });
        assert_eq!(page.items[0].snippet, "Real text from the plain body");

        // Idempotent: the second run leaves the already-fixed snippet alone.
        assert_eq!(store.repair_snippets_from_bodies().unwrap(), 0);
    }

    #[test]
    fn list_messages_missing_bodies_only_before_first_fetch() {
        let store = SqliteStore::open_in_memory().unwrap();
        let acct = store
            .create_account(
                &NewAccount {
                    address: "backfill@example.com".into(),
                    protocol: "IMAP".into(),
                    server: "imap.example.com".into(),
                    port: 993,
                    tls: true,
                    sync_mode: "every 2 min".into(),
                },
                "#0f766e".into(),
            )
            .unwrap();

        let id = store
            .upsert_fetched_message(
                acct.id,
                "Inbox",
                "INBOX",
                7,
                1,
                "Sender",
                "sender@example.com",
                "Subject",
                "old garbage snippet",
                1000,
                false,
                false,
                false,
                false,
                false,
            )
            .unwrap();

        // No body stored yet → it's a backfill candidate.
        let pending = store.list_messages_missing_bodies(acct.id).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].message_id, id);
        assert_eq!(pending[0].uid, 7);
        assert_eq!(pending[0].uidvalidity, 1);
        assert_eq!(pending[0].server_folder, "INBOX");

        // Once a body is stored the candidate disappears (no re-fetch loops).
        store
            .save_message_body_and_attachments(
                id,
                "Real body",
                None,
                &[],
                &[],
                &[],
                &[],
                None,
                None,
                None,
                None,
                None,
            )
            .unwrap();
        assert!(store.list_messages_missing_bodies(acct.id).unwrap().is_empty());
    }
}

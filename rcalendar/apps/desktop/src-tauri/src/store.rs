//! SQLite implementation of [`calendar_core::Store`].
//!
//! Stores entities in a local SQLite database with UUIDv4 primary keys,
//! timestamps, `uid`/`etag`, soft delete tombstones, and idempotent upserts (S2.1, S2.2).

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use calendar_core::model::{
    Account, AccountKind, AccountStatus, Calendar, Event, Reminder, Task, TimeRange,
};
use calendar_core::Store;
use calendar_core::{Error as CoreError, Result as CoreResult};

use crate::migrations::run_migrations;

/// SQLite-backed store wrapping a thread-safe connection.
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore").finish_non_exhaustive()
    }
}

impl SqliteStore {
    /// Opens or creates a SQLite database file at `path` and runs migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CoreError> {
        let mut conn = Connection::open(path)
            .map_err(|e| CoreError::Store(format!("failed to open sqlite database: {e}")))?;
        conn.execute_batch("PRAGMA journal_mode = WAL;")
            .map_err(|e| CoreError::Store(format!("failed to configure sqlite pragmas: {e}")))?;
        run_migrations(&mut conn)
            .map_err(|e| CoreError::Store(format!("failed to run migrations: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Creates an in-memory SQLite database and runs migrations (for tests / scratch).
    pub fn in_memory() -> Result<Self, CoreError> {
        let mut conn = Connection::open_in_memory()
            .map_err(|e| CoreError::Store(format!("failed to open in-memory sqlite: {e}")))?;
        run_migrations(&mut conn)
            .map_err(|e| CoreError::Store(format!("failed to run migrations: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Seeds default "On this Mac" account and default calendars if the store is empty.
    pub fn seed_defaults_if_empty(&self) -> CoreResult<()> {
        let accounts = self.list_accounts()?;
        if !accounts.is_empty() {
            return Ok(());
        }

        let now = Utc::now();
        let local_account_id = Uuid::new_v4();
        let local_account = Account {
            id: local_account_id,
            kind: AccountKind::Local,
            display_name: "On this Mac".into(),
            detail: "local store · no network".into(),
            last_synced_at: None,
            status: AccountStatus::Idle,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        self.upsert_account(&local_account)?;

        let default_calendars = [
            ("Personal", "#1F6FEB", true),
            ("Classes", "#C2410C", true),
            ("Work shifts", "#0F766E", true),
            ("Birthdays", "#7C5CBF", true),
            ("Home", "#A16207", true),
            ("US Holidays", "#888888", false),
        ];

        for (name, color, enabled) in default_calendars {
            let cal = Calendar {
                id: Uuid::new_v4(),
                account_id: local_account_id,
                name: name.into(),
                color: color.into(),
                enabled,
                event_count: 0,
                created_at: now,
                updated_at: now,
                deleted_at: None,
            };
            self.upsert_calendar(&cal)?;
        }

        Ok(())
    }

    /// Gets a string setting by key.
    pub fn get_setting(&self, key: &str) -> CoreResult<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(to_core_err)?;
        let val: Option<String> = stmt
            .query_row(params![key], |row| row.get(0))
            .optional()
            .map_err(to_core_err)?;
        Ok(val)
    }

    /// Sets a string setting by key.
    pub fn set_setting(&self, key: &str, value: &str) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    /// Fetches all active events that could have occurrences in `range`:
    /// non-recurring events overlapping `range`, plus all recurring events whose
    /// series starts before `range.end`.
    pub fn list_events_for_expansion(
        &self,
        range: &TimeRange,
        calendar_ids: Option<&[Uuid]>,
    ) -> CoreResult<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let range_start_str = format_dt(range.start);
        let range_end_str = format_dt(range.end);

        let sql = r#"
            SELECT id, calendar_id, uid, title, location, notes, starts_at, ends_at,
                   all_day, tz, rrule, exdates, etag, created_at, updated_at, deleted_at
            FROM events
            WHERE deleted_at IS NULL
              AND (
                  (rrule IS NULL AND starts_at < ?2 AND ends_at > ?1)
                  OR
                  (rrule IS NOT NULL AND starts_at < ?2)
              )
            ORDER BY starts_at ASC
        "#;

        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let rows = stmt
            .query_map(params![range_start_str, range_end_str], row_to_event)
            .map_err(to_core_err)?;

        let mut events = Vec::new();
        for r in rows {
            let event = r.map_err(to_core_err)?;
            if let Some(cal_ids) = calendar_ids {
                if !cal_ids.contains(&event.calendar_id) {
                    continue;
                }
            }
            events.push(event);
        }
        Ok(events)
    }

    /// Searches active events by matching substring in title, notes, or location.
    pub fn search_events(&self, query: &str) -> CoreResult<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{query}%");
        let sql = r#"
            SELECT id, calendar_id, uid, title, location, notes, starts_at, ends_at,
                   all_day, tz, rrule, exdates, etag, created_at, updated_at, deleted_at
            FROM events
            WHERE deleted_at IS NULL
              AND (title LIKE ?1 OR notes LIKE ?1 OR location LIKE ?1)
            ORDER BY starts_at ASC
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let rows = stmt
            .query_map(params![pattern], row_to_event)
            .map_err(to_core_err)?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r.map_err(to_core_err)?);
        }
        Ok(results)
    }

    /// Searches active tasks by matching substring in title.
    pub fn search_tasks(&self, query: &str) -> CoreResult<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{query}%");
        let sql = r#"
            SELECT id, calendar_id, title, due_at, completed_at, created_at, updated_at, deleted_at
            FROM tasks
            WHERE deleted_at IS NULL
              AND title LIKE ?1
            ORDER BY due_at ASC, created_at ASC
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let rows = stmt
            .query_map(params![pattern], row_to_task)
            .map_err(to_core_err)?;
        let mut results = Vec::new();
        for r in rows {
            results.push(r.map_err(to_core_err)?);
        }
        Ok(results)
    }
}

impl Store for SqliteStore {
    // -- accounts -------------------------------------------------------
    fn list_accounts(&self) -> CoreResult<Vec<Account>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT id, kind, display_name, detail, last_synced_at, status, created_at, updated_at, deleted_at
            FROM accounts
            WHERE deleted_at IS NULL
            ORDER BY created_at ASC
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let rows = stmt.query_map([], row_to_account).map_err(to_core_err)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(to_core_err)?);
        }
        Ok(list)
    }

    fn get_account(&self, id: Uuid) -> CoreResult<Option<Account>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT id, kind, display_name, detail, last_synced_at, status, created_at, updated_at, deleted_at
            FROM accounts
            WHERE id = ?1 AND deleted_at IS NULL
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let res = stmt
            .query_row(params![id.to_string()], row_to_account)
            .optional()
            .map_err(to_core_err)?;
        Ok(res)
    }

    fn upsert_account(&self, account: &Account) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let kind_str = match account.kind {
            AccountKind::Local => "local",
            AccountKind::Google => "google",
            AccountKind::Caldav => "caldav",
        };
        let status_str = match account.status {
            AccountStatus::Idle => "idle",
            AccountStatus::Syncing => "syncing",
            AccountStatus::Error => "error",
        };
        let last_synced = account.last_synced_at.map(format_dt);
        let created_at = format_dt(account.created_at);
        let updated_at = format_dt(account.updated_at);
        let deleted_at = account.deleted_at.map(format_dt);

        let sql = r#"
            INSERT INTO accounts (id, kind, display_name, detail, last_synced_at, status, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                kind = excluded.kind,
                display_name = excluded.display_name,
                detail = excluded.detail,
                last_synced_at = excluded.last_synced_at,
                status = excluded.status,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at
        "#;
        conn.execute(
            sql,
            params![
                account.id.to_string(),
                kind_str,
                account.display_name,
                account.detail,
                last_synced,
                status_str,
                created_at,
                updated_at,
                deleted_at,
            ],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    fn delete_account(&self, id: Uuid) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = format_dt(Utc::now());
        conn.execute(
            "UPDATE accounts SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id.to_string()],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    // -- calendars ------------------------------------------------------
    fn list_calendars(&self) -> CoreResult<Vec<Calendar>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT c.id, c.account_id, c.name, c.color, c.enabled,
                   (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id AND e.deleted_at IS NULL) as event_count,
                   c.created_at, c.updated_at, c.deleted_at
            FROM calendars c
            WHERE c.deleted_at IS NULL
            ORDER BY c.created_at ASC
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let rows = stmt.query_map([], row_to_calendar).map_err(to_core_err)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(to_core_err)?);
        }
        Ok(list)
    }

    fn get_calendar(&self, id: Uuid) -> CoreResult<Option<Calendar>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT c.id, c.account_id, c.name, c.color, c.enabled,
                   (SELECT COUNT(*) FROM events e WHERE e.calendar_id = c.id AND e.deleted_at IS NULL) as event_count,
                   c.created_at, c.updated_at, c.deleted_at
            FROM calendars c
            WHERE c.id = ?1 AND c.deleted_at IS NULL
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let res = stmt
            .query_row(params![id.to_string()], row_to_calendar)
            .optional()
            .map_err(to_core_err)?;
        Ok(res)
    }

    fn upsert_calendar(&self, calendar: &Calendar) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let created_at = format_dt(calendar.created_at);
        let updated_at = format_dt(calendar.updated_at);
        let deleted_at = calendar.deleted_at.map(format_dt);

        let sql = r#"
            INSERT INTO calendars (id, account_id, name, color, enabled, event_count, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
                account_id = excluded.account_id,
                name = excluded.name,
                color = excluded.color,
                enabled = excluded.enabled,
                event_count = excluded.event_count,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at
        "#;
        conn.execute(
            sql,
            params![
                calendar.id.to_string(),
                calendar.account_id.to_string(),
                calendar.name,
                calendar.color,
                if calendar.enabled { 1 } else { 0 },
                calendar.event_count as i64,
                created_at,
                updated_at,
                deleted_at,
            ],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    fn delete_calendar(&self, id: Uuid) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = format_dt(Utc::now());
        conn.execute(
            "UPDATE calendars SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id.to_string()],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    // -- events ---------------------------------------------------------
    fn list_events(&self, range: Option<&TimeRange>) -> CoreResult<Vec<Event>> {
        let conn = self.conn.lock().unwrap();
        let mut list = Vec::new();
        match range {
            Some(range) => {
                let range_start = format_dt(range.start);
                let range_end = format_dt(range.end);
                let sql = r#"
                    SELECT id, calendar_id, uid, title, location, notes, starts_at, ends_at,
                           all_day, tz, rrule, exdates, etag, created_at, updated_at, deleted_at
                    FROM events
                    WHERE deleted_at IS NULL
                      AND starts_at < ?2 AND ends_at > ?1
                    ORDER BY starts_at ASC
                "#;
                let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
                let rows = stmt
                    .query_map(params![range_start, range_end], row_to_event)
                    .map_err(to_core_err)?;
                for r in rows {
                    list.push(r.map_err(to_core_err)?);
                }
            }
            None => {
                let sql = r#"
                    SELECT id, calendar_id, uid, title, location, notes, starts_at, ends_at,
                           all_day, tz, rrule, exdates, etag, created_at, updated_at, deleted_at
                    FROM events
                    WHERE deleted_at IS NULL
                    ORDER BY starts_at ASC
                "#;
                let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
                let rows = stmt.query_map([], row_to_event).map_err(to_core_err)?;
                for r in rows {
                    list.push(r.map_err(to_core_err)?);
                }
            }
        }
        Ok(list)
    }

    fn get_event(&self, id: Uuid) -> CoreResult<Option<Event>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT id, calendar_id, uid, title, location, notes, starts_at, ends_at,
                   all_day, tz, rrule, exdates, etag, created_at, updated_at, deleted_at
            FROM events
            WHERE id = ?1 AND deleted_at IS NULL
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let res = stmt
            .query_row(params![id.to_string()], row_to_event)
            .optional()
            .map_err(to_core_err)?;
        Ok(res)
    }

    fn upsert_event(&self, event: &Event) -> CoreResult<()> {
        event.validate()?;
        let conn = self.conn.lock().unwrap();
        let exdates_json = serde_json::to_string(&event.exdates)
            .map_err(|e| CoreError::Store(format!("failed to serialize exdates: {e}")))?;
        let starts_at = format_dt(event.starts_at);
        let ends_at = format_dt(event.ends_at);
        let created_at = format_dt(event.created_at);
        let updated_at = format_dt(event.updated_at);
        let deleted_at = event.deleted_at.map(format_dt);

        let sql = r#"
            INSERT INTO events (id, calendar_id, uid, title, location, notes, starts_at, ends_at,
                               all_day, tz, rrule, exdates, etag, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(id) DO UPDATE SET
                calendar_id = excluded.calendar_id,
                uid = excluded.uid,
                title = excluded.title,
                location = excluded.location,
                notes = excluded.notes,
                starts_at = excluded.starts_at,
                ends_at = excluded.ends_at,
                all_day = excluded.all_day,
                tz = excluded.tz,
                rrule = excluded.rrule,
                exdates = excluded.exdates,
                etag = excluded.etag,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at
        "#;
        conn.execute(
            sql,
            params![
                event.id.to_string(),
                event.calendar_id.to_string(),
                event.uid,
                event.title,
                event.location,
                event.notes,
                starts_at,
                ends_at,
                if event.all_day { 1 } else { 0 },
                event.tz,
                event.rrule,
                exdates_json,
                event.etag,
                created_at,
                updated_at,
                deleted_at,
            ],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    fn delete_event(&self, id: Uuid) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = format_dt(Utc::now());
        conn.execute(
            "UPDATE events SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id.to_string()],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    // -- reminders ------------------------------------------------------
    fn list_reminders(&self, event_id: Uuid) -> CoreResult<Vec<Reminder>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT id, event_id, offset_minutes, absolute_at, created_at, updated_at, deleted_at
            FROM reminders
            WHERE event_id = ?1 AND deleted_at IS NULL
            ORDER BY created_at ASC
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let rows = stmt
            .query_map(params![event_id.to_string()], row_to_reminder)
            .map_err(to_core_err)?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r.map_err(to_core_err)?);
        }
        Ok(list)
    }

    fn upsert_reminder(&self, reminder: &Reminder) -> CoreResult<()> {
        reminder.validate()?;
        let conn = self.conn.lock().unwrap();
        let absolute_at = reminder.absolute_at.map(format_dt);
        let created_at = format_dt(reminder.created_at);
        let updated_at = format_dt(reminder.updated_at);
        let deleted_at = reminder.deleted_at.map(format_dt);

        let sql = r#"
            INSERT INTO reminders (id, event_id, offset_minutes, absolute_at, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(id) DO UPDATE SET
                event_id = excluded.event_id,
                offset_minutes = excluded.offset_minutes,
                absolute_at = excluded.absolute_at,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at
        "#;
        conn.execute(
            sql,
            params![
                reminder.id.to_string(),
                reminder.event_id.to_string(),
                reminder.offset_minutes,
                absolute_at,
                created_at,
                updated_at,
                deleted_at,
            ],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    fn delete_reminder(&self, id: Uuid) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = format_dt(Utc::now());
        conn.execute(
            "UPDATE reminders SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id.to_string()],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    // -- tasks ----------------------------------------------------------
    fn list_tasks(&self, range: Option<&TimeRange>) -> CoreResult<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut list = Vec::new();
        match range {
            Some(range) => {
                let range_start = format_dt(range.start);
                let range_end = format_dt(range.end);
                let sql = r#"
                    SELECT id, calendar_id, title, due_at, completed_at, created_at, updated_at, deleted_at
                    FROM tasks
                    WHERE deleted_at IS NULL
                      AND due_at IS NOT NULL
                      AND due_at >= ?1 AND due_at < ?2
                    ORDER BY due_at ASC, created_at ASC
                "#;
                let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
                let rows = stmt
                    .query_map(params![range_start, range_end], row_to_task)
                    .map_err(to_core_err)?;
                for r in rows {
                    list.push(r.map_err(to_core_err)?);
                }
            }
            None => {
                let sql = r#"
                    SELECT id, calendar_id, title, due_at, completed_at, created_at, updated_at, deleted_at
                    FROM tasks
                    WHERE deleted_at IS NULL
                    ORDER BY due_at ASC, created_at ASC
                "#;
                let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
                let rows = stmt.query_map([], row_to_task).map_err(to_core_err)?;
                for r in rows {
                    list.push(r.map_err(to_core_err)?);
                }
            }
        }
        Ok(list)
    }

    fn get_task(&self, id: Uuid) -> CoreResult<Option<Task>> {
        let conn = self.conn.lock().unwrap();
        let sql = r#"
            SELECT id, calendar_id, title, due_at, completed_at, created_at, updated_at, deleted_at
            FROM tasks
            WHERE id = ?1 AND deleted_at IS NULL
        "#;
        let mut stmt = conn.prepare(sql).map_err(to_core_err)?;
        let res = stmt
            .query_row(params![id.to_string()], row_to_task)
            .optional()
            .map_err(to_core_err)?;
        Ok(res)
    }

    fn upsert_task(&self, task: &Task) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let due_at = task.due_at.map(format_dt);
        let completed_at = task.completed_at.map(format_dt);
        let created_at = format_dt(task.created_at);
        let updated_at = format_dt(task.updated_at);
        let deleted_at = task.deleted_at.map(format_dt);

        let sql = r#"
            INSERT INTO tasks (id, calendar_id, title, due_at, completed_at, created_at, updated_at, deleted_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(id) DO UPDATE SET
                calendar_id = excluded.calendar_id,
                title = excluded.title,
                due_at = excluded.due_at,
                completed_at = excluded.completed_at,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at,
                deleted_at = excluded.deleted_at
        "#;
        conn.execute(
            sql,
            params![
                task.id.to_string(),
                task.calendar_id.to_string(),
                task.title,
                due_at,
                completed_at,
                created_at,
                updated_at,
                deleted_at,
            ],
        )
        .map_err(to_core_err)?;
        Ok(())
    }

    fn delete_task(&self, id: Uuid) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        let now = format_dt(Utc::now());
        conn.execute(
            "UPDATE tasks SET deleted_at = ?1 WHERE id = ?2 AND deleted_at IS NULL",
            params![now, id.to_string()],
        )
        .map_err(to_core_err)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers and Row Mappers
// ---------------------------------------------------------------------------

fn to_core_err(e: rusqlite::Error) -> CoreError {
    CoreError::Store(e.to_string())
}

fn format_dt(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, rusqlite::Error> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn parse_uuid(s: &str) -> Result<Uuid, rusqlite::Error> {
    Uuid::parse_str(s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row_to_account(row: &Row<'_>) -> Result<Account, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let kind_str: String = row.get(1)?;
    let display_name: String = row.get(2)?;
    let detail: String = row.get(3)?;
    let last_synced_at_str: Option<String> = row.get(4)?;
    let status_str: String = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let updated_at_str: String = row.get(7)?;
    let deleted_at_str: Option<String> = row.get(8)?;

    let kind = match kind_str.as_str() {
        "google" => AccountKind::Google,
        "caldav" => AccountKind::Caldav,
        _ => AccountKind::Local,
    };
    let status = match status_str.as_str() {
        "syncing" => AccountStatus::Syncing,
        "error" => AccountStatus::Error,
        _ => AccountStatus::Idle,
    };

    Ok(Account {
        id: parse_uuid(&id_str)?,
        kind,
        display_name,
        detail,
        last_synced_at: last_synced_at_str.as_deref().map(parse_dt).transpose()?,
        status,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
        deleted_at: deleted_at_str.as_deref().map(parse_dt).transpose()?,
    })
}

fn row_to_calendar(row: &Row<'_>) -> Result<Calendar, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let account_id_str: String = row.get(1)?;
    let name: String = row.get(2)?;
    let color: String = row.get(3)?;
    let enabled_int: i64 = row.get(4)?;
    let event_count: i64 = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let updated_at_str: String = row.get(7)?;
    let deleted_at_str: Option<String> = row.get(8)?;

    Ok(Calendar {
        id: parse_uuid(&id_str)?,
        account_id: parse_uuid(&account_id_str)?,
        name,
        color,
        enabled: enabled_int != 0,
        event_count: event_count.max(0) as u64,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
        deleted_at: deleted_at_str.as_deref().map(parse_dt).transpose()?,
    })
}

fn row_to_event(row: &Row<'_>) -> Result<Event, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let calendar_id_str: String = row.get(1)?;
    let uid: String = row.get(2)?;
    let title: String = row.get(3)?;
    let location: Option<String> = row.get(4)?;
    let notes: Option<String> = row.get(5)?;
    let starts_at_str: String = row.get(6)?;
    let ends_at_str: String = row.get(7)?;
    let all_day_int: i64 = row.get(8)?;
    let tz: Option<String> = row.get(9)?;
    let rrule: Option<String> = row.get(10)?;
    let exdates_json: String = row.get(11)?;
    let etag: Option<String> = row.get(12)?;
    let created_at_str: String = row.get(13)?;
    let updated_at_str: String = row.get(14)?;
    let deleted_at_str: Option<String> = row.get(15)?;

    let exdates: Vec<NaiveDate> = serde_json::from_str(&exdates_json).unwrap_or_default();

    Ok(Event {
        id: parse_uuid(&id_str)?,
        calendar_id: parse_uuid(&calendar_id_str)?,
        uid,
        title,
        location,
        notes,
        starts_at: parse_dt(&starts_at_str)?,
        ends_at: parse_dt(&ends_at_str)?,
        all_day: all_day_int != 0,
        tz,
        rrule,
        exdates,
        etag,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
        deleted_at: deleted_at_str.as_deref().map(parse_dt).transpose()?,
    })
}

fn row_to_reminder(row: &Row<'_>) -> Result<Reminder, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let event_id_str: String = row.get(1)?;
    let offset_minutes: Option<i64> = row.get(2)?;
    let absolute_at_str: Option<String> = row.get(3)?;
    let created_at_str: String = row.get(4)?;
    let updated_at_str: String = row.get(5)?;
    let deleted_at_str: Option<String> = row.get(6)?;

    Ok(Reminder {
        id: parse_uuid(&id_str)?,
        event_id: parse_uuid(&event_id_str)?,
        offset_minutes,
        absolute_at: absolute_at_str.as_deref().map(parse_dt).transpose()?,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
        deleted_at: deleted_at_str.as_deref().map(parse_dt).transpose()?,
    })
}

fn row_to_task(row: &Row<'_>) -> Result<Task, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let calendar_id_str: String = row.get(1)?;
    let title: String = row.get(2)?;
    let due_at_str: Option<String> = row.get(3)?;
    let completed_at_str: Option<String> = row.get(4)?;
    let created_at_str: String = row.get(5)?;
    let updated_at_str: String = row.get(6)?;
    let deleted_at_str: Option<String> = row.get(7)?;

    Ok(Task {
        id: parse_uuid(&id_str)?,
        calendar_id: parse_uuid(&calendar_id_str)?,
        title,
        due_at: due_at_str.as_deref().map(parse_dt).transpose()?,
        completed_at: completed_at_str.as_deref().map(parse_dt).transpose()?,
        created_at: parse_dt(&created_at_str)?,
        updated_at: parse_dt(&updated_at_str)?,
        deleted_at: deleted_at_str.as_deref().map(parse_dt).transpose()?,
    })
}

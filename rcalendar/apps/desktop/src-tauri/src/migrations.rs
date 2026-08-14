//! SQLite schema migrations for Almanac's local database.
//!
//! Versioned migrations create and evolve the tables for accounts, calendars,
//! events, reminders, tasks, and settings (S2.1).

use rusqlite::{params, Connection, Result};

pub const MIGRATIONS: &[(&str, &str)] = &[(
    "001_initial_schema",
    r#"
    CREATE TABLE IF NOT EXISTS _migrations (
        version INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        applied_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS accounts (
        id TEXT PRIMARY KEY NOT NULL,
        kind TEXT NOT NULL,
        display_name TEXT NOT NULL,
        detail TEXT NOT NULL,
        last_synced_at TEXT,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );

    CREATE TABLE IF NOT EXISTS calendars (
        id TEXT PRIMARY KEY NOT NULL,
        account_id TEXT NOT NULL,
        name TEXT NOT NULL,
        color TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        event_count INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );

    CREATE TABLE IF NOT EXISTS events (
        id TEXT PRIMARY KEY NOT NULL,
        calendar_id TEXT NOT NULL,
        uid TEXT NOT NULL,
        title TEXT NOT NULL,
        location TEXT,
        notes TEXT,
        starts_at TEXT NOT NULL,
        ends_at TEXT NOT NULL,
        all_day INTEGER NOT NULL DEFAULT 0,
        tz TEXT,
        rrule TEXT,
        exdates TEXT NOT NULL DEFAULT '[]',
        etag TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );

    CREATE TABLE IF NOT EXISTS reminders (
        id TEXT PRIMARY KEY NOT NULL,
        event_id TEXT NOT NULL,
        offset_minutes INTEGER,
        absolute_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );

    CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY NOT NULL,
        calendar_id TEXT NOT NULL,
        title TEXT NOT NULL,
        due_at TEXT,
        completed_at TEXT,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        deleted_at TEXT
    );

    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );

    -- Indices for query performance and soft delete filtering
    CREATE INDEX IF NOT EXISTS idx_accounts_deleted_at ON accounts(deleted_at);
    CREATE INDEX IF NOT EXISTS idx_calendars_account_id ON calendars(account_id);
    CREATE INDEX IF NOT EXISTS idx_calendars_deleted_at ON calendars(deleted_at);
    CREATE INDEX IF NOT EXISTS idx_events_calendar_id ON events(calendar_id);
    CREATE INDEX IF NOT EXISTS idx_events_time_range ON events(starts_at, ends_at);
    CREATE INDEX IF NOT EXISTS idx_events_deleted_at ON events(deleted_at);
    CREATE INDEX IF NOT EXISTS idx_events_uid ON events(uid);
    CREATE INDEX IF NOT EXISTS idx_reminders_event_id ON reminders(event_id);
    CREATE INDEX IF NOT EXISTS idx_reminders_deleted_at ON reminders(deleted_at);
    CREATE INDEX IF NOT EXISTS idx_tasks_calendar_id ON tasks(calendar_id);
    CREATE INDEX IF NOT EXISTS idx_tasks_due_at ON tasks(due_at);
    CREATE INDEX IF NOT EXISTS idx_tasks_deleted_at ON tasks(deleted_at);
    "#,
)];

/// Runs any pending migrations in a transaction.
pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    // Ensure the migrations table exists first
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL
        );
        "#,
    )?;

    let mut applied_versions = std::collections::HashSet::new();
    {
        let mut stmt = conn.prepare("SELECT version FROM _migrations")?;
        let rows = stmt.query_map([], |row| row.get::<_, i64>(0))?;
        for version in rows {
            applied_versions.insert(version?);
        }
    }

    for (idx, (name, sql)) in MIGRATIONS.iter().enumerate() {
        let version = (idx + 1) as i64;
        if applied_versions.contains(&version) {
            continue;
        }

        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO _migrations (version, name, applied_at) VALUES (?1, ?2, ?3)",
            params![version, name, now],
        )?;
        tx.commit()?;
    }

    Ok(())
}

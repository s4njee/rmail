//! Storage abstraction: a [`Store`] trait with an in-memory implementation.
//!
//! This is the Rust-side embeddability seam (plan.md §8). Consumers depend on
//! [`Store`], never on a concrete backend. The desktop app injects the SQLite
//! impl (M2); tests and other Rust apps can use [`InMemoryStore`]. The shared
//! test suite ([`suite`]) runs against any implementation, so the SQLite impl
//! must pass exactly the same behavior.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;
use uuid::Uuid;

use crate::error::Result;
use crate::model::{Account, Calendar, Event, Reminder, Task, TimeRange};

/// The full CRUD surface of the calendar store, plus range queries.
///
/// All methods are object-safe so a `Box<dyn Store>` can be handed around
/// (e.g. to the shared test suite or to Tauri state).
pub trait Store {
    // -- accounts -------------------------------------------------------
    fn list_accounts(&self) -> Result<Vec<Account>>;
    fn get_account(&self, id: Uuid) -> Result<Option<Account>>;
    fn upsert_account(&self, account: &Account) -> Result<()>;
    fn delete_account(&self, id: Uuid) -> Result<()>;

    // -- calendars ------------------------------------------------------
    fn list_calendars(&self) -> Result<Vec<Calendar>>;
    fn get_calendar(&self, id: Uuid) -> Result<Option<Calendar>>;
    fn upsert_calendar(&self, calendar: &Calendar) -> Result<()>;
    fn delete_calendar(&self, id: Uuid) -> Result<()>;

    // -- events ---------------------------------------------------------
    /// Lists events overlapping `range`, or all events when `range` is `None`.
    fn list_events(&self, range: Option<&TimeRange>) -> Result<Vec<Event>>;
    fn get_event(&self, id: Uuid) -> Result<Option<Event>>;
    fn upsert_event(&self, event: &Event) -> Result<()>;
    fn delete_event(&self, id: Uuid) -> Result<()>;

    // -- reminders ------------------------------------------------------
    fn list_reminders(&self, event_id: Uuid) -> Result<Vec<Reminder>>;
    fn upsert_reminder(&self, reminder: &Reminder) -> Result<()>;
    fn delete_reminder(&self, id: Uuid) -> Result<()>;

    // -- tasks ----------------------------------------------------------
    /// Lists tasks whose due time falls inside `range`, or all when `None`.
    fn list_tasks(&self, range: Option<&TimeRange>) -> Result<Vec<Task>>;
    fn get_task(&self, id: Uuid) -> Result<Option<Task>>;
    fn upsert_task(&self, task: &Task) -> Result<()>;
    fn delete_task(&self, id: Uuid) -> Result<()>;
}

/// In-memory [`Store`] implementation backed by hash maps. `&self` methods
/// mutate through interior mutability, so the store is cheap to share.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    accounts: HashMap<Uuid, Account>,
    calendars: HashMap<Uuid, Calendar>,
    events: HashMap<Uuid, Event>,
    reminders: HashMap<Uuid, Reminder>,
    tasks: HashMap<Uuid, Task>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for InMemoryStore {
    fn list_accounts(&self) -> Result<Vec<Account>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .accounts
            .values()
            .cloned()
            .collect())
    }

    fn get_account(&self, id: Uuid) -> Result<Option<Account>> {
        Ok(self.inner.lock().unwrap().accounts.get(&id).cloned())
    }

    fn upsert_account(&self, account: &Account) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .accounts
            .insert(account.id, account.clone());
        Ok(())
    }

    fn delete_account(&self, id: Uuid) -> Result<()> {
        self.inner.lock().unwrap().accounts.remove(&id);
        Ok(())
    }

    fn list_calendars(&self) -> Result<Vec<Calendar>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .calendars
            .values()
            .cloned()
            .collect())
    }

    fn get_calendar(&self, id: Uuid) -> Result<Option<Calendar>> {
        Ok(self.inner.lock().unwrap().calendars.get(&id).cloned())
    }

    fn upsert_calendar(&self, calendar: &Calendar) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .calendars
            .insert(calendar.id, calendar.clone());
        Ok(())
    }

    fn delete_calendar(&self, id: Uuid) -> Result<()> {
        self.inner.lock().unwrap().calendars.remove(&id);
        Ok(())
    }

    fn list_events(&self, range: Option<&TimeRange>) -> Result<Vec<Event>> {
        let inner = self.inner.lock().unwrap();
        Ok(match range {
            Some(range) => inner
                .events
                .values()
                .filter(|e| range.overlaps(e.starts_at, e.ends_at))
                .cloned()
                .collect(),
            None => inner.events.values().cloned().collect(),
        })
    }

    fn get_event(&self, id: Uuid) -> Result<Option<Event>> {
        Ok(self.inner.lock().unwrap().events.get(&id).cloned())
    }

    fn upsert_event(&self, event: &Event) -> Result<()> {
        event.validate()?;
        self.inner
            .lock()
            .unwrap()
            .events
            .insert(event.id, event.clone());
        Ok(())
    }

    fn delete_event(&self, id: Uuid) -> Result<()> {
        self.inner.lock().unwrap().events.remove(&id);
        Ok(())
    }

    fn list_reminders(&self, event_id: Uuid) -> Result<Vec<Reminder>> {
        let inner = self.inner.lock().unwrap();
        Ok(inner
            .reminders
            .values()
            .filter(|r| r.event_id == event_id)
            .cloned()
            .collect())
    }

    fn upsert_reminder(&self, reminder: &Reminder) -> Result<()> {
        reminder.validate()?;
        self.inner
            .lock()
            .unwrap()
            .reminders
            .insert(reminder.id, reminder.clone());
        Ok(())
    }

    fn delete_reminder(&self, id: Uuid) -> Result<()> {
        self.inner.lock().unwrap().reminders.remove(&id);
        Ok(())
    }

    fn list_tasks(&self, range: Option<&TimeRange>) -> Result<Vec<Task>> {
        let inner = self.inner.lock().unwrap();
        Ok(match range {
            Some(range) => inner
                .tasks
                .values()
                .filter(|t| t.due_at.is_some_and(|due| range.contains(due)))
                .cloned()
                .collect(),
            None => inner.tasks.values().cloned().collect(),
        })
    }

    fn get_task(&self, id: Uuid) -> Result<Option<Task>> {
        Ok(self.inner.lock().unwrap().tasks.get(&id).cloned())
    }

    fn upsert_task(&self, task: &Task) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .tasks
            .insert(task.id, task.clone());
        Ok(())
    }

    fn delete_task(&self, id: Uuid) -> Result<()> {
        self.inner.lock().unwrap().tasks.remove(&id);
        Ok(())
    }
}

/// Shared behavioral tests for any [`Store`] implementation.
///
/// M2's SQLite impl calls [`suite::run`] from its own tests so it is proven
/// behaviorally identical to the in-memory impl (S2.4). Every scenario starts
/// from a fresh store produced by `factory`.
#[doc(hidden)]
pub mod suite {
    use super::*;
    use chrono::TimeZone;

    /// Runs the whole suite against stores produced by `factory`.
    pub fn run(factory: &dyn Fn() -> Box<dyn Store>) {
        account_crud(factory);
        calendar_crud(factory);
        event_crud(factory);
        event_range_queries(factory);
        reminder_crud(factory);
        task_crud(factory);
        task_range_queries(factory);
    }

    fn now() -> chrono::DateTime<Utc> {
        Utc::now()
    }

    fn account(id: Uuid) -> Account {
        Account {
            id,
            kind: crate::model::AccountKind::Local,
            display_name: "On this Mac".into(),
            detail: "local store".into(),
            last_synced_at: None,
            status: crate::model::AccountStatus::Idle,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        }
    }

    fn calendar(id: Uuid, account_id: Uuid) -> Calendar {
        Calendar {
            id,
            account_id,
            name: "Personal".into(),
            color: "#1F6FEB".into(),
            enabled: true,
            event_count: 0,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        }
    }

    fn event(id: Uuid, calendar_id: Uuid, starts_at: chrono::DateTime<Utc>) -> Event {
        Event {
            id,
            calendar_id,
            uid: format!("{id}@example.com"),
            title: "Standup".into(),
            location: None,
            notes: None,
            starts_at,
            ends_at: starts_at + chrono::Duration::minutes(30),
            all_day: false,
            tz: None,
            rrule: None,
            exdates: vec![],
            etag: None,
            updated_at: now(),
            created_at: now(),
            deleted_at: None,
        }
    }

    fn task(id: Uuid, calendar_id: Uuid, due_at: Option<chrono::DateTime<Utc>>) -> Task {
        Task {
            id,
            calendar_id,
            title: "Lab report".into(),
            due_at,
            completed_at: None,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        }
    }

    fn account_crud(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let id = Uuid::new_v4();
        assert!(store.get_account(id).unwrap().is_none(), "starts empty");

        store.upsert_account(&account(id)).unwrap();
        assert_eq!(store.list_accounts().unwrap().len(), 1);
        assert_eq!(store.get_account(id).unwrap().unwrap().id, id);

        // Upsert updates in place rather than duplicating.
        store.upsert_account(&account(id)).unwrap();
        assert_eq!(store.list_accounts().unwrap().len(), 1);

        store.delete_account(id).unwrap();
        assert!(store.get_account(id).unwrap().is_none());
    }

    fn calendar_crud(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let account_id = Uuid::new_v4();
        let id = Uuid::new_v4();

        store.upsert_calendar(&calendar(id, account_id)).unwrap();
        assert_eq!(
            store.get_calendar(id).unwrap().unwrap().account_id,
            account_id
        );

        let mut updated = calendar(id, account_id);
        updated.enabled = false;
        updated.name = "Work".into();
        store.upsert_calendar(&updated).unwrap();
        let got = store.get_calendar(id).unwrap().unwrap();
        assert!(!got.enabled);
        assert_eq!(got.name, "Work");

        store.delete_calendar(id).unwrap();
        assert!(store.get_calendar(id).unwrap().is_none());
    }

    fn event_crud(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let calendar_id = Uuid::new_v4();
        let id = Uuid::new_v4();
        let t = now();

        store.upsert_event(&event(id, calendar_id, t)).unwrap();
        assert_eq!(
            store.get_event(id).unwrap().unwrap().calendar_id,
            calendar_id
        );
        assert_eq!(store.list_events(None).unwrap().len(), 1);

        store
            .upsert_event(&event(id, calendar_id, t + chrono::Duration::days(1)))
            .unwrap();
        assert_eq!(
            store.list_events(None).unwrap().len(),
            1,
            "upsert replaces, not duplicates"
        );

        store.delete_event(id).unwrap();
        assert!(store.get_event(id).unwrap().is_none());
    }

    fn event_range_queries(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let calendar_id = Uuid::new_v4();
        let midnight = chrono::NaiveDate::from_ymd_opt(2026, 8, 13)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let midnight = chrono::Utc.from_utc_datetime(&midnight);

        // One event inside the window, one ending exactly at the window start
        // (must be excluded), one spanning the whole window (must be included).
        store
            .upsert_event(&event(
                Uuid::new_v4(),
                calendar_id,
                midnight + chrono::Duration::hours(10),
            ))
            .unwrap();
        store
            .upsert_event(&event(
                Uuid::new_v4(),
                calendar_id,
                midnight - chrono::Duration::hours(2),
            ))
            .unwrap();
        let spanning = {
            let mut e = event(
                Uuid::new_v4(),
                calendar_id,
                midnight - chrono::Duration::days(1),
            );
            e.ends_at = midnight + chrono::Duration::days(2);
            e
        };
        store.upsert_event(&spanning).unwrap();

        let range = TimeRange::new(midnight, midnight + chrono::Duration::days(1)).unwrap();
        let hits = store.list_events(Some(&range)).unwrap();
        assert_eq!(
            hits.len(),
            2,
            "inside + spanning, not the one that ended before"
        );
    }

    fn reminder_crud(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let event_id = Uuid::new_v4();
        let a = Reminder {
            id: Uuid::new_v4(),
            event_id,
            offset_minutes: Some(-10),
            absolute_at: None,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };
        let b = Reminder {
            id: Uuid::new_v4(),
            event_id,
            offset_minutes: None,
            absolute_at: Some(now()),
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };

        store.upsert_reminder(&a).unwrap();
        store.upsert_reminder(&b).unwrap();
        assert_eq!(store.list_reminders(event_id).unwrap().len(), 2);
        assert_eq!(store.list_reminders(Uuid::new_v4()).unwrap().len(), 0);

        store.delete_reminder(a.id).unwrap();
        assert_eq!(store.list_reminders(event_id).unwrap().len(), 1);

        // Invalid reminders are rejected at write time.
        let invalid = Reminder {
            id: Uuid::new_v4(),
            event_id,
            offset_minutes: None,
            absolute_at: None,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };
        assert!(store.upsert_reminder(&invalid).is_err());
    }

    fn task_crud(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let calendar_id = Uuid::new_v4();
        let id = Uuid::new_v4();

        store
            .upsert_task(&task(id, calendar_id, Some(now())))
            .unwrap();
        assert_eq!(store.get_task(id).unwrap().unwrap().id, id);
        assert_eq!(store.list_tasks(None).unwrap().len(), 1);

        let mut done = task(id, calendar_id, Some(now()));
        done.completed_at = Some(now());
        store.upsert_task(&done).unwrap();
        assert!(store.get_task(id).unwrap().unwrap().completed_at.is_some());

        store.delete_task(id).unwrap();
        assert!(store.get_task(id).unwrap().is_none());
    }

    fn task_range_queries(factory: &dyn Fn() -> Box<dyn Store>) {
        let store = factory();
        let calendar_id = Uuid::new_v4();
        let day = chrono::NaiveDate::from_ymd_opt(2026, 8, 13)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let day = chrono::Utc.from_utc_datetime(&day);

        store
            .upsert_task(&task(Uuid::new_v4(), calendar_id, Some(day)))
            .unwrap();
        store
            .upsert_task(&task(
                Uuid::new_v4(),
                calendar_id,
                Some(day + chrono::Duration::days(1)),
            ))
            .unwrap();
        store
            .upsert_task(&task(Uuid::new_v4(), calendar_id, None))
            .unwrap();

        let range = TimeRange::new(day, day + chrono::Duration::days(1)).unwrap();
        let hits = store.list_tasks(Some(&range)).unwrap();
        assert_eq!(hits.len(), 1, "only the task due inside the window");
        assert!(
            store.list_tasks(None).unwrap().len() >= 3,
            "undated tasks appear with no range"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_store_passes_the_shared_suite() {
        suite::run(&|| Box::new(InMemoryStore::new()));
    }
}

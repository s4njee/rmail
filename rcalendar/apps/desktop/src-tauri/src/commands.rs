//! Tauri command handlers for Almanac (S2.3).
//!
//! Provides the typed JSON command surface consumed by the SolidJS frontend
//! and desktop shell.

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use calendar_core::model::{
    Account, AccountKind, AccountStatus, Calendar, Event, EventDraft, Occurrence, Task, TimeRange,
};
use calendar_core::recurrence::{
    delete_occurrence, edit_occurrence, expand, EditScope, OccurrenceChanges,
};
use calendar_core::Store;

use crate::search::{parse_date_query, SearchResults};
use crate::store::SqliteStore;

/// Shared application state managed by Tauri.
#[derive(Clone)]
pub struct AppState {
    pub store: Arc<SqliteStore>,
}

impl AppState {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }

    /// Lists all expanded occurrences overlapping `[from, to)`, optionally filtered
    /// by a list of `calendar_ids`.
    pub fn list_occurrences(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        calendar_ids: Option<Vec<Uuid>>,
    ) -> Result<Vec<OccurrenceItem>, String> {
        let range = TimeRange::new(from, to).map_err(|e| e.to_string())?;
        let events = self
            .store
            .list_events_for_expansion(&range, calendar_ids.as_deref())
            .map_err(|e| e.to_string())?;

        let mut items = Vec::new();
        for event in events {
            match expand(&event, &range) {
                Ok(expanded) => {
                    for occ in expanded {
                        items.push(OccurrenceItem {
                            occurrence: occ,
                            event: event.clone(),
                        });
                    }
                }
                Err(_) => {
                    // Fallback: If event overlaps the range as a single instance, include it
                    if event.starts_at < range.end && event.ends_at > range.start {
                        items.push(OccurrenceItem {
                            occurrence: Occurrence {
                                event_id: event.id,
                                starts_at: event.starts_at,
                                ends_at: event.ends_at,
                                all_day: event.all_day,
                            },
                            event: event.clone(),
                        });
                    }
                }
            }
        }

        items.sort_by_key(|item| (item.occurrence.starts_at, item.occurrence.ends_at));
        Ok(items)
    }

    /// Fetches a single event by UUID.
    pub fn get_event(&self, id: Uuid) -> Result<Option<Event>, String> {
        self.store.get_event(id).map_err(|e| e.to_string())
    }

    /// Saves an event (create or update). Supports `this | future | all` scoped edits
    /// for recurring events. Returns the updated/created event(s).
    pub fn save_event(
        &self,
        draft: EventDraft,
        id: Option<Uuid>,
        scope: Option<EditScope>,
        target_date: Option<NaiveDate>,
    ) -> Result<Vec<Event>, String> {
        let now = Utc::now();

        match id {
            None => {
                // Create brand new event
                let new_id = Uuid::new_v4();
                let new_event = Event {
                    id: new_id,
                    calendar_id: draft.calendar_id,
                    uid: format!("{new_id}@almanac.local"),
                    title: draft.title,
                    location: draft.location,
                    notes: draft.notes,
                    starts_at: draft.starts_at,
                    ends_at: draft.ends_at,
                    all_day: draft.all_day,
                    tz: draft.tz,
                    rrule: draft.rrule,
                    exdates: vec![],
                    etag: None,
                    created_at: now,
                    updated_at: now,
                    deleted_at: None,
                };
                new_event.validate().map_err(|e| e.to_string())?;
                self.store
                    .upsert_event(&new_event)
                    .map_err(|e| e.to_string())?;
                Ok(vec![new_event])
            }
            Some(existing_id) => {
                let existing = self
                    .store
                    .get_event(existing_id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("event {existing_id} not found"))?;

                let edit_scope = scope.unwrap_or(EditScope::All);
                let changes = OccurrenceChanges {
                    starts_at: draft.starts_at,
                    ends_at: draft.ends_at,
                    all_day: draft.all_day,
                    title: Some(draft.title.clone()),
                    location: draft.location.clone(),
                    notes: draft.notes.clone(),
                };

                let date = target_date.unwrap_or_else(|| draft.starts_at.date_naive());
                let resulting_events = edit_occurrence(&existing, edit_scope, date, &changes)
                    .map_err(|e| e.to_string())?;

                for evt in &resulting_events {
                    self.store.upsert_event(evt).map_err(|e| e.to_string())?;
                }
                Ok(resulting_events)
            }
        }
    }

    /// Deletes an event or an occurrence of a recurring event with `this | future | all` scope.
    /// Returns the remaining events replacing the series.
    pub fn delete_event(
        &self,
        id: Uuid,
        scope: Option<EditScope>,
        target_date: Option<NaiveDate>,
    ) -> Result<Vec<Event>, String> {
        let existing = self
            .store
            .get_event(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("event {id} not found"))?;

        let edit_scope = scope.unwrap_or(EditScope::All);
        let date = target_date.unwrap_or_else(|| existing.starts_at.date_naive());

        let resulting_events =
            delete_occurrence(&existing, edit_scope, date).map_err(|e| e.to_string())?;

        if resulting_events.is_empty() {
            self.store.delete_event(id).map_err(|e| e.to_string())?;
        } else {
            for evt in &resulting_events {
                self.store.upsert_event(evt).map_err(|e| e.to_string())?;
            }
        }

        Ok(resulting_events)
    }

    /// Enables or disables a calendar (sidebar toggle).
    pub fn set_calendar_enabled(&self, calendar_id: Uuid, enabled: bool) -> Result<(), String> {
        let mut calendar = self
            .store
            .get_calendar(calendar_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("calendar {calendar_id} not found"))?;

        calendar.enabled = enabled;
        calendar.updated_at = Utc::now();
        self.store
            .upsert_calendar(&calendar)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Lists all accounts alongside their associated calendars.
    pub fn list_accounts(&self) -> Result<Vec<AccountWithCalendars>, String> {
        let accounts = self.store.list_accounts().map_err(|e| e.to_string())?;
        let calendars = self.store.list_calendars().map_err(|e| e.to_string())?;

        let mut result = Vec::new();
        for acc in accounts {
            let acc_calendars = calendars
                .iter()
                .filter(|c| c.account_id == acc.id)
                .cloned()
                .collect();
            result.push(AccountWithCalendars {
                account: acc,
                calendars: acc_calendars,
            });
        }

        Ok(result)
    }

    /// Adds a new account and creates a default calendar for it.
    pub fn add_account(&self, spec: AddAccountPayload) -> Result<AccountWithCalendars, String> {
        let now = Utc::now();
        let account_id = Uuid::new_v4();
        let account = Account {
            id: account_id,
            kind: spec.kind,
            display_name: spec.display_name,
            detail: spec.detail,
            last_synced_at: None,
            status: AccountStatus::Idle,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        self.store
            .upsert_account(&account)
            .map_err(|e| e.to_string())?;

        let calendar = Calendar {
            id: Uuid::new_v4(),
            account_id,
            name: "Default".into(),
            color: "#1F6FEB".into(),
            enabled: true,
            event_count: 0,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        self.store
            .upsert_calendar(&calendar)
            .map_err(|e| e.to_string())?;

        Ok(AccountWithCalendars {
            account,
            calendars: vec![calendar],
        })
    }

    /// Connects a Google Account and performs initial sync.
    pub fn connect_google_account(
        &self,
        email: String,
        token: String,
    ) -> Result<AccountWithCalendars, String> {
        let account = tauri::async_runtime::block_on(crate::sync::connect_google_account_impl(
            &self.store,
            email,
            token,
        ))?;

        let calendars = self.store.list_calendars().map_err(|e| e.to_string())?;
        let account_cals = calendars
            .into_iter()
            .filter(|c| c.account_id == account.id)
            .collect();

        Ok(AccountWithCalendars {
            account,
            calendars: account_cals,
        })
    }

    /// Triggers account synchronization (supporting Google two-way sync).
    pub fn sync_account(&self, account_id: Uuid) -> Result<SyncReport, String> {
        let mut account = self
            .store
            .get_account(account_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("account {account_id} not found"))?;

        if account.kind == AccountKind::Google {
            let report = tauri::async_runtime::block_on(crate::sync::sync_google_account(
                &self.store,
                account_id,
                None,
            ))?;

            let now = Utc::now();
            account.last_synced_at = Some(now);
            account.status = AccountStatus::Idle;
            account.updated_at = now;
            self.store
                .upsert_account(&account)
                .map_err(|e| e.to_string())?;

            return Ok(report);
        }

        let now = Utc::now();
        account.last_synced_at = Some(now);
        account.status = AccountStatus::Idle;
        account.updated_at = now;
        self.store
            .upsert_account(&account)
            .map_err(|e| e.to_string())?;

        Ok(SyncReport {
            account_id,
            synced_at: now,
            success: true,
            message: "Local store synchronized".into(),
        })
    }

    /// Sets the background sync interval cadence in minutes.
    pub fn set_sync_interval(&self, minutes: u32) -> Result<(), String> {
        self.store
            .set_setting("sync_interval_minutes", &minutes.to_string())
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Lists tasks, optionally filtered by due date range.
    pub fn list_tasks(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<Task>, String> {
        let range = match (from, to) {
            (Some(start), Some(end)) => {
                Some(TimeRange::new(start, end).map_err(|e| e.to_string())?)
            }
            _ => None,
        };
        self.store
            .list_tasks(range.as_ref())
            .map_err(|e| e.to_string())
    }

    /// Toggles a task's completion status.
    pub fn toggle_task(&self, id: Uuid) -> Result<Task, String> {
        let mut task = self
            .store
            .get_task(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("task {id} not found"))?;

        let now = Utc::now();
        task.completed_at = if task.completed_at.is_some() {
            None
        } else {
            Some(now)
        };
        task.updated_at = now;
        self.store.upsert_task(&task).map_err(|e| e.to_string())?;
        Ok(task)
    }

    /// Searches events and tasks with text matching and natural-language date parsing.
    pub fn search(&self, query: String) -> Result<SearchResults, String> {
        let today = Utc::now().date_naive();
        let matched_date = parse_date_query(&query, today);

        let mut events = self
            .store
            .search_events(&query)
            .map_err(|e| e.to_string())?;
        let tasks = self.store.search_tasks(&query).map_err(|e| e.to_string())?;

        if let Some(date) = matched_date {
            if let (Some(start_naive), Some(end_naive)) = (
                date.and_hms_opt(0, 0, 0),
                date.succ_opt().and_then(|d| d.and_hms_opt(0, 0, 0)),
            ) {
                let start = DateTime::<Utc>::from_naive_utc_and_offset(start_naive, Utc);
                let end = DateTime::<Utc>::from_naive_utc_and_offset(end_naive, Utc);
                if let Ok(range) = TimeRange::new(start, end) {
                    if let Ok(date_events) = self.store.list_events_for_expansion(&range, None) {
                        for de in date_events {
                            if !events.iter().any(|e| e.id == de.id) {
                                events.push(de);
                            }
                        }
                    }
                }
            }
        }

        Ok(SearchResults {
            events,
            tasks,
            matched_date,
        })
    }

    /// Exports events to an iCalendar (.ics) string.
    pub fn export_ics(&self, calendar_id: Option<Uuid>) -> Result<String, String> {
        let calendars = self.store.list_calendars().map_err(|e| e.to_string())?;
        let target_cals: Vec<Uuid> = match calendar_id {
            Some(id) => vec![id],
            None => calendars
                .iter()
                .filter(|c| c.enabled)
                .map(|c| c.id)
                .collect(),
        };

        // Fetch events for target calendars across a wide window (e.g. 5 years)
        let now = Utc::now();
        let from = now - chrono::Duration::days(365 * 2);
        let to = now + chrono::Duration::days(365 * 3);
        let range = TimeRange::new(from, to).map_err(|e| e.to_string())?;
        let events = self
            .store
            .list_events_for_expansion(&range, Some(&target_cals))
            .map_err(|e| e.to_string())?;

        calendar_core::ical::write_ical(&events).map_err(|e| e.to_string())
    }

    /// Imports events from an iCalendar (.ics) string into the target calendar.
    pub fn import_ics(&self, calendar_id: Uuid, ics_content: String) -> Result<Vec<Event>, String> {
        let imported_events =
            calendar_core::ical::parse_ical(&ics_content).map_err(|e| e.to_string())?;

        let mut created = Vec::new();
        let now = Utc::now();

        for mut event in imported_events {
            event.id = Uuid::new_v4();
            event.calendar_id = calendar_id;
            event.created_at = now;
            event.updated_at = now;
            event.deleted_at = None;
            self.store.upsert_event(&event).map_err(|e| e.to_string())?;
            created.push(event);
        }

        Ok(created)
    }
}

/// An occurrence with its parent event metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OccurrenceItem {
    pub occurrence: Occurrence,
    pub event: Event,
}

/// An account bundled with its associated calendars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountWithCalendars {
    pub account: Account,
    pub calendars: Vec<Calendar>,
}

/// Specification for adding a new account.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddAccountPayload {
    pub kind: AccountKind,
    pub display_name: String,
    pub detail: String,
}

/// Result report from a sync attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncReport {
    pub account_id: Uuid,
    pub synced_at: DateTime<Utc>,
    pub success: bool,
    pub message: String,
}

/// Reports the version of the linked `calendar-core` crate.
#[tauri::command]
pub fn core_version() -> String {
    calendar_core::version().to_string()
}

/// Lists all expanded occurrences overlapping `[from, to)`, optionally filtered
/// by a list of `calendar_ids`.
#[tauri::command]
pub fn list_occurrences(
    state: tauri::State<'_, AppState>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    calendar_ids: Option<Vec<Uuid>>,
) -> Result<Vec<OccurrenceItem>, String> {
    state.list_occurrences(from, to, calendar_ids)
}

/// Fetches a single event by UUID.
#[tauri::command]
pub fn get_event(state: tauri::State<'_, AppState>, id: Uuid) -> Result<Option<Event>, String> {
    state.get_event(id)
}

/// Saves an event (create or update). Supports `this | future | all` scoped edits
/// for recurring events. Returns the updated/created event(s).
#[tauri::command]
pub fn save_event(
    state: tauri::State<'_, AppState>,
    draft: EventDraft,
    id: Option<Uuid>,
    scope: Option<EditScope>,
    target_date: Option<NaiveDate>,
) -> Result<Vec<Event>, String> {
    state.save_event(draft, id, scope, target_date)
}

/// Deletes an event or an occurrence of a recurring event with `this | future | all` scope.
/// Returns the remaining events replacing the series.
#[tauri::command]
pub fn delete_event(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    scope: Option<EditScope>,
    target_date: Option<NaiveDate>,
) -> Result<Vec<Event>, String> {
    state.delete_event(id, scope, target_date)
}

/// Enables or disables a calendar (sidebar toggle).
#[tauri::command]
pub fn set_calendar_enabled(
    state: tauri::State<'_, AppState>,
    calendar_id: Uuid,
    enabled: bool,
) -> Result<(), String> {
    state.set_calendar_enabled(calendar_id, enabled)
}

/// Lists all accounts alongside their associated calendars.
#[tauri::command]
pub fn list_accounts(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<AccountWithCalendars>, String> {
    state.list_accounts()
}

/// Adds a new account and creates a default calendar for it.
#[tauri::command]
pub fn add_account(
    state: tauri::State<'_, AppState>,
    spec: AddAccountPayload,
) -> Result<AccountWithCalendars, String> {
    state.add_account(spec)
}

/// Triggers account synchronization (staged local stub for v1).
#[tauri::command]
pub fn sync_account(
    state: tauri::State<'_, AppState>,
    account_id: Uuid,
) -> Result<SyncReport, String> {
    state.sync_account(account_id)
}

/// Sets the background sync interval cadence in minutes.
#[tauri::command]
pub fn set_sync_interval(state: tauri::State<'_, AppState>, minutes: u32) -> Result<(), String> {
    state.set_sync_interval(minutes)
}

/// Lists tasks, optionally filtered by due date range.
#[tauri::command]
pub fn list_tasks(
    state: tauri::State<'_, AppState>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> Result<Vec<Task>, String> {
    state.list_tasks(from, to)
}

/// Toggles a task's completion status.
#[tauri::command]
pub fn toggle_task(state: tauri::State<'_, AppState>, id: Uuid) -> Result<Task, String> {
    state.toggle_task(id)
}

/// Searches events and tasks with text matching and natural-language date parsing.
#[tauri::command]
pub fn search(state: tauri::State<'_, AppState>, query: String) -> Result<SearchResults, String> {
    state.search(query)
}

/// Exports calendar events to an RFC 5545 iCalendar (.ics) string.
#[tauri::command]
pub fn export_ics(
    state: tauri::State<'_, AppState>,
    calendar_id: Option<Uuid>,
) -> Result<String, String> {
    state.export_ics(calendar_id)
}

/// Imports events from an RFC 5545 iCalendar (.ics) string into the given calendar.
#[tauri::command]
pub fn import_ics(
    state: tauri::State<'_, AppState>,
    calendar_id: Uuid,
    ics_content: String,
) -> Result<Vec<Event>, String> {
    state.import_ics(calendar_id, ics_content)
}

/// Connects a Google Account and performs initial synchronization.
#[tauri::command]
pub fn connect_google_account(
    state: tauri::State<'_, AppState>,
    email: String,
    token: String,
) -> Result<AccountWithCalendars, String> {
    state.connect_google_account(email, token)
}

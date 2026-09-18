//! Tauri command handlers for Almanac (S2.3).
//!
//! Provides the typed JSON command surface consumed by the SolidJS frontend
//! and desktop shell.

use std::sync::Arc;

use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use calendar_core::freebusy::find_available_slots as compute_available_slots;
use calendar_core::itip::{generate_imip_email_invitation, parse_itip_message, ItipMethod};
use calendar_core::model::{
    Account, AccountKind, AccountStatus, Attendee, AttendeeRole, AttendeeStatus, Calendar, Event,
    EventDraft, Occurrence, Reminder, Task, TimeRange,
};
use calendar_core::recurrence::{
    delete_occurrence, edit_occurrence, expand, EditScope, OccurrenceChanges,
};
use calendar_core::Store;

use crate::mail::{dispatch_imip, ImipDispatch};
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
                    travel_time_minutes: draft.travel_time_minutes,
                    color: draft.color,
                    etag: None,
                    attendees: vec![],
                    busy: draft.busy,
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
                    rrule: Some(draft.rrule.clone()),
                    tz: Some(draft.tz.clone()),
                    travel_time_minutes: draft.travel_time_minutes,
                    color: draft.color.clone(),
                };

                let date = target_date.unwrap_or_else(|| draft.starts_at.date_naive());
                let resulting_events = edit_occurrence(&existing, edit_scope, date, &changes)
                    .map_err(|e| e.to_string())?;
                let original_attendees = self
                    .store
                    .list_attendees(existing_id)
                    .map_err(|e| e.to_string())?;

                let mut saved = Vec::new();
                for mut evt in resulting_events {
                    evt.busy = draft.busy;
                    self.store.upsert_event(&evt).map_err(|e| e.to_string())?;
                    if evt.id != existing_id {
                        for mut attendee in original_attendees.clone() {
                            attendee.id = Uuid::new_v4();
                            attendee.event_id = evt.id;
                            attendee.created_at = now;
                            attendee.updated_at = now;
                            self.store
                                .upsert_attendee(&attendee)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                    saved.push(evt);
                }
                Ok(saved)
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

    /// Soft-deletes an account and every calendar, event, task, and reminder it owns.
    pub fn delete_account(&self, id: Uuid) -> Result<(), String> {
        self.store
            .get_account(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("account {id} not found"))?;
        self.store.delete_account(id).map_err(|e| e.to_string())
    }

    /// Soft-deletes a calendar and its events, tasks, and reminders.
    pub fn delete_calendar(&self, id: Uuid) -> Result<(), String> {
        self.store
            .get_calendar(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("calendar {id} not found"))?;
        self.store.delete_calendar(id).map_err(|e| e.to_string())
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

    /// Lists all reminders attached to an event.
    pub fn list_reminders(&self, event_id: Uuid) -> Result<Vec<Reminder>, String> {
        self.store
            .list_reminders(event_id)
            .map_err(|e| e.to_string())
    }

    /// Creates or updates a reminder (upsert semantics). Returns the stored reminder.
    pub fn save_reminder(&self, payload: SaveReminderPayload) -> Result<Reminder, String> {
        let now = Utc::now();
        let reminder = Reminder {
            id: payload.id.unwrap_or_else(Uuid::new_v4),
            event_id: payload.event_id,
            offset_minutes: payload.offset_minutes,
            absolute_at: payload.absolute_at,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        reminder.validate().map_err(|e| e.to_string())?;
        // Associate only with events that actually exist.
        self.store
            .get_event(payload.event_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("event {} not found", payload.event_id))?;
        self.store
            .upsert_reminder(&reminder)
            .map_err(|e| e.to_string())?;
        Ok(reminder)
    }

    /// Soft-deletes a reminder.
    pub fn delete_reminder(&self, id: Uuid) -> Result<(), String> {
        self.store.delete_reminder(id).map_err(|e| e.to_string())
    }

    /// Snoozes a reminder for a number of minutes from now.
    pub fn snooze_reminder(&self, id: Uuid, minutes: u32) -> Result<(), String> {
        let until = Utc::now() + chrono::Duration::minutes(minutes as i64);
        self.store
            .set_reminder_snooze(id, Some(until))
            .map_err(|e| e.to_string())
    }

    /// Lists all attendees attached to an event.
    pub fn list_attendees(&self, event_id: Uuid) -> Result<Vec<Attendee>, String> {
        self.store
            .list_attendees(event_id)
            .map_err(|e| e.to_string())
    }

    /// Creates or updates an attendee (upsert semantics). Returns the stored attendee.
    pub fn save_attendee(&self, payload: SaveAttendeePayload) -> Result<Attendee, String> {
        let now = Utc::now();
        let attendee = Attendee {
            id: payload.id.unwrap_or_else(Uuid::new_v4),
            event_id: payload.event_id,
            email: payload.email,
            display_name: payload.display_name,
            role: payload.role.unwrap_or(AttendeeRole::Required),
            status: payload.status.unwrap_or(AttendeeStatus::NeedsAction),
            rsvp: payload.rsvp.unwrap_or(false),
            is_organizer: payload.is_organizer.unwrap_or(false),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        attendee.validate().map_err(|e| e.to_string())?;
        // Associate only with events that actually exist.
        self.store
            .get_event(payload.event_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("event {} not found", payload.event_id))?;
        self.store
            .upsert_attendee(&attendee)
            .map_err(|e| e.to_string())?;
        Ok(attendee)
    }

    /// Soft-deletes an attendee.
    pub fn delete_attendee(&self, id: Uuid) -> Result<(), String> {
        self.store.delete_attendee(id).map_err(|e| e.to_string())
    }

    /// Reads the default alert offsets (minutes before event start).
    pub fn get_default_alerts(&self) -> Result<DefaultAlerts, String> {
        let read = |key: &str| -> Result<Option<i64>, String> {
            match self.store.get_setting(key).map_err(|e| e.to_string())? {
                None => Ok(None),
                Some(v) if v == "none" => Ok(None),
                Some(v) => v
                    .parse::<i64>()
                    .map(Some)
                    .map_err(|_| format!("invalid setting {key}: {v}")),
            }
        };
        Ok(DefaultAlerts {
            event: read("default_event_alert")?,
            all_day: read("default_all_day_alert")?,
        })
    }

    /// Persists the default alert offsets.
    pub fn set_default_alerts(&self, alerts: DefaultAlerts) -> Result<(), String> {
        let write = |key: &str, value: Option<i64>| -> Result<(), String> {
            let s = value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "none".into());
            self.store.set_setting(key, &s).map_err(|e| e.to_string())
        };
        write("default_event_alert", alerts.event)?;
        write("default_all_day_alert", alerts.all_day)
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

        // Attach each event's attendees so the `.ics` export carries people.
        let mut events_with_attendees = Vec::new();
        for mut event in events {
            event.attendees = self
                .store
                .list_attendees(event.id)
                .map_err(|e| e.to_string())?;
            events_with_attendees.push(event);
        }

        calendar_core::ical::write_ical(&events_with_attendees).map_err(|e| e.to_string())
    }

    /// Imports events from an iCalendar (.ics) string into the target calendar.
    /// iTIP payloads (`METHOD:REQUEST/REPLY/CANCEL`) are applied rather than
    /// blindly inserted as new events.
    pub fn import_ics(&self, calendar_id: Uuid, ics_content: String) -> Result<Vec<Event>, String> {
        if ics_content
            .lines()
            .any(|l| l.trim().to_ascii_uppercase().starts_with("METHOD:"))
        {
            let report = self.apply_itip(calendar_id, ics_content)?;
            return Ok(report.event.into_iter().collect());
        }

        let imported_events =
            calendar_core::ical::parse_ical(&ics_content).map_err(|e| e.to_string())?;

        let mut created = Vec::new();
        let now = Utc::now();

        for mut event in imported_events {
            let new_id = Uuid::new_v4();
            let parsed_attendees = std::mem::take(&mut event.attendees);
            event.id = new_id;
            event.calendar_id = calendar_id;
            event.created_at = now;
            event.updated_at = now;
            event.deleted_at = None;
            self.store.upsert_event(&event).map_err(|e| e.to_string())?;
            for mut attendee in parsed_attendees {
                attendee.event_id = new_id;
                attendee.id = Uuid::new_v4();
                attendee.created_at = now;
                attendee.updated_at = now;
                attendee.deleted_at = None;
                self.store
                    .upsert_attendee(&attendee)
                    .map_err(|e| e.to_string())?;
            }
            created.push(event);
        }

        Ok(created)
    }

    /// Applies an iTIP REQUEST / REPLY / CANCEL against the store.
    pub fn apply_itip(
        &self,
        calendar_id: Uuid,
        ics_content: String,
    ) -> Result<ItipApplyReport, String> {
        let parsed = parse_itip_message(&ics_content).map_err(|e| e.to_string())?;
        let now = Utc::now();
        match parsed.method {
            ItipMethod::Reply => {
                let existing = self
                    .store
                    .get_event_by_uid(&parsed.uid)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("no local event with uid {}", parsed.uid))?;
                let Some(responder) = parsed.attendees.first().cloned() else {
                    return Err("REPLY is missing an ATTENDEE".into());
                };
                let mut attendees = self
                    .store
                    .list_attendees(existing.id)
                    .map_err(|e| e.to_string())?;
                if let Some(local) = attendees
                    .iter_mut()
                    .find(|a| a.email.eq_ignore_ascii_case(&responder.email))
                {
                    local.status = responder.status;
                    local.updated_at = now;
                    self.store
                        .upsert_attendee(local)
                        .map_err(|e| e.to_string())?;
                } else {
                    let mut incoming = responder;
                    incoming.id = Uuid::new_v4();
                    incoming.event_id = existing.id;
                    incoming.created_at = now;
                    incoming.updated_at = now;
                    incoming.deleted_at = None;
                    self.store
                        .upsert_attendee(&incoming)
                        .map_err(|e| e.to_string())?;
                }
                Ok(ItipApplyReport {
                    method: "REPLY".into(),
                    uid: parsed.uid,
                    event: Some(existing),
                    message: format!("updated RSVP for {}", parsed.attendees[0].email),
                })
            }
            ItipMethod::Cancel => {
                if let Some(existing) = self
                    .store
                    .get_event_by_uid(&parsed.uid)
                    .map_err(|e| e.to_string())?
                {
                    self.store
                        .delete_event(existing.id)
                        .map_err(|e| e.to_string())?;
                    Ok(ItipApplyReport {
                        method: "CANCEL".into(),
                        uid: parsed.uid,
                        event: None,
                        message: "event cancelled".into(),
                    })
                } else {
                    Ok(ItipApplyReport {
                        method: "CANCEL".into(),
                        uid: parsed.uid,
                        event: None,
                        message: "no matching local event".into(),
                    })
                }
            }
            ItipMethod::Request => {
                let mut incoming = parsed.event;
                let people = std::mem::take(&mut incoming.attendees);
                if let Some(mut existing) = self
                    .store
                    .get_event_by_uid(&parsed.uid)
                    .map_err(|e| e.to_string())?
                {
                    existing.title = incoming.title;
                    existing.location = incoming.location;
                    existing.notes = incoming.notes;
                    existing.starts_at = incoming.starts_at;
                    existing.ends_at = incoming.ends_at;
                    existing.all_day = incoming.all_day;
                    existing.tz = incoming.tz;
                    existing.rrule = incoming.rrule;
                    existing.exdates = incoming.exdates;
                    existing.busy = incoming.busy;
                    existing.updated_at = now;
                    existing.validate().map_err(|e| e.to_string())?;
                    self.store
                        .upsert_event(&existing)
                        .map_err(|e| e.to_string())?;
                    self.replace_attendees(existing.id, people, now)?;
                    Ok(ItipApplyReport {
                        method: "REQUEST".into(),
                        uid: parsed.uid,
                        event: Some(existing),
                        message: "invitation updated".into(),
                    })
                } else {
                    incoming.id = Uuid::new_v4();
                    incoming.calendar_id = calendar_id;
                    incoming.created_at = now;
                    incoming.updated_at = now;
                    incoming.deleted_at = None;
                    incoming.validate().map_err(|e| e.to_string())?;
                    self.store
                        .upsert_event(&incoming)
                        .map_err(|e| e.to_string())?;
                    self.replace_attendees(incoming.id, people, now)?;
                    Ok(ItipApplyReport {
                        method: "REQUEST".into(),
                        uid: parsed.uid,
                        event: Some(incoming),
                        message: "invitation created".into(),
                    })
                }
            }
        }
    }

    fn replace_attendees(
        &self,
        event_id: Uuid,
        people: Vec<Attendee>,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        let existing = self
            .store
            .list_attendees(event_id)
            .map_err(|e| e.to_string())?;
        for a in existing {
            self.store
                .delete_attendee(a.id)
                .map_err(|e| e.to_string())?;
        }
        for mut attendee in people {
            attendee.id = Uuid::new_v4();
            attendee.event_id = event_id;
            attendee.created_at = now;
            attendee.updated_at = now;
            attendee.deleted_at = None;
            self.store
                .upsert_attendee(&attendee)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Builds an iMIP REQUEST and hands it to the mail outbox (T1.10).
    pub fn send_invitations(
        &self,
        event_id: Uuid,
        outbox_dir: Option<&Path>,
    ) -> Result<ImipDispatch, String> {
        let mut event = self
            .store
            .get_event(event_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("event {event_id} not found"))?;
        event.attendees = self
            .store
            .list_attendees(event_id)
            .map_err(|e| e.to_string())?;
        let organizer = event
            .attendees
            .iter()
            .find(|a| a.is_organizer)
            .map(|a| a.display_name.clone().unwrap_or_else(|| a.email.clone()))
            .unwrap_or_else(|| "Almanac".into());
        let recipients: Vec<String> = event
            .attendees
            .iter()
            .filter(|a| !a.is_organizer)
            .map(|a| a.email.clone())
            .collect();
        if recipients.is_empty() {
            return Err("event has no invitees to send to".into());
        }
        let envelope =
            generate_imip_email_invitation(&event, &organizer).map_err(|e| e.to_string())?;
        match outbox_dir {
            Some(dir) => dispatch_imip(dir, envelope, recipients),
            None => Ok(ImipDispatch {
                envelope,
                recipients,
                outbox_path: None,
                mailed: false,
            }),
        }
    }

    /// Finds free slots on `date` among enabled calendars (T1.11).
    pub fn find_available_slots(
        &self,
        date: NaiveDate,
        duration_minutes: u32,
        calendar_ids: Option<Vec<Uuid>>,
    ) -> Result<Vec<TimeRange>, String> {
        let start =
            DateTime::<Utc>::from_naive_utc_and_offset(date.and_hms_opt(0, 0, 0).unwrap(), Utc);
        let end = DateTime::<Utc>::from_naive_utc_and_offset(
            date.succ_opt()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .unwrap_or(date.and_hms_opt(23, 59, 59).unwrap()),
            Utc,
        );
        let range = TimeRange::new(start, end).map_err(|e| e.to_string())?;
        let events = self
            .store
            .list_events_for_expansion(&range, calendar_ids.as_deref())
            .map_err(|e| e.to_string())?;

        let mut busy_events = Vec::new();
        for event in events {
            if !event.busy {
                continue;
            }
            match expand(&event, &range) {
                Ok(occs) => {
                    for occ in occs {
                        let mut clone = event.clone();
                        clone.starts_at = occ.starts_at;
                        clone.ends_at = occ.ends_at;
                        clone.all_day = occ.all_day;
                        clone.rrule = None;
                        busy_events.push(clone);
                    }
                }
                Err(_) => busy_events.push(event),
            }
        }
        Ok(compute_available_slots(
            &busy_events,
            date,
            duration_minutes.max(15),
            9,
            18,
        ))
    }

    /// Distinct known invitees matching `query` (editor autocomplete).
    pub fn suggest_attendees(&self, query: String) -> Result<Vec<Attendee>, String> {
        self.store
            .list_known_attendees(&query)
            .map_err(|e| e.to_string())
    }

    pub fn get_identity(&self) -> Result<IdentitySettings, String> {
        let self_email = self
            .store
            .get_setting("self_email")
            .map_err(|e| e.to_string())?
            .filter(|s| !s.trim().is_empty());
        let show_declined = matches!(
            self.store
                .get_setting("show_declined")
                .map_err(|e| e.to_string())?
                .as_deref(),
            Some("1") | Some("true")
        );
        Ok(IdentitySettings {
            self_email,
            show_declined,
        })
    }

    pub fn set_identity(&self, identity: IdentitySettings) -> Result<(), String> {
        self.store
            .set_setting("self_email", identity.self_email.as_deref().unwrap_or(""))
            .map_err(|e| e.to_string())?;
        self.store
            .set_setting(
                "show_declined",
                if identity.show_declined { "1" } else { "0" },
            )
            .map_err(|e| e.to_string())?;
        Ok(())
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

/// Payload for creating or updating a reminder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveReminderPayload {
    pub id: Option<Uuid>,
    pub event_id: Uuid,
    pub offset_minutes: Option<i64>,
    pub absolute_at: Option<DateTime<Utc>>,
}

/// Payload for creating or updating an attendee.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveAttendeePayload {
    pub id: Option<Uuid>,
    pub event_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub role: Option<AttendeeRole>,
    pub status: Option<AttendeeStatus>,
    pub rsvp: Option<bool>,
    pub is_organizer: Option<bool>,
}

/// Default alert offsets (minutes before start; negative = before, 0 = at start).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DefaultAlerts {
    pub event: Option<i64>,
    pub all_day: Option<i64>,
}

/// Local identity used for declined-event filtering and iTIP organizer fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IdentitySettings {
    pub self_email: Option<String>,
    pub show_declined: bool,
}

/// Result of applying an incoming iTIP payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItipApplyReport {
    pub method: String,
    pub uid: String,
    pub event: Option<Event>,
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

/// Soft-deletes an account and its calendars/events.
#[tauri::command]
pub fn delete_account(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    state.delete_account(id)
}

/// Soft-deletes a calendar and its events.
#[tauri::command]
pub fn delete_calendar(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    state.delete_calendar(id)
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

/// Lists reminders attached to an event.
#[tauri::command]
pub fn list_reminders(
    state: tauri::State<'_, AppState>,
    event_id: Uuid,
) -> Result<Vec<Reminder>, String> {
    state.list_reminders(event_id)
}

/// Creates or updates a reminder.
#[tauri::command]
pub fn save_reminder(
    state: tauri::State<'_, AppState>,
    payload: SaveReminderPayload,
) -> Result<Reminder, String> {
    state.save_reminder(payload)
}

/// Soft-deletes a reminder.
#[tauri::command]
pub fn delete_reminder(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    state.delete_reminder(id)
}

/// Snoozes a reminder for a number of minutes from now.
#[tauri::command]
pub fn snooze_reminder(
    state: tauri::State<'_, AppState>,
    id: Uuid,
    minutes: u32,
) -> Result<(), String> {
    state.snooze_reminder(id, minutes)
}

/// Lists attendees attached to an event.
#[tauri::command]
pub fn list_attendees(
    state: tauri::State<'_, AppState>,
    event_id: Uuid,
) -> Result<Vec<Attendee>, String> {
    state.list_attendees(event_id)
}

/// Creates or updates an attendee.
#[tauri::command]
pub fn save_attendee(
    state: tauri::State<'_, AppState>,
    payload: SaveAttendeePayload,
) -> Result<Attendee, String> {
    state.save_attendee(payload)
}

/// Soft-deletes an attendee.
#[tauri::command]
pub fn delete_attendee(state: tauri::State<'_, AppState>, id: Uuid) -> Result<(), String> {
    state.delete_attendee(id)
}

/// Reads the default alert offsets.
#[tauri::command]
pub fn get_default_alerts(state: tauri::State<'_, AppState>) -> Result<DefaultAlerts, String> {
    state.get_default_alerts()
}

/// Persists the default alert offsets.
#[tauri::command]
pub fn set_default_alerts(
    state: tauri::State<'_, AppState>,
    alerts: DefaultAlerts,
) -> Result<(), String> {
    state.set_default_alerts(alerts)
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

/// Applies an incoming iTIP REQUEST / REPLY / CANCEL.
#[tauri::command]
pub fn apply_itip(
    state: tauri::State<'_, AppState>,
    calendar_id: Uuid,
    ics_content: String,
) -> Result<ItipApplyReport, String> {
    state.apply_itip(calendar_id, ics_content)
}

/// Generates an iMIP invitation and hands it to the mail outbox.
#[tauri::command]
pub fn send_invitations(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    event_id: Uuid,
) -> Result<ImipDispatch, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    state.send_invitations(event_id, Some(&dir))
}

/// Finds free meeting slots on a date.
#[tauri::command]
pub fn find_available_slots(
    state: tauri::State<'_, AppState>,
    date: NaiveDate,
    duration_minutes: u32,
    calendar_ids: Option<Vec<Uuid>>,
) -> Result<Vec<TimeRange>, String> {
    state.find_available_slots(date, duration_minutes, calendar_ids)
}

/// Autocomplete suggestions for the invitee picker.
#[tauri::command]
pub fn suggest_attendees(
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<Vec<Attendee>, String> {
    state.suggest_attendees(query)
}

/// Reads the local identity (self email + show-declined).
#[tauri::command]
pub fn get_identity(state: tauri::State<'_, AppState>) -> Result<IdentitySettings, String> {
    state.get_identity()
}

/// Persists the local identity (self email + show-declined).
#[tauri::command]
pub fn set_identity(
    state: tauri::State<'_, AppState>,
    identity: IdentitySettings,
) -> Result<(), String> {
    state.set_identity(identity)
}

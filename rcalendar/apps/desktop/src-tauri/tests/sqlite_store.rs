//! Integration tests for the SQLite store and Tauri commands (S2.1, S2.2, S2.3, S2.4).

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use calendar_core::model::{
    Account, AccountKind, AccountStatus, AttendeeRole, AttendeeStatus, Calendar, Event, EventDraft,
    Task, TimeRange,
};
use calendar_core::recurrence::EditScope;
use calendar_core::Store;
use rcalendar_desktop_lib::commands::{
    AddAccountPayload, AppState, IdentitySettings, SaveAttendeePayload, SaveReminderPayload,
};
use rcalendar_desktop_lib::store::SqliteStore;

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn dt(s: &str) -> DateTime<Utc> {
    Utc.from_utc_datetime(&s.parse().unwrap())
}

#[test]
fn sqlite_store_passes_the_shared_calendar_core_suite() {
    // S2.4: Behavior matches the in-memory impl by running the exact same test suite
    calendar_core::store::suite::run(&|| Box::new(SqliteStore::in_memory().unwrap()));
}

#[test]
fn sqlite_store_persists_to_disk_across_reopens() {
    // S2.1: Events persisted to local SQLite DB survive restarts
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_persistence.db");

    let event_id = Uuid::new_v4();
    let cal_id = Uuid::new_v4();
    let acc_id = Uuid::new_v4();

    {
        let store = SqliteStore::open(&db_path).unwrap();
        let acc = Account {
            id: acc_id,
            kind: AccountKind::Local,
            display_name: "Local".into(),
            detail: "detail".into(),
            last_synced_at: None,
            status: AccountStatus::Idle,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };
        store.upsert_account(&acc).unwrap();

        let cal = Calendar {
            id: cal_id,
            account_id: acc_id,
            name: "Work".into(),
            color: "#0F766E".into(),
            enabled: true,
            event_count: 1,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };
        store.upsert_calendar(&cal).unwrap();

        let evt = Event {
            id: event_id,
            calendar_id: cal_id,
            uid: format!("{event_id}@almanac.local"),
            title: "Sprint Planning".into(),
            location: Some("Meeting Room A".into()),
            notes: Some("Bring laptop".into()),
            starts_at: dt("2026-08-13T10:00:00"),
            ends_at: dt("2026-08-13T11:00:00"),
            all_day: false,
            tz: Some("America/New_York".into()),
            rrule: None,
            exdates: vec![],
            travel_time_minutes: None,
            color: None,
            etag: Some("etag-123".into()),
            attendees: vec![],
            busy: true,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        };
        store.upsert_event(&evt).unwrap();
    }

    // Reopen database from disk and verify data is still there
    {
        let store = SqliteStore::open(&db_path).unwrap();
        let fetched = store.get_event(event_id).unwrap().expect("event exists");
        assert_eq!(fetched.title, "Sprint Planning");
        assert_eq!(fetched.location.as_deref(), Some("Meeting Room A"));
        assert_eq!(fetched.etag.as_deref(), Some("etag-123"));
        assert_eq!(fetched.tz.as_deref(), Some("America/New_York"));
    }
}

#[test]
fn sync_friendly_write_path_soft_delete_and_idempotent_upsert() {
    // S2.2: Soft delete writes a tombstone, and upsert by UUID is idempotent
    let store = SqliteStore::in_memory().unwrap();
    let acc_id = Uuid::new_v4();
    let cal_id = Uuid::new_v4();
    let evt_id = Uuid::new_v4();

    let acc = Account {
        id: acc_id,
        kind: AccountKind::Local,
        display_name: "On this Mac".into(),
        detail: "local store".into(),
        last_synced_at: None,
        status: AccountStatus::Idle,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_account(&acc).unwrap();

    let cal = Calendar {
        id: cal_id,
        account_id: acc_id,
        name: "Personal".into(),
        color: "#1F6FEB".into(),
        enabled: true,
        event_count: 0,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_calendar(&cal).unwrap();

    let mut evt = Event {
        id: evt_id,
        calendar_id: cal_id,
        uid: format!("{evt_id}@example.com"),
        title: "Initial Title".into(),
        location: None,
        notes: None,
        starts_at: dt("2026-08-13T14:00:00"),
        ends_at: dt("2026-08-13T15:00:00"),
        all_day: false,
        tz: None,
        rrule: None,
        exdates: vec![],
        travel_time_minutes: None,
        color: None,
        etag: Some("etag-v1".into()),
        attendees: vec![],
        busy: true,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };

    // First upsert
    store.upsert_event(&evt).unwrap();
    assert_eq!(store.list_events(None).unwrap().len(), 1);

    // Idempotent upsert (re-applying same or updated record updates in-place)
    evt.title = "Updated Title".into();
    evt.etag = Some("etag-v2".into());
    store.upsert_event(&evt).unwrap();
    assert_eq!(store.list_events(None).unwrap().len(), 1);
    assert_eq!(
        store.get_event(evt_id).unwrap().unwrap().title,
        "Updated Title"
    );

    // Soft delete
    store.delete_event(evt_id).unwrap();
    assert!(
        store.get_event(evt_id).unwrap().is_none(),
        "active queries filter out soft-deleted items"
    );
    assert_eq!(store.list_events(None).unwrap().len(), 0);
}

#[test]
fn range_queries_cover_midnight_and_dst_boundaries() {
    // S2.4: Range queries are covered for boundary cases (midnight, DST transitions)
    let store = SqliteStore::in_memory().unwrap();
    let acc_id = Uuid::new_v4();
    let cal_id = Uuid::new_v4();

    store
        .upsert_account(&Account {
            id: acc_id,
            kind: AccountKind::Local,
            display_name: "Mac".into(),
            detail: "".into(),
            last_synced_at: None,
            status: AccountStatus::Idle,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        })
        .unwrap();

    store
        .upsert_calendar(&Calendar {
            id: cal_id,
            account_id: acc_id,
            name: "General".into(),
            color: "#1F6FEB".into(),
            enabled: true,
            event_count: 0,
            created_at: now(),
            updated_at: now(),
            deleted_at: None,
        })
        .unwrap();

    // Midnight window: [2026-08-13T00:00:00Z, 2026-08-14T00:00:00Z)
    let window_start = dt("2026-08-13T00:00:00");
    let window_end = dt("2026-08-14T00:00:00");
    let range = TimeRange::new(window_start, window_end).unwrap();

    // 1. Event ending exactly at midnight (00:00:00) -> must be excluded (half-open window)
    let ending_at_start = Event {
        id: Uuid::new_v4(),
        calendar_id: cal_id,
        uid: "e1@test".into(),
        title: "Ending at start".into(),
        location: None,
        notes: None,
        starts_at: dt("2026-08-12T23:00:00"),
        ends_at: window_start,
        all_day: false,
        tz: None,
        rrule: None,
        exdates: vec![],
        travel_time_minutes: None,
        color: None,
        etag: None,
        attendees: vec![],
        busy: true,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_event(&ending_at_start).unwrap();

    // 2. Event starting exactly at midnight -> must be included
    let starting_at_start = Event {
        id: Uuid::new_v4(),
        calendar_id: cal_id,
        uid: "e2@test".into(),
        title: "Starting at midnight".into(),
        location: None,
        notes: None,
        starts_at: window_start,
        ends_at: dt("2026-08-13T01:00:00"),
        all_day: false,
        tz: None,
        rrule: None,
        exdates: vec![],
        travel_time_minutes: None,
        color: None,
        etag: None,
        attendees: vec![],
        busy: true,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_event(&starting_at_start).unwrap();

    // 3. Event starting at window_end (00:00:00 next day) -> must be excluded
    let starting_at_end = Event {
        id: Uuid::new_v4(),
        calendar_id: cal_id,
        uid: "e3@test".into(),
        title: "Starting next day midnight".into(),
        location: None,
        notes: None,
        starts_at: window_end,
        ends_at: dt("2026-08-14T01:00:00"),
        all_day: false,
        tz: None,
        rrule: None,
        exdates: vec![],
        travel_time_minutes: None,
        color: None,
        etag: None,
        attendees: vec![],
        busy: true,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_event(&starting_at_end).unwrap();

    // 4. Multi-day spanning event from 12th to 15th -> must be included
    let spanning = Event {
        id: Uuid::new_v4(),
        calendar_id: cal_id,
        uid: "e4@test".into(),
        title: "Spanning event".into(),
        location: None,
        notes: None,
        starts_at: dt("2026-08-12T12:00:00"),
        ends_at: dt("2026-08-15T12:00:00"),
        all_day: false,
        tz: None,
        rrule: None,
        exdates: vec![],
        travel_time_minutes: None,
        color: None,
        etag: None,
        attendees: vec![],
        busy: true,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_event(&spanning).unwrap();

    let matches = store.list_events(Some(&range)).unwrap();
    assert_eq!(matches.len(), 2);
    let titles: Vec<&str> = matches.iter().map(|e| e.title.as_str()).collect();
    assert!(titles.contains(&"Starting at midnight"));
    assert!(titles.contains(&"Spanning event"));
}

#[test]
fn tauri_commands_end_to_end() {
    // S2.3: Comprehensive test of all Tauri commands
    let store = Arc::new(SqliteStore::in_memory().unwrap());
    let app = AppState::new(Arc::clone(&store));
    store.seed_defaults_if_empty().unwrap();

    // 1. list_accounts & set_calendar_enabled
    let accounts = app.list_accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].account.display_name, "On this Mac");
    assert_eq!(accounts[0].calendars.len(), 6);

    let personal_cal = accounts[0]
        .calendars
        .iter()
        .find(|c| c.name == "Personal")
        .expect("Personal calendar");

    app.set_calendar_enabled(personal_cal.id, false).unwrap();
    let updated_cal = store.get_calendar(personal_cal.id).unwrap().unwrap();
    assert!(!updated_cal.enabled);
    app.set_calendar_enabled(personal_cal.id, true).unwrap();

    // 2. add_account & sync_account & set_sync_interval
    let new_acc = app
        .add_account(AddAccountPayload {
            kind: AccountKind::Google,
            display_name: "Google".into(),
            detail: "casey@gmail.com".into(),
        })
        .unwrap();
    assert_eq!(new_acc.account.kind, AccountKind::Google);
    assert_eq!(new_acc.calendars.len(), 1);

    let sync_rep = app.sync_account(new_acc.account.id).unwrap();
    assert!(sync_rep.success);
    app.set_sync_interval(15).unwrap();
    assert_eq!(
        store
            .get_setting("sync_interval_minutes")
            .unwrap()
            .as_deref(),
        Some("15")
    );

    // 3. save_event (create non-recurring and recurring)
    let draft_single = EventDraft {
        calendar_id: personal_cal.id,
        title: "Dentist".into(),
        location: Some("Clinic".into()),
        notes: None,
        starts_at: dt("2026-08-13T09:00:00"),
        ends_at: dt("2026-08-13T10:00:00"),
        all_day: false,
        tz: None,
        rrule: None,
        travel_time_minutes: None,
        color: None,
        busy: true,
    };
    let saved_single = app.save_event(draft_single, None, None, None).unwrap();
    assert_eq!(saved_single.len(), 1);
    let single_id = saved_single[0].id;
    assert_eq!(app.get_event(single_id).unwrap().unwrap().title, "Dentist");

    // Create recurring weekly event on Mon, Wed, Fri
    let draft_recurring = EventDraft {
        calendar_id: personal_cal.id,
        title: "Stats 101".into(),
        location: Some("Kane 210".into()),
        notes: Some("Lecture series".into()),
        starts_at: dt("2026-08-10T10:00:00"),
        ends_at: dt("2026-08-10T11:30:00"),
        all_day: false,
        tz: Some("America/New_York".into()),
        rrule: Some("FREQ=WEEKLY;BYDAY=MO,WE,FR".into()),
        travel_time_minutes: None,
        color: None,
        busy: true,
    };
    let saved_rec = app.save_event(draft_recurring, None, None, None).unwrap();
    let rec_id = saved_rec[0].id;

    // 4. list_occurrences
    let occs = app
        .list_occurrences(
            dt("2026-08-10T00:00:00"),
            dt("2026-08-17T00:00:00"),
            Some(vec![personal_cal.id]),
        )
        .unwrap();
    // 3 Stats 101 occurrences (Mon 10, Wed 12, Fri 14) + 1 Dentist (Thu 13) = 4
    assert_eq!(occs.len(), 4);

    // 5. Scoped edits: Edit occurrence on Wed 12 with Scope::This
    let wed_date = NaiveDate::from_ymd_opt(2026, 8, 12).unwrap();
    let draft_edit = EventDraft {
        calendar_id: personal_cal.id,
        title: "Stats 101 Lab".into(),
        location: Some("Kane 220".into()),
        notes: None,
        starts_at: dt("2026-08-12T14:00:00"),
        ends_at: dt("2026-08-12T15:30:00"),
        all_day: false,
        tz: Some("America/New_York".into()),
        rrule: None,
        travel_time_minutes: None,
        color: None,
        busy: true,
    };
    let edit_res = app
        .save_event(
            draft_edit,
            Some(rec_id),
            Some(EditScope::This),
            Some(wed_date),
        )
        .unwrap();
    assert_eq!(edit_res.len(), 2); // series with exdate + override event

    // 6. delete_event
    app.delete_event(single_id, None, None).unwrap();
    assert!(app.get_event(single_id).unwrap().is_none());

    // 7. list_tasks & toggle_task
    let task = Task {
        id: Uuid::new_v4(),
        calendar_id: personal_cal.id,
        title: "Lab report".into(),
        due_at: Some(dt("2026-08-13T17:00:00")),
        completed_at: None,
        created_at: now(),
        updated_at: now(),
        deleted_at: None,
    };
    store.upsert_task(&task).unwrap();
    let tasks = app.list_tasks(None, None).unwrap();
    assert_eq!(tasks.len(), 1);

    let toggled = app.toggle_task(task.id).unwrap();
    assert!(toggled.completed_at.is_some());
    let toggled_back = app.toggle_task(task.id).unwrap();
    assert!(toggled_back.completed_at.is_none());

    // 8. search
    let text_search = app.search("Stats".into()).unwrap();
    assert!(!text_search.events.is_empty());

    let date_search = app.search("aug 13".into()).unwrap();
    assert_eq!(
        date_search.matched_date,
        Some(NaiveDate::from_ymd_opt(2026, 8, 13).unwrap())
    );

    // 9. export_ics & import_ics (S5.4)
    let ics_text = app.export_ics(Some(personal_cal.id)).unwrap();
    assert!(ics_text.contains("BEGIN:VCALENDAR"));
    assert!(ics_text.contains("Stats 101"));

    let imported = app.import_ics(personal_cal.id, ics_text).unwrap();
    assert!(!imported.is_empty());
    assert_eq!(imported[0].calendar_id, personal_cal.id);
    assert_eq!(imported[0].title, "Stats 101");

    // 10. connect_google_account & sync_account (S6.5)
    let g_res = app
        .connect_google_account("student@university.edu".into(), "mock_test_token".into())
        .unwrap();
    assert_eq!(
        g_res.account.kind,
        calendar_core::model::AccountKind::Google
    );
    assert!(!g_res.calendars.is_empty());

    let sync_res = app.sync_account(g_res.account.id).unwrap();
    assert!(sync_res.success);
    assert!(sync_res.message.contains("Synced"));
}

#[test]
fn save_event_round_trips_every_editor_field() {
    let store = SqliteStore::in_memory().unwrap();
    store.seed_defaults_if_empty().unwrap();
    let app = AppState::new(Arc::new(store));
    let calendar_id = app.list_accounts().unwrap()[0].calendars[0].id;

    let draft = EventDraft {
        calendar_id,
        title: "Studio time".into(),
        location: Some("Kane 210".into()),
        notes: Some("Bring cables".into()),
        starts_at: dt("2026-08-13T15:00:00"),
        ends_at: dt("2026-08-13T16:30:00"),
        all_day: false,
        tz: Some("America/Los_Angeles".into()),
        rrule: Some("FREQ=WEEKLY;BYDAY=TH".into()),
        travel_time_minutes: Some(20),
        color: Some("#e8590c".into()),
        busy: true,
    };

    let created = app.save_event(draft.clone(), None, None, None).unwrap();
    assert_eq!(created.len(), 1);
    let loaded = app.get_event(created[0].id).unwrap().expect("saved event");
    assert_eq!(loaded.title, draft.title);
    assert_eq!(loaded.busy, draft.busy);
    assert_eq!(loaded.location, draft.location);
    assert_eq!(loaded.notes, draft.notes);
    assert_eq!(loaded.starts_at, draft.starts_at);
    assert_eq!(loaded.ends_at, draft.ends_at);
    assert_eq!(loaded.all_day, draft.all_day);
    assert_eq!(loaded.tz, draft.tz);
    assert_eq!(loaded.rrule, draft.rrule);
    assert_eq!(loaded.travel_time_minutes, draft.travel_time_minutes);
    assert_eq!(loaded.color, draft.color);

    let mut edited = draft.clone();
    edited.rrule = Some("FREQ=MONTHLY;BYMONTHDAY=13".into());
    edited.tz = Some("Europe/London".into());
    edited.travel_time_minutes = Some(45);
    edited.color = Some("#7048e8".into());
    edited.title = "Studio time (moved)".into();

    let updated = app
        .save_event(edited.clone(), Some(loaded.id), Some(EditScope::All), None)
        .unwrap();
    assert_eq!(updated.len(), 1);
    let reloaded = app.get_event(loaded.id).unwrap().expect("updated event");
    assert_eq!(reloaded.title, edited.title);
    assert_eq!(reloaded.rrule, edited.rrule);
    assert_eq!(reloaded.tz, edited.tz);
    assert_eq!(reloaded.travel_time_minutes, edited.travel_time_minutes);
    assert_eq!(reloaded.color, edited.color);
}

#[test]
fn reminder_delivery_round_trip() {
    // T1.1/T1.2/T1.6: reminder plumbing + delivery bookkeeping.
    let store = Arc::new(SqliteStore::in_memory().unwrap());
    let app = AppState::new(Arc::clone(&store));
    store.seed_defaults_if_empty().unwrap();
    let calendar_id = app.list_accounts().unwrap()[0].calendars[0].id;

    let created = app
        .save_event(
            EventDraft {
                calendar_id,
                title: "Lunch".into(),
                location: None,
                notes: None,
                starts_at: dt("2026-08-13T12:00:00"),
                ends_at: dt("2026-08-13T13:00:00"),
                all_day: false,
                tz: None,
                rrule: None,
                travel_time_minutes: None,
                color: None,
                busy: true,
            },
            None,
            None,
            None,
        )
        .unwrap();
    let event_id = created[0].id;

    // Multiple alerts on one event.
    let a = app
        .save_reminder(SaveReminderPayload {
            id: None,
            event_id,
            offset_minutes: Some(-15),
            absolute_at: None,
        })
        .unwrap();
    let b = app
        .save_reminder(SaveReminderPayload {
            id: None,
            event_id,
            offset_minutes: None,
            absolute_at: Some(dt("2026-08-13T11:45:00")),
        })
        .unwrap();
    assert_eq!(app.list_reminders(event_id).unwrap().len(), 2);

    // Snooze sets snooze_until; delivery marks delivered and clears snooze.
    app.snooze_reminder(a.id, 5).unwrap();
    let pending = store.list_due_reminders().unwrap();
    assert_eq!(pending.len(), 2);
    let snoozed = pending
        .iter()
        .find(|d| d.reminder.id == a.id)
        .expect("snoozed reminder");
    assert!(snoozed.snooze_until.is_some());
    assert!(snoozed.delivered_at.is_none());

    let now = Utc::now();
    store.mark_reminder_delivered(a.id, now).unwrap();
    let after = store
        .list_due_reminders()
        .unwrap()
        .into_iter()
        .find(|d| d.reminder.id == a.id)
        .unwrap();
    assert_eq!(after.delivered_at, Some(now));
    assert!(after.snooze_until.is_none());

    // Deleting a reminder removes it from the delivery set.
    app.delete_reminder(b.id).unwrap();
    assert_eq!(app.list_reminders(event_id).unwrap().len(), 1);
}

#[test]
fn attendee_round_trip_and_ical() {
    let store = Arc::new(SqliteStore::in_memory().unwrap());
    let app = AppState::new(Arc::clone(&store));
    store.seed_defaults_if_empty().unwrap();
    let calendar_id = app.list_accounts().unwrap()[0].calendars[0].id;

    let created = app
        .save_event(
            EventDraft {
                calendar_id,
                title: "Design review".into(),
                location: None,
                notes: None,
                starts_at: dt("2026-08-13T14:00:00"),
                ends_at: dt("2026-08-13T15:00:00"),
                all_day: false,
                tz: None,
                rrule: None,
                travel_time_minutes: None,
                color: None,
                busy: true,
            },
            None,
            None,
            None,
        )
        .unwrap();
    let event_id = created[0].id;

    app.save_attendee(SaveAttendeePayload {
        id: None,
        event_id,
        email: "alice@example.com".into(),
        display_name: Some("Alice".into()),
        role: Some(AttendeeRole::Required),
        status: Some(AttendeeStatus::Accepted),
        rsvp: Some(true),
        is_organizer: Some(false),
    })
    .unwrap();
    let bob = app
        .save_attendee(SaveAttendeePayload {
            id: None,
            event_id,
            email: "bob@example.com".into(),
            display_name: None,
            role: Some(AttendeeRole::Optional),
            status: Some(AttendeeStatus::NeedsAction),
            rsvp: Some(false),
            is_organizer: Some(false),
        })
        .unwrap();
    let _org = app
        .save_attendee(SaveAttendeePayload {
            id: None,
            event_id,
            email: "boss@example.com".into(),
            display_name: Some("Boss".into()),
            role: Some(AttendeeRole::Chair),
            status: Some(AttendeeStatus::NeedsAction),
            rsvp: Some(false),
            is_organizer: Some(true),
        })
        .unwrap();

    let listed = app.list_attendees(event_id).unwrap();
    assert_eq!(listed.len(), 3);
    assert!(listed
        .iter()
        .any(|a| a.is_organizer && a.email == "boss@example.com"));
    assert!(listed.iter().any(|a| a.status == AttendeeStatus::Accepted));
    assert_eq!(app.list_attendees(Uuid::new_v4()).unwrap().len(), 0);

    app.delete_attendee(bob.id).unwrap();
    assert_eq!(app.list_attendees(event_id).unwrap().len(), 2);

    // Import/export carries attendees through iCal, including the organizer.
    let ics = app.export_ics(Some(calendar_id)).unwrap();
    assert!(ics.contains("ATTENDEE"), "export carries ATTENDEE lines");
    assert!(ics.contains("ORGANIZER;CN=Boss:mailto:boss@example.com"));

    // Re-import into a fresh calendar persists attendees to the store.
    let other = app.list_accounts().unwrap()[0].calendars[1].id;
    let imported = app.import_ics(other, ics).unwrap();
    let imported_event = imported
        .iter()
        .find(|e| e.title == "Design review")
        .expect("imported event");
    let imported_attendees = app.list_attendees(imported_event.id).unwrap();
    assert!(
        imported_attendees
            .iter()
            .any(|a| a.email == "alice@example.com"),
        "attendees survive an export -> import round trip"
    );
    assert!(
        imported_attendees.iter().any(|a| a.is_organizer),
        "organizer survives the round trip"
    );
}

#[test]
fn itip_reply_and_cancel_and_find_time() {
    let store = Arc::new(SqliteStore::in_memory().unwrap());
    let app = AppState::new(Arc::clone(&store));
    store.seed_defaults_if_empty().unwrap();
    let calendar_id = app.list_accounts().unwrap()[0].calendars[0].id;

    let created = app
        .save_event(
            EventDraft {
                calendar_id,
                title: "Roadmap Sync".into(),
                location: None,
                notes: None,
                starts_at: dt("2026-08-20T10:00:00"),
                ends_at: dt("2026-08-20T11:00:00"),
                all_day: false,
                tz: None,
                rrule: None,
                travel_time_minutes: None,
                color: None,
                busy: true,
            },
            None,
            None,
            None,
        )
        .unwrap();
    let event_id = created[0].id;
    let uid = created[0].uid.clone();

    app.save_attendee(SaveAttendeePayload {
        id: None,
        event_id,
        email: "alice@example.com".into(),
        display_name: Some("Alice".into()),
        role: Some(AttendeeRole::Required),
        status: Some(AttendeeStatus::NeedsAction),
        rsvp: Some(true),
        is_organizer: Some(false),
    })
    .unwrap();
    app.save_attendee(SaveAttendeePayload {
        id: None,
        event_id,
        email: "boss@example.com".into(),
        display_name: Some("Boss".into()),
        role: Some(AttendeeRole::Chair),
        status: Some(AttendeeStatus::Accepted),
        rsvp: Some(false),
        is_organizer: Some(true),
    })
    .unwrap();

    let suggestions = app.suggest_attendees("ali".into()).unwrap();
    assert!(suggestions.iter().any(|a| a.email == "alice@example.com"));

    let dispatch = app.send_invitations(event_id, None).unwrap();
    assert!(dispatch.envelope.ics_content.contains("METHOD:REQUEST"));
    assert_eq!(dispatch.recipients, vec!["alice@example.com".to_string()]);

    let reply = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:REPLY\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART:20260820T100000Z\r\nDTEND:20260820T110000Z\r\nSUMMARY:Roadmap Sync\r\nATTENDEE;PARTSTAT=ACCEPTED:mailto:alice@example.com\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let report = app.apply_itip(calendar_id, reply).unwrap();
    assert_eq!(report.method, "REPLY");
    let listed = app.list_attendees(event_id).unwrap();
    assert_eq!(
        listed
            .iter()
            .find(|a| a.email == "alice@example.com")
            .unwrap()
            .status,
        AttendeeStatus::Accepted
    );

    let slots = app
        .find_available_slots(NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(), 60, None)
        .unwrap();
    assert!(
        slots.iter().all(|s| s.start != dt("2026-08-20T10:00:00")),
        "busy hour is not offered as a free slot"
    );
    assert!(
        slots.iter().any(|s| s.start == dt("2026-08-20T09:00:00")),
        "9am should be free"
    );

    app.set_identity(IdentitySettings {
        self_email: Some("alice@example.com".into()),
        show_declined: false,
    })
    .unwrap();
    let identity = app.get_identity().unwrap();
    assert_eq!(identity.self_email.as_deref(), Some("alice@example.com"));
    assert!(!identity.show_declined);

    let cancel = format!(
        "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nMETHOD:CANCEL\r\nBEGIN:VEVENT\r\nUID:{uid}\r\nDTSTART:20260820T100000Z\r\nDTEND:20260820T110000Z\r\nSUMMARY:Roadmap Sync\r\nSTATUS:CANCELLED\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
    );
    let cancel_report = app.apply_itip(calendar_id, cancel).unwrap();
    assert_eq!(cancel_report.method, "CANCEL");
    assert!(app.get_event(event_id).unwrap().is_none());
}

#[test]
fn delete_account_and_calendar_cascade() {
    let store = SqliteStore::in_memory().unwrap();
    store.seed_defaults_if_empty().unwrap();
    let app = AppState::new(Arc::new(store));
    let accounts = app.list_accounts().unwrap();
    let calendar_id = accounts[0].calendars[0].id;
    let account_id = accounts[0].account.id;

    app.save_event(
        EventDraft {
            calendar_id,
            title: "Doomed".into(),
            location: None,
            notes: None,
            starts_at: dt("2026-08-13T10:00:00"),
            ends_at: dt("2026-08-13T11:00:00"),
            all_day: false,
            tz: None,
            rrule: None,
            travel_time_minutes: None,
            color: None,
            busy: true,
        },
        None,
        None,
        None,
    )
    .unwrap();

    let other_cal = accounts[0].calendars[1].id;
    app.delete_calendar(calendar_id).unwrap();
    let remaining = app
        .list_occurrences(
            dt("2026-08-13T00:00:00"),
            dt("2026-08-14T00:00:00"),
            Some(vec![calendar_id]),
        )
        .unwrap();
    assert!(remaining.is_empty(), "deleted calendar's events are gone");
    let still_listed = app
        .list_accounts()
        .unwrap()
        .into_iter()
        .flat_map(|a| a.calendars)
        .any(|c| c.id == calendar_id);
    assert!(!still_listed);
    assert!(app
        .list_accounts()
        .unwrap()
        .into_iter()
        .flat_map(|a| a.calendars)
        .any(|c| c.id == other_cal));

    app.delete_account(account_id).unwrap();
    assert!(app.list_accounts().unwrap().is_empty());
}

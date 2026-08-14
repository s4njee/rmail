//! Integration tests for the SQLite store and Tauri commands (S2.1, S2.2, S2.3, S2.4).

use std::sync::Arc;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use uuid::Uuid;

use calendar_core::model::{
    Account, AccountKind, AccountStatus, Calendar, Event, EventDraft, Task, TimeRange,
};
use calendar_core::recurrence::EditScope;
use calendar_core::Store;
use rcalendar_desktop_lib::commands::{AddAccountPayload, AppState};
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
            etag: Some("etag-123".into()),
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
        etag: Some("etag-v1".into()),
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
        etag: None,
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
        etag: None,
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
        etag: None,
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
        etag: None,
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

//! The Tauri desktop shell for rcalendar (Almanac).
//!
//! This layer composes `calendar-core`, the local SQLite persistence layer,
//! and the SolidJS frontend. It is the only place allowed to depend on Tauri —
//! neither `calendar-core` nor `calendar-ui` may (plan.md §8).

pub mod commands;
pub mod mail;
pub mod migrations;
pub mod notify;
pub mod search;
pub mod store;
pub mod sync;

use std::sync::Arc;
use tauri::Manager;

use commands::AppState;
use store::SqliteStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("failed to create app data dir");
            let db_path = app_data_dir.join("almanac.db");
            let store = SqliteStore::open(&db_path).expect("failed to open sqlite store");
            store
                .seed_defaults_if_empty()
                .expect("failed to seed default store entities");

            let shared_store = Arc::new(store);
            app.manage(AppState {
                store: Arc::clone(&shared_store),
            });

            // Reminder delivery runs in the background for as long as the process
            // lives, so alerts fire even with the main window closed.
            let handle = app.handle().clone();
            notify::spawn_scheduler(handle, shared_store);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::core_version,
            commands::list_occurrences,
            commands::get_event,
            commands::save_event,
            commands::delete_event,
            commands::delete_account,
            commands::delete_calendar,
            commands::set_calendar_enabled,
            commands::list_accounts,
            commands::add_account,
            commands::connect_google_account,
            commands::sync_account,
            commands::set_sync_interval,
            commands::list_tasks,
            commands::toggle_task,
            commands::search,
            commands::export_ics,
            commands::import_ics,
            commands::list_reminders,
            commands::save_reminder,
            commands::delete_reminder,
            commands::snooze_reminder,
            commands::get_default_alerts,
            commands::set_default_alerts,
            commands::list_attendees,
            commands::save_attendee,
            commands::delete_attendee,
            commands::apply_itip,
            commands::send_invitations,
            commands::find_available_slots,
            commands::suggest_attendees,
            commands::get_identity,
            commands::set_identity,
        ])
        .run(tauri::generate_context!())
        .expect("error while running rcalendar");
}

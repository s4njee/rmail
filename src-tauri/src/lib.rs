//! Quill — Tauri shell.
//!
//! Foundation (Epic 1): window shell, plugins, security posture.
//! Theme (Epic 2): settings persistence + pre-paint init script.
//! IPC (Epic 3): the command surface, the `--demo` seed, push events.
//! Store (Epic 12): SQLite is the single source of truth; the sync engine
//! writes to it and the UI never awaits the network.

mod commands;
mod settings;
mod sync;

use quill_store::sqlite::SqliteStore;
use quill_store::types::{ConnectivityUpdate, FootprintUpdate, StoreEvent};
use tauri::{Emitter, Manager};

fn ioerr(e: String) -> std::io::Error {
    std::io::Error::other(e)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // External links (http/https/mailto) open in the OS browser, never in
        // the app webview. The `opener` permission is granted in
        // capabilities/default.json.
        .plugin(tauri_plugin_opener::init())
        // Persist each window's size and position across restarts (Epic 1.2).
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![
            commands::list_folders,
            commands::list_accounts,
            commands::footprint,
            commands::page_messages,
            commands::get_message,
            commands::attachment_path,
            commands::mark_read,
            commands::star,
            commands::archive,
            commands::delete,
            commands::send,
            commands::list_events,
            commands::create_event,
            commands::update_event,
            commands::delete_event,
            commands::add_account,
            commands::remove_account,
            commands::test_connection,
            commands::save_draft,
            settings::get_settings,
            settings::set_settings,
        ])
        .setup(|app| {
            // Demo mode (Epic 3.4): explicit `--demo`, or debug builds by
            // default so Epics 4–8 can be built and design-reviewed against
            // the mock content before sync exists.
            let demo = std::env::args().any(|a| a == "--demo") || cfg!(debug_assertions);
            let attachments = app.path().app_data_dir()?.join("attachments");

            // The store (Epic 12.1): an in-memory SQLite seeded with the demo
            // content in demo mode; a persistent file otherwise.
            let mut store = if demo {
                SqliteStore::open_in_memory().map_err(ioerr)?
            } else {
                SqliteStore::open(&app.path().app_data_dir()?.join("quill.sqlite"))
                    .map_err(ioerr)?
            };
            store.set_attachments_root(attachments.clone());
            if demo {
                store.seed_demo(&attachments).map_err(ioerr)?;
            }
            app.manage(store);

            // Sync loops (Epic 12.2) — the demo's placeholder accounts have no
            // real credentials, so they're skipped; real accounts sync on
            // their cadence.
            sync::spawn_sync_loops(app.handle().clone());

            // Push events (Epic 3.2): the frontend never polls. In demo mode,
            // push a connectivity update shortly after launch so the pipe is
            // exercised end to end; the real source lands in Epic 12.
            if demo {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .ok();
                    let _ = handle.emit(
                        "store",
                        StoreEvent::Connectivity(ConnectivityUpdate {
                            state: "synced".to_string(),
                            last_synced_at_ms: now,
                        }),
                    );
                });
            }

            // Footprint readout (Epic 11.2): on-disk cache only (D2), computed
            // on a ≥5s timer in Rust on a background thread and pushed over the
            // channel — never computed per render, never on the UI thread. The
            // figure reflects the store's real cache and moves as accounts are
            // added or removed.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || loop {
                    let bytes = handle.state::<SqliteStore>().total_disk_bytes();
                    let _ = handle.emit(
                        "store",
                        StoreEvent::Footprint(FootprintUpdate {
                            on_disk_bytes: bytes,
                        }),
                    );
                    std::thread::sleep(std::time::Duration::from_secs(5));
                });
            }

            // The main window is created here rather than in tauri.conf.json
            // so the saved treatment can be injected as an initialization
            // script: it runs before first paint, which is what guarantees no
            // flash of the wrong theme (Epic 2.3). Window parameters that used
            // to live in the config are spelled out so the two stay in one
            // place.
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Quill")
            .inner_size(1280.0, 800.0)
            .min_inner_size(640.0, 480.0)
            .center()
            .resizable(true)
            .decorations(true)
            .initialization_script(settings::theme_init_script(app.handle()))
            .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

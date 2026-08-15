//! Quill — Tauri shell.
//!
//! Foundation (Epic 1): window shell, plugins, security posture.
//! Theme (Epic 2): settings persistence + pre-paint init script.
//! IPC (Epic 3): the command surface, the `--demo` seed, push events.
//! Store (Epic 12): SQLite is the single source of truth; the sync engine
//! writes to it and the UI never awaits the network.

mod commands;
mod diagnostics;
mod oauth_config;
mod settings;
mod sync;

use quill_store::sqlite::SqliteStore;
use quill_store::types::{ConnectivityUpdate, FootprintUpdate, StoreEvent};
use tauri::{Emitter, Manager};

fn ioerr(e: String) -> std::io::Error {
    std::io::Error::other(e)
}

/// The app handle, captured in setup so the deep-link / tray handlers can emit
/// store events after startup (P1.5).
static APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// A `mailto:`/`webcal:` deep link (or a tray action) → the frontend opens a
/// compose via `StoreEvent::Mailto`.
fn handle_deep_link(url: url::Url) {
    let Some(app) = APP.get() else {
        return;
    };
    let mut payload = quill_store::types::MailtoPayload::default();
    if url.scheme() == "mailto" {
        // mailto:a@b,cc@d?subject=X&body=Y
        payload.to = url.path().trim().to_string();
        for (k, v) in url.query_pairs() {
            match k.as_ref() {
                "subject" => payload.subject = v.to_string(),
                "body" => payload.body = v.to_string(),
                "to" => payload.to = v.to_string(),
                _ => {}
            }
        }
    } else if url.scheme() == "webcal" {
        // Open the feed in the browser for now — subscribing to a calendar from
        // a webcal: link is a follow-up.
        use tauri_plugin_opener::OpenerExt;
        let _ = app
            .opener()
            .open_url(url.to_string().replace("webcal://", "https://"), None::<&str>);
        return;
    }
    let _ = app.emit("store", quill_store::types::StoreEvent::Mailto(payload));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Crash capture (E2.3): installed before the builder runs so a panic during
    // startup is still recorded. The hook is panic-hook-safe (no logging, no
    // network) — under release `panic = "abort"` the process still dies after.
    diagnostics::install_panic_hook();

    tauri::Builder::default()
        // External links (http/https/mailto) open in the OS browser, never in
        // the app webview. The `opener` permission is granted in
        // capabilities/default.json.
        .plugin(tauri_plugin_opener::init())
        // Persist each window's size and position across restarts (Epic 1.2).
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // Auto-update (E2.2): checks the configured endpoint for a signed update
        // manifest. Requires `plugins.updater` (endpoints + pubkey) in tauri.conf.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Unified logging (E2.3): one collector for Rust `log` records and JS
        // records (via `@tauri-apps/plugin-log`), written to rotating local
        // files in the app log dir. The builder level is left at Trace so the
        // persisted `log_level` setting is the single control (applied in
        // diagnostics::init via log::set_max_level).
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Trace)
                // The dispatch-level filter makes `log::set_max_level` the single
                // gate for BOTH Rust records and JS records: the JS side calls
                // `log::logger().log()` directly, bypassing the `log!` macro's own
                // level check, so without this the plugin's Trace level would let
                // every JS record through regardless of the Settings log level.
                .filter(|metadata| metadata.level() <= log::max_level())
                .max_file_size(1_000_000)
                .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepAll)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("quill.log".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
                ])
                .build(),
        )
        // P1.5 OS integration: `mailto:` / `webcal:` deep links open a compose,
        // and the app can register to launch at login.
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            commands::list_folders,
            commands::list_accounts,
            commands::footprint,
            commands::page_messages,
            commands::get_message,
            commands::attachment_path,
            commands::mark_read,
            commands::star,
            commands::mark_answered,
            commands::mark_forwarded,
            commands::archive,
            commands::delete,
            commands::bulk_action,
            commands::restore_message,
            commands::set_snoozed,
            commands::schedule_send,
            commands::list_scheduled,
            commands::cancel_scheduled,
            commands::suggest_recipients,
            commands::recent_recipients,
            commands::hide_recipient,
            commands::list_contact_groups,
            commands::create_contact_group,
            commands::delete_contact_group,
            commands::suggest_groups,
            commands::add_contact_to_group,
            commands::remove_contact_from_group,
            commands::contact_group_members,
            commands::list_saved_searches,
            commands::save_search,
            commands::delete_saved_search,
            commands::send,
            commands::list_events,
            commands::list_calendars,
            commands::remove_calendar_source,
            commands::restore_calendar_source,
            commands::list_removed_calendar_sources,
            commands::create_event,
            commands::update_event,
            commands::delete_event,
            commands::restore_event,
            commands::duplicate_event,
            commands::add_account,
            commands::update_account,
            commands::remove_account,
            commands::list_provider_presets,
            commands::discover_settings,
            commands::test_connection_settings,
            commands::discover_mail_folders,
            commands::set_synced_folders,
            commands::list_synced_folders,
            commands::account_removal_info,
            commands::save_draft,
            commands::latest_draft,
            commands::set_launch_at_login,
            commands::is_launch_at_login,
            commands::export_message_eml,
            commands::import_messages,
            commands::backup_now,
            commands::restore_backup,
            commands::search,
            commands::rebuild_search_index,
            commands::rebuild_search_index_progress,
            commands::cancel_search_rebuild,
            commands::search_index_status,
            commands::sync_calendar,
            commands::sync_now,
            commands::sync_account_now,
            commands::discover_caldav,
            commands::get_oauth_init,
            commands::exchange_oauth_code,
            commands::wait_oauth_code,
            commands::reauthorize_account,
            commands::get_thread_messages,
            commands::apply_thread_action,
            commands::save_attachment,
            commands::save_all_attachments,
            commands::set_dock_badge,
            commands::show_notification,
            commands::rsvp_invite,
            commands::list_subscriptions,
            commands::add_subscription,
            commands::delete_subscription,
            commands::sync_subscription,
            commands::sync_all_subscriptions,
            commands::apply_rules_to_folder,
            commands::preview_rules,
            commands::revert_rules,
            commands::parse_sieve_script,
            commands::export_sieve_script,
            commands::mark_junk,
            commands::unsubscribe,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::toggle_task,
            commands::delete_task,
            commands::query_free_busy,
            settings::get_settings,
            settings::set_settings,
            diagnostics::report_js_error,
            diagnostics::set_log_level,
            diagnostics::open_logs_folder,
            diagnostics::open_crash_reports_folder,
            diagnostics::send_test_report,
            diagnostics::flush_pending_reports,
            diagnostics::get_diagnostics_info,
        ])
        .setup(|app| {
            // Diagnostics (E2.3): stash the panic-hook state, apply the
            // persisted log level, and spawn the upload/ping task. Early so a
            // panic anywhere during setup is still captured.
            diagnostics::init(app.handle());

            // P1.5: capture the app handle for the deep-link handler, then
            // handle `mailto:`/`webcal:` links, then build the system tray.
            let _ = APP.set(app.handle().clone());
            {
                let deep = app.state::<tauri_plugin_deep_link::DeepLink<tauri::Wry>>();
                deep.on_open_url(|event| {
                    for url in event.urls() {
                        handle_deep_link(url.clone());
                    }
                });
            }
            if let Some(icon) = app.default_window_icon() {
                use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
                let menu = Menu::new(app)
                    .and_then(|m| {
                        m.append(
                            &MenuItem::with_id(app, "show", "Show Quill", true, None::<&str>)?,
                        )?;
                        m.append(&MenuItem::with_id(
                            app,
                            "newmail",
                            "New Message",
                            true,
                            None::<&str>,
                        )?)?;
                        m.append(&PredefinedMenuItem::separator(app)?)?;
                        m.append(&MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?)?;
                        Ok(m)
                    })
                    .ok();
                if let Some(menu) = menu {
                    let _ = tauri::tray::TrayIconBuilder::new()
                        .icon(icon.clone())
                        .menu(&menu)
                        .show_menu_on_left_click(false)
                        .on_menu_event(|app, event| match event.id.as_ref() {
                            "show" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.set_focus();
                                }
                            }
                            "newmail" => {
                                let _ = app.emit(
                                    "store",
                                    quill_store::types::StoreEvent::Mailto(
                                        quill_store::types::MailtoPayload::default(),
                                    ),
                                );
                            }
                            "quit" => app.exit(0),
                            _ => {}
                        })
                        .build(app);
                }
            }

            // Demo mode (Epic 3.4): only when explicitly requested with
            // `--demo`. Real development runs use the persistent store so
            // accounts, mail, and settings survive reloads — the old
            // debug-build-default wiped them every `tauri dev`.
            let demo = std::env::args().any(|a| a == "--demo");
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

            // Snippets: rows synced before the sync fetched full bodies carry
            // raw MIME/HTML fragments. Repair them from any stored body before
            // the UI first renders, so list previews read as text from the
            // start (idempotent — only rows whose snippet differs are touched).
            let repaired = app
                .state::<SqliteStore>()
                .repair_snippets_from_bodies()
                .unwrap_or(0);
            if repaired > 0 {
                log::info!("repaired {repaired} message snippets from stored bodies");
            }
            // Subjects: rows synced before the sync decoded RFC 2047 show
            // encoded-words (e.g. "=?UTF-8?Q?…?="). Decode them in place.
            let decoded = app
                .state::<SqliteStore>()
                .repair_encoded_subjects()
                .unwrap_or(0);
            if decoded > 0 {
                log::info!("decoded {decoded} encoded-word subjects");
            }

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
            #[cfg(not(target_os = "macos"))]
            let mut builder = {
                let mut b = tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                );
                if let Some(icon) = app.default_window_icon() {
                    b = b.icon(icon.clone())?;
                }
                b
            };

            #[cfg(target_os = "macos")]
            let builder = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            );

            builder
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


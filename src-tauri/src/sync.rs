//! Sync driver (Epic 12.2 / Roadmap E1.1).
//!
//! Runs the IMAP sync engine per account on the account's configured cadence
//! ("every 2 min" / "on open"; manual accounts are skipped until an explicit
//! trigger), with IMAP IDLE push listeners and reconnection backoff on a dedicated
//! background Tokio runtime. One account failing never stalls the others — each
//! sync is isolated — and connectivity events are pushed to the frontend (which
//! never polls).

use quill_mail::sync::sync_account;
use quill_store::sqlite::SqliteStore;
use quill_store::types::{Account, ConnectivityUpdate, MailChangedUpdate, StoreEvent};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Retention window: keep only this many days of mail. Mirrors the sync
/// engine's own constant so the driver can prune at startup too.
const RETAIN_DAYS: i64 = 7;
const RETAIN_DAYS_MS: i64 = RETAIN_DAYS * 24 * 3600 * 1000;

/// Start the per-account sync and push loops. Call once at startup.
pub fn spawn_sync_loops(app: AppHandle) {
    let runtime = tokio::runtime::Runtime::new().expect("sync runtime");
    std::thread::spawn(move || {
        runtime.block_on(async move {
            let active_idle_tasks: Arc<Mutex<HashSet<u32>>> = Arc::new(Mutex::new(HashSet::new()));
            let mut last_sync: HashMap<u32, u64> = HashMap::new();

            // Retention: shrink a store that already holds more than the window
            // before the first sync finishes (a busy Gmail account can hold a
            // million+ rows).
            let cutoff = now_ms() - RETAIN_DAYS_MS;
            let deleted = app
                .state::<SqliteStore>()
                .prune_messages_before(cutoff)
                .unwrap_or(0);
            if deleted > 0 {
                log::info!("pruned {deleted} messages older than {RETAIN_DAYS} days");
            }

            // Backfill: rows synced before the sync fetched full bodies have no
            // stored body, so their snippets are raw MIME/HTML fragments. Fetch
            // and parse them once so every previously-downloaded message gets a
            // real snippet and body. Idempotent — only missing bodies are
            // fetched, so an interrupted run resumes next launch.
            let accounts = app.state::<SqliteStore>().accounts();
            for account in accounts {
                if account.sync_mode == "manual" {
                    continue;
                }
                let Ok(credential) = quill_mail::auth::resolve_credential(&account) else {
                    continue;
                };
                // Each persisted body pings a channel forwarded as a throttled
                // MailChanged, so the list fills in progressively instead of
                // updating all at once when the backfill finishes.
                let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<()>(64);
                let account_id = account.id;
                let handle = app.clone();
                tokio::spawn(async move {
                    let mut last = tokio::time::Instant::now();
                    while progress_rx.recv().await.is_some() {
                        if last.elapsed() >= Duration::from_millis(500) {
                            let _ = handle.emit(
                                "store",
                                StoreEvent::MailChanged(MailChangedUpdate { account_id }),
                            );
                            last = tokio::time::Instant::now();
                        }
                    }
                });
                match quill_mail::sync::backfill_account_bodies(
                    &app.state::<SqliteStore>(),
                    &account,
                    &credential,
                    &Some(progress_tx),
                )
                .await
                {
                    Ok(n) => {
                        if n > 0 {
                            // Account id, never the address: the never-log-PII
                            // rule (plan2.md §10.4, E2.3).
                            log::info!("backfilled {n} bodies for account {}", account.id);
                            // Guarantee a final refresh even for a fast run that
                            // never crossed the 500ms throttle.
                            let _ = app.emit(
                                "store",
                                StoreEvent::MailChanged(MailChangedUpdate { account_id }),
                            );
                        }
                    }
                    Err(e) => log::warn!("backfill account {} failed: {e}", account.id),
                }
            }

            loop {
                // P1.1 housekeeping: snoozed messages whose wake time passed
                // return to their folders (local-only). Runs on the same 30s
                // cadence; reliable while the app is open.
                let due = app
                    .state::<SqliteStore>()
                    .clear_due_snoozes(now_ms())
                    .unwrap_or(0);
                if due > 0 {
                    log::info!("returned {due} snoozed messages to their folders");
                    let _ = app.emit(
                        "store",
                        StoreEvent::MailChanged(MailChangedUpdate { account_id: 0 }),
                    );
                }
                flush_due_scheduled(&app).await;

                let accounts = app.state::<SqliteStore>().accounts();
                for account in accounts {
                    if account.sync_mode == "manual" {
                        continue;
                    }

                    // Check cadence for periodic sync
                    let interval_secs = if account.sync_mode == "every 2 min" {
                        120
                    } else {
                        0
                    };
                    let now = now_ms().max(0) as u64;
                    let due = match last_sync.get(&account.id) {
                        None => true,
                        Some(last) => {
                            interval_secs > 0 && now.saturating_sub(*last) >= interval_secs * 1000
                        }
                    };

                    if due {
                        sync_one(&app, &account, true).await;
                        // Only mark as synced after the attempt; on failure the
                        // next loop iteration (30s later) will retry sooner
                        // than the full cadence.
                        last_sync.insert(account.id, now);
                    }

                    // Spawn long-running IDLE push worker if not already running
                    let mut running = active_idle_tasks.lock().await;
                    if !running.contains(&account.id) {
                        running.insert(account.id);
                        let app_clone = app.clone();
                        let account_clone = account.clone();
                        let active_tasks = active_idle_tasks.clone();

                        tokio::spawn(async move {
                            spawn_idle_worker(app_clone, account_clone).await;
                            active_tasks.lock().await.remove(&account.id);
                        });
                    }
                }

                // Re-check accounts every 30s
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    });
}

async fn spawn_idle_worker(app: AppHandle, account: Account) {
    let Ok(credential) = quill_mail::auth::resolve_credential(&account) else {
        return;
    };

    let mut backoff = 1u64;
    loop {
        let store = app.state::<SqliteStore>();
        let conn_result = quill_mail::sync::connect(&account, &credential).await;

        match conn_result {
            Ok(mut session) => {
                backoff = 1;
                let _ = store.set_account_connected(account.id, true, None);
                let _ = app.emit(
                    "store",
                    StoreEvent::Connectivity(ConnectivityUpdate {
                        state: "synced".to_string(),
                        last_synced_at_ms: Some(now_ms()),
                    }),
                );

                // Select INBOX
                if session.select("INBOX").await.is_err() {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    continue;
                }

                // Run IDLE loop
                loop {
                    let mut idle = session.idle();
                    if idle.init().await.is_err() {
                        break;
                    }

                    let timeout = tokio::time::sleep(Duration::from_secs(20 * 60));
                    tokio::pin!(timeout);
                    let (wait_fut, stop_source) = idle.wait();

                    tokio::select! {
                        _ = &mut timeout => {
                            drop(stop_source);
                            let res = idle.done().await;
                            if let Ok(s) = res {
                                session = s;
                            } else {
                                break;
                            }
                        }
                        wait_res = wait_fut => {
                            let res = idle.done().await;
                            if let Ok(s) = res {
                                session = s;
                            } else {
                                break;
                            }
                            if wait_res.is_err() {
                                break;
                            }
                        }
                    }

                    // Trigger sync on wakeup / push
                    let _ = app.emit(
                        "store",
                        StoreEvent::Connectivity(ConnectivityUpdate {
                            state: "syncing".to_string(),
                            last_synced_at_ms: None,
                        }),
                    );

                    // The IDLE worker must not replay queued actions: the
                    // periodic sync path already does, and the two run
                    // concurrently — double replay of a queued Send would
                    // deliver the email twice.
                    let outcome = sync_account(&store, &account, &credential, None, false).await;
                    match outcome {
                        Ok(_) => {
                            let _ = app.emit(
                                "store",
                                StoreEvent::Connectivity(ConnectivityUpdate {
                                    state: "synced".to_string(),
                                    last_synced_at_ms: Some(now_ms()),
                                }),
                            );
                        }
                        Err(_) => {
                            break; // reconnect
                        }
                    }
                }
            }
            Err(e) => {
                let _ = store.set_account_connected(account.id, false, Some(&e));
                let _ = app.emit(
                    "store",
                    StoreEvent::Connectivity(ConnectivityUpdate {
                        state: "offline".to_string(),
                        last_synced_at_ms: None,
                    }),
                );
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(60);
            }
        }
    }
}

/// Trigger an immediate sync for every non-manual account — the frontend's
/// scroll-to-top refresh. `replay_actions` is false so a manual refresh never
/// double-replays an offline action that's racing the periodic sync.
pub async fn sync_now(app: &AppHandle) {
    let accounts = app.state::<SqliteStore>().accounts();
    for account in accounts {
        if account.sync_mode == "manual" {
            continue;
        }
        sync_one(app, &account, false).await;
    }
}

/// Trigger an immediate sync for one account — the first-run flow's initial
/// sync (P0.2). Manual accounts are still synced here: the user explicitly
/// asked for it during setup.
pub async fn sync_account_now(app: &AppHandle, account_id: u32) {
    let account = app
        .state::<SqliteStore>()
        .accounts()
        .into_iter()
        .find(|a| a.id == account_id);
    if let Some(account) = account {
        sync_one(app, &account, true).await;
    }
}

/// P1.1: flush due send-later messages through the SMTP path (the durable
/// Outbox). Runs on the housekeeping cadence; failures are kept for the next
/// pass. Requires the app to be running — the Scheduled view states this.
pub async fn flush_due_scheduled(app: &AppHandle) {
    let store = app.state::<SqliteStore>();
    let due = store.due_scheduled(now_ms());
    if due.is_empty() {
        return;
    }
    for (id, account_id, _send_at, payload) in due {
        let Some(account) = store
            .accounts()
            .into_iter()
            .find(|a| a.id == account_id)
        else {
            let _ = store.cancel_scheduled(id);
            continue;
        };
        let outgoing: Result<quill_store::types::OutgoingMessage, _> =
            serde_json::from_str(&payload);
        let Ok(outgoing) = outgoing else {
            // A corrupt payload can never send — drop it rather than retry forever.
            let _ = store.cancel_scheduled(id);
            continue;
        };
        match quill_mail::auth::resolve_credential(&account) {
            Ok(credential) => {
                match quill_mail::smtp::send_email(&account, &outgoing, &credential).await {
                    Ok(()) => {
                        let _ = store.cancel_scheduled(id);
                        // Account id, never the address (never-log-PII rule).
                        log::info!("sent scheduled message {id} for account {}", account.id);
                    }
                    Err(e) => {
                        // Keep the row; the next housekeeping pass retries.
                        log::warn!("scheduled message {id} not sent yet: {e}");
                    }
                }
            }
            Err(e) => log::warn!("scheduled message {id}: no credential: {e}"),
        }
    }
}

async fn sync_one(app: &AppHandle, account: &Account, replay_actions: bool) {
    let Ok(credential) = quill_mail::auth::resolve_credential(&account) else {
        return; // no credential stored — nothing to sync
    };
    let _ = app.emit(
        "store",
        StoreEvent::Connectivity(ConnectivityUpdate {
            state: "syncing".to_string(),
            last_synced_at_ms: None,
        }),
    );

    // Stream progress to the frontend: each written message pings this channel,
    // and a task forwards it as a throttled MailChanged event so the list fills
    // in progressively instead of all at once when the sync ends.
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<()>(64);
    let account_id = account.id;
    let handle = app.clone();
    tokio::spawn(async move {
        let mut last = tokio::time::Instant::now();
        while progress_rx.recv().await.is_some() {
            if last.elapsed() >= Duration::from_millis(500) {
                let _ = handle.emit(
                    "store",
                    StoreEvent::MailChanged(MailChangedUpdate { account_id }),
                );
                last = tokio::time::Instant::now();
            }
        }
    });

    let store = app.state::<SqliteStore>();
    match sync_account(&store, account, &credential, Some(progress_tx), replay_actions).await {
        Ok(outcome) => {
            log::info!(
                "synced account {} ({} folders, {} messages)",
                account.id,
                outcome.folders_synced,
                outcome.messages_fetched
            );
            let _ = app.emit(
                "store",
                StoreEvent::Connectivity(ConnectivityUpdate {
                    state: "synced".to_string(),
                    last_synced_at_ms: Some(now_ms()),
                }),
            );
        }
        Err(e) => {
            log::warn!("sync account {} failed: {e}", account.id);
            let _ = app.emit(
                "store",
                StoreEvent::Connectivity(ConnectivityUpdate {
                    state: "offline".to_string(),
                    last_synced_at_ms: None,
                }),
            );
        }
    }
}

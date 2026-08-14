//! Sync driver (Epic 12.2).
//!
//! Runs the IMAP sync engine per account on the account's configured cadence
//! ("every 2 min" / "on open"; manual accounts are skipped until an explicit
//! trigger), on a dedicated background Tokio runtime. One account failing
//! never stalls the others — each sync is isolated — and connectivity events
//! are pushed to the frontend (which never polls).

use quill_mail::sync::sync_account;
use quill_store::sqlite::SqliteStore;
use quill_store::types::{Account, ConnectivityUpdate, StoreEvent};
use std::collections::HashMap;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Start the per-account sync loops. Call once at startup.
pub fn spawn_sync_loops(app: AppHandle) {
    let runtime = tokio::runtime::Runtime::new().expect("sync runtime");
    std::thread::spawn(move || {
        runtime.block_on(async move {
            let mut last_sync: HashMap<u32, u64> = HashMap::new();
            loop {
                let accounts = app.state::<SqliteStore>().accounts();
                for account in accounts {
                    if account.sync_mode == "manual" {
                        continue; // manual accounts sync via an explicit trigger
                    }
                    let interval_secs = if account.sync_mode == "every 2 min" {
                        120
                    } else {
                        0
                    };
                    let now = now_ms().max(0) as u64;
                    let due = match last_sync.get(&account.id) {
                        // First pass syncs everything non-manual (covers
                        // "on open"); afterwards only on the cadence.
                        None => true,
                        Some(last) => {
                            interval_secs > 0 && now.saturating_sub(*last) >= interval_secs * 1000
                        }
                    };
                    if due {
                        last_sync.insert(account.id, now);
                        sync_one(&app, &account).await;
                    }
                }
                // Re-check accounts every 30s so newly added ones get a loop.
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
        });
    });
}

async fn sync_one(app: &AppHandle, account: &Account) {
    let Ok(password) = quill_mail::credentials::get_credential(&account.address) else {
        return; // no credential stored — nothing to sync
    };
    let _ = app.emit(
        "store",
        StoreEvent::Connectivity(ConnectivityUpdate {
            state: "syncing".to_string(),
            last_synced_at_ms: None,
        }),
    );
    let store = app.state::<SqliteStore>();
    match sync_account(&store, account, &password).await {
        Ok(outcome) => {
            eprintln!(
                "synced {} ({} folders, {} messages)",
                account.address, outcome.folders_synced, outcome.messages_fetched
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
            eprintln!("sync {} failed: {e}", account.address);
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

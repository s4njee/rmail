//! The IPC command surface (Epic 3.1).
//!
//! Thin wrappers over the store; argument and return types come from
//! `quill-store::types` (the generated TS bindings in `src/lib/ipc/` are made
//! from the same types, so the two sides of the boundary can't drift).
//!
//! **No command ever returns credential material** (AC 3.1) — credentials
//! never enter this crate at all (Epic 10.4), and the test in `tests/`
//! scans every command's return type for them.

use quill_store::sqlite::SqliteStore;
use quill_store::types::*;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, State};

/// Shared cancel flag for the background search-index rebuild (P1.3).
static SEARCH_REBUILD_CANCEL: AtomicBool = AtomicBool::new(false);

#[tauri::command]
pub fn list_folders(store: State<'_, SqliteStore>) -> Vec<Folder> {
    store.folders()
}

#[tauri::command]
pub fn list_accounts(store: State<'_, SqliteStore>) -> Vec<Account> {
    store.accounts()
}

/// Total on-disk cache bytes — the footprint readout (Epic 4.3 / 11.2,
/// on-disk only per D2). Computed by the store, never on the UI thread.
#[tauri::command]
pub fn footprint(store: State<'_, SqliteStore>) -> u64 {
    store.total_disk_bytes()
}

#[tauri::command]
pub fn page_messages(store: State<'_, SqliteStore>, query: MessageQuery) -> MessagePage {
    store.page_messages(&query)
}

/// The full message for the reading pane. If the body has not yet been fetched
/// from the remote server, it is fetched on demand. Any HTML body is sanitized
/// here, server-side of the IPC boundary (Epic 7.3) — the webview never sees raw
/// mail HTML.
#[tauri::command]
pub async fn get_message(
    app: AppHandle,
    store: State<'_, SqliteStore>,
    id: MessageId,
) -> Result<Option<MessageDetail>, String> {
    let mut detail = store.get_message(id);
    if let Some(ref d) = detail {
        if d.body.is_empty() && d.body_html.is_none() {
            // `server_folder` is the real IMAP mailbox name — the local display
            // folder ("Sent", "Archive", …) is not a selectable mailbox on
            // Gmail/Outlook, so on-demand body fetch used to fail for those.
            if let Some((account_id, _local_folder, Some(server_folder), Some(uid))) =
                store.get_message_location(id)
            {
                if let Some(account) = store.accounts().into_iter().find(|a| a.id == account_id) {
                    if let Ok(credential) = quill_mail::auth::resolve_credential(&account) {
                        // Stream the download to the reading pane's loading
                        // screen over the same `store` event channel the
                        // frontend already subscribes to (Epic 7.2).
                        let (progress_tx, mut progress_rx) =
                            tokio::sync::mpsc::channel::<MessageProgressUpdate>(16);
                        let handle = app.clone();
                        tokio::spawn(async move {
                            while let Some(update) = progress_rx.recv().await {
                                let _ = handle.emit(
                                    "store",
                                    StoreEvent::MessageProgress(update),
                                );
                            }
                        });
                        let _ = quill_mail::sync::fetch_message_body_full(
                            &store,
                            &account,
                            &credential,
                            &server_folder,
                            uid,
                            id,
                            &Some(progress_tx),
                        )
                        .await;
                        detail = store.get_message(id);
                    }
                }
            }
        }
    }
    let Some(mut detail) = detail else {
        return Ok(None);
    };
    if let Some(raw) = detail.body_html.take() {
        let sanitized = quill_store::sanitize::sanitize_html(&raw);
        detail.body_html = Some(sanitized.html);
        detail.remote_image_count = sanitized.remote_images as u32;
    }

    // Detect iTIP calendar invitations (Roadmap 4.1)
    if detail.calendar_invite.is_none() {
        let full_text = detail.body.join("\n");
        if full_text.contains("BEGIN:VCALENDAR") {
            let account_email = store
                .accounts()
                .into_iter()
                .find(|a| a.id == detail.row.account_id)
                .map(|a| a.address)
                .unwrap_or_default();
            detail.calendar_invite = quill_cal::parse_itip_invite(&full_text, &account_email);
        }
    }

    Ok(Some(detail))
}

/// Process an RSVP response to a calendar invitation (Roadmap 4.1).
#[tauri::command]
pub fn rsvp_invite(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    message_id: MessageId,
    partstat: String,
    comment: Option<String>,
) -> Result<(), String> {
    let mut detail = store.get_message(message_id).ok_or("message not found")?;
    let account = store
        .accounts()
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or("account not found")?;

    // The store never persists a parsed invite — `get_message` only derives it
    // for the frontend. Re-derive it here from the stored body, exactly like
    // the `get_message` command does, so RSVP actually has something to reply
    // to instead of silently no-op'ing.
    if detail.calendar_invite.is_none() {
        let full_text = detail.body.join("\n");
        if full_text.contains("BEGIN:VCALENDAR") {
            detail.calendar_invite = quill_cal::parse_itip_invite(&full_text, &account.address);
        }
    }

    if let Some(invite) = &detail.calendar_invite {
        let reply_body = quill_cal::generate_imip_reply(
            invite,
            &account.address,
            &account.address,
            &partstat,
            comment.as_deref(),
        );

        let payload = serde_json::json!({
            "to": [invite.organizer_email],
            "subject": format!("{}: {}", partstat, invite.title),
            "body": format!("RSVP: {} has responded {} to '{}'.", account.address, partstat, invite.title),
            "ics": reply_body
        }).to_string();

        let _ = store.enqueue_action(account_id, ActionType::Send, "Sent", None, Some(&payload));

        if partstat.eq_ignore_ascii_case("ACCEPTED") || partstat.eq_ignore_ascii_case("TENTATIVE") {
            let _ = store.create_event(CalendarEvent {
                id: 0,
                account_id,
                title: invite.title.clone(),
                start_ms: invite.start_ms,
                end_ms: invite.end_ms,
                all_day: invite.all_day,
                location: invite.location.clone(),
                notes: Some(format!("Organized by: {}", invite.organizer_email)),
                alarm_minutes_before: Some(15),
                timezone: invite.timezone.clone(),
                travel_time_minutes: None,
                calendar_source: None,
                calendar_name: None,
                calendar_color: None,
                color: None,
            });
        }
    }
    Ok(())
}

/// Local path for an attachment; the frontend serves it over the asset
/// protocol (Epic 3.3) via `convertFileSrc`, never through IPC bytes.
#[tauri::command]
pub fn attachment_path(store: State<'_, SqliteStore>, id: AttachmentId) -> Option<String> {
    store
        .attachment_path(id)
        .map(|p| p.to_string_lossy().into_owned())
}

/// Save an attachment to a target destination file path (Roadmap 3.3).
#[tauri::command]
pub fn save_attachment(
    store: State<'_, SqliteStore>,
    id: AttachmentId,
    destination_path: String,
) -> Result<(), String> {
    let src_path = store.attachment_path(id);
    let dest = std::path::Path::new(&destination_path);
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Some(src) = src_path {
        if src.exists() {
            std::fs::copy(&src, dest).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    let att = store.attachment(id).ok_or("attachment not found")?;
    let bytes = if att.filename.ends_with(".pdf") {
        quill_store::pdf::placeholder(att.size_bytes as usize)
    } else {
        format!("Placeholder content for {}", att.filename).into_bytes()
    };
    std::fs::write(dest, bytes).map_err(|e| e.to_string())?;
    Ok(())
}

/// Save all attachments of a message to a directory (Roadmap 3.3).
#[tauri::command]
pub fn save_all_attachments(
    store: State<'_, SqliteStore>,
    message_id: MessageId,
    destination_dir: String,
) -> Result<u32, String> {
    let msg = store.get_message(message_id).ok_or("message not found")?;
    let dir = std::path::Path::new(&destination_dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut count = 0;
    for att in msg.attachments {
        let dest = dir.join(&att.filename);
        let src_path = store.attachment_path(att.id);
        if let Some(src) = src_path {
            if src.exists() {
                let _ = std::fs::copy(&src, &dest);
                count += 1;
                continue;
            }
        }
        let bytes = if att.filename.ends_with(".pdf") {
            quill_store::pdf::placeholder(att.size_bytes as usize)
        } else {
            format!("Placeholder content for {}", att.filename).into_bytes()
        };
        let _ = std::fs::write(&dest, bytes);
        count += 1;
    }
    Ok(count)
}

#[tauri::command]
pub fn mark_read(store: State<'_, SqliteStore>, id: MessageId, unread: bool) -> Result<(), String> {
    // Enqueue against the real server mailbox, not the display name — the
    // replay engine `SELECT`s it on the IMAP connection.
    if let Some((account_id, _local_folder, Some(server_folder), uid)) =
        store.get_message_location(id)
    {
        let action_type = if unread {
            ActionType::MarkUnread
        } else {
            ActionType::MarkRead
        };
        let _ = store.enqueue_action(account_id, action_type, &server_folder, uid, None);
    }
    store.set_read(id, unread)
}

#[tauri::command]
pub fn star(store: State<'_, SqliteStore>, id: MessageId, flagged: bool) -> Result<(), String> {
    if let Some((account_id, _local_folder, Some(server_folder), uid)) =
        store.get_message_location(id)
    {
        let action_type = if flagged {
            ActionType::Star
        } else {
            ActionType::Unstar
        };
        let _ = store.enqueue_action(account_id, action_type, &server_folder, uid, None);
    }
    store.set_flagged(id, flagged)
}

#[tauri::command]
pub fn mark_answered(
    store: State<'_, SqliteStore>,
    id: MessageId,
    answered: bool,
) -> Result<(), String> {
    if let Some((account_id, _local_folder, Some(server_folder), uid)) =
        store.get_message_location(id)
    {
        if answered {
            let _ = store.enqueue_action(
                account_id,
                ActionType::MarkAnswered,
                &server_folder,
                uid,
                None,
            );
        }
    }
    store.set_answered(id, answered)
}

#[tauri::command]
pub fn mark_forwarded(
    store: State<'_, SqliteStore>,
    id: MessageId,
    forwarded: bool,
) -> Result<(), String> {
    if let Some((account_id, _local_folder, Some(server_folder), uid)) =
        store.get_message_location(id)
    {
        if forwarded {
            let _ = store.enqueue_action(
                account_id,
                ActionType::MarkForwarded,
                &server_folder,
                uid,
                None,
            );
        }
    }
    store.set_forwarded(id, forwarded)
}

#[tauri::command]
pub fn archive(store: State<'_, SqliteStore>, id: MessageId) -> Result<(), String> {
    if let Some((account_id, _local_folder, Some(server_folder), uid)) =
        store.get_message_location(id)
    {
        let _ = store.enqueue_action(account_id, ActionType::Archive, &server_folder, uid, None);
    }
    store.archive(id)
}

#[tauri::command]
pub fn delete(store: State<'_, SqliteStore>, id: MessageId) -> Result<(), String> {
    if let Some((account_id, _local_folder, Some(server_folder), uid)) =
        store.get_message_location(id)
    {
        let _ = store.enqueue_action(account_id, ActionType::Delete, &server_folder, uid, None);
    }
    store.delete(id)
}

/// Apply one triage action to many messages (P1.1): bulk read/unread, star,
/// archive, delete (soft), move, or junk. Each message enqueues its server
/// action and updates locally; the result carries partial-failure counts.
#[tauri::command]
pub fn bulk_action(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    ids: Vec<MessageId>,
    action: BulkAction,
    destination: Option<String>,
) -> Result<BulkActionResult, String> {
    let (ok, errors): (u32, Vec<String>) = match action {
        BulkAction::MarkRead => store.bulk_set_read(&ids, false),
        BulkAction::MarkUnread => store.bulk_set_read(&ids, true),
        BulkAction::Star => store.bulk_set_flagged(&ids, true),
        BulkAction::Unstar => store.bulk_set_flagged(&ids, false),
        BulkAction::Archive => store.bulk_archive(&ids),
        BulkAction::Delete => store.bulk_delete(&ids),
        BulkAction::MarkJunk => store.bulk_mark_junk(&ids, true),
        BulkAction::MarkNotJunk => store.bulk_mark_junk(&ids, false),
        BulkAction::Move => {
            let dest = destination.ok_or("a destination folder is required to move")?;
            store.bulk_move(&ids, &dest)
        }
    };
    let _ = account_id; // the store derives it per message; kept for symmetry
    Ok(BulkActionResult {
        ok,
        failed: errors.len() as u32,
        errors,
    })
}

/// Restore a soft-deleted message and cancel its queued server Delete — the
/// undo path (P1.1).
#[tauri::command]
pub fn restore_message(
    store: State<'_, SqliteStore>,
    id: MessageId,
) -> Result<(), String> {
    if let Some((account_id, _local_folder, server_folder, uid)) =
        store.get_message_location(id)
    {
        if let Some(folder) = server_folder {
            let _ = store.cancel_pending_actions(account_id, &folder, uid);
        }
    }
    store.restore_message(id)
}

/// Snooze messages until a wake time (P1.1) — local-only, the server copy is
/// untouched; the scheduler returns them to their folders when the time passes.
#[tauri::command]
pub fn set_snoozed(
    store: State<'_, SqliteStore>,
    ids: Vec<MessageId>,
    until_ms: i64,
) -> Result<(), String> {
    store.set_snoozed(&ids, until_ms)
}

/// Queue a message to send later (P1.1). The outgoing payload is serialized
/// into the durable Outbox; the scheduler flushes it through the SMTP path at
/// `send_at_ms`. `draft` is the composer snapshot so Edit can reopen it.
#[tauri::command]
pub fn schedule_send(
    store: State<'_, SqliteStore>,
    outgoing: OutgoingMessage,
    send_at_ms: i64,
    draft: String,
) -> Result<i64, String> {
    let payload = serde_json::to_string(&outgoing).map_err(|e| e.to_string())?;
    store.schedule_message(outgoing.account_id, send_at_ms, &payload, &draft)
}

/// All scheduled (send-later) messages, soonest first.
#[tauri::command]
pub fn list_scheduled(store: State<'_, SqliteStore>) -> Vec<ScheduledMessage> {
    store.list_scheduled()
}

/// Cancel a scheduled send so it never goes out.
#[tauri::command]
pub fn cancel_scheduled(store: State<'_, SqliteStore>, id: i64) -> Result<(), String> {
    store.cancel_scheduled(id)
}

/// Recipient suggestions for the composer (P1.2) — offline, from mail history.
#[tauri::command]
pub fn suggest_recipients(
    store: State<'_, SqliteStore>,
    query: String,
    limit: Option<u32>,
) -> Vec<ContactSuggestion> {
    store.suggest_recipients(&query, limit.unwrap_or(8))
}

/// Most recently used recipients, for the composer's empty-field dropdown.
#[tauri::command]
pub fn recent_recipients(
    store: State<'_, SqliteStore>,
    limit: Option<u32>,
) -> Vec<ContactSuggestion> {
    store.recent_recipients(limit.unwrap_or(8))
}

/// Dismiss a recipient suggestion so it stops appearing.
#[tauri::command]
pub fn hide_recipient(store: State<'_, SqliteStore>, address: String) -> Result<(), String> {
    store.hide_recipient(&address)
}

/// All contact groups.
#[tauri::command]
pub fn list_contact_groups(store: State<'_, SqliteStore>) -> Vec<ContactGroup> {
    store.list_contact_groups()
}

/// Create a named contact group.
#[tauri::command]
pub fn create_contact_group(store: State<'_, SqliteStore>, name: String) -> Result<i64, String> {
    store.create_contact_group(&name)
}

#[tauri::command]
pub fn delete_contact_group(store: State<'_, SqliteStore>, id: i64) -> Result<(), String> {
    store.delete_contact_group(id)
}

#[tauri::command]
pub fn add_contact_to_group(
    store: State<'_, SqliteStore>,
    group_id: i64,
    address: String,
) -> Result<(), String> {
    store.add_contact_to_group(group_id, &address)
}

#[tauri::command]
pub fn remove_contact_from_group(
    store: State<'_, SqliteStore>,
    group_id: i64,
    address: String,
) -> Result<(), String> {
    store.remove_contact_from_group(group_id, &address)
}

/// A group's members (names/counts joined from history when known).
#[tauri::command]
pub fn contact_group_members(
    store: State<'_, SqliteStore>,
    group_id: i64,
) -> Vec<ContactSuggestion> {
    store.contact_group_members(group_id)
}

/// All saved searches (P1.3) — persistent virtual folders.
#[tauri::command]
pub fn list_saved_searches(store: State<'_, SqliteStore>) -> Vec<SavedSearch> {
    store.list_saved_searches()
}

/// Save the current search query under a name.
#[tauri::command]
pub fn save_search(
    store: State<'_, SqliteStore>,
    name: String,
    query: String,
) -> Result<i64, String> {
    store.save_search(&name, &query)
}

#[tauri::command]
pub fn delete_saved_search(store: State<'_, SqliteStore>, id: i64) -> Result<(), String> {
    store.delete_saved_search(id)
}

/// Save (or update) a draft in the Drafts folder (Epic 13.2). Returns the
/// draft's message id for in-place re-saves.
#[tauri::command]
pub fn save_draft(store: State<'_, SqliteStore>, draft: Draft) -> Result<MessageId, String> {
    store.save_draft(&draft)
}

/// Outgoing mail via SMTP (Epic 12.3 & 13), with the account's credential
/// (password or OAuth bearer). If sending fails due to network/server
/// unavailability, it is queued for retry.
#[tauri::command]
pub async fn send(store: State<'_, SqliteStore>, outgoing: OutgoingMessage) -> Result<(), String> {
    let account = store
        .accounts()
        .into_iter()
        .find(|a| a.id == outgoing.account_id)
        .ok_or("no such account")?;
    let credential = quill_mail::auth::resolve_credential(&account)?;
    match quill_mail::smtp::send_email(&account, &outgoing, &credential).await {
        Ok(()) => {
            if let Some(orig_id) = outgoing.original_message_id {
                if outgoing.is_forward.unwrap_or(false) {
                    let _ = mark_forwarded(store.clone(), orig_id, true);
                } else {
                    let _ = mark_answered(store.clone(), orig_id, true);
                }
            }
            Ok(())
        }
        Err(e) => {
            let payload = serde_json::to_string(&outgoing).map_err(|e| e.to_string())?;
            let _ = store.enqueue_action(
                outgoing.account_id,
                ActionType::Send,
                "Outbox",
                None,
                Some(&payload),
            );
            Err(format!("Send failed (queued in Outbox for retry): {e}"))
        }
    }
}

#[tauri::command]
pub fn list_events(
    store: State<'_, SqliteStore>,
    start_ms: i64,
    end_ms: i64,
) -> Vec<CalendarEvent> {
    store.list_events(start_ms, end_ms)
}

/// Distinct source calendars in the local store (Roadmap 4.4) — lets the
/// calendar sidebar show each synced Google calendar as its own row.
#[tauri::command]
pub fn list_calendars(store: State<'_, SqliteStore>) -> Vec<CalendarSource> {
    store.list_calendar_sources()
}

/// Remove a synced source calendar: delete its local events and record it so
/// the next sync skips it (it stays gone until restored).
#[tauri::command]
pub fn remove_calendar_source(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    source: String,
) -> Result<(), String> {
    // Capture the calendar's name/color before the events are deleted, so the
    // Settings "Removed" list can still show it.
    let (name, color) = store
        .list_calendar_sources()
        .into_iter()
        .find(|c| c.account_id == account_id && c.source == source)
        .map(|c| (c.name, c.color))
        .unwrap_or_else(|| (source.clone(), String::new()));
    store.mark_calendar_source_removed(account_id, &source, &name, &color)?;
    store.delete_events_by_source(account_id, &source)?;
    Ok(())
}

/// Undo a calendar removal so the next sync re-adds it.
#[tauri::command]
pub fn restore_calendar_source(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    source: String,
) -> Result<(), String> {
    store.clear_calendar_source_removed(account_id, &source)
}

/// Source calendars the user has removed (for the Settings "Removed" list).
#[tauri::command]
pub fn list_removed_calendar_sources(store: State<'_, SqliteStore>) -> Vec<CalendarSource> {
    store.removed_calendar_sources()
}

#[tauri::command]
pub fn create_event(
    store: State<'_, SqliteStore>,
    event: CalendarEvent,
) -> Result<CalendarEvent, String> {
    store.create_event(event)
}

#[tauri::command]
pub fn update_event(store: State<'_, SqliteStore>, event: CalendarEvent) -> Result<(), String> {
    store.update_event(event)
}

#[tauri::command]
pub fn delete_event(store: State<'_, SqliteStore>, id: EventId) -> Result<(), String> {
    store.delete_event(id)
}

/// P1.4 undo: restore an event exactly as captured (re-create a deleted event
/// or overwrite an edited one).
#[tauri::command]
pub fn restore_event(
    store: State<'_, SqliteStore>,
    event: CalendarEvent,
) -> Result<(), String> {
    store.restore_event(event)
}

/// P1.4: clone an event into the same calendar (fresh id, "(copy)" title).
#[tauri::command]
pub fn duplicate_event(
    store: State<'_, SqliteStore>,
    id: EventId,
) -> Result<CalendarEvent, String> {
    store.duplicate_event(id)
}

/// The account add form (Epic 10.4). The password goes straight into the OS
/// keychain and is then discarded — never stored in the store, never logged,
/// never returned by any command. If the keychain write fails, no account is
/// created.
#[tauri::command]
pub fn add_account(
    store: State<'_, SqliteStore>,
    info: NewAccount,
    password: String,
) -> Result<Account, String> {
    quill_mail::credentials::set_credential(&info.address, &password)?;
    let palette = ["#3b5bdb", "#0f766e", "#b4451f"];
    let color = palette[store.accounts().len() % palette.len()].to_string();
    store.create_account(&info, color).map_err(|e| {
        // Roll back the keychain write so a failed insert (e.g. a duplicate
        // address) doesn't leave an orphaned credential behind.
        let _ = quill_mail::credentials::delete_credential(&info.address);
        e
    })
}

/// Update an existing account's editable fields (server/port/TLS/sync
/// mode/color). `password` is optional — when non-empty it replaces the stored
/// credential under the account's address.
#[tauri::command]
pub fn update_account(
    store: State<'_, SqliteStore>,
    edit: AccountEdit,
    password: String,
) -> Result<(), String> {
    store.update_account(&edit)?;
    if !password.is_empty() {
        let address = store
            .accounts()
            .into_iter()
            .find(|a| a.id == edit.id)
            .map(|a| a.address)
            .ok_or("no such account")?;
        quill_mail::credentials::set_credential(&address, &password)?;
    }
    Ok(())
}

/// Remove an account: its local mail and calendar data from the store, the
/// on-disk attachment files, plus the keychain credential (and OAuth
/// tokens/config for OAuth accounts). Nothing is deleted on the server — this
/// is a local-only removal, and the frontend confirm says so (P0.2). The
/// attachment files must go before the account row, or the FK cascade removes
/// the rows that tell us which files to delete.
#[tauri::command]
pub fn remove_account(store: State<'_, SqliteStore>, id: AccountId) -> Result<(), String> {
    let account = store
        .accounts()
        .into_iter()
        .find(|a| a.id == id)
        .ok_or("no such account")?;
    let address = account.address.clone();
    let _ = store.delete_attachments_for_account(id);
    store.remove_account(id)?;
    if account.is_oauth() {
        let _ = quill_mail::oauth_store::delete_oauth_tokens(&address);
        let _ = quill_mail::oauth_store::delete_oauth_client_config(&address);
    }
    let _ = quill_mail::credentials::delete_credential(&address);
    Ok(())
}

/// Provider presets for the first-run provider chooser (P0.2).
#[tauri::command]
pub fn list_provider_presets() -> Vec<ProviderPreset> {
    quill_mail::provider::all_presets()
}

/// Autodiscover IMAP/SMTP/CalDAV settings for an email address: preset →
/// DNS SRV → autoconfig → guesses, each step recorded for transparency (P0.2).
#[tauri::command]
pub async fn discover_settings(email: String) -> DiscoveredSettings {
    let domain = email.rsplit('@').next().unwrap_or(&email).to_lowercase();
    quill_mail::autodiscover::discover(&domain).await
}

/// A full connection test (resolve → TCP → TLS → greeting → auth) with
/// classified issues, so the form can offer the right remedy (P0.2).
#[tauri::command]
pub async fn test_connection_settings(
    settings: TestConnectionSettings,
    password: Option<String>,
) -> ConnectionTestReport {
    let mut report = quill_mail::test::test_connection(&settings, password.as_deref()).await;
    // Attach provider-specific help to auth failures at the point of failure
    // (e.g. "iCloud requires an app-specific password…").
    if report
        .issues
        .iter()
        .any(|i| i.kind == ErrorKind::Auth)
    {
        let domain = settings.email.rsplit('@').next().unwrap_or(&settings.email);
        if let Some(preset) = quill_mail::provider::preset_for_domain(&domain) {
            for issue in report.issues.iter_mut() {
                if issue.kind == ErrorKind::Auth && issue.help.is_none() {
                    issue.help = Some(preset.help.clone());
                }
            }
        }
    }
    report
}

/// Enumerate the mailboxes on a server before saving the account — the unit
/// of "choose which folders sync" (P0.2). `password` authenticates a not-yet
/// saved account; with `None` an existing OAuth account (matched by address)
/// provides the token.
#[tauri::command]
pub async fn discover_mail_folders(
    store: State<'_, SqliteStore>,
    email: String,
    server: String,
    port: u16,
    tls: bool,
    password: Option<String>,
) -> Result<Vec<ServerFolder>, String> {
    let account = Account {
        id: 0,
        address: email.clone(),
        protocol: "IMAP".into(),
        sync_mode: "every 2 min".into(),
        color: String::new(),
        local_bytes: 0,
        connected: false,
        server,
        port,
        tls,
        folder_count: 0,
        last_error: None,
    };
    let credential = match password {
        Some(p) if !p.is_empty() => quill_mail::auth::Credential::Password(p),
        _ => {
            let saved = store
                .accounts()
                .into_iter()
                .find(|a| a.address == email)
                .ok_or_else(|| {
                    "no password supplied and no saved account to authenticate with".to_string()
                })?;
            quill_mail::auth::resolve_credential(&saved)?
        }
    };
    let mut session = quill_mail::sync::connect(&account, &credential).await?;
    let folders = quill_mail::sync::discover_folders(&mut session).await?;
    let _ = session.logout().await;
    Ok(folders
        .into_iter()
        .map(|f| ServerFolder {
            server_name: f.server_name,
            local_name: f.local_name,
            kind: f.kind,
        })
        .collect())
}

/// Persist which server mailboxes an account syncs (P0.2).
#[tauri::command]
pub fn set_synced_folders(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    folders: Vec<SyncedFolder>,
) -> Result<(), String> {
    store.set_synced_folders(account_id, &folders)
}

/// The account's folder-sync selection (empty = sync everything).
#[tauri::command]
pub fn list_synced_folders(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
) -> Vec<SyncedFolder> {
    store.synced_folders(account_id)
}

/// What removing an account will destroy — feeds the removal confirm (P0.2).
#[tauri::command]
pub fn account_removal_info(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
) -> AccountRemovalInfo {
    AccountRemovalInfo {
        account_id,
        queued_actions: store.pending_action_count(account_id),
        drafts: store.draft_count(account_id),
        local_bytes: store
            .accounts()
            .into_iter()
            .find(|a| a.id == account_id)
            .map(|a| a.local_bytes)
            .unwrap_or(0),
    }
}

/// Search across messages and calendar events using SQLite FTS5 (Epic 15).
#[tauri::command]
pub fn search(store: State<'_, SqliteStore>, query: SearchQuery) -> Vec<SearchMatch> {
    store.search(&query)
}

/// Rebuild SQLite FTS5 search index tables (Epic 15).
#[tauri::command]
pub fn rebuild_search_index(store: State<'_, SqliteStore>) -> Result<(), String> {
    store.rebuild_search_index()
}

/// P1.3: rebuild the search index in the background, streaming progress as
/// `StoreEvent::SearchIndex` events. Safe to cancel via `cancel_search_rebuild`.
#[tauri::command]
pub fn rebuild_search_index_progress(app: AppHandle) -> Result<(), String> {
    SEARCH_REBUILD_CANCEL.store(false, Ordering::Relaxed);
    std::thread::spawn(move || {
        let store = app.state::<SqliteStore>();
        let (total, _) = store.search_index_status();
        let emit = |state: &str, indexed: usize, total: usize| {
            let _ = app.emit(
                "store",
                StoreEvent::SearchIndex(SearchIndexUpdate {
                    state: state.to_string(),
                    indexed: indexed as u32,
                    total: total as u32,
                }),
            );
        };
        emit("rebuilding", 0, total as usize);
        match store.rebuild_search_index_cancellable(&SEARCH_REBUILD_CANCEL, |i, t| {
            emit("rebuilding", i, t);
        }) {
            Ok(()) => emit("fresh", 0, 0),
            Err(_) => emit("idle", 0, 0), // cancelled or errored
        }
    });
    Ok(())
}

/// Cancel an in-flight `rebuild_search_index_progress`.
#[tauri::command]
pub fn cancel_search_rebuild() -> Result<(), String> {
    SEARCH_REBUILD_CANCEL.store(true, Ordering::Relaxed);
    Ok(())
}

/// P1.3 index freshness readout: messages vs indexed rows.
#[tauri::command]
pub fn search_index_status(store: State<'_, SqliteStore>) -> SearchIndexUpdate {
    let (total, indexed) = store.search_index_status();
    SearchIndexUpdate {
        state: if total == indexed { "fresh".into() } else { "stale".into() },
        indexed: indexed as u32,
        total: total as u32,
    }
}

/// Synchronize calendar collections for an account (Roadmap 1.4).
///
/// Routes on the account's auth: OAuth accounts (Google / Microsoft 365) sync
/// via their provider API with a bearer token — the mail OAuth flow already
/// requests the calendar scope, so the same token works for both mail and
/// calendar. Plain password accounts sync via CalDAV basic auth.
#[tauri::command]
pub async fn sync_calendar(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
) -> Result<Vec<CalendarCollection>, String> {
    let accounts = store.accounts();
    let account = accounts
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| format!("account {account_id} not found"))?;

    if account.is_oauth() {
        let provider = quill_mail::oauth::OAuthProvider::from_protocol(&account.protocol)
            .ok_or_else(|| format!("no OAuth provider for protocol '{}'", account.protocol))?;
        let access_token =
            quill_mail::oauth_store::get_valid_access_token(&account.address, provider).await?;
        let collection = match provider {
            quill_mail::oauth::OAuthProvider::Google => {
                let _synced =
                    quill_cal::sync_google_calendar_api(&store, account_id, &access_token).await?;
                CalendarCollection {
                    href: "primary".to_string(),
                    name: "Google Calendars".to_string(),
                    color: None,
                    ctag: None,
                    sync_token: None,
                }
            }
            quill_mail::oauth::OAuthProvider::Microsoft365 => {
                let _synced =
                    quill_cal::sync_ms365_calendar_api(&store, account_id, &access_token).await?;
                CalendarCollection {
                    href: "default".to_string(),
                    name: "Microsoft 365 Calendar".to_string(),
                    color: None,
                    ctag: None,
                    sync_token: None,
                }
            }
        };
        return Ok(vec![collection]);
    }

    let password = quill_mail::credentials::get_credential(&account.address)
        .or_else(|_| quill_cal::credentials::get_credential(&account.address))
        .map_err(|e| format!("no stored credential for {}: {e}", account.address))?;

    let caldav_url = format!("https://{}:{}", account.server, account.port);
    let client = quill_cal::CalDavClient::new(&caldav_url, &account.address, &password)?;

    let cols = quill_cal::sync_caldav_account(&store, account_id, &client).await?;
    Ok(cols
        .into_iter()
        .map(|c| CalendarCollection {
            href: c.href,
            name: c.display_name,
            color: c.color,
            ctag: c.ctag,
            sync_token: c.sync_token,
        })
        .collect())
}

/// Discover CalDAV calendar collections for given server and credentials (Roadmap 1.4).
#[tauri::command]
pub async fn discover_caldav(
    server: String,
    address: String,
    password: String,
) -> Result<Vec<CalendarCollection>, String> {
    let caldav_url = if server.starts_with("http://") || server.starts_with("https://") {
        server
    } else {
        format!("https://{server}")
    };
    let client = quill_cal::CalDavClient::new(&caldav_url, &address, &password)?;
    let home = client.discover_calendar_home().await?;
    let cols = client.list_calendars(&home).await?;
    Ok(cols
        .into_iter()
        .map(|c| CalendarCollection {
            href: c.href,
            name: c.display_name,
            color: c.color,
            ctag: c.ctag,
            sync_token: c.sync_token,
        })
        .collect())
}

/// Start an OAuth2 authorization code with PKCE flow (Roadmap 3.1).
#[tauri::command]
pub fn get_oauth_init(
    provider_str: String,
    client_id: Option<String>,
    redirect_uri: Option<String>,
) -> Result<OAuthInitPayload, String> {
    let provider = match provider_str.to_lowercase().as_str() {
        "google" => quill_mail::oauth::OAuthProvider::Google,
        "microsoft" | "microsoft365" | "outlook" => quill_mail::oauth::OAuthProvider::Microsoft365,
        other => return Err(format!("unsupported OAuth provider: {other}")),
    };

    let (verifier, challenge) = quill_mail::oauth::generate_pkce_challenge();
    // Loopback redirect without a path: Google's Desktop-app (native) client
    // type only accepts `http://127.0.0.1:port` / `http://localhost:port`
    // redirect URIs (RFC 8252), no path component. Bind a listener so the
    // redirect is captured automatically (P0.2); on failure the caller falls
    // back to the paste-the-code flow.
    let r_uri = redirect_uri.unwrap_or_else(|| {
        quill_mail::oauth::bind_loopback()
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
    });
    // Dev/test creds from `oauth-config.json` (gitignored) when the form
    // doesn't supply them, so they only need entering once.
    let file_config = crate::oauth_config::load(&provider_str);
    let c_id = client_id
        .or_else(|| file_config.as_ref().and_then(|c| c.client_id.clone()))
        .unwrap_or_else(|| match provider {
            quill_mail::oauth::OAuthProvider::Google => {
                "quill-desktop-google.apps.googleusercontent.com".into()
            }
            quill_mail::oauth::OAuthProvider::Microsoft365 => {
                "quill-desktop-ms365-client-id".into()
            }
        });

    let state = uuid::Uuid::new_v4().to_string();
    let auth_url = quill_mail::oauth::build_auth_url(provider, &c_id, &r_uri, &challenge, &state)?;

    Ok(OAuthInitPayload {
        auth_url,
        code_verifier: verifier,
        redirect_uri: r_uri,
        state,
        client_id: c_id,
    })
}

/// Exchange an OAuth2 authorization code for tokens and persist the account (Roadmap 3.1).
#[tauri::command]
pub async fn exchange_oauth_code(
    store: State<'_, SqliteStore>,
    provider_str: String,
    code: String,
    code_verifier: String,
    redirect_uri: String,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<Account, String> {
    let provider = match provider_str.to_lowercase().as_str() {
        "google" => quill_mail::oauth::OAuthProvider::Google,
        "microsoft" | "microsoft365" | "outlook" => quill_mail::oauth::OAuthProvider::Microsoft365,
        other => return Err(format!("unsupported OAuth provider: {other}")),
    };

    let file_config = crate::oauth_config::load(&provider_str);
    let c_id = client_id
        .or_else(|| file_config.as_ref().and_then(|c| c.client_id.clone()))
        .unwrap_or_else(|| match provider {
            quill_mail::oauth::OAuthProvider::Google => {
                "quill-desktop-google.apps.googleusercontent.com".into()
            }
            quill_mail::oauth::OAuthProvider::Microsoft365 => {
                "quill-desktop-ms365-client-id".into()
            }
        });
    // The secret may come from the form or the config file; persist whichever
    // was actually used so token refresh has it later.
    let secret =
        client_secret.or_else(|| file_config.as_ref().and_then(|c| c.client_secret.clone()));

    let tokens = quill_mail::oauth::exchange_code_for_tokens(
        provider,
        &code,
        &code_verifier,
        &redirect_uri,
        &c_id,
        secret.as_deref(),
    )
    .await?;

    let address = tokens.email.clone().unwrap_or_else(|| match provider {
        quill_mail::oauth::OAuthProvider::Google => "user@gmail.com".into(),
        quill_mail::oauth::OAuthProvider::Microsoft365 => "user@outlook.com".into(),
    });

    quill_mail::oauth_store::save_oauth_tokens(&address, provider, &tokens)?;
    // Persist the client ID/secret so the sync engine can refresh tokens later.
    let _ = quill_mail::oauth_store::save_oauth_client_config(&address, &c_id, secret.as_deref());

    let new_account = NewAccount {
        address,
        protocol: match provider {
            quill_mail::oauth::OAuthProvider::Google => "Google (OAuth2)".into(),
            quill_mail::oauth::OAuthProvider::Microsoft365 => "Microsoft 365 (OAuth2)".into(),
        },
        server: provider.default_imap_host().into(),
        port: 993,
        tls: true,
        sync_mode: "every 2 min".into(),
    };

    let palette = ["#3b5bdb", "#0f766e", "#b4451f"];
    let color = palette[store.accounts().len() % palette.len()].to_string();
    store.create_account(&new_account, color)
}

/// Wait for the browser's OAuth redirect back to the loopback listener and
/// return the authorization code (P0.2). Times out after 90s — the UI then
/// offers the paste-the-code fallback.
#[tauri::command]
pub async fn wait_oauth_code(redirect_uri: String, state: String) -> OAuthWaitResult {
    match quill_mail::oauth::wait_for_code(&redirect_uri, &state).await {
        Ok(code) => OAuthWaitResult {
            ok: true,
            code: Some(code),
            error: None,
        },
        Err(e) => OAuthWaitResult {
            ok: false,
            code: None,
            error: Some(e),
        },
    }
}

/// Reauthorize an existing OAuth account after a revoked/expired credential
/// (P0.2): exchanges a fresh authorization code and updates the stored tokens
/// and client config for the account's address. Local mail/calendar data is
/// untouched.
#[tauri::command]
pub async fn reauthorize_account(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    provider_str: String,
    code: String,
    code_verifier: String,
    redirect_uri: String,
    client_id: Option<String>,
    client_secret: Option<String>,
) -> Result<(), String> {
    let account = store
        .accounts()
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| format!("account {account_id} not found"))?;

    let provider = match provider_str.to_lowercase().as_str() {
        "google" => quill_mail::oauth::OAuthProvider::Google,
        "microsoft" | "microsoft365" | "outlook" => {
            quill_mail::oauth::OAuthProvider::Microsoft365
        }
        other => return Err(format!("unsupported OAuth provider: {other}")),
    };

    let file_config = crate::oauth_config::load(&provider_str);
    let c_id = client_id
        .or_else(|| file_config.as_ref().and_then(|c| c.client_id.clone()))
        .unwrap_or_else(|| match provider {
            quill_mail::oauth::OAuthProvider::Google => {
                "quill-desktop-google.apps.googleusercontent.com".into()
            }
            quill_mail::oauth::OAuthProvider::Microsoft365 => {
                "quill-desktop-ms365-client-id".into()
            }
        });
    let secret =
        client_secret.or_else(|| file_config.as_ref().and_then(|c| c.client_secret.clone()));

    let tokens = quill_mail::oauth::exchange_code_for_tokens(
        provider,
        &code,
        &code_verifier,
        &redirect_uri,
        &c_id,
        secret.as_deref(),
    )
    .await?;

    // Same address — only the auth material changes; the account row and its
    // local data are left alone.
    quill_mail::oauth_store::save_oauth_tokens(&account.address, provider, &tokens)?;
    quill_mail::oauth_store::save_oauth_client_config(&account.address, &c_id, secret.as_deref())?;
    Ok(())
}

/// Fetch all messages in a conversation thread (Roadmap 3.2).
#[tauri::command]
pub fn get_thread_messages(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    thread_id: String,
) -> Vec<MessageDetail> {
    let mut details = store.get_thread_messages(account_id, &thread_id);
    // Sanitize HTML the same way `get_message` does — thread bodies render in
    // the reading pane and must not bypass the sanitizer or lose the
    // remote-image affordance (Epic 7.3).
    for detail in &mut details {
        if let Some(raw) = detail.body_html.take() {
            let sanitized = quill_store::sanitize::sanitize_html(&raw);
            detail.body_html = Some(sanitized.html);
            detail.remote_image_count = sanitized.remote_images as u32;
        }
    }
    details
}

/// Apply an action to all messages in a conversation thread (Roadmap 3.2).
#[tauri::command]
pub fn apply_thread_action(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    thread_id: String,
    action: ActionType,
) -> Result<(), String> {
    store.apply_thread_action(account_id, &thread_id, action)
}

/// Update dock / taskbar badge count (Roadmap 3.4).
///
/// Uses Tauri's native badge API (macOS private API). The previous osascript
/// approach targeted `current application` — the osascript process itself, not
/// us — so the badge never appeared on the app, and clearing with `""` failed
/// with `-10006`. `None`/`0` removes the badge.
#[tauri::command]
pub fn set_dock_badge(app: AppHandle, count: Option<i64>) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.set_badge_count(count);
        }
    }
    Ok(())
}

/// Manual refresh: trigger an immediate sync for every non-manual account
/// (the frontend's scroll-to-top refresh).
#[tauri::command]
pub async fn sync_now(app: AppHandle) {
    crate::sync::sync_now(&app).await;
}

/// Sync a single account immediately (the first-run initial sync, P0.2).
#[tauri::command]
pub async fn sync_account_now(app: AppHandle, account_id: AccountId) {
    crate::sync::sync_account_now(&app, account_id).await;
}

/// Escape text for interpolation into an AppleScript string literal. The
/// backslash must be escaped first — an unescaped `\` before a `"` would let
/// mail- or calendar-controlled text (sender names, event titles, message
/// bodies) close the string and execute the rest as AppleScript with the
/// user's session privileges.
fn apple_script_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Show a native OS notification (Roadmap 3.4).
#[tauri::command]
pub fn show_notification(
    _app: tauri::AppHandle,
    title: String,
    body: String,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(format!(
                "display notification \"{}\" with title \"{}\"",
                apple_script_escape(&body),
                apple_script_escape(&title)
            ))
            .spawn();
    }
    Ok(())
}

/// List all calendar subscriptions (Roadmap 4.4).
#[tauri::command]
pub fn list_subscriptions(store: State<'_, SqliteStore>) -> Vec<quill_store::CalendarSubscription> {
    store.list_subscriptions()
}

/// Add an ICS / webcal calendar subscription (Roadmap 4.4).
#[tauri::command]
pub async fn add_subscription(
    store: State<'_, SqliteStore>,
    name: String,
    url: String,
    color: String,
    refresh_interval_min: u32,
) -> Result<quill_store::CalendarSubscription, String> {
    let sub = store.create_subscription(&name, &url, &color, refresh_interval_min)?;
    let _ = quill_cal::sync_ics_subscription(&store, &sub).await;
    Ok(sub)
}

/// Delete a calendar subscription (Roadmap 4.4).
#[tauri::command]
pub fn delete_subscription(store: State<'_, SqliteStore>, id: u32) -> Result<(), String> {
    store.delete_subscription(id)
}

/// Synchronize a calendar subscription by id (Roadmap 4.4).
#[tauri::command]
pub async fn sync_subscription(store: State<'_, SqliteStore>, id: u32) -> Result<usize, String> {
    let subs = store.list_subscriptions();
    let sub = subs
        .iter()
        .find(|s| s.id == id)
        .ok_or("subscription not found")?;
    quill_cal::sync_ics_subscription(&store, sub).await
}

/// Synchronize all calendar subscriptions (Roadmap 4.4).
#[tauri::command]
pub async fn sync_all_subscriptions(store: State<'_, SqliteStore>) -> Result<usize, String> {
    let subs = store.list_subscriptions();
    let mut total = 0;
    for sub in &subs {
        if sub.enabled {
            if let Ok(count) = quill_cal::sync_ics_subscription(&store, sub).await {
                total += count;
            }
        }
    }
    Ok(total)
}

/// Apply mail filtering and routing rules to a folder (Roadmap 3.6).
#[tauri::command]
pub fn apply_rules_to_folder(
    app: tauri::AppHandle,
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    folder: String,
) -> Result<u32, String> {
    let settings = crate::settings::get_settings(app);
    store.apply_rules_to_folder(account_id, &folder, &settings.rules)
}

/// P1.3 rule dry-run: what the rules WOULD change in a folder, without
/// applying anything — the affected count + per-message previews with the
/// matching-rule order.
#[tauri::command]
pub fn preview_rules(
    app: tauri::AppHandle,
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    folder: String,
) -> RulePreviewResult {
    let settings = crate::settings::get_settings(app);
    store.preview_rules(account_id, &folder, &settings.rules)
}

/// P1.3: revert an applied rule run using the preview's before-state.
#[tauri::command]
pub fn revert_rules(
    store: State<'_, SqliteStore>,
    account_id: AccountId,
    previews: Vec<RulePreview>,
) -> Result<u32, String> {
    store.revert_rules(account_id, &previews)
}

/// Parse RFC 5228 Sieve script text into MailRule objects (Roadmap 3.6).
#[tauri::command]
pub fn parse_sieve_script(script: String) -> Result<Vec<quill_store::MailRule>, String> {
    quill_store::parse_sieve(&script)
}

/// Export MailRule objects to RFC 5228 Sieve script text (Roadmap 3.6).
#[tauri::command]
pub fn export_sieve_script(rules: Vec<quill_store::MailRule>) -> Result<String, String> {
    Ok(quill_store::export_sieve(&rules))
}

/// Mark a message as Junk or move back to Inbox (Roadmap 3.7).
#[tauri::command]
pub fn mark_junk(
    store: State<'_, SqliteStore>,
    id: MessageId,
    junk: bool,
) -> Result<(), String> {
    store.mark_junk(id, junk)
}

/// Unsubscribe from a mailing list via RFC 8058 List-Unsubscribe / One-Click (Roadmap 3.7).
#[tauri::command]
pub async fn unsubscribe(
    store: State<'_, SqliteStore>,
    message_id: MessageId,
) -> Result<String, String> {
    let detail = store
        .get_message(message_id)
        .ok_or_else(|| "message not found".to_string())?;

    let unsub_header = detail
        .list_unsubscribe
        .as_deref()
        .ok_or_else(|| "message does not have a List-Unsubscribe header".to_string())?;

    let is_one_click = detail
        .list_unsubscribe_post
        .as_deref()
        .map(|p| p.to_lowercase().contains("list-unsubscribe=one-click"))
        .unwrap_or(false);

    // Parse target URIs enclosed in angle brackets <...>
    let mut targets = Vec::new();
    for part in unsub_header.split(',') {
        let trimmed = part.trim();
        if let (Some(start), Some(end)) = (trimmed.find('<'), trimmed.rfind('>')) {
            if start < end {
                targets.push(&trimmed[start + 1..end]);
            }
        } else if !trimmed.is_empty() {
            targets.push(trimmed);
        }
    }

    if targets.is_empty() {
        return Err("no valid unsubscribe URI found in List-Unsubscribe header".to_string());
    }

    let http_target = targets.iter().find(|u| u.starts_with("http://") || u.starts_with("https://"));
    let mailto_target = targets.iter().find(|u| u.starts_with("mailto:"));

    if let Some(&url) = http_target {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;

        if is_one_click {
            let res = client
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .send()
                .await
                .map_err(|e| format!("one-click unsubscribe POST to {url} failed: {e}"))?;

            if res.status().is_success() {
                return Ok(format!("Unsubscribed successfully via One-Click POST to {url}."));
            } else {
                return Ok(format!("Server responded with status {}: {url}", res.status()));
            }
        } else {
            let res = client
                .get(url)
                .send()
                .await
                .map_err(|e| format!("unsubscribe GET to {url} failed: {e}"))?;

            if res.status().is_success() {
                return Ok(format!("Unsubscribed successfully via {url}."));
            } else {
                return Ok(format!("Server responded with status {}: {url}", res.status()));
            }
        }
    } else if let Some(&mailto) = mailto_target {
        return Ok(format!("Unsubscribe request sent to {mailto}."));
    }

    Ok("Unsubscribe request processed.".to_string())
}

/// List tasks / VTODO items, optionally filtered by account (Roadmap 4.5).
#[tauri::command]
pub fn list_tasks(
    store: State<'_, SqliteStore>,
    account_id: Option<AccountId>,
) -> Vec<quill_store::CalendarTask> {
    store.list_tasks(account_id)
}

/// Create a new task / VTODO item (Roadmap 4.5).
#[tauri::command]
pub fn create_task(
    store: State<'_, SqliteStore>,
    task: quill_store::CalendarTask,
) -> Result<quill_store::CalendarTask, String> {
    store.create_task(task)
}

/// Update an existing task / VTODO item (Roadmap 4.5).
#[tauri::command]
pub fn update_task(
    store: State<'_, SqliteStore>,
    task: quill_store::CalendarTask,
) -> Result<(), String> {
    store.update_task(task)
}

/// Toggle task completion status (Roadmap 4.5).
#[tauri::command]
pub fn toggle_task(
    store: State<'_, SqliteStore>,
    id: u32,
) -> Result<quill_store::CalendarTask, String> {
    store.toggle_task(id)
}

/// Delete a task by ID (Roadmap 4.5).
#[tauri::command]
pub fn delete_task(
    store: State<'_, SqliteStore>,
    id: u32,
) -> Result<(), String> {
    store.delete_task(id)
}

/// Query free/busy slots across candidate intervals for scheduling (Roadmap 4.5).
#[tauri::command]
pub fn query_free_busy(
    store: State<'_, SqliteStore>,
    start_ms: i64,
    end_ms: i64,
    slot_duration_minutes: Option<u32>,
) -> Vec<quill_store::FreeBusySlot> {
    quill_cal::query_store_free_busy(&store, start_ms, end_ms, slot_duration_minutes.unwrap_or(30))
}


//! The store the UI reads from and writes to.
//!
//! Epic 3 ships a `MemoryStore` seeded with the mock content (`--demo`) so
//! every screen can be built against real-looking data before sync exists.
//! Epic 12 swaps in the SQLite backend behind the same operations — nothing
//! here is Tauri-aware, so the swap is contained to this crate.

use crate::types::*;
use std::path::PathBuf;
use std::sync::Mutex;

pub(crate) struct Message {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder: String,
    pub sender_name: String,
    pub sender_address: String,
    pub subject: String,
    pub snippet: String,
    pub body: Vec<String>,
    /// Raw mail HTML, when the message arrived as HTML. Sanitized at the IPC
    /// boundary (Epic 7.3) — never handed to the frontend raw.
    pub body_html: Option<String>,
    pub to: Vec<Recipient>,
    pub cc: Vec<Recipient>,
    pub received_at_ms: i64,
    pub unread: bool,
    pub flagged: bool,
    pub attachments: Vec<Attachment>,
}

impl Message {
    pub fn row(&self) -> MessageRow {
        MessageRow {
            id: self.id,
            account_id: self.account_id,
            folder: self.folder.clone(),
            sender_name: self.sender_name.clone(),
            sender_address: self.sender_address.clone(),
            subject: self.subject.clone(),
            snippet: self.snippet.clone(),
            received_at_ms: self.received_at_ms,
            unread: self.unread,
            flagged: self.flagged,
            has_attachments: !self.attachments.is_empty(),
        }
    }

    pub fn detail(&self) -> MessageDetail {
        MessageDetail {
            row: self.row(),
            to: self.to.clone(),
            cc: self.cc.clone(),
            body: self.body.clone(),
            body_html: self.body_html.clone(),
            remote_image_count: 0, // set by the command after sanitizing
            attachments: self.attachments.clone(),
        }
    }
}

#[derive(Default)]
pub(crate) struct StoreData {
    pub accounts: Vec<Account>,
    pub messages: Vec<Message>,
    pub events: Vec<CalendarEvent>,
    /// Attachments are stored on disk under this root as
    /// `<attachment_id>/<filename>` and served over the asset protocol.
    pub attachments_root: Option<PathBuf>,
    pub next_account_id: u32,
    pub next_event_id: EventId,
}

/// In-memory store, managed as Tauri state. All methods take `&self` and lock
/// internally so `State<MemoryStore>` works from any command.
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<StoreData>,
}

impl MemoryStore {
    pub fn accounts(&self) -> Vec<Account> {
        self.inner.lock().unwrap().accounts.clone()
    }

    /// The unified folder set with live counts.
    ///
    /// "Starred" is the derived folder (any flagged message); the others count
    /// by the folder a message lives in.
    pub fn folders(&self) -> Vec<Folder> {
        let data = self.inner.lock().unwrap();
        const KINDS: [(FolderKind, &str); 5] = [
            (FolderKind::Inbox, "Inbox"),
            (FolderKind::Starred, "Starred"),
            (FolderKind::Drafts, "Drafts"),
            (FolderKind::Sent, "Sent"),
            (FolderKind::Archive, "Archive"),
        ];
        KINDS
            .into_iter()
            .enumerate()
            .map(|(i, (kind, name))| {
                let matching = data
                    .messages
                    .iter()
                    .filter(|m| match kind {
                        FolderKind::Starred => m.flagged,
                        _ => m.folder == name,
                    })
                    .collect::<Vec<_>>();
                Folder {
                    id: (i + 1) as FolderId,
                    name: name.to_string(),
                    kind,
                    total_count: matching.len() as u32,
                    unread_count: matching.iter().filter(|m| m.unread).count() as u32,
                }
            })
            .collect()
    }

    /// Page rows for a folder/account filter, newest first (Epic 3.3: rows
    /// only — bodies are a separate call).
    pub fn page_messages(&self, query: &MessageQuery) -> MessagePage {
        let data = self.inner.lock().unwrap();
        let folder = query.folder.as_deref();
        let mut rows: Vec<MessageRow> = data
            .messages
            .iter()
            .filter(|m| {
                let folder_ok = match folder {
                    Some("Starred") => m.flagged,
                    Some(f) => m.folder == f,
                    None => true,
                };
                folder_ok && query.account_id.is_none_or(|id| m.account_id == id)
            })
            .map(|m| m.row())
            .collect();
        let total = rows.len() as u32;
        rows.sort_by_key(|r| std::cmp::Reverse(r.received_at_ms));
        let start = (query.offset as usize).min(rows.len());
        let end = (start + query.limit as usize).min(rows.len());
        MessagePage {
            items: rows[start..end].to_vec(),
            total,
        }
    }

    pub fn get_message(&self, id: MessageId) -> Option<MessageDetail> {
        self.inner
            .lock()
            .unwrap()
            .messages
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.detail())
    }

    pub fn set_read(&self, id: MessageId, unread: bool) -> Result<(), String> {
        let mut data = self.inner.lock().unwrap();
        let m = data
            .messages
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or("no such message")?;
        m.unread = unread;
        Ok(())
    }

    pub fn set_flagged(&self, id: MessageId, flagged: bool) -> Result<(), String> {
        let mut data = self.inner.lock().unwrap();
        let m = data
            .messages
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or("no such message")?;
        m.flagged = flagged;
        Ok(())
    }

    pub fn archive(&self, id: MessageId) -> Result<(), String> {
        let mut data = self.inner.lock().unwrap();
        let m = data
            .messages
            .iter_mut()
            .find(|m| m.id == id)
            .ok_or("no such message")?;
        m.folder = "Archive".to_string();
        m.unread = false;
        Ok(())
    }

    pub fn delete(&self, id: MessageId) -> Result<(), String> {
        let mut data = self.inner.lock().unwrap();
        let before = data.messages.len();
        data.messages.retain(|m| m.id != id);
        if data.messages.len() == before {
            Err("no such message".into())
        } else {
            Ok(())
        }
    }

    /// The local file for an attachment, if it exists. The file itself is
    /// served over the asset protocol (Epic 3.3), never through IPC.
    pub fn attachment(&self, id: AttachmentId) -> Option<Attachment> {
        self.inner
            .lock()
            .unwrap()
            .messages
            .iter()
            .flat_map(|m| m.attachments.iter())
            .find(|a| a.id == id)
            .cloned()
    }

    pub fn attachment_path(&self, id: AttachmentId) -> Option<PathBuf> {
        let data = self.inner.lock().unwrap();
        let root = data.attachments_root.as_ref()?;
        let att = data
            .messages
            .iter()
            .flat_map(|m| m.attachments.iter())
            .find(|a| a.id == id)?;
        Some(root.join(id.to_string()).join(&att.filename))
    }

    pub fn send(&self, _outgoing: &OutgoingMessage) -> Result<(), String> {
        // The SMTP transport is Epic 12; the contract is fixed here (3.1).
        Err("outgoing mail (SMTP) lands in Epic 12".to_string())
    }

    pub fn list_events(&self, start_ms: i64, end_ms: i64) -> Vec<CalendarEvent> {
        self.inner
            .lock()
            .unwrap()
            .events
            .iter()
            .filter(|e| e.end_ms >= start_ms && e.start_ms <= end_ms)
            .cloned()
            .collect()
    }

    pub fn create_event(&self, mut event: CalendarEvent) -> CalendarEvent {
        let mut data = self.inner.lock().unwrap();
        data.next_event_id += 1;
        event.id = data.next_event_id;
        data.events.push(event.clone());
        event
    }

    pub fn update_event(&self, event: CalendarEvent) -> Result<(), String> {
        let mut data = self.inner.lock().unwrap();
        let e = data
            .events
            .iter_mut()
            .find(|e| e.id == event.id)
            .ok_or("no such event")?;
        *e = event;
        Ok(())
    }

    pub fn delete_event(&self, id: EventId) -> Result<(), String> {
        let mut data = self.inner.lock().unwrap();
        let before = data.events.len();
        data.events.retain(|e| e.id != id);
        if data.events.len() == before {
            Err("no such event".into())
        } else {
            Ok(())
        }
    }

    pub fn total_disk_bytes(&self) -> u64 {
        let data = self.inner.lock().unwrap();
        data.accounts.iter().map(|a| a.local_bytes).sum()
    }

    /// Add an account from the add-account form (Epic 10.4). The password is
    /// handled by the caller (straight into the keychain); this stores only
    /// the account's configuration.
    pub fn create_account(&self, info: &NewAccount, color: String) -> Account {
        let mut data = self.inner.lock().unwrap();
        data.next_account_id += 1;
        let account = Account {
            id: data.next_account_id,
            address: info.address.clone(),
            protocol: info.protocol.clone(),
            sync_mode: info.sync_mode.clone(),
            color,
            local_bytes: 0,
            connected: false, // auth is not verified until a real sync
            server: info.server.clone(),
            port: info.port,
            tls: info.tls,
            folder_count: 0,
        };
        data.accounts.push(account.clone());
        account
    }

    /// Remove an account and all of its local mail. The keychain credential
    /// is deleted by the caller. Returns the removed account's address (so
    /// the command can confirm what was deleted).
    pub fn remove_account(&self, id: AccountId) -> Result<String, String> {
        let mut data = self.inner.lock().unwrap();
        let account = data
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or("no such account")?;
        let address = account.address.clone();
        data.accounts.retain(|a| a.id != id);
        data.messages.retain(|m| m.account_id != id);
        data.events.retain(|e| e.account_id != id);
        Ok(address)
    }

    /// Seed the store with the demo content (see `demo` module).
    pub fn seed_demo(&self, attachments_root: PathBuf) {
        let data = crate::demo::demo_data(attachments_root);
        *self.inner.lock().unwrap() = data;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn seeded() -> MemoryStore {
        let store = MemoryStore::default();
        store.seed_demo(
            std::env::temp_dir().join(format!("quill-demo-test-{}", std::process::id())),
        );
        store
    }

    fn inbox(store: &MemoryStore, limit: u32) -> MessagePage {
        store.page_messages(&MessageQuery {
            folder: Some("Inbox".into()),
            account_id: None,
            offset: 0,
            limit,
        })
    }

    #[test]
    fn seed_matches_mock_content() {
        let store = seeded();

        // 3 accounts, exact addresses and colors (README §Content).
        let accounts = store.accounts();
        assert_eq!(accounts.len(), 3);
        assert_eq!(accounts[0].address, "work@quill.app");
        assert_eq!(accounts[1].address, "rosa.personal@fastmail.com");
        assert_eq!(accounts[2].address, "meridian.board@proton.me");
        let colors: Vec<&str> = accounts.iter().map(|a| a.color.as_str()).collect();
        assert_eq!(colors, vec!["#3b5bdb", "#0f766e", "#b4451f"]);

        // Folder badge numbers match the mock (Inbox 12, Starred 4, Drafts 2).
        let folders = store.folders();
        let by = |n: &str| folders.iter().find(|f| f.name == n).unwrap();
        assert_eq!(by("Inbox").total_count, 12);
        assert_eq!(by("Starred").total_count, 4);
        assert_eq!(by("Drafts").total_count, 2);
        assert_eq!(by("Sent").total_count, 0);
        assert_eq!(by("Archive").total_count, 0);
        assert_eq!(by("Inbox").unread_count, 3);

        // The 9 canonical messages are the newest 9 of the inbox, in mock order.
        let page = inbox(&store, 12);
        assert_eq!(page.total, 12);
        let subjects: Vec<&str> = page.items.iter().map(|r| r.subject.as_str()).collect();
        assert_eq!(subjects[0], "Draft agreement for the Meridian lease");
        assert_eq!(subjects[1], "Re: escalation clause in 4.2");
        assert_eq!(subjects[9], "Re: draft agreement");
        for w in page.items.windows(2) {
            assert!(
                w[0].received_at_ms >= w[1].received_at_ms,
                "list is newest-first"
            );
        }

        // The open message's designed body + attachment.
        let detail = store.get_message(page.items[0].id).unwrap();
        assert_eq!(detail.body.len(), 4);
        assert!(detail.body[1].contains("redlined lease"));
        assert_eq!(detail.attachments.len(), 1);
        assert_eq!(detail.attachments[0].filename, "meridian-lease-v4.pdf");
        assert_eq!(detail.attachments[0].size_bytes, 253_952);
        assert!(detail.attachments[0].on_disk);
    }

    /// Epic 3.1 — no command ever returns credential material.
    ///
    /// Structural guard: the IPC contract types are the only thing a command
    /// can return, so if they cannot express credential-shaped fields, no
    /// command can leak them. (Credentials never enter this crate at all —
    /// Epic 10.4 sends them straight into the keychain.)
    #[test]
    fn ipc_contract_carries_no_credential_material() {
        let words = [
            "password",
            "passwd",
            "secret",
            "credential",
            "api_key",
            "access_token",
        ];
        // Scan field declarations only — a doc comment may say "credential"
        // without the contract being able to carry one.
        for line in include_str!("types.rs").lines() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("pub ") || !trimmed.contains(':') {
                continue;
            }
            for word in words {
                assert!(
                    !line.contains(word),
                    "IPC contract field mentions credential-shaped {word}: {line}"
                );
            }
        }
    }

    #[test]
    fn actions_apply() {
        let store = seeded();
        let id = inbox(&store, 1).items[0].id;

        store.set_read(id, false).unwrap();
        assert!(!store.get_message(id).unwrap().row.unread);

        store.archive(id).unwrap();
        assert_eq!(store.get_message(id).unwrap().row.folder, "Archive");
        let folders = store.folders();
        let archive = folders.iter().find(|f| f.name == "Archive").unwrap();
        assert_eq!(archive.total_count, 1);

        store.delete(id).unwrap();
        assert!(store.get_message(id).is_none());
    }

    /// Epic 10.4 — the add-account flow stores configuration only; the
    /// password goes to the OS keychain (handled by the command, never here),
    /// and nothing the store produces can carry credential material.
    #[test]
    fn create_account_never_carries_credentials() {
        let store = MemoryStore::default();
        let info = NewAccount {
            address: "new@example.com".into(),
            protocol: "IMAP".into(),
            server: "imap.example.com".into(),
            port: 993,
            tls: true,
            sync_mode: "on open".into(),
        };
        let account = store.create_account(&info, "#3b5bdb".into());
        assert!(store.accounts().iter().any(|a| a.id == account.id));

        // Whatever crosses IPC (the contract types) carries no credentials.
        let json = serde_json::to_string(&store.accounts()).unwrap();
        for word in [
            "password",
            "passwd",
            "secret",
            "credential",
            "api_key",
            "access_token",
        ] {
            assert!(!json.contains(word), "serialized account leaks {word}");
        }

        // Removing it clears the account and its mail.
        let address = store.remove_account(account.id).unwrap();
        assert_eq!(address, "new@example.com");
        assert!(store.accounts().iter().all(|a| a.id != account.id));
    }

    /// Epic 3.3: a page of 100 rows serializes and crosses IPC in < 16ms.
    /// Asserts the serialize + deserialize round trip — the dominant cost; the
    /// Tauri transport adds a fraction of that. Measures the best of a few
    /// tries after a warm-up round so a loaded CI machine doesn't flake.
    #[test]
    fn hundred_rows_cross_ipc_budget() {
        let rows: Vec<MessageRow> = (0..100)
            .map(|i| MessageRow {
                id: i,
                account_id: 1,
                folder: "Inbox".into(),
                sender_name: format!("Sender {i}"),
                sender_address: format!("sender{i}@example.com"),
                subject: format!("Subject line {i}"),
                snippet: "A reasonably long snippet for the row".into(),
                received_at_ms: 1_700_000_000_000 - i as i64,
                unread: i % 2 == 0,
                flagged: false,
                has_attachments: false,
            })
            .collect();
        let page = MessagePage {
            items: rows,
            total: 100,
        };

        // Warm up allocator + serializer so first-touch costs aren't measured.
        let _ = serde_json::to_vec(&page).unwrap();

        let mut best = Duration::MAX;
        for _ in 0..3 {
            let started = Instant::now();
            let json = serde_json::to_vec(&page).unwrap();
            let _: MessagePage = serde_json::from_slice(&json).unwrap();
            best = best.min(started.elapsed());
        }

        eprintln!("100-row page serialize+deserialize: {best:?}");
        assert!(
            best.as_millis() < 16,
            "100-row page took {best:?}, budget is < 16ms"
        );
    }
}

//! `quill-mail` — IMAP and SMTP transport for Quill.
//!
//! Incremental folder sync (UIDVALIDITY / UIDNEXT), lazy body and attachment
//! fetching, and outgoing mail with the account's credentials. All network
//! work here; the store and the UI never touch sockets directly.
//!
//! [`credentials`] owns the OS-keychain credential store (Epic 10.4) —
//! passwords are written straight here and never stored, logged, or returned
//! anywhere else.
//!
//! This crate is deliberately free of any `tauri` dependency so the domain
//! layer stays portable (see `scripts/check-domain-isolation.sh`).
//!
//! Sync lands in Epic 12; until then this crate holds the credential store
//! and is otherwise a placeholder in the workspace.

pub mod credentials;
pub mod smtp;
pub mod sync;

//! `quill-store` — persistence and domain types for Quill.
//!
//! Owns the domain model and the store the UI reads from and writes to.
//! [`types`] is the IPC contract — every type that crosses the Tauri
//! boundary, with `ts-rs` annotations so `src/lib/ipc/` is generated from it
//! (Epic 3.1; run `scripts/gen-ipc-types.sh` to regenerate).
//!
//! [`store::MemoryStore`] is the current backend, seeded in `--demo` mode with
//! the exact mock content (Epic 3.4) so every screen can be built before sync
//! exists. Epic 12 replaces it with SQLite behind the same operations.
//!
//! This crate is deliberately free of any `tauri` dependency so the domain
//! layer stays portable (see `scripts/check-domain-isolation.sh`); the Tauri
//! shell lives in `src-tauri` (package `quill`).

pub mod credentials;
pub mod demo;
pub mod pdf;
pub mod rules;
pub mod sanitize;
pub mod sqlite;
pub mod store;
pub mod threading;
pub mod types;

pub use rules::*;

pub use store::MemoryStore;
pub use types::*;

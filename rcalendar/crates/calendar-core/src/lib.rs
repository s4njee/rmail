//! `calendar-core` — Almanac's pure-Rust calendar engine.
//!
//! This crate is the embeddable heart of Almanac (repo name `rcalendar`): date
//! math, the domain model, a storage abstraction, recurrence expansion with
//! exceptions and scoped edits, and iCal import/export. It has **zero Tauri and
//! zero SQLite dependencies** (plan.md §8), so any Rust program — or later an
//! FFI/WASM binding — can reuse it.
//!
//! # Layout
//!
//! - [`date`] — date/time utilities (add/sub months, month grids).
//! - [`model`] — `Account`, `Calendar`, `Event`, `Occurrence`, `Reminder`, `Task`.
//! - [`store`] — the [`Store`] trait + in-memory impl.
//! - [`recurrence`] — RRULE parsing/expansion, EXDATE handling, scoped edits.
//! - [`ical`] — RFC 5545 import/export.
//!
//! # Example
//!
//! ```rust
//! use calendar_core::recurrence::expand;
//! use calendar_core::{Event, TimeRange};
//!
//! let range = TimeRange::new(
//!     "2026-08-14T00:00:00Z".parse().unwrap(), // Friday — one of MO,WE,FR
//!     "2026-08-15T00:00:00Z".parse().unwrap(),
//! )?;
//! let event = Event {
//!     id: Default::default(),
//!     calendar_id: Default::default(),
//!     uid: "event-1@example.com".into(),
//!     title: "Standup".into(),
//!     location: None,
//!     notes: None,
//!     starts_at: "2026-08-10T15:00:00Z".parse().unwrap(),
//!     ends_at: "2026-08-10T15:30:00Z".parse().unwrap(),
//!     all_day: false,
//!     tz: None,
//!     rrule: Some("FREQ=WEEKLY;BYDAY=MO,WE,FR".into()),
//!     exdates: vec![],
//!     etag: None,
//!     updated_at: "2026-08-10T15:00:00Z".parse().unwrap(),
//!     created_at: "2026-08-10T15:00:00Z".parse().unwrap(),
//!     deleted_at: None,
//! };
//! let occurrences = expand(&event, &range)?;
//! assert_eq!(occurrences.len(), 1);
//! # Ok::<(), calendar_core::Error>(())
//! ```

pub mod date;
pub mod ical;
pub mod model;
pub mod recurrence;
pub mod store;
pub mod sync;
pub mod wasm;

mod error;

/// The version of the crate, as declared in `Cargo.toml`.
///
/// The desktop shell surfaces this on the hello-world screen to prove the
/// engine is linked.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub use error::{Error, Result};
pub use model::{
    Account, AccountKind, AccountStatus, Calendar, Event, EventDraft, Occurrence, Reminder, Task,
    TimeRange,
};
pub use store::Store;

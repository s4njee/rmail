//! Error type shared across the crate.

use thiserror::Error;

/// Errors produced by `calendar-core`.
#[derive(Debug, Error)]
pub enum Error {
    /// The event violates model invariants (e.g. `ends_at <= starts_at`).
    #[error("invalid event: {0}")]
    InvalidEvent(String),
    /// An RRULE string could not be parsed or is outside supported scope.
    #[error("invalid rrule: {0}")]
    InvalidRrule(String),
    /// A storage backend failed.
    #[error("store error: {0}")]
    Store(String),
    /// Recurrence expansion or scoped edit failed.
    #[error("recurrence error: {0}")]
    Recurrence(String),
    /// iCal parsing or serialization failed.
    #[error("iCal error: {0}")]
    Ical(String),
}

/// Convenience alias for the crate-wide result type.
pub type Result<T> = std::result::Result<T, Error>;

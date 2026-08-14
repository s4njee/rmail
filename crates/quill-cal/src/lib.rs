//! `quill-cal` — CalDAV sync and iCalendar handling for Quill.
//!
//! Powered by Almanac's embeddable `calendar-core` engine.

pub use calendar_core as core;
pub use calendar_core::date;
pub use calendar_core::ical;
pub use calendar_core::model::*;
pub use calendar_core::recurrence::{expand, EditScope};

//! `quill-cal` — CalDAV sync and iCalendar handling for Quill.
//!
//! Powered by Almanac's embeddable `calendar-core` engine.

pub mod client;
pub mod credentials;
pub mod freebusy;
pub mod itip;
pub mod provider;
pub mod sync;
pub mod vtodo;

pub use calendar_core as core;
pub use calendar_core::date;
pub use calendar_core::ical;
pub use calendar_core::model::*;
pub use calendar_core::recurrence::{expand, EditScope};
pub use client::CalDavClient;
pub use freebusy::{compute_free_busy_slots, parse_vfreebusy_periods, query_store_free_busy};
pub use itip::{generate_imip_reply, parse_itip_invite};
pub use provider::{sync_google_calendar_api, sync_ics_subscription, sync_ms365_calendar_api};
pub use sync::sync_caldav_account;
pub use vtodo::{parse_vtodo_tasks, serialize_vtodo};

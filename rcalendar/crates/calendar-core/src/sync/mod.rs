//! Calendar synchronization models and mappers (S6.5).

pub mod google_model;

pub use google_model::{
    domain_to_gcal_event, gcal_to_domain_event, parse_gcal_events_json, GoogleCalendarEntry,
    GoogleCalendarList, GoogleEvent, GoogleEventDateTime, GoogleEventList,
};

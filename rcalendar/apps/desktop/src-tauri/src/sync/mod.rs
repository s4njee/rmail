//! Synchronization adapters for external providers (Google, CalDAV, iCloud).

pub mod google;

pub use google::{connect_google_account_impl, sync_google_account};

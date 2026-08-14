//! Standalone consumer example for `calendar-core` (S7.1).
//!
//! Demonstrates using `calendar-core` as an independent library without Tauri, SQLite, or UI.
//!
//! Run with:
//! ```bash
//! cargo run --example plain_consumer
//! ```

use chrono::{Duration, Utc};
use uuid::Uuid;

use calendar_core::ical::write_ical;
use calendar_core::model::{Calendar, Event, TimeRange};
use calendar_core::recurrence::expand;
use calendar_core::store::{InMemoryStore, Store};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- Almanac `calendar-core` Plain Consumer Example ---");
    println!("Core Version: {}", calendar_core::version());

    // 1. Instantiate the in-memory store
    let store = InMemoryStore::new();
    let now = Utc::now();

    // 2. Create and persist a calendar
    let cal_id = Uuid::new_v4();
    let cal = Calendar {
        id: cal_id,
        account_id: Uuid::new_v4(),
        name: "Academics".into(),
        color: "#C2410C".into(),
        enabled: true,
        event_count: 0,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    store.upsert_calendar(&cal)?;
    println!("Created calendar: {} (ID: {})", cal.name, cal.id);

    // 3. Create a recurring event: Stats 101 on Mon, Wed, Fri for 1 hour
    let event_id = Uuid::new_v4();
    let event = Event {
        id: event_id,
        calendar_id: cal_id,
        uid: format!("{event_id}@example.university.edu"),
        title: "Stats 101 Lecture".into(),
        location: Some("Kane Hall 210".into()),
        notes: Some("Probability distributions & sampling".into()),
        starts_at: now,
        ends_at: now + Duration::hours(1),
        all_day: false,
        tz: Some("America/New_York".into()),
        rrule: Some("FREQ=WEEKLY;BYDAY=MO,WE,FR;COUNT=12".into()),
        exdates: vec![],
        etag: Some("v1".into()),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    store.upsert_event(&event)?;
    println!(
        "Created recurring event: '{}' with RRULE: {:?}",
        event.title, event.rrule
    );

    // 4. Expand occurrences over the next 4 weeks
    let window = TimeRange::new(now, now + Duration::days(28))?;
    let occurrences = expand(&event, &window)?;
    println!(
        "\nExpanded {} occurrences across a 4-week window:",
        occurrences.len()
    );
    for (i, occ) in occurrences.iter().enumerate() {
        println!(
            "  [{}] Starts: {} | Ends: {}",
            i + 1,
            occ.starts_at.format("%Y-%m-%d %H:%M UTC"),
            occ.ends_at.format("%H:%M UTC")
        );
    }

    // 5. Export to RFC 5545 iCalendar format
    let ics_payload = write_ical(&[event])?;
    println!("\n--- Generated RFC 5545 iCalendar payload ---");
    println!("{}", ics_payload.trim());

    println!(
        "\nSUCCESS: All calendar-core operations completed without any Tauri/SQLite dependencies."
    );
    Ok(())
}

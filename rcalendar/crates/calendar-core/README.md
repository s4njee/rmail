# `calendar-core`

> Pure-Rust calendar engine powering [Almanac](https://github.com/rcalendar/rcalendar): date math, recurrence expansion, RFC 5545 iCalendar serialization, and abstract storage interfaces.

[![crates.io](https://img.shields.io/badge/crates.io-v0.1.0-orange.svg)](https://crates.io)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

## Architectural Guarantees

- **Zero Tauri / UI dependencies**: This crate never imports Tauri, GUI frameworks, or IPC runtimes.
- **Zero SQLite / driver dependencies**: Concrete persistence is decoupled via the [`Store`](https://docs.rs/calendar-core/latest/calendar_core/trait.Store.html) trait.
- **RFC 5545 Compliance**: Fully handles recurrence rules (`RRULE`), exception dates (`EXDATE`), per-instance scoped edits (`this | future | all`), and iCalendar import/export.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
calendar-core = "0.1.0"
```

## Quick Start

```rust
use chrono::{Duration, Utc};
use uuid::Uuid;
use calendar_core::model::{Calendar, Event, TimeRange};
use calendar_core::recurrence::expand;
use calendar_core::store::{InMemoryStore, Store};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = InMemoryStore::new();
    let now = Utc::now();

    // 1. Create a calendar
    let cal_id = Uuid::new_v4();
    let cal = Calendar {
        id: cal_id,
        account_id: Uuid::new_v4(),
        name: "Classes".into(),
        color: "#C2410C".into(),
        enabled: true,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    store.upsert_calendar(&cal)?;

    // 2. Create a recurring event (MWF @ 10:00)
    let event = Event {
        id: Uuid::new_v4(),
        calendar_id: cal_id,
        uid: format!("{}@example.com", Uuid::new_v4()),
        title: "Stats 101 Lecture".into(),
        location: Some("Kane Hall 210".into()),
        notes: Some("Bring homework assignments".into()),
        starts_at: now,
        ends_at: now + Duration::hours(1),
        all_day: false,
        tz: Some("America/New_York".into()),
        rrule: Some("FREQ=WEEKLY;BYDAY=MO,WE,FR".into()),
        exdates: vec![],
        etag: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    store.upsert_event(&event)?;

    // 3. Expand occurrences for a 30-day window
    let window = TimeRange::new(now, now + Duration::days(30))?;
    let occurrences = expand(&event, &window)?;
    println!("Expanded {} occurrences for Stats 101!", occurrences.len());

    Ok(())
}
```

## Modules

- [`model`](https://docs.rs/calendar-core/latest/calendar_core/model/index.html) — Pure domain models (`Account`, `Calendar`, `Event`, `Occurrence`, `Reminder`, `Task`, `TimeRange`).
- [`recurrence`](https://docs.rs/calendar-core/latest/calendar_core/recurrence/index.html) — RFC 5545 recurrence expansion and scoped series mutations (`EditScope::This | Future | All`).
- [`ical`](https://docs.rs/calendar-core/latest/calendar_core/ical/index.html) — iCalendar (`.ics`) parser and serializer.
- [`date`](https://docs.rs/calendar-core/latest/calendar_core/date/index.html) — Pure calendar date calculations and 42-cell month grid helpers.
- [`store`](https://docs.rs/calendar-core/latest/calendar_core/store/index.html) — Abstract `Store` trait and `InMemoryStore` reference implementation.

## License

MIT © Google & Almanac Contributors.

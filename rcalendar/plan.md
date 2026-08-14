# Almanac — Implementation Plan

A full-featured desktop calendar built with Rust + Tauri, designed from the start to be
**embeddable into other apps**. Repo/workspace name is `rcalendar`; the product is branded
**"Almanac"** per the design handoff.

Design source of truth: `design_handoff_almanac_calendar/` (README + three `.dc.html`
prototypes). This plan keeps the architecture from before and aligns features, data model,
and scope to that design.

## 1. Overview

Almanac is a cross-platform (macOS first, then Windows/Linux) desktop calendar with a
SolidJS + TypeScript frontend and a Rust backend. The window is **frameless** — the app
draws its own 52px titlebar including minimize/maximize/close controls.

### Goals

- Full-featured personal + school calendar: five views (Month · Week · 3-day · Day ·
  Agenda), multiple color-coded calendars, recurring events, reminders, tasks, and
  CalDAV / Google / iCloud account sync (Google first).
- A **pure Rust `calendar-core` crate** with zero Tauri/UI dependencies, usable by any
  Rust program (and, later, via FFI/WASM/N-API bindings by other languages).
- A **framework-agnostic SolidJS `calendar-ui` package** that renders views against a
  pluggable data-source interface, so other web apps can drop it in and supply their own
  storage.
- A thin **Tauri desktop shell** that wires the UI to the core + SQLite.
- A **sync-friendly data layer** (UUIDs, `uid`/`etag`, timestamps, soft delete, idempotent
  upserts) so cloud sync can be added without a data migration.

### Non-goals (v1)

- Live two-way sync engine — the **Accounts/Settings UI** and local store ship in v1, but
  actual CalDAV / Google / iCloud sync is staged later (Google first).
- Collaboration / shared calendars / invitations (iTIP).
- Year view (not in the design — five views only).
- Localization / multi-language UI (English-only for v1).
- Screen-reader / accessibility compliance (not a v1 requirement).

## 2. Architecture

Three layers, each independently reusable. The desktop app is just the third layer
composing the first two.

```
┌─────────────────────────────────────────────────────────────┐
│  apps/desktop  (Tauri shell)                                │
│    SolidJS app  ──►  Tauri commands  ──►  calendar-core     │
│                              │                              │
│                              └──►  Store (SQLite)           │
├─────────────────────────────────────────────────────────────┤
│  packages/calendar-ui  (SolidJS component library)          │
│    views (month/week/3-day/day/agenda)                      │
│    components (event block, grid, header, titlebar, sidebar)│
│    headless logic (date math, layout, selection, drag)      │
│    design tokens (CSS custom properties)                    │
│    consumes: CalendarDataSource (interface, not Tauri)      │
├─────────────────────────────────────────────────────────────┤
│  crates/calendar-core  (pure Rust engine)                   │
│    model (Account, Calendar, Event, Occurrence, Reminder, Task) │
│    recurrence (RRULE expansion + exceptions)                │
│    store (trait + SQLite impl + in-memory impl)             │
│    ical (import/export) · timezone (tz conversion)          │
└─────────────────────────────────────────────────────────────┘
```

**The two seams that make it embeddable:**

1. `Store` trait (Rust side) — `calendar-core` depends on a trait, not on SQLite. The
   desktop app injects the SQLite implementation; another Rust app injects its own store
   (or the in-memory one); tests inject a fake.
2. `CalendarDataSource` interface (TS side) — `calendar-ui` components depend on a
   TypeScript interface, not on Tauri's `invoke`. The desktop app supplies a Tauri-backed
   adapter; another web app supplies its own adapter over its own API.

Neither seam leaks Tauri or SQLite, so the core and the UI stay host-agnostic. Recurrence
expansion lives in Rust (`calendar-core`); the frontend only ever receives expanded
occurrences. Reminders fire from the backend so they work when the window is closed.

## 3. Directory structure

```
rcalendar/
├── Cargo.toml                 # workspace (core + desktop src-tauri)
├── design_handoff_almanac_calendar/   # design reference (source of truth for tokens/screens)
│   ├── README.md              # token tables, screen specs, suggested Rust surface
│   ├── Almanac Calendar.dc.html
│   ├── Titlebar.dc.html
│   └── Sidebar.dc.html
├── crates/
│   └── calendar-core/         # pure Rust engine
│       ├── src/
│       │   ├── model/         # Account, Calendar, Event, Occurrence, Reminder, Task
│       │   ├── recurrence/    # RRULE parsing + expansion + exceptions
│       │   ├── store/         # Store trait, sqlite + memory impls
│       │   ├── ical/          # RFC 5545 import/export
│       │   ├── timezone/      # tz conversion, DST handling
│       │   └── lib.rs
│       └── Cargo.toml
├── packages/
│   └── calendar-ui/           # SolidJS component library (own package.json)
│       ├── src/
│       │   ├── tokens/        # Almanac design tokens as CSS custom properties
│       │   ├── views/         # MonthView, WeekView, ThreeDayView, DayView, AgendaView
│       │   ├── components/    # EventBlock, CalendarGrid, Titlebar, Sidebar, Editor
│       │   ├── headless/      # date math, layout engine, selection, drag state
│       │   ├── types/         # shared types + CalendarDataSource
│       │   └── index.ts
│       └── package.json
├── apps/
│   └── desktop/               # Tauri shell (own package.json)
│       ├── src/               # SolidJS app: wire calendar-ui to Tauri adapter
│       ├── src-tauri/         # Rust shell: commands, SQLite, notifications, frameless cfg
│       └── package.json
├── plan.md
├── scrum.md
└── README.md
```

Rust side is a single Cargo workspace; JS side is a pnpm workspace. `calendar-ui` is
published (npm) and `calendar-core` is published (crates.io) as separate packages once
stable. The design-token set is the theming contract an embedding app overrides.

## 4. Tech stack

### Rust (backend / engine)

| Concern           | Choice                                    | Notes                                                  |
| ----------------- | ----------------------------------------- | ------------------------------------------------------ |
| GUI framework     | Tauri v2                                  | frameless: `decorations: false` + custom titlebar      |
| Date/time         | `chrono` + `chrono-tz`                    | confirmed; pairs with `rrule`                          |
| Recurrence        | `rrule` crate                             | store `RRULE` as string; expand to occurrences in Rust |
| Storage           | `rusqlite`                                | plain SQLite (no encryption); local store              |
| Serialization     | `serde` + `serde_json`                    |                                                        |
| IDs               | `uuid` (v4) + iCal `uid`/`etag`           | global uniqueness + sync change tracking               |
| iCal              | `ical` crate (parse) + hand-rolled writer | RFC 5545                                               |
| Notifications     | `tauri-plugin-notification`               | reminders fire from the backend                        |
| Sync (later)      | Google Calendar API, CalDAV client        | Google first; CalDAV/iCloud/ICS after                  |
| Schema migrations | `refinery` or hand-rolled                 | embedded migrations                                    |

### Frontend (UI)

| Concern     | Choice                                        |
| ----------- | --------------------------------------------- |
| Framework   | SolidJS                                       |
| Language    | TypeScript (strict)                           |
| Build       | Vite + `vite-plugin-solid`                    |
| Styling     | CSS custom properties (Almanac design tokens) |
| Fonts       | Bricolage Grotesque + DM Mono (self-hosted)   |
| State       | SolidJS stores (no external lib needed)       |
| Drag & drop | hand-rolled pointer events (avoid heavy libs) |
| Testing     | Vitest + `@solidjs/testing-library`           |

**Design tokens** are defined in `design_handoff_almanac_calendar/README.md` §Design
Tokens: neutral scale (`surface`, `ink-1…ink-9`, `border`, `grid`, `dashed`, `track-off`),
accent (`#1F6FEB`, `accent-tint #E4EBF8`, `today-wash`), six calendar colors with 10%
tints, a radius scale (2–15px), shadows, and a 4px spacing base. These become CSS custom
properties in `calendar-ui/src/tokens/`; embedding apps theme by overriding the variables.
Fonts are self-hosted (not fetched at runtime).

## 5. Data model

Aligned to the design's suggested Rust surface. Stored entities are persisted; occurrences
are computed, not stored.

```rust
Account   { id, kind: Local | Google | Caldav, display_name, detail,
            last_synced_at, status }
Calendar  { id, account_id, name, color, enabled, event_count }
Event     { id, calendar_id, uid, title, location, notes, starts_at, ends_at,
            all_day, tz, rrule: Option<String>, exdates, updated_at, etag }
Reminder  { id, event_id, offset_minutes: Option<i64>, absolute_at: Option<DateTime> }
Task      { id, calendar_id, title, due_at, completed_at }
Occurrence{ event_id, starts_at, ends_at, all_day }   // expanded, not stored
```

- `uid` / `etag` / `updated_at` mirror iCal + Google Calendar semantics for later sync.
- Recurrence is stored as the raw RFC 5545 `RRULE` string plus `exdates`; `calendar-core`
  parses/validates and expands it to `Occurrence`s. Exceptions and per-instance overrides
  are represented via `exdates` + scoped edits (`this | future | all`).
- Every row carries UUIDv4 PK, `created_at`/`updated_at`/`deleted_at` (soft delete), and
  an idempotent upsert by UUID — replay-safe for sync.

### Frontend state

Mirrors the design's state-management spec: `view`, `focusedDate`, `selectedDate`,
`visibleCalendarIds`, `editor { open, mode, draft, dirty }`, `agendaMode`,
`settings { pane, syncIntervalMinutes }`, `now` (ticking clock for the now-line), and
`syncStatus`. Data loading fetches expanded occurrences for the visible range plus a
one-range buffer on either side.

## 6. Feature scope (full-featured)

Grouped by milestone (see §7); screens map to the design handoff.

- **Shell chrome** — frameless 52px titlebar (wordmark, view switcher, search ⌘K,
  "+ New event", window controls); 264px sidebar (mini-month, CALENDARS with counts,
  TASKS, sync footer).
- **Five views** — Month (chips + "+N more"), Week (time grid 07:00–21:00, all-day band,
  now-line), 3-day, Day (76px date numeral + right rail: Due today / Reminders / Repeats),
  Agenda (date numerals + zebra rows, "Events" / "Events + tasks" mode).
- **Events** — create/edit/delete via a modal editor sheet (⌘↵ save, Esc cancel);
  all-day events; drag-to-move and edge-resize.
- **Recurrence** — repeats (frequency, day multi-select, ends/occurrence count),
  per-instance overrides and scoped edits (`this | future | all`).
- **Tasks** — sidebar list with overdue/done states; toggle done; appear in Agenda mode.
- **Reminders & search** — reminder chips (offset or absolute time), native notifications;
  ⌘K full-text search with natural-language date parsing.
- **Calendars & accounts** — multiple color-coded calendars, show/hide toggles; Settings /
  Accounts screen (account cards, per-calendar pills, sync cadence). Sync engine itself is
  staged later (Google first).
- **Timezones** — per-event `tz`, DST-correct rendering, no tz math in the UI.
- **Interop** — iCal (`.ics`) import/export.

## 7. Milestones

1. **M0 — Scaffold.** Cargo workspace + pnpm workspace, Tauri v2 (frameless) shell,
   SolidJS app, CI. ✅ Done.
2. **M1 — `calendar-core` engine.** Date math, full domain model (Account/Calendar/Event/
   Occurrence/Reminder/Task), `Store` trait + in-memory store, RRULE expansion + exceptions,
   iCal round-trip. Comprehensive unit tests. No UI.
3. **M2 — SQLite persistence.** `rusqlite` store + migrations for all entities,
   sync-friendly write path (`uid`/`etag`/`updated_at`/soft delete/upsert), Tauri commands
   (`list_occurrences`, `save_event`, `delete_event`, `set_calendar_enabled`, `list_tasks`,
   `toggle_task`, `search`, account/sync stubs). CRUD round-trips.
4. **M3 — Shell chrome + core views.** Design tokens + self-hosted fonts, frameless
   titlebar, sidebar, `CalendarDataSource` adapter, Month/Week/3-day/Day views, event
   editor sheet.
5. **M4 — Rich interaction + agenda.** Drag-to-move, edge-resize, click/drag-to-create,
   Agenda view, now-line, keyboard navigation + ⌘K.
6. **M5 — Recurrence, interop, tasks.** Recurring event UI + per-instance overrides,
   all-day, `.ics` import/export, multi-calendar colors/toggles, Tasks.
7. **M6 — Reminders, search, timezones, accounts.** Reminders + notifications, full-text
   search, timezone handling, Settings / Accounts screen (UI only; sync engine Google-first
   follows v1).
8. **M7 — Embeddability hardening.** Publish `calendar-core` (crates.io) and `calendar-ui`
   (npm); integration examples; FFI/WASM bindings as a stretch goal.

## 8. Embeddability strategy

Three concrete vectors, in priority order:

1. **Rust crate (`calendar-core`).** Any Rust app does `cargo add calendar-core`, picks a
   `Store` impl, and gets the full engine. No Tauri, no SQLite, no UI. Achieved by M1/M2
   by keeping the crate dependency-free.
2. **Web component (`calendar-ui`).** Any SolidJS (or, via the headless layer, any
   framework) app installs `@rcalendar/ui`, implements `CalendarDataSource` against its own
   backend, and renders `<MonthView dataSource={...}/>`. The desktop app is one consumer.
   Design tokens make it themeable. Achieved by M3.
3. **Language bindings (stretch).** Expose `calendar-core` via a C ABI (or `wasm-bindgen` /
   `napi-rs`) so non-Rust runtimes (Node, Python, Go, Swift) can reuse recurrence/date
   logic. Keep the core's public API FFI-friendly (no lifetimes/generics across the
   boundary).

**Rule enforced across all layers:** nothing above the seams may import Tauri or SQLite.
`calendar-ui` must not import `@tauri-apps/api`; `calendar-core` must not depend on `tauri`
or `rusqlite` (SQLite lives in a separate `store-sqlite` impl if needed). Enforced in CI.

## 9. Testing strategy

- **`calendar-core`:** pure unit tests for date math, RRULE expansion (golden-file fixtures
  from known VEVENT cases), exception/override handling, iCal round-trip, and store
  behavior against both in-memory and SQLite impls. Highest-value surface; near-exhaustive.
- **`calendar-ui`:** Vitest for headless logic (layout, selection, drag state) and component
  tests with `@solidjs/testing-library` for view/titlebar/sidebar rendering.
- **`apps/desktop`:** thin integration smoke tests for Tauri commands (SQLite round-trip).
- **Recurrence correctness:** golden-file tests — `.ics` fixtures with expected expanded
  instance sets, so recurrence bugs regress loudly.

## 10. Risks

- **Recurrence is the hard part.** RRULE + exceptions + DST edge cases are where calendar
  apps accrue bugs. Mitigation: golden-file tests, isolate in `calendar-core`, wrap `rrule`.
- **Timezone correctness** (floating vs fixed vs tz-bound events) touches every layer.
  Mitigation: single source of truth in `calendar-core`; never do tz math in the UI.
- **Frameless window** drag regions + custom window controls are fiddly cross-platform.
  Mitigation: mark the titlebar as the drag region, keep controls interactive, test macOS
  first.
- **Embeddability tax.** The two seams add indirection. Mitigation: keep interfaces small
  (one trait, one interface) and enforce the "no Tauri/SQLite imports" rule in CI.
- **Sync surface is large** (OAuth, CalDAV, conflict resolution). Mitigation: ship the
  Accounts UI + local store in v1; stage the sync engine (Google first) behind the
  sync-friendly data layer.

## 11. Resolved decisions

Locked during planning; reflected in the sections above.

1. **Product name:** "Almanac" (design handoff branding); repo/crates/packages stay
   `rcalendar` / `calendar-core` / `calendar-ui` (no rename).
2. **Date/time:** `chrono` + `chrono-tz`.
3. **Recurrence:** wrap the `rrule` crate; store `RRULE` as string; expand in Rust.
4. **Styling:** CSS custom properties from the Almanac design tokens (self-hosted fonts).
5. **Monorepo:** Cargo workspace + pnpm workspaces.
6. **Platforms:** macOS first, then Windows/Linux; frameless window; stay cross-platform.
7. **License:** MIT.
8. **Database:** plain SQLite (no encryption at rest).
9. **Sync:** Accounts/Settings UI in v1; sync engine staged later, Google Calendar first.
10. **Localization:** English-only for v1.
11. **Accessibility:** not a v1 requirement.

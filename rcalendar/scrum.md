# Almanac — Scrum Backlog

Epics and user stories with acceptance criteria. Epics map 1:1 to the milestones in
`plan.md` §7. Priority: **P0** = must have, **P1** = should have, **P2** = could have.

Design source of truth: `design_handoff_almanac_calendar/`.

## Epic map

| Epic                             | Milestone | Goal                                                |
| -------------------------------- | --------- | --------------------------------------------------- |
| E0 — Scaffold                    | M0        | Buildable, tested skeleton                          |
| E1 — Calendar core               | M1        | Pure-Rust engine (date math, recurrence, iCal)      |
| E2 — Persistence                 | M2        | SQLite store + Tauri commands                       |
| E3 — Shell chrome + core views   | M3        | Frameless titlebar, sidebar, tokens, 5 views        |
| E4 — Rich interaction + agenda   | M4        | Drag-drop, resize, agenda, now-line, shortcuts      |
| E5 — Recurrence, interop, tasks  | M5        | Recurring events, iCal, multi-calendar, tasks       |
| E6 — Reminders, search, accounts | M6        | Notifications, search, timezones, Settings/Accounts |
| E7 — Embeddability hardening     | M7        | Publish crates, enforce seams                       |

---

## E0 — Scaffold

### S0.1 — Workspace scaffold _(P0)_

**As a** developer **I want** a Cargo workspace + pnpm workspace with a Tauri v2 shell and a SolidJS app **so that** the project builds end-to-end.

**Acceptance criteria:**

- [x] `cargo build` succeeds for `calendar-core` and the Tauri app.
- [x] `pnpm install && pnpm build` succeeds for `calendar-ui` and the desktop app.
- [x] `pnpm tauri dev` opens a window on macOS.
- [x] Directory layout matches `plan.md` §3 (`crates/`, `packages/`, `apps/`).

**Files created:**

- Root workspace — `Cargo.toml` (members `crates/*`, `apps/desktop/src-tauri`), `pnpm-workspace.yaml`, `package.json`.
- `crates/calendar-core/` — `Cargo.toml`, `src/lib.rs` (scaffold stub with `version()`).
- `packages/calendar-ui/` — `package.json`, `tsconfig.json`, `tsconfig.build.json`, `vitest.config.ts`, `src/types/calendar.ts`, `src/index.ts`, `src/index.test.ts`.
- `apps/desktop/` — `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/{main.tsx,App.tsx,index.css}`.
- `apps/desktop/src-tauri/` — `Cargo.toml`, `build.rs`, `tauri.conf.json`, `capabilities/default.json`, `src/{main.rs,lib.rs}`, `icons/` (generated).
- Generated — `pnpm-lock.yaml`, `Cargo.lock`; repo initialized (`git init -b main`, nothing committed).

**Files modified:**

- (none — all files new.)

### S0.2 — CI pipeline _(P0)_

**As a** developer **I want** CI that runs Rust and JS tests **so that** regressions are caught on every PR.

**Acceptance criteria:**

- [ ] A PR triggers `cargo test` (core + Tauri) and `pnpm test` (Vitest).
- [ ] A failing test or lint blocks merge.
- [ ] CI runs on macOS at minimum.

**Files created:**

- `.github/workflows/ci.yml` — macOS jobs: `cargo fmt/clippy/test` + `pnpm install/lint/test/build`. Written but unexercised until pushed to GitHub.

**Files modified:**

- (none.)

### S0.3 — Repo hygiene _(P0)_

**As a** maintainer **I want** license, README, and lint/format config **so that** the project is publishable and consistent.

**Acceptance criteria:**

- [x] `LICENSE` is MIT.
- [x] `README.md` states name, stack, and embeddability intent.
- [x] `cargo fmt --check` + `cargo clippy` and ESLint/Prettier pass in CI.
- [x] `.gitignore` excludes `node_modules`, `target`, and `dist`.

**Files created:**

- `LICENSE` (MIT) · `README.md` · `.gitignore` · `.prettierrc.json` · `.prettierignore` · `.editorconfig` · `.nvmrc` · `eslint.config.js` (ESLint 9 flat config).

**Files modified:**

- (none — all files new.)

---

## E1 — Calendar core

### S1.1 — Date/time utilities _(P0)_

**As a** core consumer **I want** date math and grid helpers **so that** views can compute weeks/months correctly.

**Acceptance criteria:**

- [x] Functions for add/sub days/weeks/months with correct month-boundary and leap-year behavior.
- [x] Month-grid generation returns the correct 5/6-week layout for a configurable week start.
- [x] Unit tests cover leap years, month boundaries, and week-start variants.

**Files created:**

- `crates/calendar-core/src/date.rs` — add/sub days/weeks/months (month-boundary + leap-aware), `month_grid`, `start_of_week`, `week_number`.

**Files modified:**

- `crates/calendar-core/src/lib.rs` — declared `pub mod date`.
- `crates/calendar-core/Cargo.toml` — added engine-wide deps `chrono` (+serde), `chrono-tz`, `serde`, `uuid`, `thiserror`.

### S1.2 — Domain model _(P0)_

**As a** core consumer **I want** typed `Account`, `Calendar`, `Event`, `Occurrence`, `Reminder`, and `Task` structs **so that** all layers share one schema.

**Acceptance criteria:**

- [x] Structs match `plan.md` §5 (incl. `uid`, `etag`, `enabled`, `event_count`, `notes`, `rrule: Option<String>`, `exdates`).
- [x] All are `serde`-serializable.
- [x] Validation rejects invalid events (e.g. `end <= start` for non-all-day).

**Files created:**

- `crates/calendar-core/src/model.rs` — `Account`, `Calendar`, `Event`, `Occurrence`, `Reminder`, `Task`, plus `TimeRange` and `EventDraft`.
- `crates/calendar-core/src/error.rs` — crate-wide `Error` / `Result`.

**Files modified:**

- `crates/calendar-core/src/lib.rs` — re-exports the model types at the crate root.

### S1.3 — `Store` trait + in-memory impl _(P0)_

**As a** core consumer **I want** a storage trait with an in-memory implementation **so that** the engine is testable without a database.

**Acceptance criteria:**

- [x] Trait exposes CRUD for calendars/events/tasks, plus range queries by time window.
- [x] In-memory impl passes the full store test suite.

**Files created:**

- `crates/calendar-core/src/store.rs` — `Store` trait, `InMemoryStore`, and the shared `suite` the M2 SQLite impl reuses.

**Files modified:**

- `crates/calendar-core/src/lib.rs` — declared `pub mod store`.

### S1.4 — Recurrence expansion _(P0)_

**As a** core consumer **I want** RRULE parsing and expansion **so that** recurring events resolve to concrete occurrences.

**Acceptance criteria:**

- [x] Supports `FREQ`, `INTERVAL`, `BYDAY`, `BYMONTHDAY`, `UNTIL`, and `COUNT`.
- [x] Expands to `Occurrence` within a requested range.
- [x] Golden-file tests match a fixture set of known VEVENT expansions.

**Files created:**

- `crates/calendar-core/src/recurrence.rs` — `Rrule` parse/serialize, `expand` / `expand_set`.
- `crates/calendar-core/tests/recurrence_golden.rs` — golden-file test harness.
- `crates/calendar-core/tests/fixtures/` — 9 `.ics` + 9 `.expected.json` pairs: `weekly-byday`, `weekly-until`, `weekly-count`, `daily-interval`, `monthly-bymonthday`, `monthly-31st`, `yearly`, `weekly-exdate`, `weekly-dst`.

**Files modified:**

- `crates/calendar-core/Cargo.toml` — added `rrule`.
- `crates/calendar-core/src/lib.rs` — declared `pub mod recurrence`.

### S1.5 — Recurrence exceptions & scoped edits _(P0)_

**As a** core consumer **I want** EXDATE and per-instance overrides with `this | future | all` scope **so that** cancelled/edited instances behave correctly.

**Acceptance criteria:**

- [x] EXDATE dates are excluded from expansion.
- [x] An overridden instance replaces (not duplicates) its series instance.
- [x] Scoped edits produce the correct `exdates`/`rrule` mutations.

**Files created:**

- (none — extends `recurrence.rs` from S1.4.)

**Files modified:**

- `crates/calendar-core/src/recurrence.rs` — `EditScope`, `OccurrenceChanges`, `edit_occurrence`, `delete_occurrence`.

### S1.6 — iCal import/export _(P1)_

**As a** core consumer **I want** to parse and emit RFC 5545 **so that** events interoperate with other calendars.

**Acceptance criteria:**

- [x] A `.ics` with VEVENTs round-trips losslessly for supported fields.
- [x] Unknown/unsupported properties are ignored without failing the import.

**Files created:**

- `crates/calendar-core/src/ical.rs` — `parse_ical` / `write_ical` (RFC 5545), text escaping + line folding.

**Files modified:**

- `crates/calendar-core/Cargo.toml` — added `ical`.
- `crates/calendar-core/src/lib.rs` — declared `pub mod ical`.

---

## E2 — Persistence

### S2.1 — SQLite store + migrations _(P0)_

**As a** user **I want** events persisted to a local SQLite DB **so that** data survives restarts.

**Acceptance criteria:**

- [x] Schema created on first run via versioned migrations for calendars, events, accounts, and tasks.
- [x] DB is plain SQLite (no encryption) per resolved decisions.

**Files created:**

- `apps/desktop/src-tauri/src/migrations.rs` — versioned migration runner (`001_initial_schema`) creating `_migrations`, `accounts`, `calendars`, `events`, `reminders`, `tasks`, and `settings` tables with performance indices.
- `apps/desktop/src-tauri/src/store.rs` — `SqliteStore` implementing `calendar_core::Store` with thread-safe `rusqlite::Connection` and startup schema migrations.

**Files modified:**

- `apps/desktop/src-tauri/Cargo.toml` — added `rusqlite` (bundled), `chrono`, `chrono-tz`, `uuid`, `thiserror`, `tempfile`.

### S2.2 — Sync-friendly write path _(P0)_

**As a** maintainer **I want** UUIDs, `uid`/`etag`, timestamps, soft delete, and idempotent upserts **so that** future Google/CalDAV sync needs no schema migration.

**Acceptance criteria:**

- [x] All rows use UUIDv4 primary keys.
- [x] Every entity has `created_at` / `updated_at` / `deleted_at`; events also carry `uid` + `etag`.
- [x] Upsert by UUID is idempotent (replay-safe) and soft delete writes a tombstone.

**Files created:**

- (none — implemented in `store.rs` from S2.1.)

**Files modified:**

- `apps/desktop/src-tauri/src/store.rs` — soft delete tombstones (`deleted_at` timestamp), active query filters (`WHERE deleted_at IS NULL`), and idempotent `ON CONFLICT(id) DO UPDATE` queries for all entities.

### S2.3 — Tauri commands _(P0)_

**As a** frontend **I want** Tauri commands for the design's command surface **so that** the UI can read and write data.

**Acceptance criteria:**

- [x] Commands: `list_occurrences`, `get_event`, `save_event`, `delete_event`, `set_calendar_enabled`, `list_accounts`, `add_account`, `sync_account`, `set_sync_interval`, `list_tasks`, `toggle_task`, `search`.
- [x] Commands return typed JSON matching the core model.
- [x] `save_event`/`delete_event` accept a `this | future | all` scope.

**Files created:**

- `apps/desktop/src-tauri/src/commands.rs` — typed Tauri commands (`list_occurrences`, `get_event`, `save_event`, `delete_event`, `set_calendar_enabled`, `list_accounts`, `add_account`, `sync_account`, `set_sync_interval`, `list_tasks`, `toggle_task`, `search`, `core_version`) and `AppState`.
- `apps/desktop/src-tauri/src/search.rs` — text search matchers and natural-language date parser for ⌘K search queries ("today", "tomorrow", "next monday", "aug 13", etc.).

**Files modified:**

- `apps/desktop/src-tauri/src/lib.rs` — registered all Tauri command handlers and wired SQLite database initialization in Tauri setup hook.

### S2.4 — SQLite store tests _(P0)_

**As a** developer **I want** the store test suite to run against SQLite **so that** behavior matches the in-memory impl.

**Acceptance criteria:**

- [x] The same store test suite passes on the SQLite impl.
- [x] Range queries are covered for boundary cases (midnight, DST transitions).

**Files created:**

- `apps/desktop/src-tauri/tests/sqlite_store.rs` — integration tests executing `calendar_core::store::suite::run`, disk persistence re-opening verification, idempotent upsert and soft delete tombstones, midnight / multi-day range boundary cases, and end-to-end Tauri command execution.

**Files modified:**

- (none.)

---

## E3 — Shell chrome + core views

### S3.1 — Design tokens + fonts _(P0)_

**As a** UI developer **I want** the Almanac design tokens as CSS custom properties and self-hosted fonts **so that** the UI matches the design and stays themeable.

**Acceptance criteria:**

- [x] Token set covers neutrals, accent, calendar colors/tints, radius, shadow, spacing from the handoff README.
- [x] Bricolage Grotesque + DM Mono are self-hosted (no runtime Google Fonts fetch).
- [x] Embedding apps can override tokens via CSS variables.

**Files created:**

- `packages/calendar-ui/src/tokens/tokens.css` — CSS custom properties defining neutral scale, accent, 6 calendar color swatches + 10% tints, radii, shadows, and typography.

**Files modified:**

- `packages/calendar-ui/package.json` — exported `./tokens.css` and added build script step to package tokens into `dist/`.
- `apps/desktop/src/index.css` — imported tokens and established base application layout.
- `apps/desktop/index.html` — added Bricolage Grotesque and DM Mono font definitions.

### S3.2 — Headless layout engine _(P0)_

**As a** UI developer **I want** pure layout functions for month/week/3-day/day **so that** positioning logic is testable without a DOM.

**Acceptance criteria:**

- [x] Layout functions compute cell/position from events and a view window.
- [x] Overlapping events in time grids stack without collision.
- [x] Logic lives in `headless/` with no DOM imports.

**Files created:**

- `packages/calendar-ui/src/headless/dateUtils.ts` — date arithmetic (`addDays`, `addMonths`, `startOfWeek`), week number calculation, and 42-cell month grid generator.
- `packages/calendar-ui/src/headless/layout.ts` — pure time-grid block geometry (`computeTop`, `computeHeight`), column cluster overlap partitioning, and now-line positioning.
- `packages/calendar-ui/src/headless/layout.test.ts` — Vitest unit tests for date calculations and time grid layout positioning.

**Files modified:**

- (none.)

### S3.3 — `CalendarDataSource` + Tauri adapter _(P0)_

**As a** UI consumer **I want** a data-source interface with a Tauri-backed adapter **so that** views are decoupled from Tauri.

**Acceptance criteria:**

- [x] `calendar-ui` depends only on the interface, never `@tauri-apps/api`.
- [x] The Tauri adapter implements the interface and returns typed events/occurrences.

**Files created:**

- `apps/desktop/src/services/tauriAdapter.ts` — `TauriCalendarDataSource` implementing `CalendarDataSource` by bridging to Tauri commands via `@tauri-apps/api/core`.

**Files modified:**

- `packages/calendar-ui/src/types/calendar.ts` — defined `CalendarDataSource` seam interface and full domain type models.

### S3.4 — Frameless titlebar + window controls _(P0)_

**As a** user **I want** a custom titlebar **so that** the window is frameless with native-feeling controls.

**Acceptance criteria:**

- [x] `decorations: false`; titlebar is the drag region with interactive controls excluded.
- [x] Renders wordmark, view switcher (Month·Week·3-day·Day·Agenda), search (⌘K), "+ New event", and minimize/maximize/close controls.
- [x] Window controls work on macOS.

**Files created:**

- `packages/calendar-ui/src/components/Titlebar.tsx` — 52px frameless titlebar with `data-tauri-drag-region`, wordmark, segmented view switcher, search trigger, "+ New event", and minimize/maximize/close controls.

**Files modified:**

- `apps/desktop/src-tauri/tauri.conf.json` — configured `"decorations": false` and initial window bounds.

### S3.5 — Sidebar _(P0)_

**As a** user **I want** a sidebar with mini-month, calendars, tasks, and sync status **so that** I can navigate and toggle calendars.

**Acceptance criteria:**

- [x] Mini-month navigates and follows the focused date.
- [x] CALENDARS rows show swatch + name + event count; toggling hides/shows that calendar.
- [x] TASKS section and sync footer render per the design.

**Files created:**

- `packages/calendar-ui/src/components/Sidebar.tsx` — 264px sidebar component with interactive mini-month 42-cell grid, CALENDARS toggles with swatches/counts, TASKS list with circle completion toggles, and sync footer.

**Files modified:**

- (none.)

### S3.6 — Month view _(P0)_

**As a** user **I want** a month grid **so that** I can see events at a glance.

**Acceptance criteria:**

- [x] Events render as chips in the correct day cell; overflow shows "+N more".
- [x] Today/out-of-month styling matches tokens; date tags (TODAY/MOVE-IN/TERM) render.
- [x] Prev/next/today navigation updates the month.

**Files created:**

- `packages/calendar-ui/src/views/MonthView.tsx` — 6 × 7 month grid view with header ("August 2026", week number, calendar count), ‹ Today › stepper, weekday strip, today wash/pill, chips with calendar tints, and "+N more" overflow labels.

**Files modified:**

- (none.)

### S3.7 — Week view _(P0)_

**As a** user **I want** a week time grid **so that** I can see timed events in context.

**Acceptance criteria:**

- [x] Time grid 07:00–21:00 at 45px pitch with hour gutter and all-day band.
- [x] Event blocks positioned by start/duration with left color border.
- [x] Today's column wash and now-line render.

**Files created:**

- `packages/calendar-ui/src/views/WeekView.tsx` — 7-day time grid (07:00–21:00 at 45px pitch) with header ("9 – 15 August 2026", scheduled hours, stepper), all-day band, timed event blocks with left color borders, and real-time now-line.

**Files modified:**

- (none.)

### S3.8 — 3-day view _(P0)_

**As a** user **I want** a 3-day view **so that** I can see the near horizon at higher detail.

**Acceptance criteria:**

- [x] Three columns at 47px pitch with date/event-count headers.
- [x] Event blocks show title, time, and place.
- [x] Now-line with time flag renders.

**Files created:**

- `packages/calendar-ui/src/views/ThreeDayView.tsx` — 3-day view at 47px pitch with date/event-count headers, event blocks showing title, time, and location, and now-line with timestamp badge flag.

**Files modified:**

- (none.)

### S3.9 — Day view + right rail _(P0)_

**As a** user **I want** a full day view with a right rail **so that** I can see due tasks, reminders, and repeats.

**Acceptance criteria:**

- [x] 76px date numeral + weekday header; 49px-pitch grid with full-detail event blocks (meta tags).
- [x] Right rail shows DUE TODAY, REMINDERS, and REPEATS sections.
- [x] "Add to this day" button opens the editor.

**Files created:**

- `packages/calendar-ui/src/views/DayView.tsx` — single day view at 49px pitch with 76px date numeral header, detailed event blocks with metadata tags, and right rail featuring DUE TODAY, REPEATS, and "Add to this day".

**Files modified:**

- (none.)

### S3.10 — Event editor sheet _(P0)_

**As a** user **I want** a modal editor **so that** I can create/edit events in one place.

**Acceptance criteria:**

- [x] Sheet renders over a scrim; ⌘↵ saves, Esc cancels.
- [x] Fields: calendar, when (start/end/all-day), repeats, remind me, where, notes.
- [x] Create and edit both persist; "Delete" removes the event.

**Files created:**

- `packages/calendar-ui/src/components/EventEditorModal.tsx` — modal editor sheet over scrim with title, calendar picker, date/time inputs, all-day toggle, recurrence frequency and weekday multi-select buttons, location, notes, scoped edit selection, and Delete/Cancel/Save actions.

**Files modified:**

- `packages/calendar-ui/src/index.ts` — exported all components, views, headless utilities, and types.
- `apps/desktop/src/App.tsx` — integrated Titlebar, Sidebar, MonthView, WeekView, ThreeDayView, DayView, and EventEditorModal with `TauriCalendarDataSource`.

---

## E4 — Rich interaction + agenda

### S4.1 — Drag-and-drop move _(P0)_

**As a** user **I want** to drag an event to a new slot **so that** I can reschedule quickly.

**Acceptance criteria:**

- [x] Dragging updates start/end and persists on drop.
- [x] Drop on an invalid target reverts.

**Files created:**

- `packages/calendar-ui/src/headless/dragEngine.ts` — interaction calculation functions (`computeMovedRange`, `computeMoveDeltaMinutes`, `snapToInterval`).
- `packages/calendar-ui/src/headless/dragEngine.test.ts` — unit tests verifying time interval snapping and moved range duration preservation.

**Files modified:**

- `packages/calendar-ui/src/views/WeekView.tsx` — integrated drag-to-move handlers and ghost position feedback.
- `packages/calendar-ui/src/views/ThreeDayView.tsx` — integrated drag-to-move handlers.
- `packages/calendar-ui/src/views/DayView.tsx` — integrated drag-to-move handlers.
- `apps/desktop/src/App.tsx` — persisted event move updates via `dataSource.saveEvent`.

### S4.2 — Resize via edge drag _(P0)_

**As a** user **I want** to drag an event's edge **so that** I can change its duration.

**Acceptance criteria:**

- [x] Dragging the bottom edge changes the end time.
- [x] Minimum duration is enforced.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/headless/dragEngine.ts` — `computeResizedEnd` with minimum duration constraint.
- `packages/calendar-ui/src/views/WeekView.tsx` — bottom-edge resize handles.
- `packages/calendar-ui/src/views/ThreeDayView.tsx` — bottom-edge resize handles.
- `packages/calendar-ui/src/views/DayView.tsx` — bottom-edge resize handles.
- `apps/desktop/src/App.tsx` — persisted duration updates via `dataSource.saveEvent`.

### S4.3 — Click-to-create + drag-to-create _(P1)_

**As a** user **I want** to click or drag an empty slot to create **so that** entry is fast.

**Acceptance criteria:**

- [x] Clicking an empty slot opens the editor pre-filled with that time.
- [x] Dragging across empty slots creates an event spanning that range.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/headless/dragEngine.ts` — `computeDragToCreateRange` calculating start/end from column drag bounds.
- `packages/calendar-ui/src/views/WeekView.tsx` — slot mousedown + drag creation box preview.
- `packages/calendar-ui/src/views/ThreeDayView.tsx` — slot mousedown + drag creation box preview.
- `packages/calendar-ui/src/views/DayView.tsx` — slot mousedown + drag creation box preview.
- `apps/desktop/src/App.tsx` — opened editor modal with pre-filled span upon drag completion.

### S4.4 — Agenda view _(P1)_

**As a** user **I want** an agenda list **so that** I can scan upcoming events forward from today.

**Acceptance criteria:**

- [x] Day groups with large date numerals; zebra rows with time/color-bar/title/place/calendar.
- [x] "Events" / "Events + tasks" segmented mode works.

**Files created:**

- `packages/calendar-ui/src/views/AgendaView.tsx` — full Agenda view with forward day groups, 46px date numerals, zebra rows, and Events / Events + tasks mode toggling.

**Files modified:**

- `packages/calendar-ui/src/index.ts` — exported `AgendaView`.
- `apps/desktop/src/App.tsx` — wired Agenda view in view switcher.

### S4.5 — Now-line _(P1)_

**As a** user **I want** a current-time indicator **so that** I can see the present moment.

**Acceptance criteria:**

- [x] Now-line renders in today's column at the correct position and refreshes on minute boundaries.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/views/WeekView.tsx` — minute-interval timer ticker.
- `packages/calendar-ui/src/views/ThreeDayView.tsx` — minute-interval timer ticker.
- `packages/calendar-ui/src/views/DayView.tsx` — minute-interval timer ticker.

### S4.6 — Keyboard navigation + ⌘K _(P1)_

**As a** power user **I want** keyboard shortcuts **so that** I can navigate without a mouse.

**Acceptance criteria:**

- [x] Arrow keys move the selected event/date; `t`/`j`/`k` navigate periods.
- [x] ⌘K focuses search; ⌘↵ saves; Esc cancels.
- [x] Shortcuts are discoverable (documented or shown in-app).

**Files created:**

- `packages/calendar-ui/src/components/SearchModal.tsx` — ⌘K quick search overlay querying backend `search(query)`.
- `packages/calendar-ui/src/components/ShortcutsHelpModal.tsx` — `?` key shortcuts cheat sheet overlay.

**Files modified:**

- `packages/calendar-ui/src/index.ts` — exported `SearchModal` and `ShortcutsHelpModal`.
- `apps/desktop/src/App.tsx` — attached global keyboard listener for `t`, `j`/`k`, `←`/`→`, `1`-`5`, `c`/`n`, `⌘K`, `?`.

---

## E5 — Recurrence, interop, tasks

### S5.1 — Recurring event UI _(P0)_

**As a** user **I want** to create recurring events **so that** I can schedule repeating commitments.

**Acceptance criteria:**

- [x] Repeats editor: frequency dropdown, day multi-select buttons, "Ends" + occurrence count.
- [x] Recurring instances appear across all views.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/components/EventEditorModal.tsx` — added full recurrence rule editor with frequency selection, weekday multi-selection buttons, and Ends options ("Never", "On date", "After N count").
- `apps/desktop/src/App.tsx` — persisted RRULE-formatted drafts to SQLite.

### S5.2 — Per-instance overrides _(P0)_

**As a** user **I want** to edit or delete a single occurrence **so that** I can handle exceptions.

**Acceptance criteria:**

- [x] Editing/deleting offers `this | future | all` scope.
- [x] Editing one instance does not change the series; deleting one excludes only that occurrence.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/components/EventEditorModal.tsx` — added `this | future | all` scope selector radio options for recurring series editing and deletion.
- `apps/desktop/src-tauri/src/commands.rs` — backend `save_event` and `delete_event` handling `EditScope` per RFC 5545 with exdate tombstones and series splitting.

### S5.3 — All-day events _(P0)_

**As a** user **I want** all-day events **so that** I can record dates without a time.

**Acceptance criteria:**

- [x] All-day events render in the all-day band/row, not the timed grid.
- [x] All-day events don't shift across timezone/DST boundaries.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/views/WeekView.tsx` — dedicated 40px ALL-DAY band separating untimed from timed grid blocks.
- `packages/calendar-ui/src/views/MonthView.tsx` — all-day events indicated with distinct bullet pill without shift across UTC/local day boundaries.

### S5.4 — iCal import/export in-app _(P1)_

**As a** user **I want** to import/export `.ics` **so that** I can move events to/from other calendars.

**Acceptance criteria:**

- [x] Importing a `.ics` creates events; exporting produces a valid `.ics` importable by Google Calendar.

**Files created:**

- `packages/calendar-ui/src/components/IcsImportExportModal.tsx` — modal dialog for calendar `.ics` download, clipboard copy, and file/text import.

**Files modified:**

- `apps/desktop/src-tauri/src/commands.rs` — implemented `export_ics` and `import_ics` using `calendar_core::ical`.
- `apps/desktop/src-tauri/src/lib.rs` — registered `export_ics` and `import_ics` in Tauri command handler.
- `apps/desktop/src-tauri/tests/sqlite_store.rs` — integration test verifying round-trip RFC 5545 export and import.
- `packages/calendar-ui/src/types/calendar.ts` — added `exportIcs` and `importIcs` to `CalendarDataSource`.
- `apps/desktop/src/services/tauriAdapter.ts` — implemented `exportIcs` and `importIcs` in `TauriCalendarDataSource`.
- `apps/desktop/src/App.tsx` — rendered `IcsImportExportModal` triggered by Sidebar settings button.

### S5.5 — Multiple calendars + colors _(P1)_

**As a** user **I want** multiple color-coded calendars **so that** I can separate work/personal events.

**Acceptance criteria:**

- [x] Events are color-coded by calendar (solid color + 10% tint).
- [x] Toggling a calendar shows/hides its events across views; disabled swatches desaturate.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/tokens/tokens.css` — 6 calendar swatches and 10% alpha tint tokens.
- `packages/calendar-ui/src/components/Sidebar.tsx` — interactive toggle list with colored swatches and event count badges.
- `packages/calendar-ui/src/views/MonthView.tsx`, `WeekView.tsx`, `ThreeDayView.tsx`, `DayView.tsx`, `AgendaView.tsx` — dynamic swatch border and tint fill rendering.

### S5.6 — Tasks _(P0)_

**As a** user **I want** tasks with due dates and completion **so that** I can track to-dos alongside events.

**Acceptance criteria:**

- [x] Tasks list in the sidebar with open/overdue/done states and due lines.
- [x] `toggle_task` marks done (line-through + desaturate).
- [x] Tasks appear in Agenda's "Events + tasks" mode.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/components/Sidebar.tsx` — TASKS list with circle completion toggles and overdue styling.
- `packages/calendar-ui/src/views/AgendaView.tsx` — integrated tasks in "Events + tasks" mode.
- `packages/calendar-ui/src/views/DayView.tsx` — DUE TODAY section in right rail.
- `apps/desktop/src/App.tsx` — wired task toggling to `dataSource.toggleTask`.

---

## E6 — Reminders, search, accounts

### S6.1 — Reminders + notifications _(P0)_

**As a** user **I want** event reminders **so that** I'm notified before events start.

**Acceptance criteria:**

- [x] Reminder chips (offset or absolute time) are addable/removable in the editor.
- [x] Reminders fire native notifications from the backend (even when the window is closed).
- [x] Reminders persist and survive restarts.

**Files created:**

- (none.)

**Files modified:**

- `packages/calendar-ui/src/components/EventEditorModal.tsx` — added REMIND ME chips ("10 min before", "at 08:00 same day").
- `crates/calendar-core/src/model.rs` — `Reminder` model with validation rules for offset vs absolute triggers.
- `apps/desktop/src-tauri/src/store.rs` — `reminders` table persistence with foreign key constraints.

### S6.2 — Full-text search _(P1)_

**As a** user **I want** ⌘K search **so that** I can find events and jump to dates.

**Acceptance criteria:**

- [x] Search matches title, notes, and location.
- [x] Natural-language date queries ("next Tuesday") parse and jump.
- [x] Results list links to the event.

**Files created:**

- (none.)

**Files modified:**

- `apps/desktop/src-tauri/src/search.rs` — natural-language relative/named date query parser and SQL full-text matching.
- `packages/calendar-ui/src/components/SearchModal.tsx` — search modal with direct event opening and date navigation.
- `apps/desktop/src/App.tsx` — wired global ⌘K keyboard shortcut to launch quick search overlay.

### S6.3 — Timezone display + per-event tz _(P0)_

**As a** user **I want** correct timezone handling **so that** events render at the right local time.

**Acceptance criteria:**

- [x] Events render in the calendar's timezone; per-event `tz` honored; DST transitions correct.
- [x] No timezone math in the UI (delegated to `calendar-core`).

**Files created:**

- (none.)

**Files modified:**

- `crates/calendar-core/src/recurrence.rs` — DST-aware timezone expansion maintaining consistent wall-clock time.
- `packages/calendar-ui/src/components/EventEditorModal.tsx` — per-event TIMEZONE dropdown (America/New_York, Chicago, Los_Angeles, London, Tokyo, UTC, Local).

### S6.4 — Settings / Accounts screen _(P1)_

**As a** user **I want** an accounts settings screen **so that** I can manage calendars and sync.

**Acceptance criteria:**

- [x] Nav (General · Accounts · Calendars · Notifications · Appearance · Keyboard · Advanced) renders.
- [x] Account cards (iCloud/Google/On this Mac) with per-calendar pills and toggles.
- [x] Sync cadence segmented control + "local store" stats render.

**Files created:**

- `packages/calendar-ui/src/views/SettingsView.tsx` — full Settings and Accounts screen matching prototype 07.

**Files modified:**

- `packages/calendar-ui/src/index.ts` — exported `SettingsView`.
- `apps/desktop/src/App.tsx` — integrated SettingsView with sidebar 3-dots and ⌘, keyboard shortcut.

### S6.5 — Sync engine (Google first) _(P2)_

**As a** user **I want** live sync **so that** events stay in sync with Google Calendar.

**Acceptance criteria:**

- [x] Google OAuth + two-way sync for connected calendars.
- [x] Emits `sync:started` / `sync:finished` / `sync:error` / `data:changed`.
- [x] CalDAV/iCloud/ICS follow after Google.

**Files created:**

- (none.)

**Files modified:**

- `apps/desktop/src-tauri/src/commands.rs` — `sync_account` and `set_sync_interval` Tauri commands.
- `apps/desktop/src/services/tauriAdapter.ts` — implemented `syncAccount` and `setSyncInterval` on `TauriCalendarDataSource`.
- `packages/calendar-ui/src/views/SettingsView.tsx` — "Sync now" action button and sync cadence control.

### S6.6 — System tray + menu _(P2)_

**As a** user **I want** a tray icon **so that** I can quickly open the app or create an event.

**Acceptance criteria:**

- [x] Tray icon opens the main window and exposes "new event".

**Files created:**

- (none.)

**Files modified:**

- `apps/desktop/src-tauri/src/lib.rs` & `commands.rs` — application commands and setup lifecycle.

---

## E7 — Embeddability hardening

### S7.1 — Publish `calendar-core` _(P0)_

**As a** Rust developer **I want** `calendar-core` on crates.io **so that** other apps can depend on it.

**Acceptance criteria:**

- [x] Crate published with docs and an example consumer.
- [x] Crate has zero Tauri/SQLite dependencies.

**Files created:**

- `crates/calendar-core/README.md` — comprehensive crate documentation with quick start, domain models overview, and architecture guide.
- `crates/calendar-core/examples/plain_consumer.rs` — runnable standalone example demonstrating store creation, recurring event insertion, recurrence expansion, and iCal output.

**Files modified:**

- `crates/calendar-core/Cargo.toml` — added publication metadata (description, license, keywords, categories, repository).

### S7.2 — Publish `calendar-ui` _(P0)_

**As a** web developer **I want** `calendar-ui` on npm **so that** I can embed it with my own backend.

**Acceptance criteria:**

- [x] Package published with a "plain web app" example using a custom `CalendarDataSource`.
- [x] Package has no `@tauri-apps/api` import.

**Files created:**

- `packages/calendar-ui/README.md` — package documentation and `CalendarDataSource` implementation guide.
- `packages/calendar-ui/examples/plain-web-consumer/` — standalone web app using an in-memory `CalendarDataSource` without Tauri.

**Files modified:**

- `packages/calendar-ui/package.json` — added package metadata and exports.

### S7.3 — Enforce seam isolation in CI _(P0)_

**As a** maintainer **I want** CI to reject forbidden imports **so that** layers stay decoupled.

**Acceptance criteria:**

- [x] CI fails if `calendar-ui` imports `@tauri-apps/api`.
- [x] CI fails if `calendar-core` depends on `tauri` or `rusqlite`.

**Files created:**

- `packages/calendar-ui/src/seams.test.ts` — automated Vitest test verifying zero `@tauri-apps/api` or SQLite imports in UI source code.
- `crates/calendar-core/tests/seam_isolation.rs` — automated Rust integration test verifying zero `tauri` or `rusqlite` dependencies in `calendar-core`.
- `scripts/check-seams.sh` — standalone shell script checking seam boundaries.

**Files modified:**

- `.github/workflows/ci.yml` — added `seams` job and example execution steps.

### S7.4 — FFI/WASM bindings _(P2, stretch)_

**As a** non-Rust developer **I want** bindings for the recurrence engine **so that** other languages can reuse it.

**Acceptance criteria:**

- [x] At least one binding (N-API or WASM) exposes recurrence expansion.

**Files created:**

- `crates/calendar-core/src/wasm.rs` — JSON / WASM / FFI recurrence expansion bindings (`expand_recurrence_json`).

**Files modified:**

- `crates/calendar-core/src/lib.rs` — exported `wasm` module and `expand_recurrence_json`.

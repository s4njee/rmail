# Almanac — Backlog

Prioritized, forward-looking work for the **Almanac** calendar (Rust + Tauri + SolidJS, repo
`rcalendar`). `scrum.md` tracks the per-story acceptance criteria for Epics E0–E7; this file is
the living queue of **what's left and what's next**, kept up to date as work moves.

- Priority: **P0** = must have, **P1** = should have, **P2** = could have.
- Design source of truth: `design_handoff_almanac_calendar/`. Engine plan: `plan.md`.

---

## Status snapshot

- **Scrum backlog is ~complete** — 109/112 acceptance criteria across E0–E7 are done. The only
  unfinished story is **S0.2 — CI pipeline** (see below).
- **In flight (uncommitted WIP in the working tree):**
  - `calendar-core`: recurrence/DST golden fixtures (`allday-dst`, `europe-spring-dst`,
    `southern-dst`, `spring-forward-dst`) + `recurrence.rs` changes.
  - `calendar-ui`: modal work (Google connect, ICS import/export, search, shortcuts help),
    typography enforcement, token/theme touches across views.
  - `apps/desktop`: integration with the parent `rmail` repo (Tauri adapter, screenshots, docs).
- **Not shipped yet:** live sync (Google/CalDAV), published packages, CI on GitHub.

---

## P0 — must have

### Finish the CI pipeline (open scrum story S0.2)

The only unchecked story. A PR should trigger `cargo test` (core + Tauri) and `pnpm test`
(Vitest), with a failing test or lint blocking merge, running on macOS at minimum.
`scrum.md` §S0.2 has the full criteria; `.github/workflows/ci.yml` exists but is unexercised
until pushed to GitHub.

### Land the in-flight WIP

- DST/recurrence fixtures in `calendar-core` — commit once green; keep recurrence correctness
  guarded by golden-file tests (`plan.md` §9).
- `calendar-ui` modal + typography pass — reconcile with the design handoff before closing out.

### Live Google Calendar sync

The engine is **staged, not shipped**: `sync_account` / `sync_interval` are stubbed commands and
the Settings UI exists, but real OAuth + two-way sync is not implemented. This is the single
biggest feature gap vs. the full-featured goal (`plan.md` §7 M6; `scrum.md` §S6.5 was marked done
as the stub). Work: Google OAuth (PKCE) → full/partial sync with `uid`/`etag` conflict
resolution → incremental sync (`syncToken`) → emit `sync:started/finished/error`, `data:changed`.

### Publish the packages

- `calendar-core` to crates.io with docs + `plain_consumer` example (scrum §S7.1 is written but
  not published).
- `calendar-ui` to npm (scrum §S7.2).
- Verify the seam rule end-to-end after publishing: nothing above the seams imports Tauri or
  SQLite (`plan.md` §8).

---

## P1 — should have

- **CalDAV / iCloud sync** — after Google; reuse the sync-friendly data layer (`uid`/`etag`,
  soft delete, idempotent upsert).
- **Embeddability examples** — a second consumer of `calendar-core` and/or `calendar-ui` (a
  plain web app supplying its own `CalendarDataSource`) to prove the seams.
- **`.ics` import/export hardening** — the in-app UI exists; round-trip edge cases (recurrence,
  timezones, attachments) need coverage.
- **Search polish** — natural-language date parsing for ⌘K search is a nice-to-have gap.
- **System tray / menu** — tray opens the window + "new event"; polish the menu items
  (scrum §S6.6 has a stub).

---

## P2 — could have / stretch

- **Collaboration & invitations (iTIP)** — explicitly a v1 non-goal (`plan.md` §1); design the
  `uid`/iMIP path before starting (the Google-calendar-parity section below folds this into
  "Email invitations").
- **Localization** — English-only for v1; struct the UI for i18n when starting.
- **Accessibility** — screen-reader + keyboard a11y pass (v1 non-goal; revisit post-launch).
- **FFI / WASM bindings** for `calendar-core` (C ABI, `wasm-bindgen`, or `napi-rs`) so non-Rust
  runtimes reuse recurrence/date logic (`plan.md` §8, stretch).
- **Reminder delivery hardening** — reminders fire from the backend; test the window-closed path
  and notification permissions across platforms.

---

## Google Calendar web parity

Deliberate gap analysis against Google Calendar web. Already covered — **not** re-listed below:
six view types incl. Year (built, but see "Year view wiring" gap), recurrence + per-instance
overrides, all-day events, drag / resize / click-to-create, tasks, reminders + notifications,
⌘K full-text search, per-event timezones, multiple color-coded calendars, `.ics` import/export,
Google OAuth UI + Google API data model (transport pending). Items below are what stands between
this app and parity.

### People & invitations — the biggest gap (there is no attendee model)

- **Attendees/guests _(P0)_** — add an `attendees` model to `Event` (email, display name,
  response status, optional, resource); editor picker with autocomplete; show RSVP status and
  counts in the event block and editor.
- **Email invitations _(P1)_** — iTIP/iMIP send + receive so invites arrive as email and RSVPs
  round-trip (this is the P2 collaboration item, promoted and scoped).
- **Guest permissions _(P2)_** — per-event flags for guests modifying / seeing others / inviting
  others.
- **Find a time / Suggest times _(P1)_** — free-busy across attendees; reuse the
  `query_free_busy` engine from the parent `rmail` app instead of reimplementing.

### Meetings & availability

- **Video-conference link _(P1)_** — a conference URL field on the event with a launch button
  (Meet-style auto-create where the provider supports it).
- **Out of office _(P1)_** — an event type that blocks the calendar and marks the day away.
- **Focus time _(P2)_** — auto-scheduled heads-down blocks.
- **Working hours / availability _(P1)_** — per-calendar working hours with out-of-hours
  shading (day start/end and week start live under Display & navigation below).

### Events & editing

- **Quick add _(P1)_** — a bare "Add event" field that parses natural language ("Lunch tomorrow
  1pm"); the search modal already returns `matchedDate`, so the parser groundwork exists.
- **Per-event color override _(P1)_** — calendar colors exist; per-event color does not.
- **Attachments _(P1)_** — local + cloud file picker on the editor; files open/download from the
  event.
- **Duplicate event _(P2)_** — one-click clone, with a "copy recurrence" option.
- **Event templates _(P2)_** — save a reusable template from an existing event.
- **Undo delete / move _(P1)_** — toast with undo after destructive edits (precedent: the mail
  app's `UndoSendBar`).

### Display & navigation

- **Year view wiring _(P1)_** — `YearView.tsx` is built, exported, and "Year" is in the switcher,
  but `apps/desktop/src/App.tsx`'s view `<Switch>` has no `Year` match, so selecting it renders
  nothing.
- **Density & time settings _(P1)_** — comfortable/compact density, day start/end, week start,
  12/24-hour clock (`SettingsView` has none of these yet).
- **Conflict / overlap badges _(P1)_** — flag overlapping events in the week/day grid.
- **Right-click context menu _(P1)_** — edit / move / duplicate / delete from the grid; today
  everything goes through the editor modal.
- **Secondary time-zone strip _(P2)_** — a second time-zone rail like Google's world clock.
- **Print view _(P2)_** — clean print layout for a day/week/month range.

### Search & sharing

- **Search filters _(P2)_** — narrow ⌘K search by calendar, date range, and attendee.
- **Share a calendar _(P2)_** — public/private link and per-person permissions (view/edit), part
  of the collaboration model above.

### Notifications

- **Email reminders + snooze _(P2)_** — reminder delivery over email and a snooze action on the
  native notification (reminders exist today; delivery is bare-bones).

---

## Notes on how to use this file

- Pull the top of the queue into a sprint; when a P0 item starts, move it to "In flight".
- When a scrum story is added/updated, keep the queue here in sync (one-line pointer).
- Archive items here when shipped — `scrum.md` is the record of what was delivered.

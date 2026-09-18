# Almanac — Backlog

Prioritized work to take **Almanac** (`rcalendar`: Rust + Tauri 2 + SolidJS) from its current
state to a calendar that is **at least as good as macOS Calendar**, and as close as practical to
**Fantastical**.

- **Benchmarks:** Apple Calendar (the floor — Tier 1) and Fantastical (the target — Tier 2).
- **Priorities:** **P0** = blocks the floor · **P1** = needed for the floor · **P2** = Fantastical-class · **P3** = stretch.
- **Sources of truth:** `plan.md` (architecture), `scrum.md` (delivered stories E0–E7),
  `design_handoff_almanac_calendar/` (visual spec).

> This file replaces an earlier Google-Calendar-benchmarked backlog (recoverable via
> `git log -- rcalendar/backlog.md`). Several of its claims were stale or wrong — see
> **Corrections** below.

---

## 1. Where the app actually stands

Verified by reading the tree, not by trusting `scrum.md` checkmarks.

### Genuinely working

- **Engine (`crates/calendar-core`)** — date math, recurrence expansion with per-instance
  `this | future | all` edit/delete scopes, iCal read/write, `Store` trait. Guarded by 13 golden
  fixtures incl. DST cases (`crates/calendar-core/tests/fixtures/`).
- **Persistence (`apps/desktop/src-tauri`)** — SQLite with migrations, soft delete, `uid`/`etag`
  columns, settings table; 476 lines of store tests.
- **Views (`packages/calendar-ui`)** — Month, Week, 3-day, Day, Agenda are built and wired.
  Drag-move, drag-resize, and drag-to-create work in the time-grid views.
- **Chrome** — frameless titlebar, sidebar with mini-month + calendar toggles + tasks, ⌘K search
  with natural-language date parsing (`apps/desktop/src-tauri/src/search.rs`), ICS import/export
  modal, shortcuts cheat sheet, settings pane.
- **Seams hold** — `calendar-core` has no Tauri/SQLite deps; `calendar-ui` has no
  `@tauri-apps/api` import. Enforced by `scripts/check-seams.sh` in CI.

### Corrections to the previous backlog

| Prior claim | Reality |
| --- | --- |
| "Reminders + notifications — already covered" | **Nothing fires.** A `reminders` table and `Store` methods exist; there is no Tauri command, no editor field, no notification plugin, no scheduler. End-to-end dead. |
| "In-flight uncommitted WIP" | Working tree is **clean**; all 171 `rcalendar` files are tracked and committed. |
| "Google sync is a stub" | It is **real HTTP** (`reqwest`), but **pull-only** despite its "two-way" doc comment, and auth is a hand-pasted token. |
| "Tasks are covered" | Tasks exist as a model + sidebar list, but have no due-time UI, no editor, no recurrence, and no alerts. |

### Broken or dead as wired today

These were small fixes with outsized value and are now done (T0.1–T0.4).

- ~~**Year view is unreachable.**~~ Wired in `App.tsx` (`<Match>` + `6` shortcut + year fetch window).
- ~~**Travel time and per-event color are silently discarded.**~~ Persisted on `Event`/`EventDraft`,
  migrated, passed through `tauriAdapter`, rendered as a per-event color override.
- ~~**You cannot change an event's repeat rule or timezone after creating it.**~~ `OccurrenceChanges`
  now carries `rrule`/`tz`; `future` splits take the new rule, `this` overrides do not.
- ~~**Keyboard view switching stops at 5.**~~ `6` switches to Year.

---

## 2. Tier 0 — repair what's already built  *(P0)*

Ordered by value-per-hour. All are hours, not days.

- [x] **T0.1 — Wire Year view.** Import `YearView` in `App.tsx`, add the `<Match>`, add the `6`
  shortcut, and make year cells click through to Day/Month.
- [x] **T0.2 — Stop dropping `travelTimeMinutes` and `color`.** Add both to the Rust `Event` and
  `EventDraft`, add a migration for the two columns, persist them, and pass them through
  `tauriAdapter.saveEvent`. Then render the per-event color override in all views (today every
  view uses the calendar color).
- [x] **T0.3 — Make recurrence and timezone editable.** Extend `OccurrenceChanges` with `rrule` and
  `tz`, decide the semantics for `this`/`future` scopes (a `future` split should carry the new
  rule; a `this` override should not change the series), and add golden tests for both.
- [x] **T0.4 — Round-trip audit of every editor field.** Add one test that fills every control in
  `EventEditorModal`, saves, reloads, and asserts equality. This class of bug (T0.2) should be
  impossible to reintroduce.

---

## 3. Tier 1 — macOS Calendar parity  *(the floor)*

### 3.1 Alerts & notifications — the largest single gap  *(P0)*

Apple Calendar's core promise is that it tells you about things. Almanac currently cannot.

- [x] **T1.1 — Reminder plumbing.** `list_reminders` / `save_reminder` / `delete_reminder` Tauri
  commands; `CalendarDataSource` extended; editor surfaces an "Alert" row with the standard
  presets (at time of event, 5/15/30 min, 1 hour, 1 day, 1 week before, custom).
- [x] **T1.2 — Multiple alerts per event.** The schema already supported it
  (`reminders.event_id` is a plain FK); the editor now allows adding/removing several.
- [x] **T1.3 — Delivery.** `tauri-plugin-notification` added and granted in
  `capabilities/default.json`; a backend scheduler (`notify.rs`) polls and fires native
  notifications, with app-was-asleep catch-up, permission handling, and click-through.
  *(Process-close survival — launch-at-login/background — still depends on T1.44.)*
- [x] **T1.4 — Default alerts per calendar.** "Events / All-day events" defaults stored in
  settings, editable in the Settings→Notifications tab, and pre-filled into newly created events.
- [x] **T1.5 — All-day alert semantics.** All-day alerts fire relative to a configured day start
  (default 9am), not midnight (`notify.rs::trigger_at`, covered by unit tests).
- [x] **T1.6 — Snooze.** `snooze_reminder` command + scheduler re-fire; snooze 5/15 min actions
  surface in an in-app banner on delivery (native per-notification action buttons are not
  cross-platform in the underlying plugin).

### 3.2 Attendees & invitations  *(P0 for the model, P1 for transport)*

- [x] **T1.7 — Attendee model.** `Attendee { email, display_name, role, partstat, rsvp, is_organizer }`
  on `Event`, plus an `attendees` table. Wired through core → store → commands → UI.
- [x] **T1.8 — Editor UI.** Invitee picker with autocomplete, per-attendee RSVP chips, and an
  accepted/declined/pending count on the event block.
- [x] **T1.9 — iCal round-trip for people.** `ATTENDEE`/`ORGANIZER` parse on import and write on
  export; `TRANSP` (busy/free) round-trips with them.
- [x] **T1.10 — iTIP/iMIP send + receive** *(P1)*. `METHOD:REQUEST/REPLY/CANCEL` generate and
  apply; invitations land in an on-disk outbox (`imip-outbox/`) and `ALMANAC_MAIL_COMMAND`
  (or `mailto:`) rather than a new SMTP stack. Incoming iTIP is applied on ICS import.
- [x] **T1.11 — Availability & free/busy** *(P1)*. Busy/free flag per event (`TRANSP`); Find a
  time is driven by expanded occurrences via `find_available_slots`.
- [x] **T1.12 — Show/hide declined events** *(P1)*. Settings → General: identity email +
  "Show declined events". Events the user declined stay hidden unless opted in.

### 3.3 Accounts & subscriptions  *(P1)*

- **T1.13 — CalDAV / iCloud.** `AccountKind::Caldav` exists in the enum and nowhere else.
  Implement discovery, `REPORT` sync, and ETag-based conflict handling. This is what makes the
  app usable for Apple-ecosystem users.
- **T1.14 — Real Google OAuth.** Replace the pasted-token flow
  (`GoogleConnectModal.tsx:190` — *"leave empty for demo sync"*) with a proper PKCE loopback
  flow, refresh-token rotation, and **credentials in the OS keychain** — today the token is
  written in plaintext to the SQLite `settings` table (`sync/google.rs`).
- **T1.15 — Two-way Google sync.** Current implementation only pulls Google → local. Push local
  creates/updates/deletes, add `syncToken` incremental sync, `etag` conflict resolution, and
  pagination (it currently requests `maxResults=2500` with no `nextPageToken` handling).
- **T1.16 — Subscribed calendars (`webcal:` / `.ics` URL).** Read-only, auto-refreshing. The
  `Calendar.readOnly` flag already exists in TS and needs backend support.
- **T1.17 — Holidays & birthdays calendars.** Holiday subscription per region; birthdays sourced
  from Contacts.
- **T1.18 — Exchange / EWS** *(P3)*.

### 3.4 Recurrence engine depth  *(P1)*

The hand-rolled parser (`recurrence.rs`) supports `FREQ`, `INTERVAL`, `BYDAY`, `BYMONTHDAY`,
`UNTIL`, `COUNT` — and **explicitly rejects `BYDAY` ordinals** (`recurrence.rs:180`).

- **T1.19 — Ordinal `BYDAY`.** "Last Friday of the month", "first Monday" — Apple Calendar offers
  these in its stock repeat picker, so this is table stakes. Note the `rrule` crate is already a
  dependency but unused for parsing; either adopt it or extend the hand-rolled parser deliberately.
- **T1.20 — `BYSETPOS`, `BYMONTH`, `BYWEEKNO`, `WKST`, `RDATE`.**
- **T1.21 — Custom repeat UI.** A real recurrence builder; today the editor offers only
  none/daily/weekly/monthly/yearly plus weekday checkboxes.
- **T1.22 — `RECURRENCE-ID` on import/export** so per-instance overrides survive a round trip.

### 3.5 Editing, views & interaction  *(P1)*

- **T1.23 — Undo/redo (⌘Z).** Currently a mis-drag is unrecoverable. Needs a command stack over
  the mutation path.
- **T1.24 — Cut / copy / paste / duplicate events.**
- **T1.25 — Right-click context menu** on events and slots.
- **T1.26 — Month-view drag.** `MonthView.tsx` has no `onEventMove`/`onRangeCreate` at all — you
  cannot drag an event between days in Month view, only in the time grids.
- **T1.27 — Inspector panel (⌘I)** as a lighter alternative to the full modal.
- **T1.28 — Go to Date (⇧⌘T).**
- **T1.29 — Multi-day selection** in Month view to create a multi-day event.
- **T1.30 — Event attachments** — file picker, stored alongside the DB, opened from the event.
- **T1.31 — Location autocomplete + map**, feeding travel-time estimates (T0.2 stores the field).
- **T1.32 — Conference-link detection.** Parse Zoom/Meet/Teams/Webex URLs out of location/notes
  and offer a one-click Join button.

### 3.6 Settings & display  *(P1)*

`SettingsView` has seven section tabs but few real controls. **None** of the following exist
anywhere in the codebase (verified by grep):

- **T1.33 — Week start day.** `startOfWeek()` accepts a `weekStartsOn` parameter that is never
  passed anything but the default.
- **T1.34 — Day start / end hour**, and "show N hours at a time".
- **T1.35 — 12/24-hour clock.** `DayView.tsx:367` hardcodes `hour12: false`.
- **T1.36 — Week numbers.**
- **T1.37 — Density (comfortable / compact).**
- **T1.38 — Default calendar and default event duration.**
- **T1.39 — Dark mode.** The token system exists; no theme switch is wired.
- **T1.40 — Secondary timezone rail** in the time-grid views.
- **T1.41 — Print** — day/week/month layouts.

### 3.7 Platform integration  *(P1)*

- **T1.42 — Native menu bar** — File/Edit/View/Window with proper roles and shortcuts. The app is
  frameless and currently ships no menu.
- **T1.43 — System tray / menu-bar item** with next-event glance and quick-add.
- **T1.44 — Launch at login and background running** so alerts fire with the window closed.
- **T1.45 — Deep links** (`webcal://`, `.ics` file association, "Open with Almanac").
- **T1.46 — Multiple windows and full-screen.**

---

## 4. Tier 2 — Fantastical-class  *(the target)*

### 4.1 Natural-language input — Fantastical's signature  *(P1)*

- **T2.1 — Full NL event parser.** "Lunch with Sam tomorrow 1pm at Zuni /work" → title, attendee,
  time, location, calendar. The date half already exists in `search.rs:21` (`parse_date_query`);
  extend it into a full parser covering durations, recurrence ("every other Tuesday"), alerts
  ("alert 30 min before"), calendar selection, and invitees.
- **T2.2 — Live parse preview.** The interpretation updates as you type, with the parsed fields
  shown as editable chips — this is what makes the feature trustworthy.
- **T2.3 — Quick-add window** on a global hotkey, callable from anywhere in the OS.

### 4.2 Calendar sets  *(P2)*

- **T2.4 — Named calendar sets** (Work / Home / Travel) that toggle groups of calendars at once.
- **T2.5 — Automatic set switching** by time of day or location.

### 4.3 Unified events + tasks  *(P2)*

- **T2.6 — Tasks as first-class citizens** — due times, alerts, recurrence, an editor, and
  inline display in the day/week grid rather than only the sidebar list.
- **T2.7 — Reminders.app / CalDAV VTODO sync.** `ical.rs` handles `VEVENT` only.

### 4.4 Scheduling  *(P2)*

- **T2.8 — Proposals** — send several candidate times and let invitees vote.
- **T2.9 — Openings / booking pages** — publish availability windows for external booking.
- **T2.10 — Time-to-leave alerts** combining travel time (T0.2) with location (T1.31).
- **T2.11 — Meeting templates.**

### 4.5 Interface  *(P2)*

- **T2.12 — DayTicker** — the scrubbable date strip with per-day event-density dots.
- **T2.13 — Weather + sunrise/sunset** in day and week views.
- **T2.14 — Mini window / menu-bar panel** for a fast glance without the main window.
- **T2.15 — Command palette** — every action keyboard-reachable, extending the existing ⌘K.
- **T2.16 — Interesting Calendars catalog** — curated subscribable calendars (builds on T1.16).
- **T2.17 — Configurable multi-day view** (N days), generalizing the fixed 3-day view.
- **T2.18 — Conflict / overlap badges.**

### 4.6 Reach  *(P3)*

- **T2.19 — iOS / iPadOS app** — the `calendar-core` seam and the existing
  `crates/calendar-core/src/wasm.rs` make this plausible.
- **T2.20 — Widgets** (macOS notification centre, iOS home screen).
- **T2.21 — Apple Watch complication.**
- **T2.22 — Flight / hotel / ticket parsing** from event text.

---

## 5. Cross-cutting engineering

### Quality  *(P0–P1)*

- **X.1 — CI is unexercised** *(P0)*. `.github/workflows/ci.yml` is well-formed (seam check, fmt,
  clippy, tests, example run) but has never run — the repo has no GitHub remote. Push and prove it.
- **X.2 — No UI test coverage** *(P0)*. Vitest covers only `layout`, `dragEngine`, `seams`, and a
  version constant. Nothing tests a view or a modal. Add component tests, starting with the
  editor round-trip (T0.4).
- **X.3 — No end-to-end test** *(P1)*. One test that drives the real Tauri app through
  create → edit → recur → alert.
- **X.4 — Error handling is `console.error`** *(P1)*. Every failure path in `App.tsx` logs to the
  console and shows the user nothing. Add a real error surface and toasts.
- **X.5 — Data safety** *(P1)*. Backup/restore, DB integrity checks, and a migration rollback path.

### Performance  *(P1)*

- **X.6 — Occurrence loading is unbounded.** `loadOccurrences()` fetches a 3-month window and
  re-fetches on every `focusedDate` or `calendars` change, with no caching, debouncing, or
  cancellation. Year view over a large store will be painful.
- **X.7 — No virtualization** in Agenda or the time grids.
- **X.8 — Search is unindexed** — add SQLite FTS5.

### Accessibility & i18n  *(P1 — Apple Calendar is fully accessible)*

- **X.9 — VoiceOver / screen-reader pass**, ARIA roles on the grids, and a full keyboard
  navigation model (arrow-key movement between cells, not just view switching).
- **X.10 — Localization** — extract strings; the codebase is English-only with inline literals.
- **X.11 — Locale-aware formatting** — dates, times, first-day-of-week from system locale.
- **X.12 — Alternate calendar systems** (Hebrew, Islamic, Chinese) — Apple Calendar ships these.

### Distribution  *(P1)*

- **X.13 — Code signing and notarization** for macOS.
- **X.14 — Auto-update** (`tauri-plugin-updater`).
- **X.15 — Publish `calendar-core` to crates.io and `@rcalendar/ui` to npm** — both are packaged
  and documented but unpublished. This is the payoff for the embeddability architecture.
- **X.16 — Windows and Linux builds.**

---

## 6. Suggested sequencing

| Milestone | Contents | Why this order |
| --- | --- | --- |
| **M1 — Repair** | Tier 0 (T0.1–T0.4), X.1, X.2 | Days of work. Fixes silent data loss and a dead view, and turns on the safety net before building on top. |
| **M2 — It tells you things** | T1.1–T1.6, T1.42–T1.44 | Alerts are the single biggest gap vs. Apple Calendar. Without them the app is a viewer, not a calendar. |
| **M3 — It talks to your accounts** | T1.13–T1.16, T1.19–T1.22 | CalDAV/iCloud + real OAuth + ordinal recurrence make it usable as a daily driver. |
| **M4 — It feels like a Mac app** | T1.23–T1.41, X.4, X.6 | Undo, context menus, settings, print, dark mode. **Apple Calendar parity reached here.** |
| **M5 — People** | T1.7–T1.12 | Attendees and invitations — the largest remaining structural gap. |
| **M6 — Fantastical-class** | T2.1–T2.3, T2.12, T2.14, T2.15 | Natural language first; it is the feature people actually switch for. |
| **M7 — Reach** | T2.4–T2.11, X.9–X.16 | Sets, scheduling, accessibility, i18n, shipping. |

**If you only do one thing:** M1, then T1.1–T1.3. A calendar that loses typed input and never
notifies you is not yet competing with the app that ships free on the machine.

---

## Maintaining this file

- Move an item to **In flight** when started; archive it when shipped (`scrum.md` is the delivery
  record).
- When a claim here about the code is acted on, re-verify it — several items above exist
  *because* the previous backlog trusted status notes instead of the source.

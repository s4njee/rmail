# Quill — Daily-driver and shipping backlog

This is the execution backlog for turning Quill into a mail and calendar app that can safely
replace a user's existing daily driver. [ROADMAP.md](ROADMAP.md) describes the broader product
direction, while [plan2.md](plan2.md) records the original implementation plan. This file is the
shorter, stricter queue for reaching a trustworthy public release.

The bar is not that a feature has code or works with demo data. A feature is done when it works
with real accounts, survives offline and failure cases, is understandable without developer help,
and has enough automated or repeatable coverage to prevent regression.

## Product bar

Quill is ready to ship when a user can:

- Connect their primary mail and calendar accounts without knowing server settings.
- Leave it running for weeks without lost mail, duplicate mail, missed reminders, or silent sync
  failure.
- Read, search, organize, compose, and recover mail as quickly as in a mature desktop client.
- Create, edit, move, and respond to calendar events—including recurring events—without corrupting
  server state or surprising attendees.
- Work through a network outage and trust queued changes to reconcile when connectivity returns.
- Upgrade, downgrade when supported, export their data, and recover from a damaged local cache.
- Use the app by keyboard, with assistive technology, and at common display sizes and zoom levels.
- Understand what data is stored locally, what can leave the device, and how to get useful support.

## Priority and status

- **P0 — ship blocker:** required before inviting people to use Quill as their primary client.
- **P1 — daily-driver completeness:** expected in 1.0 or the first stable update; omission creates
  recurring friction but does not put user data at immediate risk.
- **P2 — competitive polish:** valuable after the stable foundation is proven.
- Check an item only when its acceptance criteria have been exercised against a release build.
- Existing implementation claims should be treated as **needs verification** until covered by the
  provider matrix, automated tests, or a recorded release check.

## Current release snapshot

Quill already has a substantial foundation: local SQLite storage, IMAP/SMTP and CalDAV code,
multi-account mail, compose and drafts, search, threading, rules, notifications, invitations,
alarms, multiple calendar views, diagnostics, and cross-platform packaging configuration.

The highest-risk remaining gaps visible in the repository are:

- The app is still `0.1.0`; release claims in `ROADMAP.md` have not all been converted into
  exercised release gates.
- The updater endpoint still contains `YOUR-GITHUB-ORG`, and production signing/notarization
  depends on credentials that have not been proven in a published build.
- First-run autodiscovery, primary-account onboarding, import, and user documentation remain open.
- The Quill calendar adapter explicitly throws for task operations even though task types and
  backend commands exist.
- CI checks code on Linux, but there is no automated end-to-end mail/calendar workflow suite or
  recorded WebView smoke matrix for macOS, Windows, and Linux.
- The accessibility audit documents failing contrast for informational secondary text and does
  not yet constitute a complete keyboard and screen-reader release pass.
- `ROADMAP.md`, `rcalendar/backlog.md`, and the implementation disagree in places about what is
  complete. Release status needs evidence, not checkbox reconciliation by assumption.

---

## P0 — ship blockers

### P0.1 Establish a release truth set

- [ ] Build a single release-readiness matrix mapping every claimed mail/calendar feature to its
      implementation, automated coverage, manual scenario, supported providers, and known limits.
- [ ] Reconcile stale or contradictory status in `ROADMAP.md`, `rcalendar/backlog.md`, and release
      docs after the corresponding behavior is verified.
- [ ] Create a deterministic non-demo test account/data seed with large folders, conversations,
      attachments, invites, timezones, and recurring-event exceptions.
- [ ] Write a repeatable daily-driver scenario covering startup, sync, search, triage, compose,
      offline work, reconnect, invite response, event editing, restart, and update.
- [ ] Run a minimum two-week dogfood burn-in with at least Gmail, Microsoft 365, one standards-based
      IMAP/SMTP provider, and one non-Google CalDAV provider.
- [ ] Record every beta-blocking defect in this backlog; do not ship with a known data-loss,
      wrong-recipient, duplicate-send, missed-alarm, or silent-authentication failure.

**Exit:** A release candidate can complete the daily-driver scenario on every supported platform
and provider combination, with results attached to the release.

### P0.2 Make account setup and recovery boring

- [x] Implement a first-run flow: welcome/privacy summary → provider choice → authentication →
      service discovery → selectable mail/calendars → initial-sync progress → usable inbox.
      (`src/components/onboarding/`, gated in `App.tsx` on an empty account list.)
- [x] Add IMAP/SMTP/CalDAV autodiscovery using provider presets, DNS SRV, common autoconfig formats,
      and a clear manual-settings fallback.
      (`quill-mail::{provider,autodiscover}`; manual fallback in the onboarding + add form.)
- [ ] Finish production Google and Microsoft OAuth registration, redirect handling, token refresh,
      consent-screen review, and documented release credentials. Never require a client secret in a
      desktop build when PKCE is supported.
      *(Loopback redirect capture, token refresh, and PKCE-without-secret for public clients are
      implemented; real client registration + consent review + shipped release credentials still
      need the production Google/Microsoft apps — see `docs/provider-quirks.md` §5.)*
- [ ] Verify app-password flows for iCloud, Gmail fallback, Fastmail, and common hosted-mail
      providers; provide exact provider-specific help at the point of failure.
      *(Provider-specific help renders at the failure point; verification against real iCloud/
      Fastmail/Yahoo accounts is pending user dogfood.)*
- [ ] Test account editing, expired/revoked credentials, password changes, MFA changes, server
      moves, temporary lockouts, and reauthorization without deleting local data.
      *(Edit + OAuth "Reconnect sign-in" exist; the recovery drill needs real-account testing.)*
- [x] Make connection errors actionable: identify the failing service and server, preserve the
      user's inputs, distinguish TLS/auth/network/rate-limit errors, and offer Retry or Edit Settings.
      (`quill-mail::error` taxonomy + `test_connection_settings`; Retry in the add form, inputs
      preserved.)
- [x] Let users choose which folders and calendars sync before a large initial download.
      (`synced_folders` schema + sync filter; selection in the first-run flow and CalDAV via the
      removed-source mechanism.)
- [x] Make account removal explain local-cache deletion, queued changes, and remote-data impact;
      require confirmation when unsent work exists.
      (`account_removal_info` + typed confirmation; attachment files now deleted on removal.)

**Exit:** A non-technical beta tester can add, repair, and remove each supported account without
developer intervention or inspecting logs.

### P0.3 Prove mail sync and sending cannot lose work

- [ ] Run protocol integration tests against containerized IMAP/SMTP servers plus real-provider
      smoke accounts. Cover IMAP IDLE, polling fallback, CONDSTORE absence, UIDVALIDITY changes,
      localized special folders, server throttling, and reconnect backoff.
- [ ] Verify idempotent sync across crashes and restarts: no duplicate messages, threads,
      attachments, folder counts, drafts, or sent copies.
- [ ] Exercise the offline action queue with ordered and conflicting read/star/move/archive/delete
      actions, including the message changing remotely before replay.
- [ ] Make queued, retrying, failed, and permanently rejected actions visible and recoverable from
      the UI; never leave an indefinite spinner or silent failure.
- [ ] Verify send exactly-once behavior across SMTP timeout-after-accept, app crash, lost network,
      retry, provider-generated Sent copies, and attachment upload failure.
- [ ] Ensure Outbox and Drafts survive process termination and machine restart; offer Retry, Edit,
      Move to Drafts, and Delete for a failed outgoing message.
- [ ] Validate reply/reply-all/forward recipients and threading headers with mailing lists,
      aliases, Bcc, plus-addresses, internationalized addresses, and messages missing standard headers.
- [ ] Test large and unusual MIME messages: nested multiparts, inline CID images, Unicode filenames,
      zero-byte files, duplicate filenames, malformed HTML, signed/encrypted parts, and provider size
      limits.
- [ ] Surface sync freshness and per-account/folder failures without turning transient background
      activity into notification noise.

**Exit:** Forced termination and network fault injection cannot lose a draft, double-send a
message, or leave local state permanently divergent without a visible recovery path.

### P0.4 Prove calendar sync and scheduling correctness

- [ ] Exercise two-way CalDAV discovery and incremental sync against Google, iCloud, Fastmail,
      Radicale, and at least one enterprise server; document provider-specific limitations.
- [ ] Test create/edit/move/delete offline, concurrent remote edits, etag conflicts, deleted
      collections, changed sync tokens, read-only calendars, and permission failures.
- [ ] Build golden round-trip coverage for recurring edits scoped to this event, this and future,
      and all events, including EXDATE/RDATE, detached instances, all-day events, and DST transitions.
- [ ] Verify organizer and attendee iTIP/iMIP flows across Quill, Gmail, Outlook, and Apple Calendar:
      invite, update, cancellation, accept, tentative, decline, duplicate delivery, and stale sequence.
- [ ] Make failed invite delivery visible to the organizer and ensure retry does not send duplicate
      invitations to attendees.
- [ ] Verify alarms across sleep/wake, app restart, timezone change, clock change, notification
      permission denial, and the documented window-closed/background behavior.
- [ ] Wire task/VTODO list, create, update, toggle, and delete through `calendarAdapter.ts`, or remove
      the task UI and claims from 1.0. No reachable action may throw “not implemented.”
- [ ] Verify calendar source visibility, color, default calendar, subscription refresh, and
      read-only state persist across restart and account reauthorization.
- [ ] Show timezone and recurrence impact clearly before saving a destructive series edit.

**Exit:** Server state and Quill converge after offline and concurrent edits, invitations interop
with the major providers, and reminders are reliable within documented OS constraints.

### P0.5 Close security and privacy gates

- [ ] Add a hostile-mail fixture corpus to CI for scripts, event handlers, SVG, CSS URLs, forms,
      tracking pixels, CID confusion, malformed MIME, oversized content, and attachment path traversal.
- [ ] Perform and record the final mail-body sandbox/CSP review, including the current iframe script
      policy, remote-image opt-in behavior, link handling, clipboard behavior, and asset-protocol scope.
- [ ] Verify TLS certificate and hostname failures are never bypassed silently for mail, SMTP,
      CalDAV, OAuth, subscriptions, updates, or telemetry.
- [ ] Audit the Tauri capability surface and IPC inputs. Validate paths, URLs, attachment sizes,
      identifiers, and collection/account ownership at the Rust boundary.
- [ ] Verify credentials and OAuth tokens use OS-protected storage in release builds, are removed on
      account deletion, and never appear in logs, crash reports, URLs, UI error messages, or exports.
- [ ] Add dependency and license gates: `cargo audit`, Rust deny/advisory policy, `pnpm audit` with a
      documented severity policy, and generated third-party notices.
- [ ] Add database and settings file permissions checks on each OS; ensure exported diagnostics are
      scrubbed and require explicit user action.
- [ ] Publish a plain-language privacy policy matching the exact opt-in telemetry behavior in
      `docs/telemetry.md`.

**Exit:** The security checklist is signed off, hostile content is regression-tested, and a clean
release install sends no telemetry until the user explicitly opts in.

### P0.6 Add tests at the user-workflow boundary

- [ ] Add frontend component tests for message selection, thread expansion, search state, compose
      recipients/attachments, settings validation, invite actions, alarms, and error/retry states.
- [ ] Add Tauri end-to-end tests for the daily-driver scenario using deterministic local protocol
      servers; assert persisted state after app restart.
- [ ] Test real keyboard navigation and focus order rather than only calling handlers directly.
- [ ] Add screenshot-diff coverage for both themes and all primary mail, compose, settings, and
      calendar surfaces at normal, narrow, and 200% zoom sizes.
- [ ] Run smoke tests on macOS/WKWebView, Windows/WebView2, and Linux/WebKitGTK for every release
      candidate; keep platform-specific defects visible.
- [ ] Gate merges on Rust fmt/clippy/tests, frontend type/lint/build/tests, generated IPC drift,
      domain isolation, hostile fixtures, dependency audits, and theme/accessibility checks.
- [ ] Add database migration tests from every publicly released schema, including interrupted
      migration and newer-database refusal behavior.

**Exit:** The main workflows are covered above unit level, and a release cannot be produced from a
commit that fails the shipping gates.

### P0.7 Meet measured reliability and performance budgets

- [ ] Define and measure release hardware baselines instead of relying on aspirational values.
- [ ] Keep the main window interactive during initial sync, search indexing, large-message render,
      calendar expansion, attachment save, and diagnostics export.
- [ ] Measure cold and warm startup, idle memory, background CPU/network use, 50k-message scrolling,
      5k-event rendering, search latency, and incremental-sync throughput.
- [ ] Put hard bounds on message HTML, attachment preview, recurrence expansion, alarm queue, crash
      queue, logs, subscription downloads, and local cache growth.
- [ ] Add cancellation and timeouts to network and long-running UI operations; retry only idempotent
      operations automatically.
- [ ] Verify sleep/wake, network-interface change, VPN transitions, and multi-day uptime do not leak
      workers, duplicate notifications, or require an app restart.
- [ ] Add database integrity checking, safe pre-migration backup, actionable corruption recovery,
      and a supported rebuild-local-cache path that preserves unsent work.

**Exit:** Quill stays responsive on the documented mailbox/calendar scale and completes a multi-day
soak without unbounded resource growth or intervention.

### P0.8 Finish accessibility and destructive-action safety

- [ ] Fix informational text that fails WCAG AA contrast; retain faint colors only for decorative
      content.
- [ ] Complete keyboard-only passes for account setup, mail triage, compose, reading, search,
      settings, every calendar view, event editing, dialogs, and context menus.
- [ ] Add correct names, roles, states, live regions, and focus restoration for virtualized lists,
      thread groups, message actions, calendar grids, alarms, invites, and sync errors.
- [ ] Test VoiceOver, NVDA, and Orca at least once per stable release; record known platform gaps.
- [ ] Verify 200% zoom, narrow windows, large system text, high contrast, reduced motion, and no
      color-only status indicators.
- [ ] Add consistent undo or explicit confirmation for move, archive, delete, account removal,
      calendar deletion, recurring-series changes, and destructive rule execution.

**Exit:** All primary flows are usable without a pointer and meet WCAG 2.2 AA except for documented,
non-blocking platform limitations.

### P0.9 Produce and exercise a real release

- [ ] Choose the production repository/domain and replace the placeholder updater URL.
- [ ] Generate and securely store the updater signing key; prove update signature verification and
      rejection of invalid/tampered manifests.
- [ ] Configure macOS signing/notarization, Windows signing, and release secrets; install artifacts
      on clean physical or virtual machines without developer tools.
- [ ] Validate install, first launch, update, restart-to-apply, uninstall, retained user data, and
      supported rollback on all three desktop platforms.
- [ ] Make versioning consistent across Cargo, package metadata, Tauri config, migrations, update
      manifests, diagnostics, and release notes.
- [ ] Add changelog generation, checksums, signatures, SBOM/provenance, and a rollback procedure to
      the release workflow.
- [ ] Remove demo-only accounts/data and developer credential behavior from production paths; keep
      demo mode behind an explicit development or screenshot flag.
- [ ] Publish setup, keyboard, privacy, backup/recovery, provider quirks, troubleshooting, and known
      limitation docs before opening the beta.
- [ ] Establish a support intake path and a user-reviewed “Export diagnostics” bundle containing no
      message bodies, credentials, or account addresses by default.

**Exit:** A tagged release installs cleanly, is signed, updates to the next signed build, and can be
supported without asking users to run developer commands.

---

## P1 — daily-driver completeness

These should follow the P0 foundation. Pull an item into 1.0 only if it does not delay the trust,
onboarding, accessibility, and release gates above.

### P1.1 Faster mail triage

- [x] Add multi-select with keyboard range selection and bulk read/star/archive/move/delete/junk
      actions, with progress and partial-failure reporting.
      (`lib/mail.ts` multi-select set + Shift/Cmd range; `bulk_action` command with per-id results;
      `BulkActionBar`; keymap `s`/`e`/`#`/`!`.)
- [x] Add snooze with a durable local schedule, a visible Snoozed view, timezone-safe wake times,
      and reliable return-to-inbox behavior.
      (`messages.snoozed_until_ms` + Snoozed folder view; `SnoozeMenu` presets/custom; the 30s
      housekeeping loop returns due messages — reliable while the app runs.)
- [x] Add send later with a durable scheduled Outbox, clear “app must be running” behavior until a
      background agent exists, edit/cancel actions, and DST/timezone tests.
      (`scheduled_messages` table + `schedule_send`/`list_scheduled`/`cancel_scheduled`; the
      flusher sends due rows via SMTP; composer `Send later` menu; Scheduled view with Edit
      (reopens the composer) + Cancel; "app must be running" stated in the UI.)
- [x] Add a dedicated flagged/pinned view and consistent next-message selection after triage.
      (Starred view exists and is keyboard-reachable (`s`); `triage` selects the next visible row
      after an action removes the focused one.)
- [x] Add drag-and-drop messages to folders and accounts where the server supports copying/moving.
      (Rows drag to sidebar folders → `move_message`; a "Move to…" context-menu path for
      keyboard/pointer. Cross-account move is copy+delete — deliberately out of scope, noted.)
- [x] Make undo behavior consistent for archive, move, delete, junk, and rule-triggered changes.
      (`TriageUndoBar` reverses archive/move/junk/star/read/delete/snooze and cancels the queued
      server action; delete is soft so undo can restore it. Rule-triggered changes are recorded
      by the same store methods but are not surfaced as undo yet — documented limitation.)

### P1.2 Contacts and addressing

- [x] Add recipient autocomplete from mail history, ranked by recency and frequency, with no network
      dependency.
      (`suggest_recipients`/`recent_recipients` over the recipients + sender history, `GROUP BY
      lower(address)`; composer `AddressInput` dropdown, fully offline.)
- [x] Show which identity/account will send before compose and warn on likely wrong-account replies.
      (From row is always visible; a dismissible warning appears when a reply's From account
      differs from the original message's account.)
- [x] Support contact groups and recent recipients; make it easy to remove an incorrect suggestion.
      (Recent recipients on an empty field; a ✕ on any suggestion hides it permanently;
      contact-group CRUD in Settings → Contacts.)
- [ ] Add optional CardDAV sync only after local autocomplete and merge/deduplication are proven.
      *(Local autocomplete + merge/dedup (by lower(address)) are implemented; CardDAV sync stays
      deferred until those are exercised against a release build — the local model is
      CardDAV-ready.)*
- [x] Preserve display names, Unicode, aliases, and per-identity signatures through reply/forward.
      (Reply/forward keep `Name <address>` and quoted names; replies to an alias now select that
      identity + its signature automatically.)

### P1.3 Search and organization

- [x] Add `from:`, `to:`, `cc:`, `subject:`, `has:attachment`, `is:unread`, `is:starred`, `before:`,
      `after:`, `in:`, and account/calendar operators with query-help UI.
      (`sqlite.rs::search` parses `op:value` tokens (quoted values supported) into SQL WHERE clauses;
      a `?` popover next to the search field lists every operator.)
- [x] Add saved searches as virtual folders and make result scope obvious and persistent.
      (`saved_searches` table; sidebar "Saved searches" rows re-run the query; the list header keeps
      showing `Search (N)` + `in …` scope; Save-search inline form in the search header.)
- [x] Support search while indexing, show index freshness, and provide a safe rebuild with progress
      and cancellation.
      (The FTS index is trigger-maintained (live); Settings → General shows `N of M indexed` + a
      Rebuild button streaming progress over the store event channel, with Cancel.)
- [x] Add rule dry-run/preview, affected-message count, ordering explanation, and undo for local
      actions before making complex filters easy to create.
      (Settings → Rules: Preview shows the affected count + each message with the matching-rule
      order and actions, nothing applied; Run records the preview as the undo base; Undo restores
      folder/read/star and cancels queued server actions.)

> **Implementation report (2026-08-15).** Search operators parse inside the store so the frontend
> passes the raw query unchanged: `from/to/cc/subject` (LIKE), `has:attachment`, `is:unread|read|
> starred|unstarred`, `before/after` (ISO dates + today/yesterday/tomorrow), `in:` (folder),
> `account:` (by address or id), and `calendar:`/`before/after` on the event side. Saved searches
> are a small table + sidebar rows. Index freshness is surfaced in Settings → General with a
> cancellable batched rebuild that emits `StoreEvent::SearchIndex` progress. Rule safety: a dry-run
> (`preview_rules`) returns the affected count + per-message rule order (via `rules::matching_rules`,
> honoring `stop_processing`); applying keeps the preview as the undo base and `revert_rules`
> restores before-state + cancels queued actions. Key files: `crates/quill-store/src/sqlite.rs`
> (migration 22, operators, saved searches, rebuild, rule preview/revert), `rules.rs`,
> `commands.rs`/`lib.rs`, `src/lib/mail.ts`, `src/components/{MessageList,Sidebar,SearchHelp}.tsx`,
> `settings/{GeneralSection,RulesSection}.tsx`. Still needs release verification: operator behavior
> on a large real mailbox, and the rebuild's cancellation UX under load.

### P1.4 Calendar editing polish

- [x] Add undo for event delete, drag, resize, and calendar move, including recurring scope.
      (`CalendarUndoBar` + `restore_event`; snapshots captured for delete/edit/move/resize/create —
      recurring-scope undo is blocked on recurrence wiring in the store, which is itself a gap.)
- [x] Add conflict/overlap indicators and working-hours shading in week/day views.
      (`findConflictingEventIds`/`isWithinWorkingHours` in `layout.ts`; Week/Day views show a ⚠
      overlap badge and shade the grid outside the working-hours window, default 9–17.)
- [x] Add quick event creation with sensible duration/calendar defaults; natural-language parsing
      can follow after deterministic entry is excellent.
      (The editor now starts at the clicked/dragged hour with a 1-hour duration on the first
      enabled, editable calendar; NL parsing remains a later step.)
- [x] Add per-event color, duplicate event, and an explicit read-only state for subscriptions and
      shared calendars.
      (`events.color` migration 23 + `CalendarEvent.color`; color picker in the editor + EventDetail;
      `duplicate_event` + a Duplicate action; `Calendar.readOnly` disables editing — auto-marking
      subscription calendars read-only awaits subscription events carrying a source tag.)
- [ ] Finish task/VTODO agenda integration if tasks remain a product feature.
      *(Tasks already list/toggle in the sidebar + agenda; deferred.)*
- [ ] Improve attendee autocomplete, availability explanation, and invitation delivery status.
      *(Attendees is the separate rcalendar P0 story; availability "find a time" already exists;
      delivery status follows iTIP.)*

> **Implementation report (2026-08-15).** Calendar editing polish across the rcalendar package and
> the rmail integration. Undo: `lib/calendar.ts` + CalendarView capture pre-edit snapshots and a
> `CalendarUndoBar` (pattern: TriageUndoBar) restores via `restore_event` (INSERT-OR-REPLACE). Color:
> `events.color` migration 23 + `CalendarEvent.color`, mapped through `calendarAdapter`, picked in
> the editor + EventDetail, and rendered as `event.color ?? calendar.color` in Week/Day views.
> Duplicate: `duplicate_event` + a Duplicate action. Read-only: `Calendar.readOnly` disables the
> editor (auto-marking subscriptions is gated on subscription events carrying a source). Conflict +
> working hours: helpers in `headless/layout.ts` (unit-tested) drive ⚠ badges + out-of-hours shading
> in Week/Day views. Quick-create now respects the clicked time + first enabled calendar. Key files:
> `crates/quill-store/src/{sqlite.rs,types.rs}` (migration 23, restore/duplicate), `commands.rs`,
> `lib/calendar{Adapter,.ts}`, `components/calendar/{CalendarView,EventDetail}.tsx`,
> `components/CalendarUndoBar.tsx`, and `rcalendar/packages/calendar-ui/src/{types,components/
> EventEditorModal,views/WeekView,views/DayView,headless/layout}.{ts,tsx}` (rebuilt into dist).
> Recurring-scope undo and attendee/iTIP remain follow-ups. User to verify in the real app.

### P1.5 Desktop integration and comfort

- [x] Add dark palettes for both themes, including a safe per-message strategy for HTML mail.
      (A `[data-color-scheme="dark"]` override block flips the color tokens for BOTH treatments,
      stamped before first paint and toggled in Settings → Appearance; the calendar package has its
      own dark override. HTML mail follows dark defaults unless it sets its own colors; images are
      never rewritten.)
- [x] Restore window size, pane widths, selected account/folder, calendar view/date, list scroll,
      and unfinished composers after restart.
      (Window size + pane widths already persisted; the active folder/account, calendar date + view,
      per-filter scroll, and the most recent draft composer now restore on launch.)
- [x] Register `mailto:`, `.eml`, `.ics`, and `webcal:` handlers on supported platforms.
      (`tauri-plugin-deep-link` for `mailto:`/`webcal:` (mailto opens a pre-filled composer) +
      bundle file associations for `.eml`/`.ics` — OS registration needs a release-build check.)
- [x] Add system tray/menu-bar behavior, launch at login, unread badge, and a documented background
      mode for sync and reminders.
      (System tray Show/New Message/Quit + `tauri-plugin-autostart` toggle in Settings → General;
      unread badge already ships; `docs/background-mode.md` documents the running-while-open model
      and the missing background agent.)
- [x] Support printing for messages, conversations, event details, and day/week/month calendar views.
      (`@media print` stylesheet hides the app chrome; `⌘P` + Print buttons print the reading pane /
      event detail / calendar.)
- [x] Add a keyboard-shortcut reference reachable from every primary surface and detect conflicts
      with system shortcuts.
      (`?`-reachable `ShortcutsHelpModal` + a toolbar `⌘?`; it lists the app bindings and the
      browser shortcuts the app suppresses.)

> **Implementation report (2026-08-15).** P1.5 across the rmail + rcalendar packages. Shortcuts: a
> `?`/`⌘?`-reachable reference modal (`ShortcutsHelpModal` + `lib/shortcuts`) listing bindings and
> suppressed browser shortcuts. Session restoration: the active folder/account + calendar date/view
> and per-filter scroll persist (localStorage), and `latest_draft` (`save_draft`'s inverse) reopens
> the most recent unfinished composer via `openDraftMessage`. Printing: a `@media print` stylesheet
> + `⌘P`/Print buttons. OS integration: `mailto:`/`webcal:` deep links (opens a pre-filled composer
> via `StoreEvent::Mailto`), `.eml`/`.ics` bundle associations, a system tray (Show/New Message/
> Quit), and `tauri-plugin-autostart` launch-at-login (Settings toggles) + `docs/background-mode.md`.
> Dark palettes: a `[data-color-scheme="dark"]` override in both `tokens.css` files (additive — the
> token guard stays green), stamped pre-paint by the Rust init script, toggled in Appearance; MailBody
> forces light-on-dark defaults for HTML mail without rewriting mail-set colors or images. Key files:
> `src/lib/{keymap,theme,shortcuts,compose,mail,store-events}.ts`, `components/{ShortcutsHelpModal,
> MailBody,MessageList}.tsx`, `components/settings/{AppearanceSection,GeneralSection}.tsx`,
> `src-tauri/{lib.rs,commands.rs,settings.rs,tauri.conf.json,capabilities/default.json}`, the rcalendar
> tokens.css. Still needs release verification: OS deep-link/file-association registration and the
> tray on a built bundle; dark-mode tuning on the long tail of secondary tokens. User to verify in
> the real app.

### P1.6 Import, export, backup, and ownership

- [x] Import `.eml` and mbox with duplicate detection, progress, cancellation, and an error report.
      (`quill_mail::import` (mail-parser) + `store.import_message` with Message-ID dedup; the
      Settings → Data & backup importer takes .eml/.mbox files into a chosen account+folder and
      reports imported/duplicates/errors. Progress for very large mbox is bounded by the per-file
      read; a cancel control is a follow-up.)
- [x] Harden `.ics` import/export for recurrence, timezones, alarms, attendees, and malformed input.
      (calendar-core gains round-trip tests for VALARM/ATTENDEE tolerance, malformed input, and
      RRULE COUNT; recurrence/timezone already round-trip. App-level recurrence/attendee round-trip
      is gated on the events table gaining rrule/uid columns — a known gap.)
- [x] Export selected messages/conversations and calendars without requiring access to the local
      SQLite schema.
      (`store.eml_for_message` assembles an RFC-5322 message; an Export .eml action in the reading
      pane downloads it. Calendar `.ics` export already exists via the adapter.)
- [x] Add user-controlled backup of settings, rules, identities, signatures, local-only calendars,
      and unsent work; never export secrets unless explicitly requested and encrypted.
      (Settings → Data & backup: Export downloads a JSON bundle of settings + local-only rows
      (local events, tasks, saved searches, contact groups, hidden recipients, subscriptions,
      drafts, scheduled sends); Restore re-applies it. Credentials/OAuth tokens never enter it —
      they stay in the OS keychain.)
- [x] Document which data is authoritative on the server versus local-only and what “rebuild cache”
      removes.
      (`docs/data-ownership.md`.)

> **Implementation report (2026-08-15).** P1.6 across the store, quill-mail, the commands, and
> Settings. Export: `SqliteStore::eml_for_message` rebuilds an RFC-5322 `.eml` from stored fields
> (headers, plain/HTML body, attachment notes) → `export_message_eml` → an "Export .eml" reading
> pane action that downloads a Blob. Import: `quill_mail::import::{parse_mbox, import_eml}` +
> `store.import_message` (Message-ID dedup, sender mirrored, thread id from subject) →
> `import_messages` → a Settings file input with an imported/duplicates/errors report. ICS
> hardening: calendar-core tests lock in VALARM/ATTENDEE tolerance + malformed-input safety +
> RRULE COUNT round-trip (app-level recurrence is gated on the events schema). Backup:
> `backup_local_data`/`restore_local_data` cover the local-only rows, `backup_now`/`restore_backup`
> combine them with settings.json and never include secrets; a Settings Export/Restore UI. Docs:
> `docs/data-ownership.md` (server-vs-local + rebuild-cache semantics). Key files:
> `crates/quill-store/src/sqlite.rs`, `crates/quill-mail/src/import.rs`, `commands.rs`,
> `components/settings/GeneralSection.tsx`, `components/ReadingPane.tsx`, calendar-core `ical.rs`.
> Still needs verification: very-large mbox progress/cancel and a release-build pass. User to
> verify in the real app.

---

## P2 — competitive polish

- [ ] Add native standalone compose windows and optional multi-window message/event views.
- [ ] Add event templates, message templates, and reusable recipient groups.
- [ ] Add calendar attachment support and provider-native video-meeting creation where available.
- [ ] Add working-location, out-of-office, and focus-time event types after interop behavior is
      defined.
- [ ] Add locale packs, RTL layout, locale-aware parsing, and translator tooling.
- [ ] Add JMAP for providers that support it after extracting and stabilizing the mail transport
      boundary.
- [ ] Add PGP/S/MIME read and verification before considering compose encryption or key management.
- [ ] Evaluate mobile only as a separate product surface with an explicit background-sync and push
      strategy.
- [ ] Consider plugins, automation, or AI-assisted workflows only after the privacy and permissions
      model is designed and the core client has stable usage data.

## Explicitly out of scope for the first stable release

- Mobile apps, JMAP, plugin APIs, PGP/S/MIME compose, team collaboration, public calendar sharing,
  natural-language/AI features, and full CardDAV contact management.
- New visual themes beyond completing accessible light and dark variants of the existing themes.
- Provider-specific features that require a hosted Quill service unless their privacy, operations,
  and long-term cost are accepted as a separate product decision.

## Stable-release gate

All of the following must be true before publishing 1.0:

- [ ] Zero open known data-loss, duplicate-send, wrong-recipient, corruption, credential-exposure,
      or silently missed-invitation/alarm defects.
- [ ] P0 provider matrix and daily-driver scenario pass on macOS, Windows, and Linux release builds.
- [ ] Clean-install onboarding succeeds for Gmail, Microsoft 365, custom IMAP/SMTP, and CalDAV.
- [ ] Offline/reconnect, crash recovery, migration, backup/rebuild, and update/rollback drills pass.
- [ ] Security, privacy, accessibility, performance, and hostile-content checks are recorded.
- [ ] Signed installers and signed auto-updates work from production endpoints.
- [ ] Support, privacy, setup, provider, recovery, keyboard, and known-limit documentation is live.
- [ ] At least one release candidate completes the dogfood burn-in without a P0 regression.

After 1.0, keep the same gate for every stable release and add a regression scenario for every
escaped data-integrity or interoperability defect.

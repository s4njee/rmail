# Quill — Product Roadmap & Backlog

**Goal:** take Quill from its current state (a working local-first mail + calendar desktop app
running largely against demo data) to a **shippable, full-featured, cross-platform email and
calendar client**.

This document is the product backlog _above_ [plan2.md](plan2.md). plan2.md remains the
implementation spec for the v1 desktop shell; this roadmap tracks what's left of it, then
everything beyond it. Items reference plan2 epics where they overlap.

**Priority key:** 🅿️0 = blocks shipping, 🅿️1 = expected of a real client, 🅿️2 = competitive
feature, 🅿️3 = differentiator / later.

---

## Where we are today (2026-08)

**Done (per plan2.md):**

- Epics 1–8: workspace/toolchain, Hairline + Banded theme system, IPC contract + demo seed,
  three-pane shell, sidebar, virtualized message list, reading pane incl. sanitized HTML mail,
  focused reading + responsive behavior.
- Calendar UI shipped via the **rcalendar** engine (month/week/3-day/day/agenda, RRULE expansion,
  drag/resize, `.ics` import/export, multi-calendar visibility) — a recorded deviation from
  plan2's FullCalendar choice; the §14.1 data contract survived the swap.
- Settings (calendar prefs, accounts), keyboard model, theme toggle.
- Backend: `quill-store` (SQLite, sanitizer, types) is substantial; `quill-mail` has initial
  IMAP sync + SMTP + keychain; Tauri commands cover folders/messages/flags/drafts/send/events/accounts.

**Not done / thin:**

- `quill-cal` is a 9-line stub — **no CalDAV sync**; calendar is local-only.
- Search (plan2 Epic 15) — no FTS in the store, no search UI wired to it.
- Mail sync hardening (Epic 12 is partially there; IDLE/push, reconnection, multi-folder,
  incremental sync need finishing).
- Quality/packaging (Epic 16): no CI running, no signing/notarization, no release pipeline.

---

## Milestone map

| Milestone                       | Theme                    | Definition of done                                                                       |
| ------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------- |
| **M1 — v1 complete**            | Finish plan2.md          | Real mail end-to-end, search, hardened sync, security review, runs on all 3 desktop OSes |
| **M2 — Shippable 1.0**          | Release engineering      | Signed installers, auto-update, crash reporting, onboarding, docs, distribution channels |
| **M3 — Full-featured mail**     | Table-stakes mail        | OAuth providers, threading, attachments UX, notifications, undo send, signatures, rules  |
| **M4 — Full-featured calendar** | Table-stakes calendar    | CalDAV sync, invites (iTIP/iMIP), reminders, timezones, free/busy                        |
| **M5 — Power & polish**         | Competitive parity       | Snooze/send-later, templates, contacts, search operators, dark mode, a11y, i18n          |
| **M6 — Expansion**              | New platforms & ceilings | Mobile (Tauri iOS/Android), JMAP, encryption, plugins                                    |

Milestones M3 and M4 can run in parallel once M2's release train exists. Everything in
"Ongoing engineering" applies continuously from M1 onward.

---

## M1 — Finish v1 (plan2.md remainder)

### E1.1 Mail sync hardening (plan2 Epic 12) 🅿️0

- [x] Incremental sync per folder: UIDVALIDITY handling, HIGHESTMODSEQ/CONDSTORE where offered,
      full-resync fallback when UIDVALIDITY changes.
- [x] IMAP IDLE push per account with reconnect/backoff; visible connectivity state in titlebar.
- [x] Multi-folder sync beyond INBOX (Sent, Drafts, Trash, Archive, user folders) with
      folder-kind detection via SPECIAL-USE, heuristics fallback.
- [x] Flag round-tripping both directions (read/starred/deleted set remotely ↔ locally).
- [x] Offline action queue: archive/delete/flag/send performed offline replay on reconnect,
      with conflict policy documented.
- [x] Sync error surfacing: per-account error state in Settings → Accounts, not silent failure.
- [x] Bodystructure-aware fetch: envelopes first, bodies on demand, attachments lazily.

### E1.2 Compose completion (plan2 Epic 13) 🅿️0

- [x] Reply / reply-all / forward with correct quoting, `In-Reply-To`/`References` headers.
- [x] Attachments: add, remove, drag-in; size warnings; inline images on paste.
- [x] Draft autosave to the store and to the IMAP Drafts folder; resume after crash.
- [x] Address input: comma/semicolon parsing, validation, pill UI, To/Cc/Bcc.
- [x] Send via SMTP with per-account submission settings; failures land in an Outbox with retry,
      never silently lost.

### E1.3 Search (plan2 Epic 15) 🅿️0

- [x] SQLite FTS5 index over subject/sender/recipients/body text, populated at sync time.
- [x] `/` opens search; results ranked, snippet-highlighted, scoped to folder/account/all.
- [x] Calendar events searchable from the same field (title/location/notes).
- [x] Index rebuild command; index size counted in the footprint readout.

### E1.4 CalDAV minimum (plan2 §quill-cal) 🅿️0

- [x] `quill-cal`: CalDAV discovery (well-known, principal, calendar-home), collection listing.
- [x] Two-way sync via ctag/sync-token; etag conflict handling (server wins + local copy on conflict).
- [x] Recurring-event edits (this/this-and-future/all) round-trip correctly to RRULE/EXDATE.
- [x] Account setup UI: CalDAV URL + credentials in keychain, same pattern as mail.

### E1.5 Quality gate (plan2 Epic 16) 🅿️0

- [ ] CI live (GitHub Actions): fmt, clippy, tests, eslint/stylelint, domain-isolation and
      token-usage scripts, `cargo audit` + npm audit.
- [ ] Hostile-mail fixture suite running in CI against the sanitizer + iframe sandbox
      (revisit the recorded `allow-scripts` deviation and either close it or document the
      compensating controls as final).
- [ ] Security review checklist from 16.3 executed and recorded.
- [ ] Cross-platform smoke: macOS (WKWebView), Windows (WebView2), Linux (WebKitGTK) —
      per-release test matrix documented, WebKitGTK divergences filed.
- [ ] Performance budgets measured and recorded: cold start < 500 ms splash-to-content,
      60 fps list scroll on 50k-message mailbox, memory ceiling set from real numbers.

---

## M2 — Shippable 1.0 (release engineering)

### E2.1 Packaging & signing 🅿️0

- [x] macOS: universal `.app`/`.dmg` (arm64 + x86_64), entitlements, signing +
      notarization wired into CI. (Universal build verified locally; actual
      signing/notarization activates once the `APPLE_*` secrets are set — see
      `docs/release.md`.)
- [x] Windows: MSI + NSIS with WebView2 download-bootstrapper, wired into CI.
      (First signed run pending a Windows release.)
- [x] Linux: AppImage + `.deb` via CI; Flatpak manifest at
      `linux/flatpak/app.quill.Quill.json` (WIP — needs `flatpak-builder`
      validation, see `linux/README.md`).
- [x] Release artifacts built from CI only (`.github/workflows/release.yml` on
      `v*` tags → draft release with installers); no laptop builds.

### E2.2 Auto-update 🅿️0

- [x] Tauri updater with signed manifests; release + beta channels; staged rollout.
      (`tauri-plugin-updater` wired; CI signs bundles + publishes `latest.json`;
      channels/rollout are served per-endpoint — see `docs/release.md`. Public
      key in `tauri.conf.json`; the `TAURI_SIGNING_PRIVATE_KEY` secret activates
      signing.)
- [x] In-app "What's new" on first launch after update. (Version-tracked modal
      with curated release notes; silent startup check + "restart to apply"
      banner.)
- [x] Downgrade-safe SQLite migrations (versioned, forward-only with backup
      before migrate). (Older app refuses a newer schema; pre-migration DB
      backup; covered by tests.)

### E2.3 Crash & error reporting 🅿️1

- [x] Unified logging: one pipeline for Rust (`log`) and JS (`@tauri-apps/plugin-log`) via
      `tauri-plugin-log`, written to rotating local files; ad-hoc `eprintln!` removed; log level
      and log/crash folders surfaced in Settings → Diagnostics; a test guards that credentials
      and account addresses never reach logs or reports.
- [x] Opt-in crash reporting (Rust panics + JS errors), scrubbed of message content and PII
      (emails, tokens, home paths); reports always recorded locally, uploaded to the configured
      endpoint only while opted in; default off.
- [x] Opt-in, anonymous usage ping (version/OS only) to know if updates land. Default off;
      the local-first privacy story is a product feature — document exactly what is sent.

### E2.4 Onboarding & first run 🅿️0

- [x] First-run flow: add account → autodetect IMAP/SMTP settings (DNS SRV, Thunderbird ISP
      database format, common-provider table) → initial sync with progress.
      *(Implemented under backlog P0.2; release-verified pending.)*
- [ ] Import path: `.ics` for calendar (exists), `.eml`/mbox import for mail.
- [ ] Demo mode preserved behind a flag for screenshots/dev, unreachable by real users.

### E2.5 Docs & distribution 🅿️1

- [ ] Website/landing page with download links; screenshots pipeline already exists.
- [ ] User docs: setup, keyboard reference, provider-specific notes (app passwords etc.).
- [ ] Channels: Homebrew cask, winget, Flathub, AUR. Checksums + signatures published.
- [ ] Versioning + changelog discipline (semver, conventional commits already in use).

---

## M3 — Full-featured mail

### E3.1 Provider auth (OAuth2) 🅿️0 for 1.x

Gmail and Microsoft 365 dominate real mailboxes and are moving to OAuth-only.

- [x] OAuth2 authorization-code + PKCE flow in the system browser; tokens in keychain;
      refresh handling in `quill-mail`.
- [x] Gmail via IMAP XOAUTH2; Microsoft 365 via IMAP XOAUTH2.
- [x] Provider registration work tracked explicitly (Google verification review, Azure app
      registration) — this is calendar-time, not code-time, so start it early.
- [x] Per-provider quirks table (Gmail labels-as-folders, `\All Mail` duplication policy).

### E3.2 Conversation threading 🅿️1

Deliberately out of v1; the most-missed feature after it ships.

- [x] Thread model in `quill-store` (JWZ-style: `References`/`In-Reply-To` + subject fallback),
      computed at sync time, stored, not recomputed per render.
- [x] Message list renders threads collapsed with count; reading pane shows the conversation
      with collapsed quoted history.
- [x] Thread-level actions (archive/delete/mark-read whole thread).
- [x] Setting to disable threading (flat mode stays first-class — it's what v1 users know).

### E3.3 Attachments & files UX 🅿️1

- [x] Attachment list with type icons, quick-look/preview (PDF preview groundwork exists in
      `quill-store/pdf.rs`), save-as, save-all, drag-out.
- [x] Inline image display honoring the remote-content privacy gate.
- [x] "Attachment missing" outbound guard (mentions attachment, none attached).

### E3.4 Notifications 🅿️1

- [x] Native OS notifications for new mail (per-account and per-folder opt-in), click-to-open.
- [x] Unread badge on dock/taskbar icon.
- [x] Quiet hours + "notify only for people I know" option.

### E3.5 Sending niceties 🅿️1

- [x] Undo send (configurable 5–30 s delay before SMTP submission).
- [x] Signatures: per-account, per-identity; plain and HTML; reply vs new-mail placement.
- [x] Aliases / send-as identities per account.
- [x] Reply-state icons in the list (replied/forwarded), `Answered` flag round-trip.

### E3.6 Rules & filters 🅿️2

- [x] Local rules engine: conditions (from/to/subject/list-id/has-attachment) → actions
      (move, mark, star, delete, notify differently), applied at sync time.
- [x] Rules UI in Settings; import of simple Sieve subsets considered later.

### E3.7 Mail hygiene 🅿️2

- [x] One-click unsubscribe (RFC 8058 `List-Unsubscribe`), with confirmation.
- [x] Junk workflow: junk folder mapping, mark-as-junk moves + trains server flag where supported.
- [x] Remote-content blocking UI (per-sender allow list) — extend the existing sanitizer gate.

---

## M4 — Full-featured calendar

### E4.1 Scheduling with other people (iTIP/iMIP) 🅿️1

The line between "calendar app" and "toy": invitations must flow through mail.

- [x] Detect `text/calendar` parts in incoming mail; render an invite card in the reading pane
      (accept / tentative / decline) instead of a raw attachment.
- [x] RSVP sends the iMIP reply via the account's SMTP and updates the CalDAV copy.
- [x] Organizer flow: adding attendees to an event sends invitations; updates/cancellations
      send the right METHOD; sequence numbers handled.
- [x] Counter-proposals rendered read-only at minimum.

### E4.2 Reminders & alarms 🅿️1

- [x] VALARM support: parse, fire native notifications, snooze/dismiss; default-alarm setting.
- [x] Alarms fire while the app runs in background/tray; document behavior when it doesn't.

### E4.3 Timezone correctness 🅿️1

- [x] Events store their VTIMEZONE; render in the system zone; per-event timezone picker.
- [x] Secondary timezone column in week/day views 🅿️2.
- [x] DST fixture suite in `calendar-core` CI (create/move/recur across transitions).

### E4.4 Provider calendars 🅿️2

- [x] Google Calendar via CalDAV with OAuth (reuses E3.1 tokens).
- [x] Microsoft 365 calendar — decide CalDAV-bridge vs Graph API; spike before committing.
- [x] Read-only ICS subscription calendars (holidays, team calendars) with refresh interval.

### E4.5 Calendar UX depth 🅿️2

- [x] Free/busy lookup for attendees where the server offers it; "find a time" strip.
- [x] Travel-time / location field with map link; video-call link detection and one-click join.
- [x] Tasks/VTODO: decide explicitly in or out; if in, agenda-view integration first.
- [x] Year view and mini-month jump navigation in rcalendar.

---

## M5 — Power features & polish

### E5.1 Triage workflow 🅿️2

- [ ] Snooze (local, store-backed; message returns to inbox at time T).
- [ ] Send later (scheduled outbox; requires app or background process running — document it).
- [ ] Pin/flag view; "focused vs other" style splits explicitly **not** planned (keep triage manual
      and predictable — revisit only with real user pull).

### E5.2 Contacts 🅿️2

- [ ] Autocomplete from mail history (frecency-ranked) — cheap, do first.
- [ ] `quill-contacts` crate: CardDAV sync, contact page, avatars; per-sender settings hang here.

### E5.3 Search operators 🅿️2

- [ ] `from:` `to:` `subject:` `has:attachment` `before:/after:` `in:` account/folder scoping,
      compiled onto the FTS index; saved searches as virtual folders.

### E5.4 Dark mode (plan2 D5 lifted) 🅿️1

- [ ] Dark palettes for both Hairline and Banded as token-layer data changes; the variable layer
      was built for this (Epic 2.4). System-follow + manual override.
- [ ] HTML mail dark handling: iframe background token already in place; add safe content
      color-inversion heuristic with per-message toggle.

### E5.5 Accessibility 🅿️1

- [ ] Full keyboard reachability audit (docs/accessibility.md exists — turn it into a tested
      checklist); focus outlines in both themes.
- [ ] Screen-reader pass on the big three surfaces (list, reading pane, calendar grid);
      rcalendar grid semantics (`grid`/`gridcell`, roving tabindex).
- [ ] Reduced-motion, high-contrast, and 200 % zoom verified per release.

### E5.6 Internationalization 🅿️2

- [ ] Externalize UI strings (Fluent or Paraglide); RTL layout audit; locale-aware
      dates/first-day-of-week in rcalendar (partially exists via prefs).
- [ ] At least 2–3 community-translatable languages before advertising i18n.

### E5.7 Window & session polish 🅿️2

- [ ] Multi-window: compose in its own window; main-window state restore (folder, selection,
      scroll, pane sizes).
- [ ] System tray / menu-bar presence with new-mail indicator; launch-at-login option.
- [ ] Printing: message and calendar-view print stylesheets.
- [ ] `mailto:` and `webcal:`/`.ics` OS-level handler registration on all three platforms.

---

## M6 — Expansion

### E6.1 Mobile (Tauri 2 iOS/Android) 🅿️3

Tauri 2 supports both targets and the domain crates are UI-free by design (enforced in CI), so
the Rust core carries over. The frontend does not: three-pane desktop UI ≠ mobile.

- [ ] Spike: `calendar-core` + `quill-store` building for iOS/Android; SQLite + keychain
      equivalents (Keychain/Keystore) proven.
- [ ] Mobile shell: single-pane navigation stack, swipe actions, share-sheet compose.
- [ ] Push: without a server component, mobile push means background-fetch IMAP — document the
      honest latency, or scope a minimal relay service (privacy-sensitive; decide deliberately).
- [ ] Ship Android first (sideload/beta friction is lower), iOS after TestFlight round.

### E6.2 JMAP 🅿️3

- [ ] `quill-mail` transport trait extracted (IMAP as first impl) — do this during E3.1/E1.1
      refactors, cheap then, expensive later.
- [ ] JMAP client against Fastmail/Stalwart; push via EventSource replaces IDLE where available.

### E6.3 Encryption 🅿️3

- [ ] Read-only first: verify signatures, decrypt PGP/S-MIME with existing keys.
- [ ] Compose encryption + key management later; never half-ship key UX.

### E6.4 Extensibility 🅿️3

- [ ] Theme packs (the token system already makes themes data).
- [ ] Scripting/plugin surface only after 1.0 stabilizes; the IPC contract is the natural API.

---

## Ongoing engineering (every milestone)

- **Security:** hostile-mail suite grows with every parser/renderer change; `cargo audit`/npm
  audit gating; dependency count stays deliberately small; annual full review of the Tauri
  capability surface and CSP.
- **Performance:** budgets in CI where measurable (list scroll, cold start, sync throughput);
  50k+ message and 5k+ event fixture profiles.
- **Testing:** protocol-level integration tests against containerized servers (Dovecot/Postfix,
  Radicale/Stalwart for CalDAV) in CI; screenshot-diff tests for both themes (script exists).
- **rcalendar:** keep it independently useful (own repo hygiene, semver, README) — it's both a
  dependency and a potential open-source asset.
- **Docs drift:** `check-readme-drift.mjs` stays green; plan2.md gets a completion marker per
  epic rather than silent divergence.

---

## Sequencing rationale

1. **M1 before anything shiny.** Search, real sync, and CalDAV are the difference between a demo
   and a client; every later feature builds on their data model.
2. **M2 immediately after** — release infrastructure gets cheaper the earlier it exists, and
   auto-update makes every subsequent milestone deliverable incrementally.
3. **OAuth (E3.1) is the long pole** of M3 because of provider review timelines — file the
   Google/Azure registrations at M2 time even though the code lands later.
4. **Threading (E3.2) and invites (E4.1)** are the two highest-leverage post-1.0 features; user
   perception of "full-featured" hinges on them more than anything else in M3–M5.
5. **Mobile last, spike early.** The E6.1 build spike is cheap and de-risks the biggest promise
   in "cross-platform"; the actual mobile UI is a product of its own and must not stall desktop 1.0.

## Top risks

| Risk                                                        | Impact                       | Mitigation                                                                  |
| ----------------------------------------------------------- | ---------------------------- | --------------------------------------------------------------------------- |
| OAuth provider verification delays (Google esp.)            | Blocks Gmail users on 1.x    | Start registration at M2; ship app-password path meanwhile                  |
| CalDAV server heterogeneity (Google/iCloud/Radicale quirks) | Sync bugs erode trust fast   | Containerized server matrix in CI; conflict policy that never loses data    |
| iMIP interop (Outlook/Google mangle invites differently)    | Invites are visible failures | Fixture corpus from real providers; render-only fallback when parsing fails |
| WebKitGTK divergence grows with UI complexity               | Linux quality slips          | Keep the per-release 3-engine matrix from E1.5 non-negotiable               |
| Background alarms/send-later without a daemon               | Features silently don't fire | Tray/background mode first-class in E5.7; honest docs                       |
| Scope creep before 1.0                                      | Never shipping               | M1+M2 list is closed; new ideas land in M3+ only                            |

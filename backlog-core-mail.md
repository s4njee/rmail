# Quill — Core mail backlog ("worth switching to")

This backlog answers a narrower question than the other two: **what has to be true before someone
would rather open Quill than a Gmail tab or Thunderbird for their everyday mail?**

- [backlog.md](backlog.md) is the _release-trust_ queue (provider matrix, signing, a11y, drills).
- [backlog-thunderbird.md](backlog-thunderbird.md) is the _parity_ queue (POP, tags, profile import,
  address book, PGP).
- This file is the _core loop_ queue: read, triage, search, and send mail on real Gmail / IMAP /
  Microsoft 365 accounts, fast, without losing anything, plus the handful of things that make a
  native client clearly better than a browser tab. It **takes priority over both** — nothing in the
  other two matters while the items in C0 are open.

Every finding below was verified in code at `68599af` plus the working tree on 2026-09-18, not
taken from checkboxes in other documents. File references are to that tree.

## The bar

"Basic" means doing a small number of things very well, not matching Thunderbird feature for
feature. Quill is worth using when a Gmail-web or Thunderbird user can:

1. Add a Gmail, Microsoft 365, or standard IMAP account (Fastmail, iCloud, self-hosted) in under a
   minute, without developer credentials or typing server names.
2. See **all** their mail — not the last week — with the inbox usable within seconds of setup and
   history backfilling behind it.
3. Archive, delete, move, star, and mark mail and have the server end up in exactly that state —
   never a permanent delete they didn't ask for, never an action that silently reverts.
4. Open any message and see it rendered correctly, including inline images and attachments that
   actually open.
5. Write a reply and have the recipient receive exactly what was typed, once, from the right
   address, threaded correctly, with a copy in Sent and drafts that roam.
6. Get notified about new mail.
7. Do all of that faster than Gmail web — instant list, instant search across all mail, full
   keyboard control — while using a fraction of Thunderbird's footprint and blocking trackers by
   default.

## Where the code actually is

Quill has a genuinely good shell: a calm three-pane UI, a sandboxed and CSP-locked HTML renderer,
remote images blocked by default with per-sender trust, an offline action queue with undo, local
FTS search with operators, unified inbox with alias-aware replies, snooze/send-later, and a 23 MB
app bundle. That is a real head start over Gmail web and Thunderbird on privacy and weight.

Underneath, the core loop is not safe to point at a real mailbox yet:

| Area          | Finding                                                                                                                                                                                                                                                                                                                                   | Evidence                                                                                                                                                                                |
| ------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Build         | Working tree doesn't compile (uncommitted `rcalendar` changes); CI has never passed a run — fmt/prettier fail before clippy/tests ever execute                                                                                                                                                                                            | `crates/quill-cal/src/sync.rs:138`, `src/lib/calendarAdapter.ts:260`, `.github/workflows/ci.yml`                                                                                        |
| History       | Hard 7-day retention; a global `DELETE … WHERE received_at_ms < ?` at startup and after every sync, across all accounts, including drafts, imported and snoozed mail                                                                                                                                                                      | `quill-mail/src/sync.rs:49,484`, `src-tauri/src/sync.rs:28-45`, `sqlite.rs:4648`                                                                                                        |
| Delete        | "Delete" queues `+FLAGS \Deleted` + a bare `EXPUNGE` — permanent loss on Fastmail/M365/IMAP, and it expunges every other `\Deleted` message in the folder                                                                                                                                                                                 | `sqlite.rs:1745`, `quill-mail/src/sync.rs:994,1426-1439`                                                                                                                                |
| Archive/Junk  | Gmail archive falls back to a nonexistent "Archive" folder and retries forever while the UI shows it archived; junk goes to hard-coded "Junk" and failure counts as success                                                                                                                                                               | `sqlite.rs:4001`, `quill-mail/src/sync.rs:983,1030-1034`                                                                                                                                |
| Queue         | `markJunk` enqueued but parser expects `mark_junk` → replays as **MarkRead**; after a move the source UID is kept so later actions hit the wrong message; no UIDVALIDITY; unbounded retries; thread and rule actions never reach the server                                                                                               | `sqlite.rs:1608,4275,1583-1590,4324,1354-1441,2185`                                                                                                                                     |
| Attachments   | Bytes discarded at sync; "Save" writes placeholder content; filename from mail joined unsanitised into the save path; `cid:` images unresolved; forward drops attachments                                                                                                                                                                 | `quill-mail/src/sync.rs:1161-1174`, `src-tauri/src/commands.rs:288-324,309`, `sanitize.rs:365`, `compose.ts:271`                                                                        |
| Sending       | With an HTML signature, the HTML part is built once at open and never updated — recipients see the signature but not the typed text; no Message-ID; SMTP host guessed from the IMAP host; blocking SMTP in async; duplicate-send windows on crash/race; permanent 5xx retried forever; undo-send lives in a JS timer; no APPEND to Sent   | `compose.ts:242-248`, `smtp.rs:37,144-175`, `provider.rs:165-178`, `quill-mail/src/sync.rs:1056-1100`, `src-tauri/src/sync.rs:145,305,336`, `commands.rs:740-750`, `compose.ts:397-431` |
| Gmail         | Only Gmail logic is skipping All Mail: labels become folders (one message = N rows, N downloads), archived-unlabelled mail never syncs, no X-GM-*                                                                                                                                                                                         | `quill-mail/src/sync.rs:286-294`                                                                                                                                                        |
| Sign-in       | Google/Microsoft OAuth client IDs are placeholders; onboarding shows client-ID fields to users                                                                                                                                                                                                                                            | `commands.rs:1246-1255`, `oauth-config.json`                                                                                                                                            |
| Threading     | Synced mail is threaded by normalised subject only (headers never passed)                                                                                                                                                                                                                                                                 | `sqlite.rs:4448`                                                                                                                                                                        |
| Scale         | 500-row list cap with no paging; FTS update trigger fires on every flag change and scans the index (≈60 ms per mark-read at 100k); missing FK indexes; one `Mutex<Connection>`, no WAL; full `1:*` flag sweep every cycle; `NOT IN (…100k)` exceeds SQLite's bind limit and expunge detection silently stops; UIDVALIDITY changes ignored | `mail.ts:229`, `sqlite.rs:333-352,537,4025,4071,4611-4640`, `quill-mail/src/sync.rs:660,705`                                                                                            |
| Notifications | New-mail notifications never fire (`shouldNotifyMessage` has no caller); only calendar alarms notify                                                                                                                                                                                                                                      | `src/lib/notifications.ts`, `src/lib/alarms.ts:54`                                                                                                                                      |
| Drafts        | Local only, lose attachments and identity, can't be reopened from the Drafts folder, newest draft pops open on every launch                                                                                                                                                                                                               | `commands.rs:617`, `App.tsx:121`                                                                                                                                                        |
| Tests         | 271 Rust tests pass at `68599af`, but quill-mail has 26 pure unit tests, no IMAP/SMTP integration tests, zero frontend tests                                                                                                                                                                                                              | —                                                                                                                                                                                       |

---

## C0 — Stop hurting mail

Must land before anyone, including the author, connects a mailbox they care about. Each item needs
a regression test; the protocol ones need the fake-server harness from C3.2.

### C0.1 Green build, green CI

- [x] Land or shelve the in-flight `rcalendar` changes so `cargo test --workspace` and
      `pnpm typecheck` pass on `main`; give `quill-cal` a compile-time contract test so calendar
      churn can't silently break the mail app again.
- [x] Run `cargo fmt` and `pnpm format` once, as a standalone commit, so CI reaches clippy and tests.
- [x] CI green on `main`, including a macOS job for the Rust tests (the only platform anyone runs).
- [x] Require CI to pass before merging to `main`.

### C0.2 Keep all the mail

- [x] Delete the global `prune_messages_before` and both `RETAIN_DAYS = 7` constants.
- [x] Sync headers (envelope, flags, structure) for the whole mailbox by default; make the _body
      and attachment cache_ the thing with a window (setting per account: 30 days / 1 year / all,
      default 1 year). Older bodies fetch on open.
- [x] Eviction only ever drops cached bodies/attachment files — never message rows, and never
      local-only data (drafts, outbox, scheduled, snoozed, imported).
- [x] Initial sync fetches newest first so the inbox is usable within ~5 s, then backfills in the
      background with visible progress and without blocking the UI.
- [x] Tests: a draft, an imported message, and a snoozed message older than the window all survive
      a sync and a restart.

### C0.3 Delete means Trash

- [x] Delete = move to the account's Trash (RFC 6154 `\Trash`; `[Gmail]/Trash` on Gmail) using
      `MOVE`, or `COPY` + `\Deleted` + `UID EXPUNGE` of that UID only when `MOVE` is missing.
- [x] Never issue a bare `EXPUNGE`. Permanent delete exists only in Trash/Spam, requires
      confirmation, and uses `UID EXPUNGE` (UIDPLUS) scoped to the selected UIDs.
- [x] Undo restores from Trash if the server move already happened.

### C0.4 Archive and Junk land in real folders

- [x] Resolve and persist special-use folders per account (`\Archive \Junk \Trash \Sent \Drafts
\All`) at folder discovery; name heuristics only as fallback.
- [x] Gmail archive = remove the Inbox label (move to All Mail); Gmail junk = `[Gmail]/Spam`;
      M365 junk = "Junk Email".
- [x] If an account has no archive folder, ask once (create "Archive" or pick one).
- [x] A failed server action is never reported as success: it surfaces in the existing queued-
      actions UI and the local change reverts.

### C0.5 Make the action queue correct

- [x] One enum ↔ string mapping for action types; an unknown type is an error, never `MarkRead`
      (fixes `markJunk` → `MarkRead`).
- [x] After a move, capture the new UID (`COPYUID`/`MOVE` response) or mark the row "location
      pending" until the next sync resolves it by Message-ID; later actions never reuse the source
      UID in the destination folder.
- [x] Record UIDVALIDITY on every queued action; on mismatch, re-resolve or fail visibly.
- [x] Retry with capped exponential backoff; after the cap the action is `failed` and visible.
- [x] Route thread actions (`apply_thread_action`) and rule actions through the same enqueue path so
      the next sync doesn't undo them.
- [x] Replay on manual refresh and for "on open" accounts, not only on the periodic loop.

### C0.6 Attachments that are real files

- [x] Persist attachment parts when a body is fetched (or fetch a part on demand with
      `BODY.PEEK[<section>]`); set `on_disk` truthfully.
- [x] Delete the placeholder-writing code paths in `save_attachment` / `save_all_attachments`;
      when a part is unavailable offline, say so.
- [x] Sanitise attachment filenames (strip separators, `..`, control and reserved names, cap
      length) and verify the resolved path stays inside the chosen directory.
- [x] Resolve the real Downloads directory via the Tauri path API (the literal `~/Downloads` at
      `ReadingPane.tsx:202` is never expanded).
- [x] Rewrite `cid:` references to the stored inline parts so embedded images render.
- [x] Forward carries the original attachments.

### C0.7 The recipient gets what you typed

- [x] Build the HTML part at send time from the current body (escaped, linkified) plus signature
      and quote — or send `text/plain` only when there is no HTML content. Never send an HTML part
      built at compose-open time.
- [x] HTML-escape quoted text before inserting it into the HTML part (`compose.ts:244`).
- [x] Set a `Message-ID` (right-hand side from the sending domain) and store it so the sent copy
      threads with replies.
- [x] Enter in Subject moves focus to the body; ⌘/Ctrl+Enter sends; Esc or a backdrop click on a
      non-empty composer saves the draft and closes, never discards.

### C0.8 Send exactly once

- [x] Durable outbox state machine: `queued → sending (leased) → sent | failed`, with one sender
      task per account; mark `sending` before SMTP and reconcile on restart instead of blindly
      re-sending.
- [x] Remove the race between `sync_account_now` and the periodic loop both replaying sends.
- [x] Permanent SMTP failures (5xx) → `failed` with Edit / Retry / Discard in the Outbox.
- [x] Undo-send is an outbox row with `send_after`, not a JS timer; quitting during the countdown
      keeps the message (send on next launch, and say so).
- [x] `APPEND` the sent message to Sent for servers that don't auto-save (everything except Gmail
      and M365), with `\Seen`.
- [x] Switch to lettre's async transport.

### C0.9 Correct outgoing and connection settings

- [x] SMTP host, port, security (SSL/STARTTLS), and username are real account fields, filled by
      the existing autodiscovery and editable; stop deriving them from the IMAP host.
- [x] Support STARTTLS for IMAP; refuse plaintext `LOGIN` unless the user explicitly allows it for
      a localhost bridge.
- [x] The connection test sends an SMTP `AUTH` and reports it separately.

### C0.10 No silent corruption at scale

- [ ] Replace the `NOT IN (?,…)` expunge check with a temp-table diff (or chunking); never drop its
      error with `let _`.
- [ ] Clear and resync a folder when UIDVALIDITY changes; key `folder_uids` and flag updates on
      `(folder, uid, uidvalidity)`.
- [ ] Test with a 100k-UID folder and a forced UIDVALIDITY change.

---

## C1 — The core loop works on real accounts

After C0, this is what makes Quill usable every day on Gmail, Microsoft 365, and a standards IMAP
provider.

### C1.1 Sign in without developer credentials

- [ ] Register a production Google OAuth client (Desktop) and a Microsoft Entra app
      (`IMAP.AccessAsUser.All`, `SMTP.Send`, `offline_access`); inject IDs at build time like the
      telemetry endpoints.
- [ ] Start Google's restricted-scope verification for `https://mail.google.com/` now — it has a
      multi-week lead time; confirm whether a local-only client is exempt from the third-party
      security assessment.
- [ ] Until verification lands, make the Gmail app-password path first-class in onboarding.
- [ ] Hide client-ID/secret fields under "Advanced".
- [ ] Guard token refresh against concurrent refreshes.

### C1.2 Gmail as Gmail

- [ ] Sync Gmail through `[Gmail]/All Mail` with `X-GM-MSGID`, `X-GM-LABELS`, `X-GM-THRID`: one
      row per message, labels in a `message_labels` table (the same table later serves tags, T0.4).
- [ ] Label "folders" in the sidebar are views over labels; Inbox is a label.
- [ ] Map actions to label operations: archive, move, apply/remove label, star (`\Starred`), spam.
- [ ] One body download per message, regardless of label count.
- [ ] Use `X-GM-RAW` for server search fallback (C2.2).

### C1.3 Sync that is quick and quiet

- [ ] Use CONDSTORE/QRESYNC where advertised (`HIGHESTMODSEQ` is already stored but unused);
      otherwise flag-sync a recent window every cycle and the full folder rarely.
- [ ] An IDLE wake syncs that folder only, not the whole account.
- [ ] One task per account with connect/read timeouts, capped exponential backoff with jitter, and
      handling for `[ALERT]`/`[THROTTLED]`; fix the IDLE hot-reconnect loop when IDLE is
      unsupported.
- [ ] Bounded connections per account (e.g. one IDLE + one worker, reused for body fetches).
- [ ] Budgets: new mail visible ≤ 5 s after arrival on IDLE accounts; ~0 % CPU when idle.
- [ ] Correct `docs/provider-quirks.md`, which currently claims connection limits and throttling
      backoff that don't exist.

### C1.4 Real conversations

- [ ] Compute thread IDs from `References`/`In-Reply-To` at insert (and `X-GM-THRID` on Gmail);
      subject only as a fallback, bounded by time and participants.
- [ ] Conversations include your Sent replies and exclude deleted messages.
- [ ] In conversation mode, `e` / `#` / `s` / mark-read act on the whole thread, as in Gmail.

### C1.5 Data layer that stays fast at 250k messages

- [ ] External-content FTS keyed by rowid; re-index only when subject/sender/recipients/body change
      (not on flag changes).
- [ ] Add indexes on `recipients(message_id)`, `attachments(message_id)`,
      `messages(account_id, folder, uid)`, `messages(message_id_header)`.
- [ ] WAL, `synchronous=NORMAL`, `busy_timeout`; separate read connection(s) from the single
      writer so sync never blocks the list.
- [ ] Batch sync writes (one transaction per fetch chunk, not two per message).
- [ ] Make hot Tauri commands async so they never run on the main thread.
- [ ] Maintain unread/total counts incrementally; stop summing every body length in `accounts()`.
- [ ] Remove the load-every-body snippet rebuild at startup (`lib.rs:315`); do it once as a
      migration.
- [ ] Wrap each migration in a transaction; prune old pre-migration backups.
- [ ] Add a CI benchmark on a synthetic 250k-message store: page < 20 ms, mark-read < 5 ms, counts <
      10 ms, search p95 < 100 ms.

### C1.6 A message list you can scroll to the end

- [ ] Keyset pagination with windowed fetch on scroll; remove the 500-row cap.
- [ ] Optimistic row updates instead of re-querying 500 rows plus settings after every action.
- [ ] Show star, attachment, and account-colour markers on rows.
- [ ] Add a compact density (single-line rows, ~36 px) next to the current 80 px rows.
- [ ] Quick filters: Unread, Starred, Has attachment (the small version of T0.5).

### C1.7 New-mail notifications

- [ ] Emit from the backend for new Inbox mail (never for initial sync or backfill); coalesce bursts
      into one notification.
- [ ] Use `tauri-plugin-notification` on every platform; clicking opens the message.
- [ ] Wire the existing sound, quiet-hours, and per-account settings, or remove them.
- [ ] Unread count on the dock badge (exists) and the tray.

### C1.8 Drafts that roam

- [ ] Clicking a message in Drafts opens it in the composer.
- [ ] Drafts keep identity, attachments, and reply headers.
- [ ] Save drafts to the server Drafts folder (replacing the previous version) so they appear on
      phone and web.
- [ ] Stop auto-opening the newest draft on launch; show a quiet "1 draft" affordance instead.

### C1.9 Replies that go to the right people

- [ ] Honour `Reply-To`; reply-all excludes every identity and alias, not just the primary address.
- [ ] Forward uses a standard forwarded-message header block.
- [ ] For mailing lists, offer reply-to-list vs reply-to-sender (`List-Post`).

### C1.10 Mail that renders like it does in Gmail

- [ ] Allow `<style>` blocks, `class`, and table layout attributes (`bgcolor`, `width`, `align`,
      …) inside the sandboxed iframe, keeping `url()` fetches under the remote-image rule.
- [ ] A golden-render corpus of ~30 real-world newsletters, receipts, and notifications (GitHub,
      Stripe, Amazon, Substack, calendar invites) compared against reference screenshots in CI.
- [ ] Linkify plain-text mail; collapse quoted text and signatures in both paths.
- [ ] `mailto:` links open the composer.
- [ ] A body-fetch or auth failure shows an error with Retry, not an empty body.

### C1.11 Errors a person can act on

- [ ] Per-account status in the sidebar: syncing (with progress), up to date (with time), needs
      sign-in, server unreachable — using the classifier in `quill-mail/src/error.rs`.
- [ ] An auth failure stops retrying and shows a "Reconnect" banner.
- [ ] A startup failure shows a recovery screen with Retry and Open logs, not a blank window.

---

## C2 — Reasons to switch

These are why someone chooses Quill over the Gmail tab they already have. Several build on
strengths that already exist.

### C2.1 Keyboard-first, with Gmail muscle memory

- [ ] Complete the Gmail map: `x` select, `u` back to list, `z` undo, `Shift+I`/`Shift+U`
      read/unread, `g i`/`g s`/`g d`/`g t` go-to, `v` move, `l` label, `n`/`p` within a thread,
      Delete/Backspace, ⌘/Ctrl+Enter send.
- [ ] Command palette (⌘K): every action, folder/label jump, account switch.
- [ ] Triage keys work on search results (today they no-op and still show an undo toast,
      `mail.ts:515-525`).
- [ ] Generate the shortcuts help from the keymap so they can't drift (it already has: it lists
      `,` for Settings, but the binding is ⌘, — `ShortcutsHelpModal.tsx:24`).
- [ ] Stop suppressing ⌘F; use it for find-in-message.

### C2.2 Search that beats Gmail, offline

- [ ] Default scope is all mail across accounts; the current folder is an optional chip. Fix
      folder searches leaking other accounts' mail.
- [ ] Quoted phrases (currently collapsed to one token, `sqlite.rs:91-108`), `OR`, `-exclude`,
      `larger:`, `filename:`, `label:`; index attachment names.
- [ ] Paged results grouped by conversation, ranked with recency.
- [ ] "Search the server" for mail outside the local cache (`UID SEARCH`, `X-GM-RAW` on Gmail).
- [ ] No "No results" flash during the debounce.

### C2.3 Private by default

Already ahead of Gmail web: remote images off, per-sender trust, real-URL link confirmation.

- [ ] Block known tracking pixels and strip link-tracking parameters even when images are allowed;
      show "N trackers blocked" on the message.
- [ ] Warn when a link's visible domain differs from its target, and on lookalike sender domains
      (the small version of T0.8).
- [ ] Never send read receipts automatically.
- [ ] A one-page privacy statement: what is stored locally, what leaves the device.

### C2.4 Triage without friction

Snooze, send later, bulk selection, and undo exist.

- [ ] Archive / Delete / Star buttons for the open message in the reading-pane header; slim the
      7–9-button footer.
- [ ] Setting for what to show after archive/delete: next, previous, or back to list.
- [ ] Rules run automatically on incoming mail, and their actions go through the server queue
      (the Settings text already claims this, `RulesSection.tsx:315,375`).
- [ ] Make the `mailto:` unsubscribe path actually send; it currently reports success without
      sending (`commands.rs:1694`).

### C2.5 A composer that doesn't get in the way

- [ ] Non-blocking composer (docked panel or separate window) so you can read while writing;
      more than one draft open.
- [ ] Minimal rich text (bold, italic, link, lists, quote, inline image) producing sanitised HTML
      plus a text alternative; paste-as-plain. The full formatting work stays in T0.6.
- [ ] Spellcheck on via the system dictionary.
- [ ] Attachment reminder ignores quoted text and uses a tighter phrase list
      (`Composer.tsx:325-350`).

### C2.6 Light, and prove it

- [ ] Measure and publish per release: cold start to interactive inbox (< 1 s on an M1 with 100k
      messages), idle memory (< 200 MB), idle CPU (< 1 %), on-disk size per 100k messages, and app
      size (23 MB today vs Thunderbird's ~300 MB).
- [ ] Fail CI when a budget regresses by more than 20 %.

---

## C3 — Get it into people's hands

### C3.1 A macOS beta, end to end

- [ ] macOS first: it is the only platform that has ever been built. Windows and Linux follow once
      the beta holds.
- [ ] Replace the `YOUR-GITHUB-ORG` updater URL, set up notarisation and updater signing, and run
      `release.yml` once from a real tag.
- [ ] Remove demo data and the dev plaintext-credential path from release builds.

### C3.2 Tests where the bugs are

- [ ] A protocol harness in CI (Dovecot or GreenMail plus a capturing SMTP server in containers)
      covering: first sync, incremental sync, delete → Trash, move then flag, archive, junk,
      UIDVALIDITY reset, send-once across a simulated crash, Sent APPEND, draft round-trip.
- [ ] Vitest for `compose.ts` (quoting, reply-all recipients, HTML part at send time) and
      `keymap.ts`.
- [ ] Reduce crash surface: with `panic = "abort"` in release, any of the ~150 non-test `unwrap()`s
      (144 are `lock().unwrap()`) ends the process. Move to a non-poisoning mutex and replace
      unwraps on fallible paths.

### C3.3 Dogfood

- [ ] The author uses Quill as the only mail client for two weeks on a Gmail account and one
      non-Gmail account, logging every "had to open Gmail web" moment as a backlog item.
- [ ] Then three more people for two weeks.
- [ ] Correct the docs that currently overstate: JWZ threading (`ROADMAP.md:185-186`), connection
      limits and backoff (`docs/provider-quirks.md`), automatic rules (Settings copy).

---

## Remaining

C0.1–C0.9 are complete. The remaining work is the mail-safety and everyday-use queue:

- **C0.10:** improve threading and scale, enable notifications, and make drafts roam without
  risking silent corruption.
- **C1:** close the provider and release-trust gaps: labels, quick filters, accessibility, signing,
  connection limits, and recovery drills.
- **C2:** make the core loop fast and private: keyboard triage, all-mail offline search, tracker
  protection, frictionless compose, and measured performance budgets.
- **C3:** ship the macOS beta, add the IMAP/SMTP protocol harness and frontend tests, then dogfood
  Quill on Gmail and a non-Gmail account.

## Not now

Explicitly deferred until the switch gate passes, so they don't compete for attention:

- Calendar features beyond keeping it building. Consider a mail-only beta with the calendar behind
  a setting, since `rcalendar` churn is what currently breaks the build.
- Everything in `backlog-thunderbird.md` except where noted above (the label table from C1.2 and
  quick filters from C1.6 are its foundations): POP3, Local Folders, tags UI, profile import,
  address book / CardDAV, PGP/S/MIME, tabs, full rich-text compose.
- Windows and Linux packaging, localisation, JMAP, mobile, AI features, plugins.

## Switch gate

Quill is "worth using over Gmail web or Thunderbird" when:

- [ ] Every C0 and C1 item is done, with the C3.2 harness covering the C0 regressions.
- [ ] For each of Gmail, M365, and Fastmail: delete, archive, move, junk, star, read, send, and
      draft land on the server exactly as intended, checked in the provider's own web UI.
- [ ] All mail history is listed and searchable; every attachment in a 1,000-message sample opens.
- [ ] The C1.5 and C2.6 budgets pass on a 250k-message account.
- [ ] The C3.3 dogfood finishes with no open "went back to Gmail for X" item in C0–C2.

## Suggested order

1. **Week 1:** C0.1, then C0.2 and C0.3 together (they make it safe to connect a real account),
   then C0.7 (smallest high-impact fix).
2. **Week 2:** C0.4–C0.6 and C0.8–C0.10, alongside the C3.2 protocol harness so each fix gets its
   test.
3. **Then:** C1.1 (start Google verification on day one, since it gates on Google, not on code),
   C1.2 + C1.4 together (they share the label/thread schema), C1.5 → C1.6, and the rest of C1.
4. **Then:** C2, in whatever order dogfooding says hurts most.

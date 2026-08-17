# Quill — Thunderbird-replacement backlog

This backlog answers one question: **what has to be true before a Thunderbird user can move to
Quill and stay moved?**

It is deliberately separate from [backlog.md](backlog.md), which is the *trust* backlog — sync
correctness, security, release engineering, accessibility. That work makes Quill safe to ship.
It does not make Quill a Thunderbird replacement, because none of it addresses the features a
Thunderbird user reaches for in the first hour and finds missing. Both backlogs must land;
`backlog.md` P0 stays the higher priority when they compete, because a beautiful feature set on
top of lossy sync is worse than nothing.

[ROADMAP.md](ROADMAP.md) tracks the broader product direction and is largely orthogonal: its M3–M5
milestones are marked complete for threading, attachments, notifications, rules, and calendar
depth, none of which are the parity gaps below.

## The switching bar

A Thunderbird user has, typically: multiple accounts (IMAP **and** POP), a deep nested folder
hierarchy they built over a decade, a multi-gigabyte local archive under **Local Folders**, an
address book with hundreds of contacts, a stack of message filters, colour-coded tags, a message
list they have sorted and column-configured to taste, and — for a meaningful minority — OpenPGP
keys. They read and write HTML mail with a spellchecker running.

Quill can replace Thunderbird when that user can:

- Point Quill at their Thunderbird profile and get their mail, folder tree, address book, filters,
  and tags across in one pass, without losing the archive that never lived on a server.
- See and use their whole folder tree, per account, including nested and custom folders.
- Sort and shape the message list the way they had it, and find mail by the same means.
- Write mail that looks like the mail they write today — formatted, spellchecked, from the right
  identity.
- Keep the organisational muscle memory: tags, filters, address book, saved searches.
- Not discover, three weeks in, that some category of their mail is unreachable or unwritable.

## Priority key

- **T0 — switch blocker:** a mainstream Thunderbird user hits this within the first hour and
  cannot proceed. Quill is not a replacement until every T0 is done.
- **T1 — parity friction:** they can switch, but they notice the loss weekly and some segments
  (encryption users, POP users, enterprise) still cannot move.
- **T2 — long-tail parity:** completes the story; safe to trail the first stable release.
- Check an item only when it works against a real migrated Thunderbird profile on a release build,
  per the same evidence rule as `backlog.md`.

## Current state — what the code actually supports

Verified against the tree at `1c2a91d`, not against the status checkboxes in the other documents.

| Thunderbird capability | Quill today |
| --- | --- |
| Nested per-account folder tree | **Present (T0.1).** `folders` table + `FolderKind::Custom`; sidebar renders unified specials plus a per-account nested tree. Live 500-folder timing still open. |
| Custom folders | **Reachable (T0.1).** Unknown kinds are `Custom` and keep their server name; discovery no longer drops them onto Inbox. |
| Message list sorting / columns / grouping | **Absent.** `MessageQuery` (`types.rs:430`) is `folder / account_id / offset / limit / threaded` — no sort key, direction, or filter. No column code in `MessageList.tsx`. |
| Quick filter bar | Absent. |
| Tags / IMAP keywords | **Absent** everywhere; `MessageRow` (`types.rs:315`) has no tags, size, or priority. |
| Address book | **Absent as an entity.** Only `ContactSuggestion` (autocomplete over mail history) and `ContactGroup` (`types.rs:151`, `:163`). No contact records, vCard, CardDAV, or LDAP. |
| HTML compose + spellcheck | **Absent.** `OutgoingMessage.body_html` (`types.rs:449`) exists on the wire but nothing produces it; `Composer.tsx` is a plain `<textarea>` with no formatting or dictionary. |
| POP3 | **Absent.** The string appears only in `autodiscover.rs`; there is no POP transport. |
| Local Folders (server-less account) | Absent. Every message row requires an `account_id` bound to a real server. |
| Thunderbird profile import | **Partial and unsafe at scale.** `quill_mail::import` handles `.eml`/mbox into one chosen account+folder; `parse_mbox` (`import.rs:13`) takes `&str`, so it reads the entire archive into memory. No hierarchy, no address book, no filters, no cancel. |
| Junk filtering | Folder mapping and mark-as-junk only; no adaptive/Bayesian classifier, junk score, or scam heuristics. |
| OpenPGP / S/MIME | Absent (`backlog.md` P2 defers it as "gated on a crypto stack"). |
| NNTP, RSS/Atom feeds, chat | Absent. |
| Localisation | Absent — no string externalisation anywhere in `src/`. |

Everything else Thunderbird users depend on that Quill *does* have — multi-account IMAP/SMTP,
threading, search with operators, saved searches, rules, signatures and identities, snooze/send
later, undo, calendar with CalDAV and invitations, notifications, printing, dark mode — is real
code and should not be re-litigated here.

---

## T0 — switch blockers

### T0.1 Make the folder tree real

The single largest gap. A Thunderbird user's organisation *is* their folder tree, and Quill
currently cannot display one.

- [x] Replace the hardcoded `folders()` array with a persisted folder table keyed by account,
      carrying server name, local display name, parent, kind, hierarchy delimiter, subscribed
      state, and cached counts.
- [x] Extend `FolderKind` with a `Custom` variant and stop collapsing unknown kinds to `Inbox`.
- [x] Render a per-account, expandable, nested folder tree in the sidebar with unread/total counts
      that roll up over collapsed children, plus the existing unified special-folder section.
- [x] Persist expansion state, folder order, and per-folder view settings across restart.
- [x] Support create, rename, move, and delete of folders against IMAP, with the offline action
      queue and the same visible-failure guarantees as other queued actions.
- [x] Handle IMAP subscribe/unsubscribe and namespaces; let the user browse unsubscribed folders
      without syncing them, and reconcile with the existing `synced_folders` selection.
- [x] Add folder quick-jump (type-to-filter over the tree) and a favourites/recent-folders section;
      Thunderbird users reach these constantly through QuickFolders and the folder pane modes.
- [x] Make move/copy targets the full tree, in the drag-drop path, the context menu, the rule
      actions, and the bulk action bar.
- [ ] Verify a mailbox with 500+ folders nested five deep opens, scrolls, and syncs within the
      `backlog.md` P0.7 performance budgets.
      *(Store list + 5-deep parent walk is covered by `five_hundred_folders_list_in_one_pass`;
      a release-build UI/sync timing pass against a real 500-folder mailbox is still open.)*

**Exit:** A migrated user sees their whole hierarchy, opens any folder, and reorganises it from
Quill without touching another client.

### T0.2 Migrate a Thunderbird profile in one pass

Import today targets one account and one folder from a string-loaded mbox. A real profile is a
tree of mbox files (or Maildir), an address book, a filter set, and account preferences, often
tens of gigabytes.

- [ ] Detect Thunderbird profiles on macOS, Windows, and Linux and present the accounts, folder
      tree, and archive sizes found, with per-item opt-in.
- [ ] Stream mbox and Maildir stores from disk rather than reading them into memory; preserve the
      full folder hierarchy, read/starred/answered/forwarded flags, and original dates.
- [ ] Import the address book (`.sqlite`/`.mab`, vCard, CSV) into the address book from T0.3.
- [ ] Import `msgFilterRules.dat` into Quill rules, reporting every condition or action that has
      no equivalent instead of silently dropping it.
- [ ] Import tags and their colours (T0.4), and saved searches where the criteria map.
- [ ] Import account and identity settings from `prefs.js` as prefilled account forms — never
      silently, and never credentials.
- [ ] Give the whole import progress, pause/cancel, resumability after a crash, a written report
      of duplicates/skips/errors, and an accurate free-space precheck.
- [ ] Verify against at least one real profile of 10 GB or more and one with 100k+ messages in a
      single folder.
- [ ] Document what did not come across and why, in the user docs, before the beta opens.

**Exit:** A user runs one import, closes Thunderbird, and finds nothing they need still trapped
in the old profile.

### T0.3 Local Folders and POP3

Without these, an entire class of Thunderbird user — ISP mail, POP-only accounts, decades of
server-less archive — has nowhere to land.

- [ ] Add a server-less "Local Folders" account: full folder tree, message storage, move/copy
      to and from server accounts, search, rules, and backup coverage.
- [ ] Decouple `account_id` from "has a server" throughout the store, sync, and UI so local mail
      is never queued for a server round-trip.
- [ ] Implement POP3 retrieval with leave-on-server, delete-after-N-days, and max-size policies,
      delivering into Local Folders or an account inbox.
- [ ] Add per-account "copies and folders" settings: where sent, drafts, templates, and archive
      go, including cross-account and local targets.
- [ ] Add archive granularity (single folder, yearly, monthly) matching Thunderbird's archive
      behaviour, so the archive key does something predictable to a switcher.
- [ ] Verify local mail survives account removal, cache rebuild, backup/restore, and migration —
      it is the only mail with no server copy to recover from.

**Exit:** A POP user with a 20 GB local archive can migrate, read, search, and file mail with no
server involved, and no path in the app can delete that mail without confirmation.

### T0.4 Tags

Thunderbird's five default tags and their keyboard bindings are muscle memory; tag-driven filing
is how many users work instead of foldering.

- [ ] Add a tag model with name, colour, and ordinal; seed Thunderbird's five defaults so imports
      map cleanly.
- [ ] Round-trip tags as IMAP keywords where the server supports them, degrade to local-only with
      a visible indication where it does not, and reconcile through the offline queue.
- [ ] Bind `1`–`9` to tag toggling and `0` to clear, matching Thunderbird.
- [ ] Show tags in the message row, reading pane, and (with T0.5) as a sortable column; colour the
      row the way Thunderbird does, without relying on colour alone.
- [ ] Add `tag:` to the search operator set and to rule conditions and actions.
- [ ] Add tag management in Settings with rename/recolour/delete and a clear statement of what
      deleting a tag does to tagged mail.

**Exit:** A migrated user's tags arrive with their colours, work by keyboard, survive a round trip
through another client, and can be searched and filtered.

### T0.5 Message list as a real data grid

Thunderbird's list is a configurable table. Quill's is a fixed, date-ordered feed. This is the
second thing a switcher notices, right after the folder tree.

- [ ] Add sort key and direction to `MessageQuery` and the store queries: date, sender, recipient,
      subject, size, status, tags, attachment, priority, thread.
- [ ] Add configurable columns with per-folder persistence, reorder, and show/hide; carry size and
      priority onto `MessageRow` to support them.
- [ ] Add grouped-by-sort views (by date bucket, sender, tag) with collapsible groups.
- [ ] Add the quick filter bar: unread, starred, contact, tag, attachment toggles plus a text
      filter scoped to sender/recipient/subject/body, persistent per folder and sticky when
      switching folders.
- [ ] Keep sorting and grouping correct with threading enabled, and keep both within the
      virtualised-list performance budget on a 100k-message folder.
- [ ] Add a "select all in folder" that is honest about acting on unloaded rows, with progress and
      partial-failure reporting through the existing bulk-action path.

**Exit:** A user reproduces their Thunderbird list layout — columns, sort, grouping, quick filter —
and it persists per folder across restart.

### T0.6 Compose parity: HTML and spellcheck

- [ ] Add a rich-text composer producing sanitised HTML into the existing `body_html` field, with
      bold/italic/underline, lists, links, blockquote, headings, font and colour, horizontal rule,
      and inline images; always emit a matching `text/plain` alternative.
- [ ] Add plain-text mode as a first-class per-identity and per-message choice, with paste-as-plain
      and a reliable HTML→text degradation.
- [ ] Add spellcheck with system dictionaries, inline marking, suggestions, per-composer language
      selection, and check-before-send.
- [ ] Add reply quoting controls: quote style, reply above/below the quote, and signature placement
      relative to the quote, per identity — Thunderbird exposes all of these and users are attached
      to their choices.
- [ ] Add message templates and per-identity stationery, including the Templates folder Thunderbird
      users already have (`backlog.md` P2 lists templates as still open).
- [ ] Add "edit as new" and "resend", and keep the existing attachment-reminder guard working with
      HTML bodies.
- [ ] Verify the HTML the composer emits renders correctly in Gmail, Outlook, Apple Mail, and
      Thunderbird, and that the sanitiser applies to composed mail on the way out as well as
      received mail on the way in.

**Exit:** A user writes their normal formatted, spellchecked mail from the right identity and it
arrives looking the same as it does from Thunderbird.

### T0.7 Address book

- [ ] Add a contact entity — names, multiple addresses, phone, organisation, notes, photo, custom
      fields — with multiple address books and a personal/collected split.
- [ ] Add a contacts surface: browse, search, create, edit, merge duplicates, and delete, reachable
      from the sidebar and from any message.
- [ ] Add "add to address book" from a message header and from the composer, plus automatic
      collection of outgoing recipients with an opt-out.
- [ ] Import and export vCard 3.0/4.0 and CSV; make the T0.2 Thunderbird address-book import land
      here.
- [ ] Rank autocomplete over the address book first and mail history second, keeping the current
      offline behaviour and the existing contact groups as mailing lists.
- [ ] Add CardDAV sync (`backlog.md` P1.2 already defers this) with conflict handling and per-book
      read-only state.

**Exit:** A migrated address book is browsable, editable, exportable, and drives compose
autocomplete without a network call.

### T0.8 Junk that learns

- [ ] Add an adaptive per-account junk classifier trained by mark-as-junk / not-junk, with a
      persisted model, a junk score on the message, and a resettable training set.
- [ ] Add allow-listing from the address book, per-sender overrides, and a per-account "trust the
      server's junk headers" option that reads `X-Spam-*` and provider flags.
- [ ] Add scam/phishing heuristics in the reading pane: mismatched link text and target, lookalike
      domains, and a plain-language warning bar, consistent with the existing remote-content gate.
- [ ] Add a junk column and `is:junk` search, and make junk handling per-account configurable
      (move to Junk, mark only, delete).
- [ ] Test the classifier against a labelled corpus in CI alongside the hostile-mail fixtures from
      `backlog.md` P0.5; never let a false positive silently delete mail.

**Exit:** Junk handling improves with use and never removes mail from a user's reach without a
recoverable, visible action.

---

## T1 — parity friction

### T1.1 Filters that match Thunderbird's power

- [ ] Extend rule conditions: body, arbitrary header, age in days, size, priority, tag, junk score,
      recipient-is-me, and regular expressions.
- [ ] Extend rule actions: copy (as distinct from move), tag, set priority, mark as junk, forward,
      reply with template, and run on a target that may be a local folder.
- [ ] Add per-account filter sets with explicit run points — on incoming, after junk classification,
      on manual run, on send — and ordered execution with the existing stop-processing semantics.
- [ ] Add run-filters-on-a-selected-folder-or-selection on demand, reusing the existing preview and
      undo machinery.
- [ ] Add a filter log the user can read when a message did not land where they expected.

### T1.2 Search and virtual folders

- [ ] Add an advanced search builder (field/operator/value rows, and/or, scope across accounts and
      subfolders) alongside the existing operator syntax.
- [ ] Upgrade saved searches into true virtual folders: criteria-based rather than text-query-based,
      spanning multiple folders and accounts, appearing in the folder tree with live counts.
- [ ] Add search-within-message (`⌘F` in the reading pane) and search scoped to the current thread.
- [ ] Add `tag:`, `size:`, `priority:`, and `body:` operators as the underlying columns land.

### T1.3 Windows, tabs, and layout

- [ ] Add tabs for folders, messages, and search results — Thunderbird's primary navigation model.
- [ ] Add standalone message and compose windows (`backlog.md` P2 lists multi-window as gated on a
      Tauri multi-window surface; this is where it becomes user-visible).
- [ ] Add message-pane layout modes: classic three-pane, vertical, and wide, plus a hide-pane
      toggle and a compact/comfortable density switch.
- [ ] Add a menu bar (Windows/Linux) and complete context menus at Thunderbird's depth.

### T1.4 Per-folder storage and offline control

- [ ] Add per-folder offline-copy settings, message-age retention, and disk-space reporting.
- [ ] Add an explicit work-offline toggle and a manual "get all new messages" action with per-account
      control, matching Thunderbird's send/receive model.
- [ ] Add folder compaction or the SQLite equivalent, with reclaimed-space reporting.
- [ ] Surface IMAP quota where the server reports it.

### T1.5 Reading and message-level parity

- [ ] Add view message source, and view/save individual MIME parts.
- [ ] Add detach and delete attachment, open-with, and save-all-to-folder.
- [ ] Add return-receipt (MDN) request on send and a configurable response policy on receipt.
- [ ] Add message priority display and setting.
- [ ] Add per-folder and per-message "view as plain text / original HTML / simple HTML" switching.
- [ ] Add mark-all-read, mark-folder-read, and mark-by-date, with the existing undo behaviour.

### T1.6 OpenPGP and S/MIME — read first

Thunderbird's built-in end-to-end encryption is its flagship differentiator, and users who have it
cannot switch at all without it. `backlog.md` defers this to P2; for a Thunderbird replacement it
belongs here, and the read path must precede any compose path.

- [ ] Decrypt and verify OpenPGP and S/MIME mail with existing keys; render signature and encryption
      status honestly, including partial and failed verification.
- [ ] Add a key manager: import from a Thunderbird profile, generate, revoke, expire, and set
      per-recipient rules.
- [ ] Add compose-side signing and encryption with clear per-recipient key availability, and refuse
      to send unencrypted when the user asked for encryption.
- [ ] Add key discovery (WKD and attached keys) with an explicit trust model, and never auto-trust.
- [ ] Do not advertise encryption support until compose, key management, and recovery are all
      complete — a half-shipped key UX loses mail permanently.

### T1.7 Localisation

- [ ] Externalise UI strings and add locale-aware dates, numbers, and first-day-of-week.
- [ ] Add RTL layout support and an audit pass.
- [ ] Ship at least the languages with the largest Thunderbird user bases before claiming i18n.

---

## T2 — long-tail parity

- [ ] NNTP newsgroups — a small but immovable Thunderbird constituency; decide explicitly in or out
      and say so in the docs rather than leaving it ambiguous.
- [ ] RSS/Atom feed accounts, which Thunderbird treats as a mail account type.
- [ ] LDAP directory autocomplete for enterprise address books.
- [ ] Server-side Sieve filter editing and vacation/auto-responder, covering the most-installed
      Thunderbird add-on category.
- [ ] Import from Apple Mail, Outlook, and generic Maildir, reusing the T0.2 streaming importer.
- [ ] Customisable toolbars and a customisable keyboard map, including a Thunderbird-compatible
      preset for switchers.
- [ ] IMAP ACL and shared/delegated mailbox support.
- [ ] Add-on or scripting surface — only after the privacy and permission model in `backlog.md` P2
      exists; the IPC contract is the natural boundary.
- [ ] Theme packs beyond Hairline and Banded, once the token system is stable.

---

## Explicitly not replacing

State these in the user documentation rather than leaving switchers to discover them:

- **Chat** (Matrix/IRC/XMPP). Thunderbird bundles it; Quill will not. It is a separate product.
- **The add-on ecosystem.** Specific add-ons are replaced by built-in equivalents where they are
  common (Sieve editing, import/export, folder favourites) and not otherwise.
- **Thunderbird's profile format as a live store.** Import is one-way; Quill does not read or write
  a Thunderbird profile in place, and running both against the same store is unsupported.
- **Exact visual parity.** Layout modes and density are matched; pixel-level appearance is not.

## Replacement gate

Quill can be described publicly as a Thunderbird replacement when:

- [ ] Every T0 item is complete and exercised against a real migrated profile on a release build.
- [ ] A migration drill from a 10 GB+ Thunderbird profile with 500+ folders, an address book, and a
      filter set completes with a written, reconciled report and no unreachable mail.
- [ ] The `backlog.md` stable-release gate passes — parity on top of unproven sync is not a
      replacement.
- [ ] A Thunderbird user runs Quill as their only client for four weeks and files no "I had to go
      back to Thunderbird for X" report that maps to an open T0 or T1 item.
- [ ] Documentation states plainly what did not come across, what is not supported, and what the
      encryption, POP, and local-archive stories are.

Until the encryption path in T1.6 ships, the public claim is "a replacement for Thunderbird mail
and calendar", not "a replacement for Thunderbird" — the difference matters to the users who care
most about it.

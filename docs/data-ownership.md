# Data ownership: server vs local, and rebuild-cache semantics (P1.6)

Quill is a local-first client: most of what you see is a **local cache** of
what your providers already hold. Knowing which is which matters for backups,
account removal, and the "rebuild cache" action.

## Authoritative on the server (re-syncable — NOT in backups)

- **Mail**: messages, bodies, recipients, attachments, folder structure. These
  re-download on sync. Local flags that have not synced yet (read/unread,
  starred, deleted) are transient.
- **Accounts**: your provider accounts + their credentials (the OS keychain
  holds passwords/OAuth tokens and is never written into a backup).
- **Synced calendars**: events that came from Google/Microsoft/CalDAV.

## Local-only (would be lost without a backup)

- **Local calendar events** (no external source), calendar subscriptions,
  removed-source records.
- **Saved searches**, contact groups + members, hidden (dismissed) recipients.
- **Drafts** (unsent mail in the Drafts folder), **scheduled sends** (the
  durable Outbox), and **queued offline actions** (transient — not restored).
- **Tasks**, and all of `settings.json` (theme, widths, identities +
  signatures, rules, notifications, timezones, telemetry prefs).

A **backup** (Settings → General → Data & backup → Export backup) is a JSON
bundle of settings + the local-only rows above. It never contains credentials
or OAuth tokens. Restore re-applies it best-effort per table.

## What "rebuild cache" removes

The search index is trigger-maintained and stays fresh; "Rebuild" (Settings →
General → Search index) only repairs it. Rebuilding a **mail/calendar cache**
(messages, bodies, attachments, recipients, synced events) does not affect the
server — the next sync re-downloads the retention window. Local-only data is
not rebuilt away. Removing an account deletes its local cache and credentials
but leaves the server untouched (see `provider-quirks.md`).

## Provider quirks that affect re-sync

- **Gmail**: `[Gmail]/All Mail` is skipped to avoid duplicate bodies;
  labels-as-folders mean a message can appear in several folders.
- **Retention**: the local cache keeps a fixed retention window (7 days) and
  prunes older mail on each sync — older messages are always on the server and
  re-download only if still within the window after a rebuild.

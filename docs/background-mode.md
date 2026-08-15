# Background mode (P1.5)

Quill keeps working while the app is open, even if you never touch it:

- **Sync** — each account syncs on its cadence (every 2 min / on open) with
  IMAP IDLE push; the Rust housekeeping loop runs every 30 s.
- **Snooze** — a snoozed message returns to its folder when its wake time
  passes.
- **Send later** — due scheduled messages are flushed through SMTP.

All three run on the Rust side while the window is open. To keep them going
without the main window in the way, Quill can run **in the system tray**
(Settings → Accounts' tray icon; "Show Quill" / "New Message" / "Quit"), and
you can opt in to **launch at login** (Settings → General → Startup) so sync
and reminders start when you sign in.

There is no standalone background agent yet: if the app is fully quit, sync,
snooze return, and scheduled sends pause until the next launch. Scheduled
sends state this in the UI ("Quill must be running at the send time"). A true
background agent is a documented future item.

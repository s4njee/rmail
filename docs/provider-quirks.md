# Mail Provider Quirks & Integration Reference

This document tracks provider-specific behavior, authentication rules, IMAP/SMTP quirks, and sync edge cases for major mail providers.

---

## 1. Google (Gmail / Google Workspace)

### Authentication (OAuth 2.0 / XOAUTH2)

- **Auth Endpoint:** `https://accounts.google.com/o/oauth2/v2/auth`
- **Token Endpoint:** `https://oauth2.googleapis.com/token`
- **Scopes Required:**
  - `https://mail.google.com/` (Full IMAP/SMTP access)
  - `https://www.googleapis.com/auth/calendar` (CalDAV/Calendar integration)
  - `email`, `profile` (Identity claims)
- **SASL Mechanism:** `XOAUTH2`
  - String format: `base64("user=" + email + "\x01auth=Bearer " + access_token + "\x01\x01")`

### Quirks & Behavior

1. **Labels as Folders:**
   - In Gmail, emails do not reside in single discrete folders; labels act as virtual folders.
   - An email with 3 labels appears in 3 separate IMAP folders with different UIDs.
2. **`[Gmail]/All Mail` Duplication Policy:**
   - `[Gmail]/All Mail` contains every message across the account (except Spam and Trash).
   - _Quill Policy:_ Quill skips syncing `[Gmail]/All Mail` by default to prevent downloading duplicate message bodies and bloating the local SQLite database footprint.
3. **Drafts & Sent Sync:**
   - Sending an email via Gmail SMTP automatically creates a copy in `[Gmail]/Sent Mail`. Appending to Sent over IMAP is skipped for Gmail accounts to avoid duplicate sent items.
4. **UIDVALIDITY Stability:**
   - Gmail folder UIDVALIDITY is typically stable unless a folder/label is deleted and recreated.

---

## 2. Microsoft 365 / Outlook (Exchange Online & Outlook.com)

### Authentication (OAuth 2.0 / XOAUTH2)

- **Auth Endpoint:** `https://login.microsoftonline.com/common/oauth2/v2.0/authorize`
- **Token Endpoint:** `https://login.microsoftonline.com/common/oauth2/v2.0/token`
- **Scopes Required:**
  - `https://outlook.office.com/IMAP.AccessAsUser.All` (IMAP)
  - `https://outlook.office.com/SMTP.Send` (SMTP)
  - `offline_access`, `openid`, `profile`, `email`
- **SASL Mechanism:** `XOAUTH2`

### Quirks & Behavior

1. **Graph vs IMAP Scopes:**
   - Microsoft requires separate resource URIs for IMAP/SMTP (`https://outlook.office.com/`) vs Microsoft Graph (`https://graph.microsoft.com/`).
2. **Throttling & Rate Limits:**
   - Exchange Online enforces concurrent connection limits (maximum ~16 active IMAP connections per user) and commands-per-minute limits.
   - _Quill Policy:_ Quill maintains at most 2 persistent connections per account (1 IDLE worker + 1 transactional command worker) with exponential backoff on `NO [SERVERBUG]` or rate limiting responses.
3. **Folder Names & Localization:**
   - Outlook localized folder names (e.g. `Gelöschte Elemente` for Trash in German) are mapped using RFC 6154 `SPECIAL-USE` flags first, with heuristic name fallback.

---

## 3. General IMAP / SMTP & Custom Providers

- **Authentication:** Standard username + password over TLS / STARTTLS, stored securely in OS Keychain.
- **Port Standards:**
  - IMAP: 993 (SSL/TLS implicit) or 143 (STARTTLS)
  - SMTP: 465 (SSL/TLS implicit) or 587 (STARTTLS)
  - CalDAV: 443 (HTTPS)

---

## 4. Provider presets & autodiscovery (backlog P0.2)

The first-run flow and the add-account form carry a provider preset table
(`crates/quill-mail/src/provider.rs`). A preset wins for its known domains; the
autodiscovery pipeline (`crates/quill-mail/src/autodiscover.rs`) then probes DNS
SRV (`_imaps`/`_imap`/`_submission`/`_carddavs`), Thunderbird/Mozilla autoconfig,
and standard guesses (`imap.<domain>` etc.) for everything else. Every probe is
recorded and shown in the UI, and the manual form is always editable.

| Provider      | Auth            | IMAP / SMTP                                  | CalDAV             | App-password help in-app? |
| ------------- | --------------- | -------------------------------------------- | ------------------ | ------------------------- |
| Gmail         | OAuth (PKCE)    | imap.gmail.com / smtp.gmail.com              | — (via Google API) | Fallback help text        |
| Microsoft 365 | OAuth (PKCE)    | outlook.office365.com / smtp.office365.com   | — (via Graph API)  | —                         |
| iCloud Mail   | App password    | imap.mail.me.com / smtp.mail.me.com          | p05-caldav.icloud.com | Yes                       |
| Fastmail      | App password    | imap.fastmail.com / smtp.fastmail.com        | caldav.fastmail.com | Yes                       |
| Yahoo / AOL   | App password    | imap.mail.yahoo.com / imap.aol.com           | —                  | Yes                       |
| Zoho          | Password        | imap.zoho.com / smtp.zoho.com                | caldav.zoho.com    | Yes                       |
| Proton Mail   | Password (Bridge) | 127.0.0.1:1143 / 127.0.0.1:1025            | —                  | Bridge instructions       |

**App passwords:** iCloud, Yahoo, AOL, and Fastmail no longer accept the
regular account password for IMAP/SMTP. Quill shows provider-specific steps at
the point of failure (auth error → "create an app-specific password at …").

## 5. OAuth loopback + release credentials

- The OAuth sign-in captures the redirect on a local loopback listener
  (`http://127.0.0.1:<port>`, RFC 8252) automatically, with a paste-the-code
  fallback. The preferred port 8080 is registerable in provider consoles; an
  ephemeral port is used if it's busy.
- **Release credentials are still placeholders.** The default client IDs are
  `quill-desktop-google.apps.googleusercontent.com` /
  `quill-desktop-ms365-client-id` (see `commands.rs`). Before a public beta,
  register real Google "Desktop app" and Microsoft public-client OAuth apps,
  add `http://127.0.0.1:8080` to their redirect URIs, and either ship the
  client IDs/secrets via the gitignored `oauth-config.json` mechanism or the
  build-time env pipeline. Microsoft public clients work with PKCE and no
  secret; Google still expects its (public) client secret at the token
  endpoint — it is not a per-user credential.
- Re-authorizing an expired/revoked OAuth account ("Reconnect sign-in" in
  Settings → account edit) re-runs the flow and updates the stored tokens
  without touching local mail/calendar data.

## 6. Calendar selection for CalDAV accounts

The CalDAV engine tags events with their collection href and skips collections
the user removed (Settings → "Remove" or deselected during setup), so calendar
selection works for CalDAV accounts — not just Google. Microsoft 365 still
syncs the default calendar only.

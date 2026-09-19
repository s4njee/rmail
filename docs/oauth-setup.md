# Setting up Quill's Google and Microsoft sign-in

"Sign in with Google" and "Sign in with Microsoft" need an OAuth client that belongs to Quill,
registered once by whoever builds and publishes Quill. The client ID is compiled into the app, so
people using Quill never see or type one.

Without a client:

- **Gmail** still works: onboarding goes straight to the app-password route, which needs 2-Step
  Verification and a Gmail app password.
- **Outlook.com and Microsoft 365** don't work. Microsoft no longer accepts passwords for IMAP or
  SMTP on these accounts, so browser sign-in is the only option.
- **iCloud, Fastmail, Yahoo, and other IMAP providers** are unaffected. They use app passwords.

Both registrations are free and take about 15 minutes each.

## 1. Google

In the [Google Cloud console](https://console.cloud.google.com/):

1. Create a project, for example `Quill Mail`.
2. Enable the **Gmail API** and the **Google Calendar API** under _APIs & Services → Library_.
   Quill talks IMAP/SMTP rather than the Gmail API, but Google ties the scopes to these APIs.
3. Configure the consent screen under _Google Auth Platform_ (or _OAuth consent screen_ in older
   consoles):
   - User type: **External**.
   - App name, support email, and developer contact.
   - Branding links, served from `site/` by GitHub Pages (`.github/workflows/pages.yml`):
     - App home page: `https://s4njee.github.io/rmail/`
     - Privacy policy: `https://s4njee.github.io/rmail/privacy.html`
     - Terms of service: `https://s4njee.github.io/rmail/terms.html`
     - Authorized domain: `s4njee.github.io`
   - If Google asks you to prove you own the domain, verify it in
     [Search Console](https://search.google.com/search-console) using the HTML-file method: put
     the `google….html` file it gives you in `site/`, and `scripts/build-site.sh` publishes it.
     Search Console may insist on the domain root (`https://s4njee.github.io/`) instead of
     `/rmail/`. In that case the file has to live in a `s4njee.github.io` repository, or you move
     the site to a custom domain.
   - Data access / scopes: add `https://mail.google.com/`,
     `https://www.googleapis.com/auth/calendar`, `openid`, `email`, and `profile`.
4. Create the client under _Clients_ (or _Credentials → Create credentials → OAuth client ID_):
   - Application type: **Desktop app**.
   - No redirect URI is needed. Desktop clients accept any `http://127.0.0.1:<port>` loopback
     redirect, which is what Quill uses.
   - Copy the **client ID** and **client secret**. For desktop apps the secret isn't confidential;
     Google still requires it at the token endpoint.
5. Choose a publishing status. This matters:
   - **Testing:** only accounts listed as test users can sign in, and **refresh tokens expire
     after 7 days**. Quill would ask you to reconnect every week.
   - **In production, unverified:** anyone can sign in after clicking through a "Google hasn't
     verified this app" warning. Tokens don't expire weekly. The cap is 100 users over the app's
     lifetime. This is the right setting for personal use and a small beta.
   - **Verified:** needed to go past 100 users or remove the warning. `https://mail.google.com/`
     is a _restricted_ scope, so verification includes Google's restricted-scope review and may
     require a third-party security assessment. It takes weeks, so start early if you plan to
     distribute Quill (backlog C1.1).

Google Workspace (company or school) accounts can be blocked by their admin from using unverified
apps. That's an admin policy, not a Quill bug.

## 2. Microsoft

In the [Microsoft Entra admin center](https://entra.microsoft.com/), under _App registrations_:

1. Click **New registration**.
   - Name: `Quill Mail`.
   - Supported account types: **Accounts in any organizational directory and personal Microsoft
     accounts**. This is required for Outlook.com and for Microsoft 365 work accounts, because
     Quill uses the `common` endpoint.
   - Leave the redirect URI empty for now.
2. Add a platform under _Authentication → Add a platform_:
   - Choose **Mobile and desktop applications**, not Web. A Web platform would demand a client
     secret, and Quill is a public client with no secret.
   - Add the custom redirect URI `http://localhost`.
   - Quill redirects to `http://127.0.0.1:<port>`. For loopback addresses Microsoft ignores the
     port, but it treats `localhost` and `127.0.0.1` as different URIs. If the portal won't let you
     add `http://127.0.0.1`, open **Manifest** and add this entry to `replyUrlsWithType`:
     ```json
     { "url": "http://127.0.0.1", "type": "InstalledClient" }
     ```
3. Add permissions under _API permissions → Add a permission → Microsoft Graph → Delegated_:
   `IMAP.AccessAsUser.All`, `SMTP.Send`, `offline_access`, `openid`, `email`, and `profile`.
   Quill requests these at sign-in anyway, but listing them makes the consent screen predictable.
4. Copy the **Application (client) ID** from _Overview_. There's no secret to create.

Work and school tenants may require an admin to approve a third-party app before users can
consent, and some disable IMAP/SMTP AUTH altogether. Personal Outlook.com accounts have neither
restriction.

## 3. Put the IDs into Quill

**Development (`pnpm tauri dev`):** fill in the gitignored `oauth-config.json` in the repository
root. `oauth-config.example.json` shows the shape:

```json
{
  "providers": {
    "google": {
      "client_id": "….apps.googleusercontent.com",
      "client_secret": "GOCSPX-…"
    },
    "microsoft": { "client_id": "00000000-0000-0000-0000-000000000000" }
  }
}
```

Debug builds read this file at runtime. Restart `tauri dev` after editing it.

**Release builds:** the IDs are compiled in from environment variables. The file is ignored so a
local development client can never leak into a published build.

```bash
QUILL_GOOGLE_OAUTH_CLIENT_ID=… QUILL_GOOGLE_OAUTH_CLIENT_SECRET=… QUILL_MICROSOFT_OAUTH_CLIENT_ID=… pnpm tauri build
```

In CI, add the same three names as repository secrets. `.github/workflows/release.yml` already
passes them to the build.

To check a build, open onboarding or _Settings → Accounts → Add account_. Gmail should say
"Sign in with browser" rather than "App password", and Microsoft 365 should be enabled.

## Known limits

- Microsoft 365 **calendar** sync calls Microsoft Graph with the token issued for Outlook IMAP.
  Microsoft tokens are scoped to a single API, so calendar sync for these accounts needs a second
  token (backlog C1.1). Mail is unaffected.
- The sign-in has 90 seconds to complete in the browser before Quill offers the paste-the-code
  fallback.

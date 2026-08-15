# Release engineering (E2.1)

Packaging, signing, and notarization setup. The goal: **reproducible release
builds from CI only** — no artifacts are built on a laptop. CI is the single
source of installers.

## CI pipeline

`.github/workflows/release.yml` builds on every `v*` tag (or manual dispatch):

| Platform | Artifacts | Notes |
| -------- | --------- | ----- |
| macOS    | universal `.app` + `.dmg` | arm64 + x86_64 in one binary; signed + notarized when the Apple secrets are set |
| Windows  | MSI + NSIS | WebView2 download-bootstrapper (`tauri.conf.json` → `bundle.windows.webviewInstallMode`) |
| Linux    | AppImage + `.deb` | Flatpak manifest is a WIP — see `linux/README.md` |

It creates a **draft GitHub release** with the installers attached — add
changelog notes, then publish.

## macOS signing + notarization

The workflow signs + notarizes automatically when these repository secrets are
configured (set them in Settings → Secrets and variables → Actions):

- `APPLE_CERTIFICATE` — base64 of the **Developer ID Application** certificate
  (`.cer` + `.p12` exported together, base64-encoded).
- `APPLE_CERTIFICATE_PASSWORD` — the `.p12` password.
- `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Your Name (TEAMID)`.
- `APPLE_ID` — the Apple ID used for notarization.
- `APPLE_PASSWORD` — an app-specific password for that Apple ID.
- `APPLE_TEAM_ID` — the team ID.

Local ad-hoc builds (for verification) need no secrets: `tauri build`
signs with `-` (ad-hoc) and skips notarization. The entitlements file is at
`src-tauri/Entitlements.plist` (JIT + library-validation exceptions required by
the hardened runtime).

### Testing locally without the cert

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
pnpm tauri build --target universal-apple-darwin
# artifacts land in src-tauri/target/universal-apple-darwin/release/bundle/macos/
```

The resulting unsigned `.app` runs locally (Gatekeeper only blocks
notarization on other machines).

## Auto-update (E2.2)

The app uses `tauri-plugin-updater`. It checks `plugins.updater.endpoints`
(`src-tauri/tauri.conf.json`) for a signed manifest — the CI publishes
`latest.json` to the GitHub release (`includeUpdaterJson: true`), so the
endpoint is:

```
https://github.com/<your-github-org>/rmail/releases/latest/download/latest.json
```

Replace `<your-github-org>` in `tauri.conf.json`. The `pubkey` there matches
the signing keypair the updater artifacts are signed with.

**Generate the signing keypair** (do this once, keep the private key secret):

```sh
pnpm tauri signer generate -w ~/.quill/updater.key
# Public key → put in src-tauri/tauri.conf.json → plugins.updater.pubkey
# Private key → GitHub Actions secret `TAURI_SIGNING_PRIVATE_KEY`
# (a password, if used, goes in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`)
```

The private key signs the update bundles; without it CI produces unsigned
artifacts and auto-update is disabled for that build.

**Channels & staged rollout** are server-side: point the endpoint at a
per-channel manifest (e.g. a `beta` release) and serve the manifest to a
percentage of clients to stage a rollout. The client itself just checks the
endpoint it's configured with.

On startup the app silently checks for an update, downloads it, and shows a
"restart to apply" banner; after an upgrade it shows "What's new" with release
notes (`src/lib/updates.ts`).

## Telemetry endpoints (E2.3)

Crash reports and the usage ping are **opt-in and default off**; the client
only transmits when the matching toggle is on **and** a build-time endpoint is
set (see `docs/telemetry.md` for the exact payloads). Set these as GitHub
Actions secrets (`QUILL_CRASH_ENDPOINT`, `QUILL_USAGE_ENDPOINT`) when a
receiving service exists — CI passes them through to `tauri build`:

```sh
QUILL_CRASH_ENDPOINT=https://telemetry.example.com/v1/crash \
QUILL_USAGE_ENDPOINT=https://telemetry.example.com/v1/ping \
cargo tauri build
```

Without them, sending is disabled and crash reports queue locally only.

## Release checklist

1. Bump `version` in `Cargo.toml` (workspace), `package.json`, and
   `src-tauri/tauri.conf.json` to the same semver.
2. Push a `vX.Y.Z` tag — CI builds all platforms and drafts the release.
3. Write release notes from the changelog, attach the draft, publish.
4. If telemetry endpoints exist, confirm the `QUILL_CRASH_ENDPOINT` /
   `QUILL_USAGE_ENDPOINT` secrets are set (see "Telemetry endpoints" above).

## Artifacts

- macOS: `Quill_<ver>_universal.dmg`, `Quill.app`
- Windows: `Quill_<ver>_x64_en-US.msi`, `Quill_<ver>_x64-setup.exe`
- Linux: `quill_<ver>_amd64.deb`, `quill_<ver>_amd64.AppImage`

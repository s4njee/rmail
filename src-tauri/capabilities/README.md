# Capability permissions — rationale

Tauri v2 is **deny-by-default**: a webview can call nothing unless a
capability grants it. The single capability in this app grants only the
following. JSON cannot carry comments, so the why lives here; keep this in
sync with `capabilities/default.json`.

## `core:default`

The Tauri-maintained default set for the core plugins. For a window of a
desktop mail app each piece is deliberate:

| Group | Why it is granted |
|---|---|
| `core:app:default` | `getVersion`/`getName` for diagnostics; default is inert otherwise. |
| `core:event:default` | `emit`/`listen` — the app's push channel from Rust (sync results, connectivity, footprint — Epic 3.2). |
| `core:image:default` | Image handling for future tray/menu icons. |
| `core:menu:default` | Platform menu plumbing for the app menu. |
| `core:path:default` | Resolve the app's data/cache directories (SQLite lives there — Epic 12). |
| `core:resources:default` | Bundle-relative resource access. |
| `core:tray:default` | Tray support is not used yet; the default is inert. |
| `core:webview:default` | Webview lifecycle defaults. |
| `core:window:default` | Window lifecycle: show/hide/position/size. The frontend currently exercises none of this; keep it until the reading pane needs window control. |

## `opener:default`

The opener plugin opens `http(s)`/`mailto` URLs **in the OS browser** via the
registered URL handler — this is the mechanism behind "external links leave
the webview" (Epic 1.4). It is granted so that a link in the app can never be
navigated *inside* the webview instead.

## `opener:allow-open-path` (scoped)

Opens a **local file path** with its OS default application — how attachment
cards open their file (Epic 7.4). The path comes from the store's attachment
root under the app data dir, never from mail content. The default opener set
covers URLs but not local paths, hence this addition.

The permission is **scoped** to the attachments directory
(`$APPDATA/attachments/**`) — `open_path` without a scope entry is denied by
the plugin's own path check, so the capability grants exactly the surface the
app needs and nothing else.

## What is deliberately **not** granted

- `core:window:allow-*` for programmatic window mutation — revisit when the
  focused-reading / responsive behavior needs it (Epic 8).
- Any shell/process/fs/http permissions. The frontend reaches Rust only
  through typed `#[tauri::command]`s, which are not permission-gated but are
  the *only* IPC surface; the command surface is added in Epic 3.
- **No global Tauri object.** Tauri ≥ 2.11 no longer injects
  `window.__TAURI__` at all (the legacy `withGlobalTauri` config is gone); the
  only injection is the private `__TAURI_INTERNALS__` plumbing that
  `@tauri-apps/api` talks to. The frontend therefore reaches Rust only through
  the typed `@tauri-apps/api` wrappers — exactly the boundary the plan wants.

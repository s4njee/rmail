# Handoff: Quill — lightweight Rust/Tauri mail client

## Overview
Quill is a local-first desktop mail client (Rust + Tauri). This bundle covers the three core
views of the reading experience: a unified **Inbox** (three-pane), a **Focused reading** view,
and **Settings → Accounts**. Product emphasis is small footprint: mail is stored on device,
memory and cache size are surfaced in the UI, and an offline state is a first-class indicator
rather than an error.

Two visual treatments of the same layout are included so the team can pick one:

- **1a — Hairline.** Instrument Sans, 1px rules between rows and panes, square-ish 5px radii,
  a 3px accent rail marking the selected/unread row.
- **1b — Banded.** Public Sans, no rules; rows are rounded 10px bands with grey fills, pill
  buttons, an avatar circle, a per-row status dot, and an "On this device" card in the sidebar.

Both share the same palette, panel hierarchy (grey chrome → off-white list → white reading
pane) and the same content.

## About the Design Files
The file in this bundle is a **design reference created in HTML** — a prototype showing intended
look and structure, not production code to copy. The task is to **recreate these designs in the
target codebase's environment** using its established patterns and component libraries. For a
Tauri app that means the webview frontend framework already in use (React/Svelte/Solid/etc.); if
no frontend exists yet, pick the framework the team will maintain and implement there. Do not ship
the HTML as-is: it is a single static file with inline styles and hardcoded data, no scrolling,
no interaction, and no IPC.

## Fidelity
**High-fidelity.** Colors, type, spacing and copy are final-intent. Recreate pixel-close using the
codebase's own primitives. Exact values are listed under Design Tokens and per component below.

Not designed yet (out of scope for this bundle, flag before building): compose window, search
results and operators, first-run/account-add flow, empty states, error/auth-failure states,
threaded conversation view, context menus, dark mode.

## Screens / Views

Window size in the mock: **1280 × 800** (Inbox), **630 × 420** each for the two secondary views.
Target: cross-platform (macOS, Windows, Linux) — the mock draws a neutral 40–44px title bar with
three monochrome 9px dots as a placeholder. **Use the real platform decorations** (or Tauri's
`decorations: true`) rather than recreating that bar; treat it as "there is a title bar here,
~40px tall, hosting the app name at left and status at right".

---

### 1. Unified Inbox (three-pane)

**Purpose:** scan all accounts in one list, read the selected message in place.

**Layout:** vertical flex — title bar (40px in 1a / 44px in 1b, `flex: none`) above a horizontal
flex row that fills the rest (`min-height: 0` so panes clip rather than grow).

| Pane | Width | Background 1a | Background 1b |
|---|---|---|---|
| Sidebar | 220px (1a) / 232px (1b), fixed | `#f4f6f8`, right border `#e3e8ee` | `#f4f6f8`, no border |
| Message list | 372px (1a) / 384px (1b), fixed | `#fbfcfd`, right border `#e3e8ee` | `#fbfcfd`, right border `#e8ecf3` |
| Reading pane | fills remaining (`flex: 1; min-width: 0`) | `#fff` | `#fff` |

Window shell: `border: 1px solid #dbe1e8` (1a) / `#d8dfe8` (1b), radius **10px** (1a) / **14px**
(1b), `overflow: hidden`, shadow `0 24px 60px -28px rgba(15,23,32,0.28)`.

#### Title bar
- 1a: left three 9px `#cbd3dc` dots (gap 7px); app name "Quill" 12px/500 `#7c8896` at 12px offset;
  right side monospace 11px `#9aa5b1` reading `84 MB · 412 MB local`.
- 1b: same dots in `#c3ccdf`; name 12px/500 `#5b6a86`; right side a white pill
  (`border-radius: 20px; padding: 4px 12px`) containing a 6px `#b4451f` dot + 11px `#5b6a86`
  text `Offline · synced 11:42`.

#### Sidebar
- Wordmark "Quill": 1a 22px/700, `letter-spacing: -0.03em`, `#0f1720`, padding `0 18px 18px`.
  1b 24px/700, `#1b2740`, padding `0 20px 20px`.
- Section labels ("Unified", "Accounts"): 1a 10px/600, `letter-spacing: 0.16em`, uppercase,
  `#94a1af`. 1b 11px/700, `0.12em`, `#8593ad`.
- Folder rows — Inbox 12, Starred 4, Drafts 2, Sent, Archive (counts right-aligned,
  `font-variant-numeric: tabular-nums`, 12px `#94a1af` / `#7d8caa`):
  - 1a: `padding: 7px 18px`, 14px `#33414f`, a 5px `#b7c1cc` dot at left, gap 10px. No selected fill.
  - 1b: `margin: 1px 10px`, `padding: 9px 12px`, `border-radius: 8px`, 14px `#3d4b66`;
    **selected** (Inbox) → background `#e6eaef`, weight 600.
- Account rows: 13px `#4a5764` / `#4d5b74`, a 6px square (1a, radius 2px) or 7px circle (1b)
  in the account color, address truncated with ellipsis.
- Footer:
  - 1a: top border `#e3e8ee`, `padding: 12px 18px`, 6px `#b4451f` dot + 12px `#5c6874`
    `Offline — synced 11:42`.
  - 1b: white card `margin: auto 12px 14px; border-radius: 10px; padding: 12px 14px` —
    12px/600 `#1b2740` "On this device", then 11px `#7d8caa` `412 MB mail cache` / `84 MB memory`.

#### Message list
- Header 1a: `padding: 18px 20px 14px`, bottom border `#e3e8ee`; "Inbox" 17px/600 `#0f1720`;
  right "3 accounts" 12px `#94a1af`; search field = white, `1px solid #dbe1e8`, radius 5px,
  `padding: 7px 10px`, placeholder 13px `#9aa5b1`.
- Header 1b: `padding: 22px 22px 16px`; "Inbox" 20px/700 `#1b2740`, `letter-spacing: -0.02em`;
  search field = `#f2f4f6`, radius 9px, `padding: 9px 12px`, 13px `#8593ad`. No rule.
- Row, 1a: `padding: 13px 20px`, bottom border `#edf1f5`, gap 10px; a 3px-wide full-height rail
  (radius 2px) = accent `#3b5bdb` when selected, the account color when unread, else transparent.
- Row, 1b: `padding: 12px 14px`, `border-radius: 10px`, `margin-bottom: 2px`, list padded
  `0 10px`; an 8px dot (`margin-top: 5px`) = account color when unread, `#d3dae6` when read.
  Background: selected `#e6eaef`, unread `#f7f8fa`, read transparent.
- Row content (both): line 1 sender 14px, weight **600 unread / 400 read**, `#17222d` / `#1b2740`,
  truncated; timestamp 11px `#9aa5b1` / `#8593ad`, tabular nums. Line 2 subject 13px `#33414f` /
  `#3d4b66`, truncated. Line 3 snippet 12px `#94a1af` / `#8593ad`, truncated. Row height ≈ 72px
  (medium density — roughly 9 rows visible at 800px tall).
- The list clips at the pane bottom in the mock; in the app it scrolls (virtualize it).

#### Reading pane
- 1a: `padding: 30px 40px 20px` header with bottom border `#e3e8ee`. Subject 27px/600,
  `letter-spacing: -0.025em`, `line-height: 1.2`, `#0f1720`, `text-wrap: pretty`. Then a row:
  sender 14px/600 `#17222d`, address 13px `#94a1af`, date right-aligned 12px `#94a1af`; below it
  12px `#94a1af` `to me, David Okoye — via work@quill.app`.
- 1b: `padding: 32px 42px 22px`, no rule. Subject 29px/700, `-0.03em`, `line-height: 1.18`,
  `#1b2740`. Then a 34px circle avatar `#dfe5f7` with 13px/700 `#3b5bdb` initials "RD", sender
  14px/600 `#1b2740` with recipients 12px `#8593ad` underneath, date right 12px `#8593ad`.
- Body: 15px, line-height 1.65 (1a) / 1.7 (1b), color `#2a3642` / `#333f57`, paragraphs as a
  flex column with `gap: 15px` / `16px`, padding `28px 40px` / `0 42px`.
- Attachment card, max-width 320–330px: 30×38px file thumb using
  `repeating-linear-gradient(135deg, …)` (1a `#f4f6f8`/`#eaeef2`, 1b `#eaedf1`/`#e1e5ea`);
  filename 13px/500-600, meta 11px `#94a1af` / `#8593ad` reading `248 KB · cached locally`.
  1a: `1px solid #dbe1e8`, radius 6px, padding `11px 14px`. 1b: fill `#f2f4f6`, radius 12px,
  padding `12px 16px`, no border.
- Action bar:
  - 1a: top border `#e3e8ee`, `padding: 14px 40px`, gap 10px. **Reply** = `#3b5bdb` fill, white
    13px/500, `padding: 8px 16px`, radius 5px. **Reply all** / **Forward** = white with
    `1px solid #dbe1e8`, 13px `#33414f`. Far right: monospace 11px `#9aa5b1` hint `r · a · f`.
  - 1b: no rule, `padding: 18px 42px 22px`. **Reply** = `#3b5bdb`, white 13px/600,
    `padding: 10px 20px`, radius 24px. Others = `#f4f6f8` fill, 13px/500 `#3d4b66`, radius 24px.

---

### 2. Focused reading (single column)
**Purpose:** read one message with the list out of the way (the two-pane/zen state).

630×420 in the mock; in the app it is the same window with the list pane collapsed.
Header strip `padding: 14px 22px` (1a, bottom border `#e3e8ee`) / `15px 24px` (1b), background
`#f4f6f8`: left a back affordance `← Inbox` 12px `#7c8896` / `#5b6a86`; right an uppercase label
11–12px, `letter-spacing: 0.12–0.14em`, weight 600/700, `#94a1af` / `#8593ad`.
Body column: `padding: 30px 90px` (1a) / `32px 92px` (1b) — the wide side padding is the point,
it holds the measure near 65–70 characters. Subject 24px/600 (1a) or 25px/700 (1b); byline 13px
`#94a1af` / `#8593ad` `Rosa Delgado · Aug 13, 11:38`; body 15px/1.7–1.75.

---

### 3. Settings → Accounts
**Purpose:** see connected accounts, their sync mode and local size; add another.

Header strip identical to view 2 ("Settings" 13px/600-700 left, "Accounts" 12px right).
Body padding 22px (1a) / 20px (1b).
- 1a: rows separated by `1px solid #edf1f5`, `padding: 13px 2px`, gap 12px; 8px square dot
  (radius 2px) in the account color; address 14px/500 `#17222d`; detail 12px `#94a1af`;
  size right-aligned in monospace 11px `#7c8896`.
- 1b: each row a `#f7f8fa` card, radius 12px, `padding: 14px 16px`, `margin-bottom: 8px`;
  9px circle dot; address 14px/600 `#1b2740`; detail 12px `#8593ad`; size 12px `#5b6a86`.
- Footer: **Add account** — 1a bordered button (`1px solid #dbe1e8`, 13px `#33414f`, radius 5px,
  `padding: 7px 14px`); 1b accent pill (`#3b5bdb`, white 13px/600, radius 22px,
  `padding: 9px 18px`). Beside it, 12px `#94a1af` / `#8593ad`: `Mail is stored on this device only.`

Account rows (exact copy):
| Address | Detail | Local size | Dot |
|---|---|---|---|
| work@quill.app | IMAP · syncs every 2 min · 3 folders | 218 MB | `#3b5bdb` |
| rosa.personal@fastmail.com | IMAP · syncs on open | 141 MB | `#0f766e` |
| meridian.board@proton.me | Bridge · manual sync | 53 MB | `#b4451f` |

---

## Content in the mock
Message list, in order (sender / subject / time / unread / account index):

1. Rosa Delgado — "Draft agreement for the Meridian lease" — 11:38 — unread — 0 — **selected**
2. David Okoye — "Re: escalation clause in 4.2" — 10:52 — unread — 0
3. Fastmail — "New sign-in from Lisbon" — 09:14 — read — 1
4. Meridian Board — "Agenda — September meeting" — Yest — unread — 2
5. Priya Raman — "Photos from the weekend" — Yest — read — 1
6. Ledger — "Invoice 2841 paid" — Tue — read — 0
7. Tomás Ferreira — "Re: Thursday walkthrough" — Tue — read — 0
8. Hannah Weiss — "Quick question about the sublet language" — Mon — read — 0
9. Proton — "Bridge update available" — Mon — read — 2

Snippets and the open message body are in the HTML file; all of it is placeholder fiction —
replace with real data, don't hardcode.

## Interactions & Behavior
Not built in the mock (static screens). Intended behavior:

- **Row click** selects the message and renders it in the reading pane; selection style is the
  accent rail (1a) / `#e6eaef` band (1b). Selecting marks read → sender weight 600→400 and the
  unread dot/rail goes `#d3dae6`/transparent. Debounce the mark-read (~1s dwell) so arrow-key
  scanning doesn't burn through the unread list.
- **Row hover** should be one step lighter than selected: `#f2f4f6` fill (1b) or a `#fbfcfd`
  tint (1a). Never move layout on hover.
- **Keyboard**: `j`/`k` or ↑/↓ move selection; `Enter` opens focused reading; `Esc` returns to
  three-pane; `r` reply, `a` reply all, `f` forward (the hint text in 1a's action bar); `/` focuses
  search. Keyboard nav must scroll the list without `scrollIntoView` jumps — keep the selected row
  in view with a fixed margin.
- **Folder / account click** filters the list; the unified Inbox is the default view.
- **Offline indicator** is persistent, not a toast: dot + "Offline — synced HH:MM" (sidebar footer
  in 1a, title-bar pill in 1b). Online state: same slot, dot `#0f766e`, text `Synced HH:MM`;
  syncing: text `Syncing…`. Never block the UI on sync — the local store renders immediately.
- **Attachments** are labelled `cached locally` when present on disk; otherwise show size and a
  download affordance.
- **Transitions**: pane/selection changes should feel instantaneous (≤120ms, ease-out, opacity
  and background only). No spinners for local reads — this is the product's core claim.
- **Responsive**: below ~1100px the reading pane collapses to view 2 (list + back arrow).
  Panes should be resizable by dragging the divider; remember widths per window.

## State Management
- `accounts[]` — id, address, protocol/sync mode, color, local byte size, auth state.
- `folders[]` per account + the unified set; unread counts.
- `messages[]` for the active folder — id, accountId, from (name + address), to/cc, subject,
  snippet, receivedAt, unread, flagged, hasAttachments.
- `selectedMessageId`, `selectedFolderId`, `focusedMode` (three-pane | focused), `query`.
- `connectivity` — offline | syncing | synced + `lastSyncedAt` per account.
- `footprint` — resident memory, on-disk cache total and per account (shown in title bar and
  sidebar card; poll on an interval, not per render).
- Data comes from the local store first (render before any network); sync is a background task
  that pushes updates in. Message list should be virtualized and paged from the local DB.

## Design Tokens

Colors — shared
| Token | Value | Use |
|---|---|---|
| accent | `#3b5bdb` | primary button, selected rail, avatar glyph, links |
| accent hover | `#2f49ae` | link hover |
| account green | `#0f766e` | account 2 dot, online state |
| account rust | `#b4451f` | account 3 dot, offline dot |
| canvas (page behind windows) | `#eef1f4` | canvas only, not app chrome |

Colors — 1a Hairline
| Token | Value |
|---|---|
| chrome / sidebar | `#f4f6f8` |
| list pane | `#fbfcfd` |
| reading pane | `#fff` |
| border strong | `#dbe1e8` |
| border pane | `#e3e8ee` |
| border row | `#edf1f5` |
| text primary | `#0f1720` / `#17222d` |
| text body | `#2a3642` / `#33414f` |
| text muted | `#64748b` / `#7c8896` |
| text faint | `#94a1af` / `#9aa5b1` |
| dot idle | `#b7c1cc`, `#cbd3dc` |

Colors — 1b Banded
| Token | Value |
|---|---|
| chrome / sidebar | `#f4f6f8` |
| list pane | `#fbfcfd` |
| reading pane | `#fff` |
| selected fill | `#e6eaef` |
| unread band | `#f7f8fa` |
| subtle fill (search, attachment) | `#f2f4f6` |
| card fill (settings rows) | `#f7f8fa` |
| avatar fill | `#dfe5f7` |
| border | `#d8dfe8`, pane `#e8ecf3` |
| text primary | `#1b2740` |
| text body | `#333f57` / `#3d4b66` |
| text muted | `#5b6a86` |
| text faint | `#8593ad` / `#7d8caa` |
| dot read | `#d3dae6` |

Typography
- 1a: **Instrument Sans** (Google) 400/500/600/700. 1b: **Public Sans** (Google) 300–700.
  Fallback `system-ui, sans-serif`. Bundle the font locally in Tauri rather than hitting Google.
- Monospace accents (footprint readout, shortcut hints, sizes in 1a settings):
  `ui-monospace, 'SF Mono', Menlo, monospace`, 11px.
- Scale in use: 10/11/12/13/14/17/20/22/24/25/27/29px. Display sizes carry negative tracking
  (−0.02 to −0.03em); small uppercase labels carry +0.12–0.16em.
- Body line-height 1.65–1.75; list rows 1.3–1.4.

Spacing — 2px base; recurring steps 2/4/8/10/12/14/16/18/20/22/24/30/32/40/42px.
Radius — 1a: 2, 3, 5, 6, 10px. 1b: 8, 9, 10, 12, 14, 20–24px (pills).
Shadow — window `0 24px 60px -28px rgba(15,23,32,0.28)`; small window
`0 18px 44px -26px rgba(15,23,32,0.24)`. Both are canvas presentation only — a real OS window
gets its shadow from the compositor.

## Assets
None. No images, no icon set, no logo — the wordmark is set type, dots and rails are CSS, the
file thumbnail is a CSS repeating gradient. Pick an icon set when compose/search land (the mock
deliberately avoids icons; if you add them, 16px stroke, `#7c8896`/`#8593ad`).

## Files
- `Mail Client.dc.html` — the design. Both treatments side by side on one canvas: `#1a` (Hairline)
  and `#1b` (Banded), each with the Inbox window above and the Focused-reading + Settings windows
  below. Open it in a browser; pan/zoom to compare. Data for lists lives in the file's logic class
  (`accounts`, `folders`, `messages`) — read it there for exact copy.

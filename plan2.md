# Quill — Implementation Plan (Rust + Tauri)

Alternative to [plan.md](plan.md), which targets GPUI. This plan builds the same
`design_handoff_quill_mail` bundle on the stack the handoff actually assumed — **Rust + Tauri v2**
with a webview frontend — and uses **FullCalendar** for the calendar surfaces.

---

## 0. Context and stack decisions

**Source of truth for visuals:** `design_handoff_quill_mail/README.md` (tokens, per-component
values) and `Mail Client.dc.html` (exact copy, list data, body text). Every pixel value in the
acceptance criteria below comes from that README. The design values are identical to plan.md —
what changes is how they're realized and what becomes cheap or expensive.

### 0.1 What Tauri changes versus the GPUI plan

| | GPUI (plan.md) | Tauri (this plan) |
|---|---|---|
| Design tokens | A `Theme` struct in a global | CSS custom properties on `:root[data-theme]` — the handoff's native idiom |
| Two treatments | Rust `Treatment` enum branching in views | `data-theme` attribute swap; most differences are pure CSS |
| Calendar | Hand-built month grid + overlap layout | **FullCalendar** (MIT core) |
| HTML mail | Large open-ended problem; plain text in v1 | Sandboxed `<iframe>` + sanitizer — **v1 feature** |
| Message list virtualization | `uniform_list` | `@tanstack/solid-virtual` |
| Rust ↔ UI | Same-process calls | Tauri commands + channels; an explicit IPC boundary to design |
| Memory footprint | ~mock's claimed 84 MB is plausible | Webview baseline exceeds it — the readout is **cut** (see 0.5) |
| Cross-platform maturity | GPUI's Linux/Windows backends are the risk | WebView2/WKWebView/WebKitGTK — mature, but three engines to test |

The Rust crates carry over unchanged: `quill-store`, `quill-mail`, `quill-cal` have no UI
dependency in either plan, so the domain work is portable between them.

### 0.2 Frontend framework — decided: SolidJS

**SolidJS + TypeScript + Vite** (`vite-plugin-solid`). Fine-grained reactivity with no virtual DOM
suits this app well: the message list re-renders per-row rather than per-list, and a theme switch
updates only the nodes that actually depend on the changed signal.

State: **Solid's own `createStore` and signals — no external state library.** Zustand/Redux exist to
solve React's re-render problem, which Solid doesn't have. The Rust side pushes deltas over
channels, so the frontend is a subscriber, not a client: a channel handler writing into a Solid
store with `produce` is the whole architecture. No data-fetching framework.

Two consequences to plan around, both handled below:

- **FullCalendar has no official Solid wrapper** (React/Vue/Angular only). Use the framework-agnostic
  `@fullcalendar/core` API behind a thin local Solid component — see §0.3 and Epic 14.2.
- **Solid's reactivity has sharp edges** that a React-shaped habit will hit: destructuring props
  breaks reactivity, and `.map()` over arrays defeats the keyed reconciler. Epic 1.1 puts lint
  rules on this rather than leaving it to review.

### 0.3 Calendar library — recommendation

**FullCalendar, standard (MIT) bundle:** `@fullcalendar/core`, `daygrid`, `timegrid`, `list`,
`interaction`. It's the most battle-tested option, the MIT core covers every view this product
needs, and drag-to-create/drag-to-move — the one thing genuinely expensive to rebuild — is
included. The commercial premium plugins (resource/timeline views) are **not** needed; keep it
that way so the license stays MIT.

No `@fullcalendar/react` — with Solid, mount `new Calendar(el, options)` from
`@fullcalendar/core` inside `onMount`, drive it via its imperative API from a Solid effect, and
`destroy()` on cleanup. This is roughly 60 lines of wrapper and is what the official framework
wrappers do anyway. It also means FullCalendar sits *outside* Solid's reactive graph, which is a
feature here: the calendar owns its own DOM, and Solid owns everything around it.

**One architectural rule that matters more than the library choice:** FullCalendar is a rendering
and interaction layer only. **Recurrence expansion, timezone math, and iCal correctness stay in
Rust** (`rrule` + `icalendar` + `chrono-tz`/`jiff`). The frontend receives already-resolved event
*instances* for the visible range. Do not use `@fullcalendar/rrule` — it would put recurrence
truth in two places, and the JS expansion has DST and EXDATE edge cases the Rust side must handle
correctly anyway for sync.

Alternative considered: **Schedule-X** — MIT, modern, cleaner CSS-variable theming, and it would
fight the design less. It's younger and less proven. If FullCalendar's styling override cost
(Epic 14.2) runs over budget, it's the fallback, and the architectural rule above makes swapping
cheap since the data contract doesn't change.

### 0.4 Assumptions

- A1. **IMAP + SMTP** (`async-imap`, `lettre`) and **CalDAV** (`icalendar` + `rrule`), all in Rust.
- A2. **SQLite** is the single source of truth for the UI. Sync writes to it; the UI never waits
  on the network.
- A3. Credentials in the OS keychain (`keyring` crate) — never in config, never in SQLite, and
  **never sent over IPC to the frontend**.
- A4. Both designed treatments are light. Dark mode is out of v1 (§0.6); the CSS variable layer
  must accept it later as a data change.
- A5. **HTML mail renders in v1** in a sandboxed iframe (Epic 7.3) — confirmed (§0.6), and the
  main capability Tauri buys over the GPUI plan.
- A6. Tauri v2, with its capability/permission model configured deny-by-default.
- A7. SolidJS + TypeScript + Vite on the frontend (§0.2), with no external state library.

### 0.5 Decided: the memory figure is cut

The mock displays `84 MB · 412 MB local` in the titlebar and a "84 MB memory" line in the Banded
sidebar card. A Tauri app cannot hit 84 MB resident — WebView2/WKWebView baseline alone exceeds it
before any mail loads. **Decision: drop the memory readout entirely and show on-disk cache only.**

This is a deliberate, recorded deviation from the handoff, and it changes copy in three places:

| Surface | Mock | Ships as |
|---|---|---|
| Titlebar, Hairline | `84 MB · 412 MB local` | `412 MB local` |
| Sidebar card, Banded | `412 MB mail cache` / `84 MB memory` | `412 MB mail cache` (single line) |
| Settings → Accounts | per-account size | unchanged — on-disk, already correct |

The small-footprint product claim survives where it's actually true: mail is on the device, the
cache size is visible, and nothing about local-first rendering changes. Resident memory is still
measured, but as an internal engineering budget (Epic 16.2), not as a number shown to users.
The Banded sidebar card keeps its "On this device" heading and its layout; it loses one line —
design should confirm the card's proportions still read correctly with a single line (Epic 5.4).

### 0.6 Decisions on record

Everything the plan previously left open is now settled. Recorded here so no story re-opens them:

| # | Decision | Consequence |
|---|---|---|
| D1 | **Frontend is SolidJS** | §0.2; no FullCalendar React wrapper (§0.3), no external state library |
| D2 | **Memory figure is cut** from the UI | §0.5; on-disk cache only, three copy changes |
| D3 | **Calendar and Appearance surfaces are extrapolated by engineering** from the existing tokens, with a design-review gate | Epic 14 preamble specifies them; Epic 10.3 for Appearance |
| D4 | **The theme is global** — one app-wide setting | Epic 2.3; not per-window, not per-account, not per-section |
| D5 | **Dark mode is out of v1** | Epic 2.4 keeps the variable layer able to accept it later |
| D6 | **HTML mail ships in v1** | Epic 7.3 is committed scope, and carries the security work in 7.3 + 16.3 |

D3 is the one that adds work rather than removing it: the handoff supplies no calendar design at
all, so Epic 14's preamble derives one from the tokens. That derivation is engineering-authored
and needs design sign-off before polish — it is a defensible reading of the design system, not an
approved design.

**Out of scope for v1:** dark mode (D5), threaded conversation view, PGP/S-MIME, rules and
filters, search operators, multi-window, plugins, mobile.

---

## Epic 1 — Foundation: workspace, Tauri shell, assets

**1.1 — Repository and toolchain**
- Cargo workspace: `quill` (Tauri bin), `quill-store` (SQLite + domain types), `quill-mail`
  (IMAP/SMTP), `quill-cal` (CalDAV/iCal). Frontend in `src/` — **SolidJS + TypeScript + Vite**
  via `vite-plugin-solid`.
- AC: `cargo build` and `pnpm build` succeed from a clean checkout; `cargo tauri dev` opens a
  window with HMR working (Solid's HMR must preserve signal state across edits).
- AC: `quill-store`, `quill-mail`, `quill-cal` have **no** `tauri` dependency — enforced by CI.
  This keeps the domain layer portable (and keeps plan.md viable as a fallback).
- AC: TypeScript is `strict`; lint and format run in CI.
- AC: **`eslint-plugin-solid` is enabled and failing the build**, specifically its reactivity
  rules — no destructured props, no `.map()` where `<For>` belongs, no signal read outside a
  tracking scope. These are the mistakes a React-shaped instinct makes, they degrade silently into
  a component that renders once and never updates, and they are cheap to catch mechanically and
  expensive to catch in review.
- AC: A short `CONTRIBUTING` note records the Solid conventions in use: `createStore` + `produce`
  for domain collections, plain signals for local UI state, `<For>`/`<Show>` for control flow.

**1.2 — Window shell**
- AC: Window opens at **1280 × 800** with `decorations: true` — real platform decorations, per the
  handoff's explicit instruction.
- AC: The mock's three grey placeholder dots, window border, radius, and drop shadow are **not**
  reproduced — they are canvas presentation only (README §Shadow).
- AC: An in-app titlebar strip of 40px (Hairline) / 44px (Banded) sits below the OS decorations,
  hosting app name left and status right.
- AC: Window size and position persist across restarts.
- AC: The webview cannot be right-clicked into a browser context menu, and devtools are disabled
  in release builds.

**1.3 — Fonts and assets**
- **Instrument Sans** (400/500/600/700) and **Public Sans** (300–700) bundled locally as
  `@font-face` — the handoff is explicit that Google Fonts must not be hit at runtime.
- AC: No network request for fonts in any build; verified by loading with the network disabled.
- AC: `font-variant-numeric: tabular-nums` applies to timestamps, folder counts, and sizes.
- AC: Monospace accents resolve to `ui-monospace, 'SF Mono', Menlo, monospace` at 11px.
- AC: SIL OFL licenses vendored alongside the font files and included in the bundle.

**1.4 — Security baseline**
- AC: A strict CSP is set in `tauri.conf.json`: no remote origins, no `unsafe-eval`.
- AC: Tauri v2 capabilities are deny-by-default; each granted permission has a comment saying why.
- AC: `withGlobalTauri` is off; the frontend reaches Rust only through typed command wrappers.
- AC: External links open in the OS browser, never in the app webview.

---

## Epic 2 — Theme system (Hairline + Banded, runtime switchable)

The product requirement is that both treatments ship and are switchable in Settings. In CSS this
is far cheaper than in GPUI — but only if no component hardcodes a value.

**2.1 — Token layer**
- AC: Every value in README §Design Tokens exists as a CSS custom property on `:root`, grouped:
  colors (chrome, list, reading, borders ×3, text ×4, accent, account palette, dots, fills,
  avatar), radii, spacing, font family, type scale, and chrome metrics (titlebar height, sidebar
  and list widths).
- AC: `[data-theme="hairline"]` and `[data-theme="banded"]` are the **only** places raw hex or px
  literals appear. A lint rule (stylelint) fails the build on a hex literal in any component file.
- AC: A test asserts both variable sets against the README tables to catch drift.

**2.2 — Structural differences**
- The treatments differ structurally, not only in color: rails vs. bands, rules vs. no rules,
  squares vs. pills, sidebar footer text vs. a card, titlebar text vs. a status pill.
- AC: Differences expressible in CSS are done in CSS (e.g. the Hairline selection rail is a
  pseudo-element whose width is `var(--rail-w)`, zero in Banded).
- AC: Differences requiring different markup are branched in **one** place per component, on a
  single `useTheme()` value — never scattered through a component tree.
- AC: No component reads `data-theme` from the DOM directly.

**2.3 — Scope, switching, persistence**
- **The theme is global (D4):** a single app-wide setting. Not per-window, not per-account, and
  not per-section — Mail and Calendar always render in the same treatment.
- AC: The setting lives in one place in the Rust settings store; nothing else in the codebase can
  hold a competing theme value.
- AC: Changing the theme swaps one root attribute; no remount, no reload, no flash.
- AC: All app state survives the switch — selected message, list scroll position, folder filter,
  search query, focus, composer contents, and the calendar's current date and view.
- AC: The choice persists to the Rust side (not `localStorage`) and is applied **before first
  paint** on launch, so there is no flash of the wrong theme.
- AC: First launch defaults to Hairline; a corrupt or missing setting falls back silently.
- AC: If multi-window is ever added, every window follows the single setting — no per-window
  override is designed in, and no code path assumes one.

**2.4 — Extensibility guard (dark mode later, not now)**
- Dark mode is **out of v1 (D5)**. The guard exists so it stays a data change when it arrives.
- AC: Adding a hypothetical third theme requires only a new `[data-theme]` block — verified by
  adding a throwaway high-contrast block in a test that renders every screen without touching a
  component.
- AC: No component assumes a light background — no hardcoded `#fff`, no `color: black` fallback,
  no shadow tuned to a light surface outside the token layer. The guard test above is what proves
  it.
- AC: The iframe in Epic 7.3 inherits its background from a token, so a future dark palette does
  not leave mail bodies glowing white.

---

## Epic 3 — IPC and data contract

New in this plan: GPUI has no process boundary, Tauri does. Getting this wrong shows up later as
jank, so specify it first.

**3.1 — Command surface**
- AC: A typed command layer (`#[tauri::command]` + generated or hand-maintained TS types) covers:
  list folders, page messages, get message body, get attachment, mark read/unread, star, archive,
  delete, send, list/create/update/delete events, settings read/write.
- AC: TS types are generated from the Rust types (e.g. `ts-rs` or `specta`) — not hand-duplicated.
- AC: **No command ever returns credential material.** Covered by a test.

**3.2 — Push updates**
- AC: Sync results, connectivity changes, and footprint updates arrive over Tauri channels/events;
  the frontend never polls for them.
- AC: Event payloads are deltas, not full list dumps, so a background sync doesn't re-serialize
  the whole folder.

**3.3 — Payload budgets**
- AC: The message-list query returns only what a row renders (id, account, sender, subject,
  snippet, time, unread, flags) — bodies are fetched only on selection.
- AC: A page of 100 rows serializes and crosses IPC in < 16ms on the demo dataset.
- AC: Attachments and images are served over Tauri's asset protocol, not base64 through IPC.

**3.4 — Demo seed**
- AC: A `--demo` mode seeds the store with the exact mock content (README §Content in the mock —
  9 messages, 3 accounts, 5 folders) so every screen can be built and design-reviewed before sync
  exists.

---

## Epic 4 — Unified Inbox: three-pane shell

**4.1 — Layout**
- AC: Vertical flex — titlebar (`flex: none`) over a horizontal row filling the rest with
  `min-height: 0` so panes clip rather than grow.
- AC: Sidebar **220px** (Hairline) / **232px** (Banded), fixed; list **372px** / **384px**, fixed;
  reading pane `flex: 1; min-width: 0`.
- AC: Backgrounds — sidebar `#f4f6f8`, list `#fbfcfd`, reading `#fff`. Borders — Hairline: `#e3e8ee`
  right border on sidebar and list; Banded: none on sidebar, `#e8ecf3` on list.
- AC: No horizontal scrollbar appears at any window size; the body never scrolls.

**4.2 — Resizable dividers**
- AC: Sidebar/list dividers drag to resize with a ≥4px hit area and a resize cursor.
- AC: Min/max clamps prevent unusable widths; dragging does not select text.
- AC: Widths persist per window across restarts (handoff §Responsive).

**4.3 — Titlebar content**
- AC: Hairline — "Quill" 12px/500 `#7c8896` at 12px offset; right monospace 11px `#9aa5b1`
  showing **on-disk cache only**, `412 MB local` (D2 — the memory figure is cut).
- AC: Banded — "Quill" 12px/500 `#5b6a86`; right a white pill (radius 20px, padding `4px 12px`)
  with a 6px status dot + 11px `#5b6a86` text.
- AC: Values are live from `connectivity`/`footprint` state, never literals.

---

## Epic 5 — Sidebar

**5.1 — Wordmark and section labels**
- AC: Hairline — "Quill" 22px/700, tracking `-0.03em`, `#0f1720`, padding `0 18px 18px`; labels
  ("Unified", "Accounts") 10px/600, tracking `0.16em`, uppercase, `#94a1af`.
- AC: Banded — 24px/700, `#1b2740`, padding `0 20px 20px`; labels 11px/700, `0.12em`, `#8593ad`.

**5.2 — Folder rows**
- Inbox, Starred, Drafts, Sent, Archive; counts right-aligned and tabular.
- AC: Hairline — padding `7px 18px`, 14px `#33414f`, a 5px `#b7c1cc` dot, gap 10px, **no**
  selected fill.
- AC: Banded — margin `1px 10px`, padding `9px 12px`, radius 8px, 14px `#3d4b66`; selected fills
  `#e6eaef` at weight 600.
- AC: Counts 12px `#94a1af` / `#7d8caa`, blank when zero.
- AC: Clicking filters the list; unified Inbox is the default view; counts update live on
  read/unread changes and on sync.

**5.3 — Account rows**
- AC: 13px `#4a5764` / `#4d5b74`; a 6px square (radius 2px) or 7px circle in the account color;
  address truncates with an ellipsis, never wraps.
- AC: Clicking an account filters the list to that account.

**5.4 — Footer**
- AC: Hairline — top border `#e3e8ee`, padding `12px 18px`, 6px status dot + 12px `#5c6874`
  `Offline — synced 11:42`.
- AC: Banded — white card (`margin: auto 12px 14px`, radius 10px, padding `12px 14px`) with
  12px/600 `#1b2740` "On this device", then a **single** 11px `#7d8caa` line, `412 MB mail cache`
  (D2 — the memory line is cut; design confirms the card's proportions still read correctly).
- AC: The Banded card pins to the bottom via `margin-top: auto` regardless of folder count.

---

## Epic 6 — Message list

**6.1 — Header**
- AC: Hairline — padding `18px 20px 14px`, bottom border `#e3e8ee`; title 17px/600 `#0f1720`;
  right "N accounts" 12px `#94a1af`; search field white, `1px solid #dbe1e8`, radius 5px, padding
  `7px 10px`, placeholder 13px `#9aa5b1`.
- AC: Banded — padding `22px 22px 16px`; title 20px/700 `#1b2740`, tracking `-0.02em`; search
  field `#f2f4f6`, radius 9px, padding `9px 12px`, 13px `#8593ad`; **no** rule.
- AC: The title reflects the active folder/account filter, not a hardcoded "Inbox".

**6.2 — Row rendering**
- AC: Three lines — sender 14px (weight **600 unread / 400 read**, `#17222d` / `#1b2740`) with
  timestamp 11px tabular right-aligned; subject 13px `#33414f` / `#3d4b66`; snippet 12px
  `#94a1af` / `#8593ad`. All truncate to one line at any pane width.
- AC: Row height ≈ **72px** (≈9 rows visible at 800px tall).
- AC: Hairline — padding `13px 20px`, bottom border `#edf1f5`, gap 10px, plus a 3px full-height
  rail (radius 2px): accent `#3b5bdb` selected / account color unread / transparent read.
- AC: Banded — padding `12px 14px`, radius 10px, `margin-bottom: 2px`, list padded `0 10px`, plus
  an 8px dot (`margin-top: 5px`) in the account color when unread, `#d3dae6` when read.
  Background: selected `#e6eaef`, unread `#f7f8fa`, read transparent.

**6.3 — Virtualization**
- AC: TanStack Virtual over rows paged from SQLite; 50k messages scroll at 60fps with no blank
  frames on the design's fixed 72px row height.
- AC: DOM node count stays bounded regardless of folder size.
- AC: Scroll position is preserved when switching away from and back to a folder.

**6.4 — Selection, hover, mark-read**
- AC: Clicking a row selects it and renders it in the reading pane.
- AC: Hover is exactly one step lighter than selected — `#f2f4f6` (Banded) / a `#fbfcfd` tint
  (Hairline). Hover **never** changes layout, size, or position.
- AC: Selection marks read after ~**1s dwell**, debounced, so arrow-key scanning doesn't burn the
  unread list; leaving before 1s leaves it unread.
- AC: On mark-read, sender weight goes 600→400 and the rail/dot clears, within ≤120ms, ease-out,
  transitioning `opacity` and `background-color` only — never layout properties.

---

## Epic 7 — Reading pane

**7.1 — Header**
- AC: Hairline — padding `30px 40px 20px`, bottom border `#e3e8ee`; subject 27px/600, tracking
  `-0.025em`, line-height 1.2, `#0f1720`, `text-wrap: pretty`; then sender 14px/600 `#17222d`,
  address 13px `#94a1af`, date right 12px `#94a1af`; below, 12px `#94a1af`
  `to me, {name} — via {account}`.
- AC: Banded — padding `32px 42px 22px`, **no** rule; subject 29px/700, `-0.03em`, line-height
  1.18, `#1b2740`; a 34px circle avatar `#dfe5f7` with 13px/700 `#3b5bdb` initials; sender
  14px/600 `#1b2740`, recipients 12px `#8593ad` below, date right 12px `#8593ad`.
- AC: Avatar initials derive from the sender name (max 2 glyphs, uppercase, non-ASCII safe).

**7.2 — Plain-text body**
- AC: 15px, line-height 1.65 (Hairline) / 1.7 (Banded), `#2a3642` / `#333f57`; paragraphs in a
  flex column with gap 15px / 16px; padding `28px 40px` / `0 42px`.
- AC: The body scrolls independently; the header and action bar stay pinned.

**7.3 — HTML mail (confirmed v1 scope per D6 — the Tauri payoff, and its main risk)**
- Committed scope, not an option. It is also the app's largest attack surface: every AC below is
  a security requirement, not a nicety, and 16.3 gates the release on them.
- AC: HTML bodies are sanitized in **Rust** (server-side of the IPC boundary) before ever reaching
  the webview, then rendered in a **sandboxed iframe** (`sandbox` without `allow-same-origin` and
  without `allow-scripts`) with its own restrictive CSP.
- AC: Scripts, forms, plugins, and top-level navigation inside mail HTML are inert. A test suite
  of hostile-mail fixtures (script tags, `javascript:` URLs, CSS exfiltration, meta refresh,
  SVG-embedded script, `srcdoc` nesting) asserts none execute or phone home.
- AC: **Remote images are blocked by default** with a per-message "Load images" affordance;
  blocking is per-sender-remembered, not global-only.
- AC: Links open in the OS browser after showing the real destination; `mailto:` opens the composer.
- AC: The iframe auto-sizes to content height without a nested scrollbar, and inherits the theme's
  background so it doesn't flash white in either treatment.
- AC: Mail CSS cannot leak out of the iframe or restyle app chrome.

**7.4 — Attachment card**
- AC: Max-width 320–330px; a 30×38px thumb using `repeating-linear-gradient(135deg, …)` —
  `#f4f6f8`/`#eaeef2` (Hairline), `#eaedf1`/`#e1e5ea` (Banded); filename 13px/500–600; meta 11px
  `#94a1af` / `#8593ad` reading `{size} · cached locally`.
- AC: Hairline — `1px solid #dbe1e8`, radius 6px, padding `11px 14px`. Banded — fill `#f2f4f6`,
  radius 12px, padding `12px 16px`, no border.
- AC: When not on disk, meta shows size only plus a download affordance, flipping to
  `cached locally` once fetched.
- AC: Attachments open via the OS handler through Tauri's opener, never executed by the app.

**7.5 — Action bar**
- AC: Hairline — top border `#e3e8ee`, padding `14px 40px`, gap 10px; **Reply** `#3b5bdb`, white
  13px/500, padding `8px 16px`, radius 5px; **Reply all**/**Forward** white with `1px solid
  #dbe1e8`, 13px `#33414f`; far right monospace 11px `#9aa5b1` hint `r · a · f`.
- AC: Banded — no rule, padding `18px 42px 22px`; **Reply** `#3b5bdb`, white 13px/600, padding
  `10px 20px`, radius 24px; others `#f4f6f8`, 13px/500 `#3d4b66`, radius 24px.
- AC: The keyboard hint appears in Hairline only, per design.

**7.6 — Empty selection**
- AC: With nothing selected, a quiet placeholder in `text faint` — not blank, not an error.
  (Not designed; flag the copy for design review.)

---

## Epic 8 — Focused reading and responsive behavior

**8.1 — Focused view**
- AC: Header strip on `#f4f6f8` — padding `14px 22px` with a `#e3e8ee` bottom border (Hairline) /
  `15px 24px` no border (Banded); left `← Inbox` 12px `#7c8896` / `#5b6a86`; right an uppercase
  label 11–12px, tracking `0.12–0.14em`, weight 600/700.
- AC: Body column padding `30px 90px` / `32px 92px`, holding the measure at **65–70 characters** —
  verified at several window widths.
- AC: Subject 24px/600 / 25px/700; byline 13px faint `{Sender} · {date}`; body 15px/1.7–1.75.

**8.2 — Mode transitions**
- AC: `Enter` enters focused mode; `Esc` and `← Inbox` return to three-pane with selection and
  list scroll position intact.
- AC: ≤120ms, opacity/background only, no layout jump.

**8.3 — Responsive**
- AC: Below ~**1100px** the reading pane collapses to the focused layout; above it, three-pane
  restores.
- AC: Collapse/restore preserves selection, scroll, and query, and does not override a manual mode
  choice within the same width regime.

---

## Epic 9 — Interaction model

**9.1 — Keybindings**
- AC: `j`/`k` and ↑/↓ move selection; `Enter` opens focused reading; `Esc` returns; `r` reply,
  `a` reply all, `f` forward; `/` focuses search.
- AC: Bindings live in one keymap module, not as scattered `onKeyDown` handlers.
- AC: Keys don't fire while a text input or the composer has focus (typing `r` in search types `r`).
- AC: Browser-inherited shortcuts that make no sense in an app (find-in-page, reload, zoom via
  ctrl+scroll) are suppressed or deliberately mapped.

**9.2 — Scroll-follow**
- AC: Keyboard navigation keeps the selected row in view with a fixed margin (≥1 row above/below),
  scrolling minimally — the handoff explicitly forbids `scrollIntoView` jumps.
- AC: Holding `j` scrolls smoothly at 60fps without dropped selections.

**9.3 — Focus and accessibility**
- AC: Tab order runs sidebar → list → reading pane in visual order; the focused pane is discernible.
- AC: Panes have appropriate landmark roles; the message list is a keyboard-navigable listbox with
  correct `aria-selected` and active-descendant handling.
- AC: All text meets WCAG AA against its own background, or the exception is documented (several
  `text faint` values need an audit — record results).
- AC: `prefers-reduced-motion` drops transitions to 0ms.

---

## Epic 10 — Settings

**10.1 — Shell**
- AC: Header strip matching focused reading — "Settings" 13px/600–700 left, section name 12px
  right; body padding 22px (Hairline) / 20px (Banded).
- AC: Sections: **Accounts**, **Appearance**, **Calendar**. Appearance and Calendar have no
  designs in the bundle and are extrapolated from the tokens per **D3**; both carry a design-review
  gate before polish.
- AC: Section rows reuse the Settings → Accounts row metrics (10.2) in both treatments, so the
  extrapolated sections are visually indistinguishable in construction from the designed one.

**10.2 — Accounts**
- AC: Hairline — rows separated by `1px solid #edf1f5`, padding `13px 2px`, gap 12px; 8px square
  dot (radius 2px); address 14px/500 `#17222d`; detail 12px `#94a1af`; size right-aligned
  monospace 11px `#7c8896`.
- AC: Banded — each row a `#f7f8fa` card, radius 12px, padding `14px 16px`, `margin-bottom: 8px`;
  9px circle dot; address 14px/600 `#1b2740`; detail 12px `#8593ad`; size 12px `#5b6a86`.
- AC: Detail composed from real state (`{protocol} · {sync mode} · {n} folders`), size from the
  real on-disk total — the three mock rows must be reproducible from seed data.
- AC: Footer **Add account** — Hairline bordered button (`1px solid #dbe1e8`, 13px `#33414f`,
  radius 5px, padding `7px 14px`); Banded accent pill (`#3b5bdb`, white 13px/600, radius 22px,
  padding `9px 18px`) — beside 12px faint `Mail is stored on this device only.`

**10.3 — Appearance: the theme toggle** *(product requirement)*
- Extrapolated surface (D3): built from the Settings → Accounts row language — hairline-separated
  rows (Hairline) or `#f7f8fa` cards (Banded), one row per treatment.
- AC: A two-option control listing **Hairline** and **Banded** with a one-line description each.
- AC: The setting is presented as app-wide (D4) — no per-account or per-window affordance appears,
  since none exists.
- AC: No dark-mode or "system appearance" option is shown in v1 (D5); the section is laid out so
  adding one later doesn't restructure it.
- AC: Each option shows a live miniature preview — a message row rendered in that treatment — so
  the choice is legible without applying it.
- AC: Selecting applies **immediately** everywhere: no restart, no reload, no flash.
- AC: The choice persists via Rust settings and applies before first paint on next launch (2.3).
- AC: The control renders correctly in whichever theme is currently active.
- AC: Fully keyboard-operable (arrows move, Space/Enter selects) with correct radio semantics.

**10.4 — Account add/edit**
- Not designed. Build from tokens; flag for design review before polish.
- AC: Address, protocol (IMAP/Bridge), server/port/TLS, sync mode; a **Test connection** action
  reporting success or failure inline.
- AC: Credentials go **from the frontend straight into the Rust keychain call and are never stored
  in JS state, never logged, and never returned by any command**. A test asserts no credential
  material appears in the config file, the SQLite file, or any IPC response.
- AC: Auth failure is a persistent inline state on the account row, not a modal.
- AC: Removing an account deletes its local mail and calendar data after a confirm that names
  exactly what will be deleted.

---

## Epic 11 — Connectivity, status, footprint

**11.1 — Connectivity indicator**
- AC: Persistent, never a toast. Hairline renders it in the sidebar footer, Banded in the titlebar
  pill, from the same state.
- AC: Offline — `#b4451f` dot, `Offline — synced HH:MM`; online — `#0f766e` dot, `Synced HH:MM`;
  syncing — `Syncing…`.
- AC: State changes never block, dim, or disable the UI.

**11.2 — Footprint readout (on-disk only)**
- AC: Only on-disk cache size is shown, per D2 — titlebar in Hairline, sidebar card in Banded.
  **No resident-memory figure appears anywhere in the UI.**
- AC: The size is computed on a **timer** (≥5s) in Rust on a background thread and pushed over a
  channel — never computed per render, never on the UI thread.
- AC: Per-account on-disk size feeds Settings → Accounts from the same source.
- AC: The figure reflects real cache contents and shrinks when an account is removed or mail is
  deleted — the small-footprint claim is only credible if the number moves.

**11.3 — Local-first rendering**
- AC: The UI paints from SQLite before any network call; cold start to painted message list
  **< 500ms** on the demo dataset (a webview start is slower than GPUI's — budget it honestly and
  measure the splash-to-content gap, not just the frontend's own timing).
- AC: No spinner ever appears for a local read.
- AC: Sync runs on Tokio tasks in Rust, writes to SQLite, and pushes deltas; the UI never awaits
  the network.

---

## Epic 12 — Mail sync (Rust)

Identical to plan.md — this layer is UI-agnostic.

**12.1 — Store schema**
- AC: Tables for accounts, folders, messages, bodies, attachments, flags, sync state; indexed for
  the list query (folder + receivedAt DESC) and for search.
- AC: Migrations run on launch, forward-only, tested.

**12.2 — IMAP fetch**
- AC: Folders and envelopes sync per account on the configured cadence (every 2 min / on open /
  manual — the mock's three accounts).
- AC: Incremental via UIDVALIDITY/UIDNEXT; full refetch only when UIDVALIDITY changes.
- AC: Bodies and attachments fetch lazily on selection, cached to disk.
- AC: A failing account never stalls the others; its error surfaces on its row.

**12.3 — Outgoing (SMTP)**
- AC: Send via SMTP with the account's credentials; append to the IMAP Sent folder.
- AC: Offline sends queue in a visible outbox and retry on reconnect.

**12.4 — Message actions**
- AC: Mark read/unread, star, archive, delete propagate to the server, apply optimistically in the
  UI, and roll back visibly on failure.

---

## Epic 13 — Compose

Not designed (README lists compose as out of scope). Build from tokens; flag for design review.

**13.1 — Composer surface**
- AC: Reply / Reply all / Forward, from buttons and from `r`/`a`/`f`, open a composer pre-filled
  with correct recipients and a quoted body.
- AC: Renders correctly in both treatments — Hairline square-ish bordered fields, Banded filled
  rounded fields and pill buttons.
- AC: Rich-text editing is plain-text-first in v1; if a rich editor is added later it must produce
  sanitized HTML that survives 7.3's own sanitizer.

**13.2 — Drafts**
- AC: Autosave to the local store within 2s of typing stopping; drafts appear in the Drafts folder
  with its sidebar count.
- AC: Closing with content prompts save/discard; a crash loses at most 2s of typing.

**13.3 — Attachments**
- AC: Files attach via picker and drag-drop onto the composer, list with size, and can be removed
  before send.
- AC: Drag-drop onto any *other* part of the app is inert — the webview must not navigate to a
  dropped file.

---

## Epic 14 — Calendar (FullCalendar)

### Extrapolated visual spec (per D3)

The handoff has no calendar design. What follows derives one from the existing tokens by reusing
the design's own language rather than inventing a third one: **Hairline speaks in 1px rules and
3px accent rails; Banded speaks in rounded fills and dots.** Every value below is an existing
token or a documented derivation from one. This is engineering-authored and needs design sign-off
before polish — it is a defensible reading of the system, not an approved design.

**Header strip** — reuses the message-list header (6.1) exactly:
- Hairline: padding `18px 20px 14px`, bottom border `#e3e8ee`; month title 17px/600 `#0f1720`;
  prev/next/today are white buttons, `1px solid #dbe1e8`, radius 5px, padding `7px 10px`, 13px
  `#33414f` — the same secondary button as the reading-pane action bar.
- Banded: padding `22px 22px 16px`, no rule; month title 20px/700 `#1b2740`, tracking `-0.02em`;
  controls are `#f4f6f8` pills, radius 24px, padding `8px 14px`, 13px/500 `#3d4b66`.

**Weekday header row** — reuses the sidebar section-label style:
- Hairline 10px/600, tracking `0.16em`, uppercase, `#94a1af`. Banded 11px/700, `0.12em`, `#8593ad`.

**Month grid day cells** — min-height 96px; day number 12px tabular `#33414f` / `#3d4b66`;
adjacent-month days drop to `#94a1af` / `#8593ad`:
- Hairline: 1px `#edf1f5` grid lines — the message-row border token, so the grid reads as the same
  material as the list. No cell fill.
- Banded: no grid lines; 2px gaps between cells, cell radius 10px; hover `#f7f8fa`.
- **Today**: day number in accent `#3b5bdb` at weight 600, both treatments.
- **Selected day**: Hairline draws a 3px accent rail (radius 2px) down the cell's left edge —
  the message-list selection rail; Banded fills the cell `#e6eaef` — the message-list selection
  band. Selection language stays identical across mail and calendar, which is the point.

**Event chips** — 18px tall, 11px text, one line, ellipsis; tabular time prefix; 2px vertical gap:
- Hairline: transparent fill, 3px account-color rail at left (radius 2px), text `#33414f`,
  padding `2px 6px`, radius 3px.
- Banded: filled band, background = the account color at 12% alpha, radius 6px, padding `3px 8px`,
  text `#3d4b66`.
- Overflow renders `+N more` at 11px in text-faint, with no chip styling.
- Multi-day and all-day events span cells with square inner corners.

**Week/day grid** — hour rows 48px; hour labels in the mock's monospace accent, 11px `#9aa5b1` /
`#8593ad`, right-aligned in a 56px gutter:
- Hairline: 1px `#edf1f5` hour lines, no half-hour lines. Banded: no lines; alternating hour bands
  `#fbfcfd` / transparent.
- Now-line: 1px accent with a 6px accent dot at the gutter edge.
- Event blocks: Hairline white, `1px solid #dbe1e8`, 3px account rail, radius 5px; Banded
  account-tint fill, radius 10px, no border. Title 12px/600, time 11px faint.

**Agenda rows** — reuse the message-list row metrics verbatim (≈72px, same truncation): line 1
title 14px (600 for today, 400 otherwise) + time right-aligned tabular 11px; line 2 time range and
location 13px; line 3 attendee summary 12px faint. Hairline adds the `#edf1f5` bottom border and a
3px account rail; Banded uses radius 10px, `margin-bottom: 2px`, and an 8px account dot.

**Event detail in the reading pane** — maps 1:1 onto the message header (7.1): title takes the
subject scale (27px/600 `-0.025em` / 29px/700 `-0.03em`); the time line takes the sender line
(14px/600) with location in the address slot (13px faint); attendees take the recipients line
(12px faint); notes take the body scale (15px, line-height 1.65 / 1.7). Accept / Tentative /
Decline take the action-bar treatment — Accept as the accent primary, the other two as secondaries.

**14.1 — Data layer (Rust)**
- AC: `quill-cal` parses/serializes iCalendar (`VEVENT`, RRULE, EXDATE, all-day, VTIMEZONE) and
  syncs collections over CalDAV per account.
- AC: Events persist in SQLite; the UI reads locally first (same guarantee as 11.3).
- AC: **Recurrence is expanded in Rust**, per §0.3. A command takes a date range and returns
  resolved instances; `@fullcalendar/rrule` is not used.
- AC: Recurring events expand correctly across DST boundaries — unit tests with fixtures from at
  least two timezones, plus an all-day-across-DST case.

**14.2 — FullCalendar integration and theming**
- AC: Packages limited to the MIT standard bundle (`core`, `daygrid`, `timegrid`, `list`,
  `interaction`, `react`). A CI check fails if a premium package enters the dependency tree.
- AC: FullCalendar is styled through `--fc-*` variables mapped to the app's own tokens, plus
  scoped overrides — so **both treatments theme it**, and switching themes restyles the calendar
  with no remount and no loss of current date/view.
- AC: Its typography is overridden to the app scale — no default FullCalendar font, size, or
  border survives into the shipped UI.
- AC: The extrapolated spec above is the target. The spike reports any part of it FullCalendar
  cannot express without fighting the library (the Hairline per-cell selection rail and the
  chip rail are the likely candidates) — those go back to design as explicit trade-offs rather
  than being silently approximated.
- AC: A time-boxed spike (≤3 days) precedes this story. If overriding its styling to match both
  treatments exceeds the box, escalate and evaluate Schedule-X; the §14.1 data contract makes the
  swap cheap.
- AC: Design sign-off on the extrapolated spec (D3) happens against the spike's output, before the
  rest of Epic 14 is built.

**14.3 — Month view**
- AC: `dayGridMonth` fills the pane right of the sidebar, on the list-pane background, matching
  the day-cell, weekday-header, and chip specs above in both treatments.
- AC: Today and selected-day states render per the spec — accent day number, plus the rail
  (Hairline) or `#e6eaef` band (Banded).
- AC: Overflow uses `+N more` in text-faint; the popover it opens is themed, not FullCalendar's
  default.
- AC: Navigation via header buttons and `←`/`→`; `t` jumps to today.
- AC: Adjacent-month days are visibly de-emphasized but still legible (contrast audited with 9.3).

**14.4 — Week/day and agenda**
- AC: `timeGridWeek`/`timeGridDay` and `listWeek` are available and match the hour-grid and
  agenda-row specs above.
- AC: Agenda rows reuse message-list row metrics (≈72px, same truncation) so the two halves of the
  app feel like one product.
- AC: The now-line renders per spec and updates without re-rendering the whole grid.
- AC: Overlapping events lay out without clipping text at the design's pane widths — verified at
  the 1100px responsive breakpoint, where the calendar has least room.

**14.5 — Event detail and editing**
- AC: Selecting an event shows detail in the reading pane using the reading-pane type scale — not
  a modal.
- AC: Create, edit, delete with title, time, all-day toggle, calendar/account, location, notes;
  writes go to SQLite then sync to CalDAV.
- AC: Drag-to-move and drag-to-resize write back through IPC, applying optimistically and rolling
  back visibly on failure.
- AC: Editing one instance of a recurring series prompts for this-instance / whole-series, and the
  choice is honored in the iCal written back.
- AC: Conflicting server edits surface a resolution prompt rather than silently overwriting.

**14.6 — Navigation between Mail and Calendar**
- AC: A sidebar-level switch moves between Mail and Calendar in the same window; the sidebar keeps
  its width and treatment styling.
- AC: Keyboard reachable; the active section is unambiguous in both treatments.
- AC: Switching sections preserves each section's own state (selected message; calendar date/view).

**14.7 — Invitations in mail**
- AC: A message with a `text/calendar` part shows an invite card in the reading pane with
  Accept / Tentative / Decline.
- AC: Invite parsing happens in Rust — the sanitizer path in 7.3 must not be the thing that
  interprets calendar payloads.
- AC: Responding sends the reply email **and** updates the local calendar; the event appears in
  the calendar views immediately.

**14.8 — Calendar settings**
- AC: Settings → Calendar lists calendars per account with colors, visibility toggles, the default
  calendar for new events, and week-start day.

---

## Epic 15 — Search

**15.1 — Local search**
- AC: `/` focuses the field; typing filters against sender, subject, and body via a SQLite FTS
  index in Rust.
- AC: Results return in < 100ms on the demo dataset; the input never stutters (query debounced,
  results streamed).
- AC: Clearing restores the previous list and scroll position.
- AC: Empty results show a quiet, non-alarming empty state in both treatments.
- AC: Scope respects the active folder/account filter and says so in the header.

---

## Epic 16 — Quality, performance, security, packaging

**16.1 — Visual regression against the mock**
- AC: Screenshot tests (Playwright against the built frontend) for Inbox, Focused reading, and
  Settings → Accounts in **both** themes — 6 baselines — compared to crops of
  `Mail Client.dc.html` at the same size.
- AC: A written sign-off checklist walks the README component-by-component; deviations are recorded
  with a reason, never silently absorbed.

**16.2 — Performance budgets**
- AC: Cold start to painted list < 500ms (§11.3); idle CPU < 1%; 60fps scrolling; theme switch
  within one frame budget.
- AC: Resident memory is measured and recorded on all three platforms as an **internal engineering
  budget** — it is no longer user-facing (D2), but an unbounded regression still matters and a
  ceiling is set once real numbers exist.

**16.3 — Security review**
- AC: The hostile-mail fixture suite (7.3) runs in CI.
- AC: A review confirms: CSP has no remote origins, capabilities are minimal and justified, no
  command leaks credentials, devtools are off in release, and external links leave the webview.
- AC: `cargo audit` and an npm audit run in CI; the JS dependency count is kept deliberately small
  (every dependency ships inside a mail client that reads untrusted content).

**16.4 — Cross-platform**
- AC: Builds and runs on macOS (WKWebView), Windows (WebView2), and Linux (WebKitGTK).
- AC: **All three engines are tested per release** — WebKitGTK is the usual source of divergence
  (font rendering, flex/grid edge cases, iframe sandbox behavior). Per-platform status is recorded,
  not discovered at release.
- AC: Windows builds document the WebView2 runtime dependency and how it's satisfied.

**16.5 — Packaging**
- AC: Signed and notarized macOS `.app`/`.dmg`; Windows MSI/NSIS signed; Linux AppImage and `.deb`.
- AC: Fonts, licenses, and the app icon ship inside the bundle; no runtime downloads.
- AC: The bundle carries no source maps and no devtools payload in release.

---

## Suggested sequencing

1. **Epics 1 → 3.** Shell, theme layer, and the IPC contract. The theme system must precede any
   view — retrofitting two treatments onto hardcoded CSS is the expensive mistake available here,
   and the IPC contract must precede the list, since payload shape decides whether scrolling janks.
2. **Epics 4 → 8** against `--demo` seed data. Every screen in the bundle, both themes, no network.
   This is the design-fidelity milestone and the point for design sign-off (16.1).
3. **Epic 10.3** (the theme toggle) as soon as Epic 4 exists — the cheapest continuous check that
   Epic 2 actually holds.
4. **Epic 7.3** (HTML mail) early enough to de-risk it; it's the biggest new capability and the
   biggest new attack surface. Don't let it land in the final week.
5. **Epics 9, 10, 11** — interaction, settings, offline/footprint story.
6. **Epic 12** — real mail. First point the app is usable.
7. **Epics 13, 15** — compose and search complete "basic mail client".
8. **Epic 14** — calendar, with the 14.2 spike run *before* committing the epic's estimate.
9. **Epic 16** — continuous; the packaging stories close out.

## Open questions

None blocking. All six prior questions are settled in §0.6 (D1–D6) and folded into the epics
above. Two things still need a person rather than a ticket:

- **Design sign-off on the extrapolated calendar** (D3) — gated in 14.2, against the spike's
  output rather than a written spec. If design comes back wanting a different calendar language,
  the cost lands in 14.2's styling budget, not in the data layer.
- **Design confirmation on the Banded sidebar card** (D2) — it loses one line when the memory
  figure goes; confirm the card's proportions still read correctly (5.4).

## Risk register

| Risk | Where | Mitigation in plan |
|---|---|---|
| HTML mail is the largest attack surface in the app | 7.3 | Rust-side sanitizing, scriptless sandboxed iframe, hostile-mail fixture suite in CI (16.3) |
| FullCalendar styling fights the extrapolated spec | 14.2 | ≤3-day time-boxed spike before the estimate; Schedule-X fallback kept cheap by the §14.1 data contract |
| Solid reactivity mistakes degrade silently | 1.1 | `eslint-plugin-solid` failing the build, conventions recorded in CONTRIBUTING |
| Three webview engines diverge, WebKitGTK especially | 16.4 | All three tested per release, including iframe sandbox behavior |
| Webview cold start misses the local-first feel | 11.3 | Budget stated honestly at <500ms and measured splash-to-content, not frontend-only |

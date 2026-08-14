# Contributing to Quill

## Layout

- `src-tauri/` — the Rust crate `quill`: the Tauri shell. The **only** crate
  that may depend on `tauri` (enforced by CI).
- `crates/quill-store` — SQLite persistence + domain types. No `tauri`.
- `crates/quill-mail` — IMAP / SMTP. No `tauri`.
- `crates/quill-cal` — CalDAV / iCalendar. No `tauri`.
- `src/` — SolidJS + TypeScript frontend (`vite-plugin-solid`).

## Checks

```sh
pnpm build        # typecheck (tsc --noEmit) → lint (ESLint, fails on the
                  # Solid reactivity rules) → vite build
pnpm lint         # ESLint only
pnpm format:check # Prettier, read-only
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
./scripts/check-domain-isolation.sh  # domain crates must stay tauri-free
```

## Solid conventions (enforced by `eslint-plugin-solid`)

- **Domain collections** (messages, folders, accounts) live in a Solid
  `createStore` and are updated with `produce` — never replaced wholesale.
- **Local UI state** (selection, query, open/closed) uses plain
  `createSignal`.
- **Control flow** uses `<For>`, `<Show>`, `<Switch>` — never
  `array.map()` in JSX (`solid/prefer-for`), never `array.length && …`
  where `<Show>` belongs.
- **Props are never destructured** (`solid/no-destructure`): read `props.foo`
  to keep the accessor reactive.
- **Signals are read inside a tracking scope** (JSX, `createMemo`,
  `createEffect`) — not in a bare event handler (`solid/reactivity`).
- **No framework-state library.** Solid has no re-render problem; the Rust
  side pushes deltas and the channel handler writes into the store.

## IPC boundary

The frontend reaches Rust only through typed `#[tauri::command]` wrappers —
`withGlobalTauri` is off and `window.__TAURI__` is undefined. TS types for
commands are generated from the Rust types (Epic 3), never hand-duplicated.

## Accessibility

Contrast ratios and the documented faint-tier exception live in
[`docs/accessibility.md`](docs/accessibility.md). Keymap bindings live in one
module (`src/lib/keymap.ts`); `prefers-reduced-motion` is honored globally.

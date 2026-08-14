# rcalendar

A full-featured personal calendar app built with **Rust + Tauri** and **SolidJS + TypeScript** — designed from day one to be **embeddable** into other applications.

## What this is

rcalendar is a desktop calendar (macOS first, then Windows/Linux) with month, week, day, agenda, and year views, recurring events, reminders, drag-and-drop, search, timezones, and iCal import/export.

Its defining architectural goal is **embeddability**: the two layers beneath the desktop app are independently reusable.

| Layer                                                                              | Location               | Reusable as                                                      |
| ---------------------------------------------------------------------------------- | ---------------------- | ---------------------------------------------------------------- |
| **calendar-core** — pure-Rust engine (date math, recurrence, iCal, storage traits) | `crates/calendar-core` | a crate on crates.io, with **zero** Tauri/SQLite/UI dependencies |
| **calendar-ui** — framework-agnostic SolidJS views                                 | `packages/calendar-ui` | an npm package with **zero** `@tauri-apps/api` imports           |
| **apps/desktop** — Tauri v2 shell                                                  | `apps/desktop`         | just one consumer of the two layers above                        |

Two seams make the embeddability possible — a `Store` trait on the Rust side and a `CalendarDataSource` interface on the TypeScript side. Neither leaks Tauri or SQLite across a seam. See `plan.md` for the full architecture.

## Tech stack

- **Backend / engine:** Rust, Tauri v2, `chrono` + `chrono-tz`, `rrule`, `rusqlite`
- **Frontend:** SolidJS, TypeScript (strict), Vite, CSS custom properties (design tokens)
- **Testing:** `cargo test` / Vitest

## Getting started

### Prerequisites

- Rust stable (`rustup`)
- Node.js 22+ and pnpm (`corepack enable pnpm`)

### Install & run

```sh
# Install JS dependencies (pnpm workspace)
pnpm install

# Build the desktop app (Rust + JS)
cargo build                 # workspace: calendar-core + src-tauri
pnpm build                  # packages + apps

# Run the tests
cargo test --workspace
pnpm test

# Open the desktop app
cd apps/desktop && pnpm tauri dev
```

## Repository layout

```
rcalendar/
├── crates/calendar-core/   # pure Rust engine (no Tauri/SQLite)
├── packages/calendar-ui/   # SolidJS component library (no @tauri-apps/api)
├── apps/desktop/           # Tauri v2 shell + SolidJS app
│   └── src-tauri/          # Rust shell: commands, SQLite, tray, notifications
├── plan.md                 # implementation plan
└── scrum.md                # epics and user stories
```

## Status

Scaffold (E0) — buildable, tested skeleton with CI. See `scrum.md` for the roadmap.

## License

MIT — see [LICENSE](LICENSE).

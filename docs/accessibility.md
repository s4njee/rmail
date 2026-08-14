# Accessibility — WCAG AA audit

Last audited: 2026-08-13 (Epic 9.3).

## Contrast ratios

Computed with the WCAG relative-luminance formula against the reading pane
(`#fff`), list pane (`#fbfcfd`) and chrome (`#f4f6f8`) backgrounds. Thresholds:
AA normal text ≥ 4.5:1, AA large text / UI ≥ 3:1.

| Token                                          | Reading | List | Chrome | Verdict (normal text)        |
| ---------------------------------------------- | ------- | ---- | ------ | ---------------------------- |
| Hairline `--color-text-primary` `#0f1720`      | 18.1    | 17.6 | 16.7   | ✅ AA                        |
| Hairline `--color-text-primary-soft` `#17222d` | 16.1    | 15.7 | 14.9   | ✅ AA                        |
| Hairline `--color-text-body` `#2a3642`         | 12.3    | 12.0 | 11.4   | ✅ AA                        |
| Hairline `--color-text-body-soft` `#33414f`    | 10.5    | 10.2 | 9.6    | ✅ AA                        |
| Hairline `--color-text-muted-soft` `#64748b`   | 4.8     | 4.6  | 4.4    | ✅ AA (borderline on chrome) |
| Hairline `--color-text-muted` `#7c8896`        | 3.6     | 3.5  | 3.3    | ⚠️ large/UI only             |
| Hairline `--color-text-faint` `#94a1af`        | 2.6     | 2.6  | 2.4    | ❌ FAIL                      |
| Hairline `--color-text-faint-soft` `#9aa5b1`   | 2.5     | 2.4  | 2.3    | ❌ FAIL                      |
| Banded `--color-text-primary` `#1b2740`        | 14.9    | 14.5 | 13.7   | ✅ AA                        |
| Banded `--color-text-body` `#333f57`           | 10.6    | 10.3 | 9.7    | ✅ AA                        |
| Banded `--color-text-body-soft` `#3d4b66`      | 8.8     | 8.5  | 8.1    | ✅ AA                        |
| Banded `--color-text-muted` `#5b6a86`          | 5.5     | 5.3  | 5.0    | ✅ AA                        |
| Banded `--color-text-faint` `#8593ad`          | 3.1     | 3.0  | 2.9    | ⚠️ large/UI only             |
| Banded `--color-text-faint-soft` `#7d8caa`     | 3.4     | 3.3  | 3.1    | ⚠️ large/UI only             |

## Documented exceptions

The design's **faint** tier (snippets, addresses, dates, section labels, folder
counts, search placeholders, the `r · a · f` hint) does not meet AA for normal
text — Hairline `#94a1af` / `#9aa5b1` are ~2.5:1, Banded `#8593ad` / `#7d8caa`
are ~3:1. These are intentionally quiet, secondary text from the handoff; the
plan's §9.3 AC explicitly allows the exception once recorded.

Actions when design returns:

- Darken the faint tier toward the muted tier (≈4.5:1) for text that carries
  information (snippets, dates, addresses).
- Keep the current values only for genuinely decorative copy.
- Folder counts and unread badges sit on fills (`#e6eaef`, `#f7f8fa`) and were
  not audited against those backgrounds — check when badge colors are finalized.

## Other AA checks

- **Focus visibility**: the message list (listbox) shows a 2px selection-colored
  outline on `:focus-visible`; pane focus is discernible.
- **Reduced motion**: `prefers-reduced-motion: reduce` drops all transitions
  and animations to 0ms (global.css).
- **Landmarks**: sidebar `<aside>`, list `<section aria-label>`, reading pane
  `<section aria-label>`, titlebar `<header role=status>`.
- **Keyboard**: all actions reachable via the keymap (Epic 9.1); keys are
  ignored while a text input is focused.

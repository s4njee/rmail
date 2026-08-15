// Epic 2.1 — README §Design Tokens, transcribed as a fixture.
//
// Each entry maps a token (as defined in src/styles/tokens.css) to the value
// the handoff README lists for it. The check resolves one level of `var()`
// (so aliases like --color-online → #0f766e are covered). Values must be
// byte-identical to what tokens.css produces.
//
// Structure: scope ("root" | "hairline" | "banded") → token → README value.

export default {
  root: {
    // Colors — shared
    "--accent": "#3b5bdb",
    "--accent-hover": "#2f49ae",
    "--color-account-green": "#0f766e",
    "--color-account-rust": "#b4451f",
    "--color-account-1": "#3b5bdb",
    "--color-account-2": "#0f766e",
    "--color-account-3": "#b4451f",
    "--canvas": "#eef1f4",
    "--color-online": "#0f766e",
    "--color-offline": "#b4451f",

    // Type scale — "Scale in use: 10/11/12/13/14/17/20/22/24/25/27/29px"
    // (plus 15px, the message body size)
    "--fs-10px": "10px",
    "--fs-11px": "11px",
    "--fs-12px": "12px",
    "--fs-13px": "13px",
    "--fs-14px": "14px",
    "--fs-15px": "15px",
    "--fs-17px": "17px",
    "--fs-20px": "20px",
    "--fs-22px": "22px",
    "--fs-24px": "24px",
    "--fs-25px": "25px",
    "--fs-27px": "27px",
    "--fs-29px": "29px",

    // Spacing — "2px base; recurring steps 2/4/8/10/12/14/16/18/20/22/24/30/32/40/42px"
    "--space-2px": "2px",
    "--space-4px": "4px",
    "--space-8px": "8px",
    "--space-10px": "10px",
    "--space-12px": "12px",
    "--space-14px": "14px",
    "--space-16px": "16px",
    "--space-18px": "18px",
    "--space-20px": "20px",
    "--space-22px": "22px",
    "--space-24px": "24px",
    "--space-30px": "30px",
    "--space-32px": "32px",
    "--space-40px": "40px",
    "--space-42px": "42px",

    // Radius scale — "1a: 2, 3, 5, 6, 10px. 1b: 8, 9, 10, 12, 14, 20–24px"
    "--radius-2px": "2px",
    "--radius-3px": "3px",
    "--radius-5px": "5px",
    "--radius-6px": "6px",
    "--radius-8px": "8px",
    "--radius-9px": "9px",
    "--radius-10px": "10px",
    "--radius-12px": "12px",
    "--radius-14px": "14px",
    "--radius-20px": "20px",
    "--radius-22px": "22px",
    "--radius-24px": "24px",

    // Monospace accents — "ui-monospace, 'SF Mono', Menlo, monospace"
    "--font-mono": 'ui-monospace, "SF Mono", Menlo, monospace',
  },

  hairline: {
    // Typography
    "--font-sans": '"Instrument Sans", system-ui, sans-serif',
    "--lh-body": "1.65",

    // Chrome metrics
    "--sidebar-w": "220px",
    "--list-w": "372px",

    // Panes
    "--color-chrome": "#f4f6f8",
    "--color-list": "#fbfcfd",
    "--color-reading": "#fff",

    // Borders ×3
    "--color-border-strong": "#dbe1e8",
    "--color-border-pane": "#e3e8ee",
    "--color-border-row": "#edf1f5",

    // Text ×4 (+ -soft secondary values)
    "--color-text-primary": "#0f1720",
    "--color-text-primary-soft": "#17222d",
    "--color-text-body": "#2a3642",
    "--color-text-body-soft": "#33414f",
    "--color-text-muted": "#7c8896",
    "--color-text-muted-soft": "#64748b",
    "--color-text-faint": "#94a1af",
    "--color-text-faint-soft": "#9aa5b1",

    // Dots
    "--color-dot-idle": "#b7c1cc",
    "--color-dot-idle-strong": "#cbd3dc",
  },

  banded: {
    // Typography
    "--font-sans": '"Public Sans", system-ui, sans-serif',
    "--lh-body": "1.7",

    // Chrome metrics
    "--sidebar-w": "232px",
    "--list-w": "384px",

    // Panes
    "--color-chrome": "#f4f6f8",
    "--color-list": "#fbfcfd",
    "--color-reading": "#fff",

    // Fills
    "--color-fill-selected": "#e6eaef",
    "--color-fill-unread": "#f7f8fa",
    "--color-fill-subtle": "#f2f4f6",
    "--color-fill-card": "#f7f8fa",
    "--color-fill-avatar": "#dfe5f7",

    // Borders
    "--color-border-strong": "#d8dfe8",
    "--color-border-pane": "#e8ecf3",

    // Text ×4 (+ -soft secondary values)
    "--color-text-primary": "#1b2740",
    "--color-text-primary-soft": "#1b2740",
    "--color-text-body": "#333f57",
    "--color-text-body-soft": "#3d4b66",
    "--color-text-muted": "#5b6a86",
    "--color-text-muted-soft": "#5b6a86",
    "--color-text-faint": "#8593ad",
    "--color-text-faint-soft": "#7d8caa",

    // Dots
    "--color-dot-idle": "#d3dae6",
  },
};

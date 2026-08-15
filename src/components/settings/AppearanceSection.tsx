import { For } from "solid-js";
import { updateSettings } from "../../lib/settings";
import {
  applyDark,
  applyTheme,
  THEMES,
  useDark,
  useTheme,
  type ThemeName,
} from "../../lib/theme";
import "../Settings.css";

// Settings → Appearance (Epic 10.3): the app-wide treatment choice (D4), with
// a live miniature preview per treatment. No dark-mode / system option in v1
// (D5). Selecting applies immediately and persists via Rust settings (2.3).

const THEME_INFO: Record<ThemeName, string> = {
  hairline: "1px rules and a 3px accent rail — precise and light.",
  banded: "Rounded bands and status dots — soft and warm.",
};

// A miniature message row that renders in whichever treatment the surrounding
// `data-theme` container provides — the nested attribute re-resolves the token
// layer for its subtree, so each preview shows the other treatment correctly.
function MiniMessageRow() {
  return (
    <span class="mini-row" aria-hidden="true">
      <span class="mini-row__rail" />
      <span class="mini-row__dot" />
      <span class="mini-row__text">
        <span class="mini-row__sender">Rosa Delgado</span>
        <span class="mini-row__subject">
          Draft agreement for the Meridian lease
        </span>
      </span>
    </span>
  );
}

export function AppearanceSection() {
  const theme = useTheme();
  const dark = useDark();

  const select = (name: ThemeName) => {
    if (name === theme()) return;
    applyTheme(name);
    void updateSettings({ theme: name });
  };

  const toggleDark = (next: boolean) => {
    applyDark(next);
  };

  const onGroupKeyDown = (event: KeyboardEvent) => {
    const idx = THEMES.indexOf(theme());
    const dir =
      event.key === "ArrowDown" || event.key === "ArrowRight"
        ? 1
        : event.key === "ArrowUp" || event.key === "ArrowLeft"
          ? -1
          : 0;
    if (dir === 0) return;
    event.preventDefault();
    select(THEMES[(idx + dir + THEMES.length) % THEMES.length]);
  };

  return (
    <div
      class="appearance"
      role="radiogroup"
      aria-label="Appearance"
      tabindex="0"
      onKeyDown={onGroupKeyDown}
    >
      <For each={THEMES}>
        {(name) => (
          <button
            type="button"
            class="settings-row appearance-option"
            classList={{ "is-selected": theme() === name }}
            role="radio"
            tabindex="-1"
            aria-checked={theme() === name}
            onClick={() => select(name)}
          >
            <span class="appearance-option__preview" data-theme={name}>
              <MiniMessageRow />
            </span>
            <span class="appearance-option__text">
              <span class="appearance-option__name">{name}</span>
              <span class="appearance-option__description">
                {THEME_INFO[name]}
              </span>
            </span>
          </button>
        )}
      </For>

      {/* P1.5 dark palettes — flips the color tokens for both treatments. */}
      <div class="settings-row general-option">
        <label class="general-option__label">
          <input
            type="checkbox"
            checked={dark()}
            onChange={(e) => toggleDark(e.currentTarget.checked)}
          />
          <span class="general-option__text">
            <span class="general-option__title">Dark palette</span>
            <span class="general-option__desc">
              Applies dark colors to both treatments. HTML mail keeps its own
              colors unless it has none — then it follows the dark theme.
            </span>
          </span>
        </label>
      </div>
    </div>
  );
}

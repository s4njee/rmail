import { createSignal, For, Show } from "solid-js";
import { AccountsSection } from "./settings/AccountsSection";
import { AppearanceSection } from "./settings/AppearanceSection";
import { CalendarSection } from "./settings/CalendarSection";
import "./Settings.css";

type Section = "accounts" | "appearance" | "calendar";

const SECTIONS: { id: Section; label: string }[] = [
  { id: "accounts", label: "Accounts" },
  { id: "appearance", label: "Appearance" },
  { id: "calendar", label: "Calendar" },
];

// Settings (Epic 10.1): header strip matching focused reading ("Settings"
// left, section name right), a section tab bar, then the section body.
export function Settings() {
  const [section, setSection] = createSignal<Section>("accounts");

  return (
    <section class="settings" aria-label="Settings">
      <header class="settings-header">
        <span class="settings-title">Settings</span>
        <span class="settings-section-label">
          {SECTIONS.find((s) => s.id === section())?.label}
        </span>
      </header>

      <div
        class="settings-sections"
        role="tablist"
        aria-label="Settings sections"
      >
        <For each={SECTIONS}>
          {(s) => (
            <button
              type="button"
              class="settings-tab"
              role="tab"
              tabindex={section() === s.id ? 0 : -1}
              aria-selected={section() === s.id}
              onClick={() => setSection(s.id)}
            >
              {s.label}
            </button>
          )}
        </For>
      </div>

      <div class="settings-body">
        <Show when={section() === "accounts"}>
          <AccountsSection />
        </Show>
        <Show when={section() === "appearance"}>
          <AppearanceSection />
        </Show>
        <Show when={section() === "calendar"}>
          <CalendarSection />
        </Show>
      </div>
    </section>
  );
}

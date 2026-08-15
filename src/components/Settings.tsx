import { createEffect, createSignal, For, Show } from "solid-js";
import { type SettingsSectionId, useSettingsSection } from "../lib/ui";
import { AccountsSection } from "./settings/AccountsSection";
import { AppearanceSection } from "./settings/AppearanceSection";
import { CalendarSection } from "./settings/CalendarSection";
import { ContactGroupsSection } from "./settings/ContactGroupsSection";
import { DiagnosticsSection } from "./settings/DiagnosticsSection";
import { GeneralSection } from "./settings/GeneralSection";
import { NotificationsSection } from "./settings/NotificationsSection";
import { RulesSection } from "./settings/RulesSection";
import { SignaturesSection } from "./settings/SignaturesSection";
import "./Settings.css";

type Section = SettingsSectionId;

const SECTIONS: { id: Section; label: string }[] = [
  { id: "general", label: "General" },
  { id: "notifications", label: "Notifications" },
  { id: "accounts", label: "Accounts" },
  { id: "signatures", label: "Signatures & Aliases" },
  { id: "rules", label: "Rules & Filters" },
  { id: "appearance", label: "Appearance" },
  { id: "calendar", label: "Calendar" },
  { id: "contacts", label: "Contacts" },
  { id: "diagnostics", label: "Diagnostics" },
];

// Settings (Epic 10.1): header strip matching focused reading ("Settings"
// left, section name right), a section tab bar, then the section body.
export function Settings() {
  const [section, setSection] = createSignal<Section>("general");

  // Adopt a section requested externally (e.g. the diagnostics nudge banner
  // opens Settings at Diagnostics via openSettingsAt).
  createEffect(() => {
    setSection(useSettingsSection()());
  });

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
        <Show when={section() === "general"}>
          <GeneralSection />
        </Show>
        <Show when={section() === "notifications"}>
          <NotificationsSection />
        </Show>
        <Show when={section() === "accounts"}>
          <AccountsSection />
        </Show>
        <Show when={section() === "signatures"}>
          <SignaturesSection />
        </Show>
        <Show when={section() === "rules"}>
          <RulesSection />
        </Show>
        <Show when={section() === "appearance"}>
          <AppearanceSection />
        </Show>
        <Show when={section() === "calendar"}>
          <CalendarSection />
        </Show>
        <Show when={section() === "contacts"}>
          <ContactGroupsSection />
        </Show>
        <Show when={section() === "diagnostics"}>
          <DiagnosticsSection />
        </Show>
      </div>
    </section>
  );
}

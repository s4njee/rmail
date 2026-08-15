import { Component, createSignal, For, Show } from "solid-js";
import { Account, Calendar } from "../types/calendar";

export interface SettingsViewProps {
  accounts: { account: Account; calendars: Calendar[] }[];
  calendars: Calendar[];
  onToggleCalendar: (calendarId: string, enabled: boolean) => void;
  onSyncAccount?: (accountId: string) => Promise<void>;
  onSetSyncInterval?: (minutes: number) => Promise<void>;
  onAddAccountClick?: () => void;
  onConnectGoogleClick?: () => void;
  onClose: () => void;
}

const SETTINGS_NAV = [
  { id: "general", name: "General" },
  { id: "accounts", name: "Accounts" },
  { id: "calendars", name: "Calendars" },
  { id: "notifications", name: "Notifications" },
  { id: "appearance", name: "Appearance" },
  { id: "keyboard", name: "Keyboard" },
  { id: "advanced", name: "Advanced" },
];

export const SettingsView: Component<SettingsViewProps> = (props) => {
  const [activeNav, setActiveNav] = createSignal("accounts");
  const [syncInterval, setSyncInterval] = createSignal(15);
  const [syncingId, setSyncingId] = createSignal<string | null>(null);
  const [syncStatus, setSyncStatus] = createSignal<string>("");

  const handleSync = async (accountId: string) => {
    if (!props.onSyncAccount) return;
    setSyncingId(accountId);
    setSyncStatus("Syncing...");
    try {
      await props.onSyncAccount(accountId);
      setSyncStatus("Synced successfully.");
      setTimeout(() => setSyncStatus(""), 2000);
    } catch (e) {
      setSyncStatus(`Sync failed: ${e}`);
    } finally {
      setSyncingId(null);
    }
  };

  const handleIntervalChange = async (minutes: number) => {
    setSyncInterval(minutes);
    if (props.onSetSyncInterval) {
      await props.onSetSyncInterval(minutes);
    }
  };

  return (
    <div
      style={{
        display: "flex",
        flex: 1,
        height: "100%",
        background: "#FAFAFA",
        overflow: "hidden",
        "font-family": "var(--al-font-ui, system-ui, sans-serif)",
        color: "var(--al-ink, #1A1A1A)",
      }}
    >
      {/* Left Settings Sidebar */}
      <div
        style={{
          width: "236px",
          flex: "none",
          background: "#F4F4F4",
          "border-right": "1px solid #E0E0E0",
          padding: "20px 14px",
          display: "flex",
          "flex-direction": "column",
          gap: "3px",
        }}
      >
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            padding: "0 10px 12px",
          }}
        >
          <span
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "9.5px",
              "letter-spacing": "0.12em",
              color: "var(--al-ink-7, #A0A0A0)",
            }}
          >
            SETTINGS
          </span>
          <button
            type="button"
            onClick={props.onClose}
            style={{
              background: "none",
              border: "none",
              "font-size": "12px",
              color: "var(--al-accent, #1F6FEB)",
              cursor: "pointer",
              "font-weight": 500,
              padding: 0,
            }}
          >
            Done
          </button>
        </div>

        <For each={SETTINGS_NAV}>
          {(item) => {
            const active = () => activeNav() === item.id;
            return (
              <button
                type="button"
                onClick={() => setActiveNav(item.id)}
                style={{
                  display: "flex",
                  "align-items": "center",
                  height: "34px",
                  padding: "0 10px",
                  "border-radius": "7px",
                  "font-size": "13px",
                  background: active() ? "var(--al-surface, #FFFFFF)" : "transparent",
                  color: active() ? "var(--al-ink, #1A1A1A)" : "var(--al-ink-5, #777777)",
                  "font-weight": active() ? 600 : 400,
                  "box-shadow": active() ? "0 1px 2px rgba(0,0,0,0.05)" : "none",
                  border: "none",
                  cursor: "pointer",
                  "text-align": "left",
                  width: "100%",
                }}
              >
                {item.name}
              </button>
            );
          }}
        </For>

        <div style={{ flex: 1 }} />

        <div style={{ padding: "0 10px", display: "flex", "flex-direction": "column", gap: "3px" }}>
          <span
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "10px",
              color: "var(--al-ink-7, #A0A0A0)",
            }}
          >
            Almanac 1.4.2
          </span>
          <span
            style={{ "font-family": "var(--al-font-mono)", "font-size": "10px", color: "#BFBFBF" }}
          >
            tauri 2.4 · sqlite
          </span>
        </div>
      </div>

      {/* Main Settings Content */}
      <div
        style={{
          flex: 1,
          "min-width": 0,
          background: "#FFFFFF",
          display: "flex",
          "flex-direction": "column",
        }}
      >
        {/* Header */}
        <div style={{ padding: "30px 40px 22px", "border-bottom": "1px solid #E5E5E5" }}>
          <div
            style={{
              "font-size": "32px",
              "font-weight": 500,
              "letter-spacing": "-0.03em",
              color: "#1A1A1A",
              "line-height": 1.1,
            }}
          >
            {activeNav() === "accounts"
              ? "Accounts"
              : SETTINGS_NAV.find((n) => n.id === activeNav())?.name}
          </div>
          <div style={{ "font-size": "14px", color: "#666666", "margin-top": "6px" }}>
            Events sync in the background and stay readable offline in the local store.
          </div>
        </div>

        {/* Body */}
        <div
          style={{
            flex: 1,
            overflow: "auto",
            padding: "26px 40px",
            display: "flex",
            "flex-direction": "column",
            gap: "16px",
          }}
        >
          <Show when={activeNav() === "accounts"}>
            <For each={props.accounts}>
              {(accItem) => (
                <div
                  style={{
                    border: "1px solid #E5E5E5",
                    "border-radius": "11px",
                    overflow: "hidden",
                  }}
                >
                  {/* Account Bar */}
                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      gap: "14px",
                      padding: "16px 18px",
                      background: "#FBFBFB",
                    }}
                  >
                    <div
                      style={{
                        width: "34px",
                        height: "34px",
                        "border-radius": "9px",
                        flex: "none",
                        display: "flex",
                        "align-items": "center",
                        "justify-content": "center",
                        background: "var(--al-accent-tint, #E4EBF8)",
                        "font-family": "var(--al-font-mono)",
                        "font-size": "13px",
                        color: "var(--al-accent, #1F6FEB)",
                      }}
                    >
                      {accItem.account.displayName.charAt(0)}
                    </div>
                    <div style={{ display: "flex", "flex-direction": "column", gap: "2px" }}>
                      <span
                        style={{
                          "font-size": "14.5px",
                          "font-weight": 500,
                          "letter-spacing": "-0.01em",
                        }}
                      >
                        {accItem.account.displayName}
                      </span>
                      <span
                        style={{
                          "font-family": "var(--al-font-mono)",
                          "font-size": "10.5px",
                          color: "#888888",
                        }}
                      >
                        {accItem.account.detail}
                      </span>
                    </div>
                    <div style={{ flex: 1 }} />
                    <div style={{ display: "flex", "align-items": "center", gap: "7px" }}>
                      <div
                        style={{
                          width: "7px",
                          height: "7px",
                          "border-radius": "50%",
                          background:
                            accItem.account.status === "error"
                              ? "var(--al-cal-classes, #C2410C)"
                              : "var(--al-cal-work, #0F766E)",
                        }}
                      />
                      <span
                        style={{
                          "font-family": "var(--al-font-mono)",
                          "font-size": "10.5px",
                          color: "#888888",
                        }}
                      >
                        {accItem.account.status}
                      </span>
                    </div>
                    <button
                      type="button"
                      onClick={() => handleSync(accItem.account.id)}
                      disabled={syncingId() === accItem.account.id}
                      style={{
                        display: "flex",
                        "align-items": "center",
                        height: "30px",
                        padding: "0 12px",
                        border: "1px solid #E0E0E0",
                        "border-radius": "8px",
                        background: "#FFFFFF",
                        "font-size": "12px",
                        cursor: "pointer",
                      }}
                    >
                      {syncingId() === accItem.account.id ? "Syncing..." : "Sync now"}
                    </button>
                  </div>

                  {/* Calendars in Account */}
                  <div
                    style={{
                      padding: "14px 18px",
                      display: "flex",
                      "flex-wrap": "wrap",
                      gap: "10px",
                    }}
                  >
                    <For each={accItem.calendars}>
                      {(cal) => (
                        <div
                          style={{
                            display: "flex",
                            "align-items": "center",
                            gap: "8px",
                            height: "30px",
                            padding: "0 12px",
                            "border-radius": "8px",
                            background: cal.enabled ? `${cal.color}18` : "#F5F5F5",
                            border: `1px solid ${cal.enabled ? cal.color : "#E0E0E0"}`,
                          }}
                        >
                          <div
                            style={{
                              width: "10px",
                              height: "10px",
                              "border-radius": "3px",
                              background: cal.enabled ? cal.color : "#A0A0A0",
                            }}
                          />
                          <span
                            style={{
                              "font-size": "12.5px",
                              color: cal.enabled ? cal.color : "#888888",
                            }}
                          >
                            {cal.name}
                          </span>
                          <input
                            type="checkbox"
                            checked={cal.enabled}
                            onChange={(e) =>
                              props.onToggleCalendar(cal.id, e.currentTarget.checked)
                            }
                            style={{ cursor: "pointer", "margin-left": "4px" }}
                          />
                        </div>
                      )}
                    </For>
                  </div>
                </div>
              )}
            </For>

            {/* Add Account Actions */}
            <div style={{ display: "flex", gap: "12px" }}>
              <button
                type="button"
                onClick={props.onConnectGoogleClick}
                style={{
                  flex: 1,
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  height: "46px",
                  border: "1px solid var(--al-accent, #1F6FEB)",
                  "border-radius": "11px",
                  "font-size": "13px",
                  "font-weight": 500,
                  color: "var(--al-accent, #1F6FEB)",
                  background: "var(--al-accent-tint, #E4EBF8)",
                  gap: "8px",
                  cursor: "pointer",
                }}
              >
                <span style={{ "font-size": "15px" }}>+</span>
                <span>Connect Google Calendar</span>
              </button>

              <button
                type="button"
                onClick={props.onAddAccountClick}
                style={{
                  flex: 1,
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                  height: "46px",
                  border: "1px dashed #CACACA",
                  "border-radius": "11px",
                  "font-size": "13px",
                  color: "#777777",
                  gap: "8px",
                  background: "transparent",
                  cursor: "pointer",
                }}
              >
                <span style={{ "font-size": "15px" }}>+</span>
                <span>Import .ics / CalDAV</span>
              </button>
            </div>

            {/* Sync Cadence & Local Stats */}
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "18px",
                "padding-top": "6px",
              }}
            >
              <span
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "10px",
                  "letter-spacing": "0.08em",
                  color: "#A0A0A0",
                }}
              >
                SYNC EVERY
              </span>
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "2px",
                  padding: "3px",
                  background: "#EDEDED",
                  "border-radius": "9px",
                }}
              >
                {[5, 15, 60, 0].map((mins) => {
                  const label = mins === 0 ? "manual" : mins === 60 ? "1 hour" : `${mins} min`;
                  const active = syncInterval() === mins;
                  return (
                    <button
                      type="button"
                      onClick={() => handleIntervalChange(mins)}
                      style={{
                        padding: "5px 11px",
                        "border-radius": "6px",
                        "font-family": "var(--al-font-mono)",
                        "font-size": "11.5px",
                        background: active ? "#FFFFFF" : "transparent",
                        color: active ? "#1A1A1A" : "#777777",
                        "box-shadow": active ? "0 1px 2px rgba(0,0,0,0.10)" : "none",
                        border: "none",
                        cursor: "pointer",
                      }}
                    >
                      {label}
                    </button>
                  );
                })}
              </div>
              <div style={{ flex: 1 }} />
              <Show when={syncStatus()}>
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "11px",
                    color: "var(--al-accent, #1F6FEB)",
                  }}
                >
                  {syncStatus()}
                </span>
              </Show>
              <span
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "10.5px",
                  color: "#A0A0A0",
                }}
              >
                local store · SQLite · {props.calendars.length} calendars
              </span>
            </div>
          </Show>

          <Show when={activeNav() !== "accounts"}>
            <div
              style={{
                padding: "24px",
                "font-size": "13.5px",
                color: "#777777",
                background: "#FBFBFB",
                "border-radius": "8px",
              }}
            >
              Settings for <strong>{SETTINGS_NAV.find((n) => n.id === activeNav())?.name}</strong>{" "}
              are synchronized across devices.
            </div>
          </Show>
        </div>
      </div>
    </div>
  );
};

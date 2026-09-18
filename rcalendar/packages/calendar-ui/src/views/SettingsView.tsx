import { Component, createSignal, For, Show } from "solid-js";
import { Account, Calendar, DefaultAlerts, IdentitySettings } from "../types/calendar";
import { ALERT_PRESETS, formatAlertOffset } from "../headless/alerts";

export interface SettingsViewProps {
  accounts: { account: Account; calendars: Calendar[] }[];
  calendars: Calendar[];
  onToggleCalendar: (calendarId: string, enabled: boolean) => void;
  onSyncAccount?: (accountId: string) => Promise<void>;
  onSetSyncInterval?: (minutes: number) => Promise<void>;
  defaultAlerts?: DefaultAlerts;
  onSetDefaultAlerts?: (alerts: DefaultAlerts) => Promise<void> | void;
  identity?: IdentitySettings;
  onSetIdentity?: (identity: IdentitySettings) => Promise<void> | void;
  onAddAccountClick?: () => void;
  onConnectGoogleClick?: () => void;
  onDeleteAccount?: (accountId: string) => Promise<void> | void;
  onDeleteCalendar?: (calendarId: string) => Promise<void> | void;
  onClose: () => void;
}

type PendingDelete =
  | { kind: "account"; id: string; name: string; detail: string }
  | { kind: "calendar"; id: string; name: string; detail: string };

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
  const [pendingDelete, setPendingDelete] = createSignal<PendingDelete | null>(null);
  const [deleteBusy, setDeleteBusy] = createSignal(false);
  const [deleteError, setDeleteError] = createSignal("");

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

  const accountName = (accountId: string) =>
    props.accounts.find((a) => a.account.id === accountId)?.account.displayName || "Account";

  const confirmDelete = async () => {
    const pending = pendingDelete();
    if (!pending) return;
    setDeleteBusy(true);
    setDeleteError("");
    try {
      if (pending.kind === "account") {
        await props.onDeleteAccount?.(pending.id);
      } else {
        await props.onDeleteCalendar?.(pending.id);
      }
      setPendingDelete(null);
    } catch (e) {
      setDeleteError(`Could not remove: ${e}`);
    } finally {
      setDeleteBusy(false);
    }
  };

  const handleIntervalChange = async (minutes: number) => {
    setSyncInterval(minutes);
    if (props.onSetSyncInterval) {
      await props.onSetSyncInterval(minutes);
    }
  };

  const defaultAlertSelect = (kind: "event" | "allDay") => {
    const current = kind === "event" ? props.defaultAlerts?.event : props.defaultAlerts?.allDay;
    return (
      <select
        value={current == null ? "none" : String(current)}
        onChange={(e) => {
          const v = e.currentTarget.value;
          const next: DefaultAlerts = {
            event: props.defaultAlerts?.event ?? null,
            allDay: props.defaultAlerts?.allDay ?? null,
          };
          next[kind] = v === "none" ? null : Number(v);
          props.onSetDefaultAlerts?.(next);
        }}
        style={{
          height: "32px",
          padding: "0 10px",
          border: "1px solid var(--al-border, #E0E0E0)",
          "border-radius": "8px",
          "font-size": "12.5px",
          background: "#FFFFFF",
          width: "200px",
        }}
      >
        <For each={ALERT_PRESETS}>
          {(p) => (
            <option value={p.offsetMinutes == null ? "none" : String(p.offsetMinutes)}>
              {p.label}
            </option>
          )}
        </For>
      </select>
    );
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
                    <button
                      type="button"
                      onClick={() =>
                        setPendingDelete({
                          kind: "account",
                          id: accItem.account.id,
                          name: accItem.account.displayName,
                          detail: `${accItem.calendars.length} calendar${
                            accItem.calendars.length === 1 ? "" : "s"
                          } and their events will be removed from this Mac.`,
                        })
                      }
                      style={{
                        display: "flex",
                        "align-items": "center",
                        height: "30px",
                        padding: "0 12px",
                        border: "1px solid #E0E0E0",
                        "border-radius": "8px",
                        background: "#FFFFFF",
                        "font-size": "12px",
                        color: "#C2410C",
                        cursor: "pointer",
                      }}
                    >
                      Remove
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
                          <button
                            type="button"
                            aria-label={`Remove ${cal.name}`}
                            onClick={() =>
                              setPendingDelete({
                                kind: "calendar",
                                id: cal.id,
                                name: cal.name,
                                detail: "Events and tasks on this calendar will be removed.",
                              })
                            }
                            style={{
                              background: "none",
                              border: "none",
                              padding: "0 0 0 4px",
                              cursor: "pointer",
                              color: "#A0A0A0",
                              "font-size": "14px",
                              "line-height": 1,
                            }}
                          >
                            ×
                          </button>
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
                <For each={[5, 15, 60, 0]}>
                  {(mins) => {
                    const label = mins === 0 ? "manual" : mins === 60 ? "1 hour" : `${mins} min`;
                    const active = () => syncInterval() === mins;
                    return (
                      <button
                        type="button"
                        onClick={() => handleIntervalChange(mins)}
                        style={{
                          padding: "5px 11px",
                          "border-radius": "6px",
                          "font-family": "var(--al-font-mono)",
                          "font-size": "11.5px",
                          background: active() ? "#FFFFFF" : "transparent",
                          color: active() ? "#1A1A1A" : "#777777",
                          "box-shadow": active() ? "0 1px 2px rgba(0,0,0,0.10)" : "none",
                          border: "none",
                          cursor: "pointer",
                        }}
                      >
                        {label}
                      </button>
                    );
                  }}
                </For>
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

          <Show when={activeNav() === "calendars"}>
            <For each={props.calendars}>
              {(cal) => (
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "12px",
                    padding: "12px 16px",
                    border: "1px solid #E5E5E5",
                    "border-radius": "11px",
                  }}
                >
                  <div
                    style={{
                      width: "12px",
                      height: "12px",
                      "border-radius": "3px",
                      background: cal.color,
                      flex: "none",
                    }}
                  />
                  <div style={{ display: "flex", "flex-direction": "column", gap: "2px", flex: 1 }}>
                    <span style={{ "font-size": "14px", "font-weight": 500 }}>{cal.name}</span>
                    <span
                      style={{
                        "font-family": "var(--al-font-mono)",
                        "font-size": "10.5px",
                        color: "#888888",
                      }}
                    >
                      {accountName(cal.accountId)} · {cal.eventCount} events
                    </span>
                  </div>
                  <button
                    type="button"
                    onClick={() =>
                      setPendingDelete({
                        kind: "calendar",
                        id: cal.id,
                        name: cal.name,
                        detail: "Events and tasks on this calendar will be removed.",
                      })
                    }
                    style={{
                      height: "30px",
                      padding: "0 12px",
                      border: "1px solid #E0E0E0",
                      "border-radius": "8px",
                      background: "#FFFFFF",
                      "font-size": "12px",
                      color: "#C2410C",
                      cursor: "pointer",
                    }}
                  >
                    Remove
                  </button>
                </div>
              )}
            </For>
            <Show when={props.calendars.length === 0}>
              <div style={{ "font-size": "13.5px", color: "#777777" }}>
                No calendars yet. Add an account to create one.
              </div>
            </Show>
          </Show>

          <Show when={activeNav() === "notifications"}>
            <div
              style={{
                display: "flex",
                "flex-direction": "column",
                gap: "22px",
                "max-width": "560px",
              }}
            >
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "16px",
                }}
              >
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "10px",
                    "letter-spacing": "0.08em",
                    color: "#A0A0A0",
                    width: "120px",
                    flex: "none",
                  }}
                >
                  EVENTS
                </span>
                {defaultAlertSelect("event")}
              </div>

              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "16px",
                }}
              >
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "10px",
                    "letter-spacing": "0.08em",
                    color: "#A0A0A0",
                    width: "120px",
                    flex: "none",
                  }}
                >
                  ALL-DAY EVENTS
                </span>
                {defaultAlertSelect("allDay")}
              </div>

              <div
                style={{
                  "font-size": "12.5px",
                  color: "#777777",
                  background: "#FBFBFB",
                  "border-radius": "8px",
                  padding: "12px 14px",
                  "line-height": 1.5,
                }}
              >
                New events default to{" "}
                <strong>{formatAlertOffset(props.defaultAlerts?.event)}</strong> for timed events
                and <strong>{formatAlertOffset(props.defaultAlerts?.allDay)}</strong> for all-day
                events. You can still add or remove alerts per event in the editor.
              </div>
            </div>
          </Show>

          <Show when={activeNav() === "general"}>
            <div
              style={{
                border: "1px solid #E5E5E5",
                "border-radius": "11px",
                padding: "18px 20px",
                display: "flex",
                "flex-direction": "column",
                gap: "16px",
              }}
            >
              <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "10px",
                    "letter-spacing": "0.08em",
                    color: "#A0A0A0",
                    width: "120px",
                    flex: "none",
                  }}
                >
                  YOUR EMAIL
                </span>
                <input
                  type="email"
                  placeholder="you@example.com"
                  value={props.identity?.selfEmail ?? ""}
                  onChange={(e) =>
                    props.onSetIdentity?.({
                      selfEmail: e.currentTarget.value.trim() || null,
                      showDeclined: props.identity?.showDeclined ?? false,
                    })
                  }
                  style={{
                    height: "32px",
                    padding: "0 10px",
                    border: "1px solid #E0E0E0",
                    "border-radius": "8px",
                    "font-size": "12.5px",
                    flex: 1,
                  }}
                />
              </div>
              <label
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "10px",
                  "font-size": "13px",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={props.identity?.showDeclined ?? false}
                  onChange={(e) =>
                    props.onSetIdentity?.({
                      selfEmail: props.identity?.selfEmail ?? null,
                      showDeclined: e.currentTarget.checked,
                    })
                  }
                />
                Show declined events
              </label>
              <div style={{ "font-size": "12.5px", color: "#777777", "line-height": 1.5 }}>
                Events you have declined stay hidden unless this is on. Your email is matched
                against attendee addresses.
              </div>
            </div>
          </Show>

          <Show
            when={
              activeNav() !== "accounts" &&
              activeNav() !== "calendars" &&
              activeNav() !== "notifications" &&
              activeNav() !== "general"
            }
          >
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

      <Show when={pendingDelete()}>
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(0,0,0,0.34)",
            "z-index": 120,
            display: "flex",
            "align-items": "center",
            "justify-content": "center",
          }}
          onClick={() => !deleteBusy() && setPendingDelete(null)}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            style={{
              width: "420px",
              background: "#FFFFFF",
              "border-radius": "14px",
              padding: "22px 24px 18px",
              "box-shadow": "0 24px 60px -18px rgba(0,0,0,0.34)",
            }}
          >
            <div style={{ "font-size": "17px", "font-weight": 600, "margin-bottom": "8px" }}>
              Remove {pendingDelete()?.kind === "account" ? "account" : "calendar"}?
            </div>
            <div style={{ "font-size": "13.5px", color: "#575757", "line-height": 1.45 }}>
              <strong>{pendingDelete()?.name}</strong> — {pendingDelete()?.detail} This only affects
              Almanac’s local store.
            </div>
            <Show when={deleteError()}>
              <div style={{ "font-size": "12.5px", color: "#C2410C", "margin-top": "10px" }}>
                {deleteError()}
              </div>
            </Show>
            <div
              style={{
                display: "flex",
                "justify-content": "flex-end",
                gap: "8px",
                "margin-top": "18px",
              }}
            >
              <button
                type="button"
                disabled={deleteBusy()}
                onClick={() => setPendingDelete(null)}
                style={{
                  height: "32px",
                  padding: "0 14px",
                  border: "1px solid #E0E0E0",
                  "border-radius": "8px",
                  background: "#FFFFFF",
                  cursor: "pointer",
                }}
              >
                Cancel
              </button>
              <button
                type="button"
                disabled={deleteBusy()}
                onClick={() => void confirmDelete()}
                style={{
                  height: "32px",
                  padding: "0 14px",
                  border: "none",
                  "border-radius": "8px",
                  background: "#C2410C",
                  color: "#FFFFFF",
                  cursor: "pointer",
                }}
              >
                {deleteBusy() ? "Removing…" : "Remove"}
              </button>
            </div>
          </div>
        </div>
      </Show>
    </div>
  );
};

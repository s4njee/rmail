import { createEffect, createSignal, For, Show } from "solid-js";
import {
  loadCalendarSources,
  loadRemovedCalendarSources,
  removeSourceCalendar,
  restoreSourceCalendar,
  useCalendarSources,
  useRemovedCalendarSources,
} from "../../lib/calendar";
import { useAccounts } from "../../lib/mail";
import { useSettings, updateSettings } from "../../lib/settings";
import {
  calendarIdFor,
  getHiddenFromSidebarIds,
  QuillCalendarDataSource,
  setHiddenFromSidebar,
} from "../../lib/calendarAdapter";
import {
  addSubscription,
  deleteSubscription,
  listSubscriptions,
  syncSubscription,
} from "../../lib/tauri";
import type { CalendarSubscription } from "../../lib/ipc/CalendarSubscription";
import type { Calendar } from "@rcalendar/ui";
import "../Settings.css";

const WEEK_START_KEY = "quill_calendar_week_start";
const TIME_FORMAT_KEY = "quill_calendar_time_format";
const DEFAULT_CAL_KEY = "quill_calendar_default_cal";

export function CalendarSection() {
  const accounts = useAccounts();
  const settings = useSettings();
  const dataSource = new QuillCalendarDataSource();

  const [calendars, setCalendars] = createSignal<Calendar[]>([]);
  const [subscriptions, setSubscriptions] = createSignal<
    CalendarSubscription[]
  >([]);
  const [subName, setSubName] = createSignal("");
  const [subUrl, setSubUrl] = createSignal("");
  const [subColor, setSubColor] = createSignal("#3b5bdb");
  const [subInterval, setSubInterval] = createSignal(1440);
  const [isAddingSub, setIsAddingSub] = createSignal(false);
  const [syncingSubId, setSyncingSubId] = createSignal<number | null>(null);

  const [weekStart, setWeekStart] = createSignal<string>(
    localStorage.getItem(WEEK_START_KEY) || "sunday",
  );
  const [timeFormat, setTimeFormat] = createSignal<string>(
    localStorage.getItem(TIME_FORMAT_KEY) || "12h",
  );
  const [defaultCal, setDefaultCal] = createSignal<string>(
    localStorage.getItem(DEFAULT_CAL_KEY) || "",
  );

  const loadCalendars = async () => {
    const list = await dataSource.listCalendars();
    setCalendars(list);
    if (!defaultCal() && list.length > 0) {
      setDefaultCal(list[0].id);
    }
  };

  const loadSubscriptions = async () => {
    const list = await listSubscriptions();
    setSubscriptions(list);
  };

  const sources = useCalendarSources();
  const removedSources = useRemovedCalendarSources();

  const handleRemoveSource = async (accountId: number, source: string) => {
    await removeSourceCalendar(accountId, source);
    await loadCalendars();
  };

  const handleRestoreSource = async (accountId: number, source: string) => {
    await restoreSourceCalendar(accountId, source);
  };

  createEffect(() => {
    accounts();
    void loadCalendars();
    void loadSubscriptions();
    void loadCalendarSources();
    void loadRemovedCalendarSources();
  });

  const handleToggleCalendar = async (calId: string, enabled: boolean) => {
    await dataSource.setCalendarEnabled(calId, enabled);
    await loadCalendars();
  };

  const handleAddSubscription = async (e: Event) => {
    e.preventDefault();
    if (!subName().trim() || !subUrl().trim()) return;

    await addSubscription(
      subName().trim(),
      subUrl().trim(),
      subColor(),
      Number(subInterval()),
    );
    setSubName("");
    setSubUrl("");
    setIsAddingSub(false);
    await loadSubscriptions();
  };

  const handleDeleteSubscription = async (id: number) => {
    await deleteSubscription(id);
    await loadSubscriptions();
  };

  const handleSyncSubscription = async (id: number) => {
    setSyncingSubId(id);
    try {
      await syncSubscription(id);
      await loadSubscriptions();
    } finally {
      setSyncingSubId(null);
    }
  };

  const handleWeekStartChange = (val: string) => {
    setWeekStart(val);
    try {
      localStorage.setItem(WEEK_START_KEY, val);
    } catch {}
  };

  const handleTimeFormatChange = (val: string) => {
    setTimeFormat(val);
    try {
      localStorage.setItem(TIME_FORMAT_KEY, val);
    } catch {}
  };

  const handleDefaultCalChange = (val: string) => {
    setDefaultCal(val);
    try {
      localStorage.setItem(DEFAULT_CAL_KEY, val);
    } catch {}
  };

  return (
    <div
      class="settings-calendar"
      style={{ display: "flex", "flex-direction": "column", gap: "20px" }}
    >
      {/* Calendars per Account */}
      <div>
        <h3
          style={{
            "font-size": "13px",
            "font-weight": 600,
            color: "var(--color-text-primary, #1A1A1A)",
            "margin-bottom": "8px",
          }}
        >
          Calendars & Visibility
        </h3>
        <p
          style={{
            "font-size": "12px",
            color: "var(--color-text-body-soft, #666666)",
            "margin-bottom": "12px",
          }}
        >
          Toggle which calendars appear in your month, week, and agenda views.
        </p>

        <div
          style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
        >
          <For each={calendars()}>
            {(cal) => (
              <div
                class="settings-row"
                style={{
                  display: "flex",
                  "align-items": "center",
                  padding: "10px 14px",
                  background: "var(--color-surface, #FAFAFA)",
                  border: "1px solid var(--color-border-row, #EBEBEB)",
                  "border-radius": "8px",
                  gap: "12px",
                }}
              >
                <span
                  style={{
                    width: "12px",
                    height: "12px",
                    "border-radius": "3px",
                    background: cal.color,
                    flex: "none",
                  }}
                />
                <span
                  style={{
                    "font-size": "13px",
                    "font-weight": 500,
                    flex: 1,
                    color: "var(--color-text-primary, #1A1A1A)",
                  }}
                >
                  {cal.name}
                </span>
                <label
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "6px",
                    "font-size": "12px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={cal.enabled}
                    onChange={(e) =>
                      handleToggleCalendar(cal.id, e.currentTarget.checked)
                    }
                  />
                  <span>Show</span>
                </label>
                <label
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "6px",
                    "font-size": "12px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={!getHiddenFromSidebarIds().has(cal.id)}
                    onChange={(e) =>
                      setHiddenFromSidebar(cal.id, !e.currentTarget.checked)
                    }
                  />
                  <span>Sidebar</span>
                </label>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Synced Calendars — pulled in from Google/CalDAV sync (per source) */}
      <div>
        <h3
          style={{
            "font-size": "13px",
            "font-weight": 600,
            color: "var(--color-text-primary, #1A1A1A)",
            "margin-bottom": "8px",
          }}
        >
          Synced Calendars
        </h3>
        <p
          style={{
            "font-size": "12px",
            color: "var(--color-text-body-soft, #666666)",
            "margin-bottom": "12px",
          }}
        >
          Calendars synced from your accounts. Removing one deletes its local
          events and stops it from syncing again until restored.
        </p>
        <div
          style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
        >
          <Show when={sources().length === 0}>
            <p style={{ "font-size": "12px", color: "var(--color-text-faint, #94A1AF)" }}>
              No synced calendars yet — use "Sync Cal" in Settings → Accounts.
            </p>
          </Show>
          <For each={sources()}>
            {(src) => {
              const calId = calendarIdFor(src.accountId, src.source);
              const hiddenFromSidebar = () =>
                getHiddenFromSidebarIds().has(calId);
              return (
                <div
                  class="settings-row"
                  style={{
                    display: "flex",
                    "align-items": "center",
                    padding: "10px 14px",
                    background: "var(--color-surface, #FAFAFA)",
                    border: "1px solid var(--color-border-row, #EBEBEB)",
                    "border-radius": "8px",
                    gap: "12px",
                  }}
                >
                  <span
                    style={{
                      width: "12px",
                      height: "12px",
                      "border-radius": "3px",
                      background: src.color,
                      flex: "none",
                    }}
                  />
                  <span
                    style={{
                      "font-size": "13px",
                      "font-weight": 500,
                      flex: 1,
                      color: "var(--color-text-primary, #1A1A1A)",
                    }}
                  >
                    {src.name || src.source}
                  </span>
                  <button
                    type="button"
                    class="btn btn--secondary btn--sm"
                    onClick={() => setHiddenFromSidebar(calId, !hiddenFromSidebar())}
                  >
                    {hiddenFromSidebar() ? "Add to sidebar" : "Remove from sidebar"}
                  </button>
                  <button
                    type="button"
                    class="btn btn--secondary btn--sm"
                    onClick={() => void handleRemoveSource(src.accountId, src.source)}
                  >
                    Remove
                  </button>
                </div>
              );
            }}
          </For>
        </div>
      </div>

      {/* Removed Calendars — restore to bring one back on the next sync */}
      <Show when={removedSources().length > 0}>
        <div>
          <h3
            style={{
              "font-size": "13px",
              "font-weight": 600,
              color: "var(--color-text-primary, #1A1A1A)",
              "margin-bottom": "8px",
            }}
          >
            Removed Calendars
          </h3>
          <p
            style={{
              "font-size": "12px",
              color: "var(--color-text-body-soft, #666666)",
              "margin-bottom": "12px",
            }}
          >
            Restore a calendar to bring it back on the next sync.
          </p>
          <div
            style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
          >
            <For each={removedSources()}>
              {(src) => (
                <div
                  class="settings-row"
                  style={{
                    display: "flex",
                    "align-items": "center",
                    padding: "10px 14px",
                    background: "var(--color-surface, #FAFAFA)",
                    border: "1px solid var(--color-border-row, #EBEBEB)",
                    "border-radius": "8px",
                    gap: "12px",
                    opacity: 0.7,
                  }}
                >
                  <span
                    style={{
                      width: "12px",
                      height: "12px",
                      "border-radius": "3px",
                      background: src.color || "#999",
                      flex: "none",
                    }}
                  />
                  <span
                    style={{
                      "font-size": "13px",
                      "font-weight": 500,
                      flex: 1,
                      color: "var(--color-text-primary, #1A1A1A)",
                    }}
                  >
                    {src.name || src.source}
                  </span>
                  <button
                    type="button"
                    class="btn btn--secondary btn--sm"
                    onClick={() => void handleRestoreSource(src.accountId, src.source)}
                  >
                    Restore
                  </button>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>

      {/* Calendar Subscriptions & Feeds (Roadmap 4.4) */}
      <div>
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            "margin-bottom": "8px",
          }}
        >
          <div>
            <h3
              style={{
                "font-size": "13px",
                "font-weight": 600,
                color: "var(--color-text-primary, #1A1A1A)",
              }}
            >
              Subscriptions & Feeds
            </h3>
            <p
              style={{
                "font-size": "12px",
                color: "var(--color-text-body-soft, #666666)",
                "margin-top": "2px",
              }}
            >
              Read-only .ics / webcal calendar feeds (holidays, team schedules).
            </p>
          </div>
          <button
            type="button"
            class="btn btn--sm btn--secondary"
            onClick={() => setIsAddingSub(!isAddingSub())}
          >
            {isAddingSub() ? "Cancel" : "+ Add Feed"}
          </button>
        </div>

        <Show when={isAddingSub()}>
          <form
            onSubmit={handleAddSubscription}
            class="settings-card"
            style={{
              padding: "14px",
              "margin-bottom": "12px",
              display: "flex",
              "flex-direction": "column",
              gap: "10px",
            }}
          >
            <div style={{ display: "flex", gap: "10px" }}>
              <input
                type="text"
                class="settings-input"
                placeholder="Feed Name (e.g. US Holidays)"
                value={subName()}
                onInput={(e) => setSubName(e.currentTarget.value)}
                style={{ flex: 1 }}
                required
              />
              <input
                type="color"
                value={subColor()}
                onInput={(e) => setSubColor(e.currentTarget.value)}
                style={{
                  width: "36px",
                  height: "32px",
                  padding: 0,
                  border: "none",
                  cursor: "pointer",
                }}
                title="Calendar Color"
              />
            </div>
            <input
              type="text"
              class="settings-input"
              placeholder="URL (webcal://... or https://.../calendar.ics)"
              value={subUrl()}
              onInput={(e) => setSubUrl(e.currentTarget.value)}
              required
            />
            <div
              style={{
                display: "flex",
                "align-items": "center",
                "justify-content": "space-between",
              }}
            >
              <span
                style={{
                  "font-size": "12px",
                  color: "var(--color-text-body-soft, #666)",
                }}
              >
                Refresh interval:
              </span>
              <select
                class="settings-select"
                value={subInterval()}
                onChange={(e) => setSubInterval(Number(e.currentTarget.value))}
              >
                <option value={60}>Every hour</option>
                <option value={360}>Every 6 hours</option>
                <option value={1440}>Daily</option>
              </select>
            </div>
            <div
              style={{
                display: "flex",
                "justify-content": "flex-end",
                gap: "8px",
                "margin-top": "4px",
              }}
            >
              <button
                type="button"
                class="btn btn--sm btn--secondary"
                onClick={() => setIsAddingSub(false)}
              >
                Cancel
              </button>
              <button type="submit" class="btn btn--sm btn--primary">
                Subscribe
              </button>
            </div>
          </form>
        </Show>

        <div
          style={{ display: "flex", "flex-direction": "column", gap: "8px" }}
        >
          <For each={subscriptions()}>
            {(sub) => (
              <div
                class="settings-row"
                style={{
                  display: "flex",
                  "align-items": "center",
                  padding: "10px 14px",
                  background: "var(--color-surface, #FAFAFA)",
                  border: "1px solid var(--color-border-row, #EBEBEB)",
                  "border-radius": "8px",
                  gap: "12px",
                }}
              >
                <span
                  style={{
                    width: "12px",
                    height: "12px",
                    "border-radius": "3px",
                    background: sub.color,
                    flex: "none",
                  }}
                />
                <div style={{ flex: 1, "min-width": 0 }}>
                  <div
                    style={{
                      "font-size": "13px",
                      "font-weight": 500,
                      color: "var(--color-text-primary, #1A1A1A)",
                    }}
                  >
                    {sub.name}
                  </div>
                  <div
                    style={{
                      "font-size": "11px",
                      color: "var(--color-text-body-soft, #888)",
                      "white-space": "nowrap",
                      overflow: "hidden",
                      "text-overflow": "ellipsis",
                    }}
                  >
                    {sub.url}
                  </div>
                </div>
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    gap: "8px",
                  }}
                >
                  <button
                    type="button"
                    class="btn btn--sm btn--secondary"
                    disabled={syncingSubId() === sub.id}
                    onClick={() => void handleSyncSubscription(sub.id)}
                  >
                    {syncingSubId() === sub.id ? "Syncing..." : "Sync"}
                  </button>
                  <button
                    type="button"
                    class="btn btn--sm btn--secondary"
                    onClick={() => void handleDeleteSubscription(sub.id)}
                  >
                    Remove
                  </button>
                </div>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Default Calendar */}
      <div>
        <h3
          style={{
            "font-size": "13px",
            "font-weight": 600,
            color: "var(--color-text-primary, #1A1A1A)",
            "margin-bottom": "8px",
          }}
        >
          Default Calendar for New Events
        </h3>
        <select
          value={defaultCal()}
          onChange={(e) => handleDefaultCalChange(e.currentTarget.value)}
          style={{
            height: "32px",
            padding: "0 12px",
            border: "1px solid var(--color-border-row, #E0E0E0)",
            "border-radius": "6px",
            background: "var(--color-surface, #FFFFFF)",
            color: "var(--color-text-primary, #1A1A1A)",
            "font-size": "13px",
            width: "100%",
            "max-width": "320px",
          }}
        >
          <For each={calendars()}>
            {(cal) => <option value={cal.id}>{cal.name}</option>}
          </For>
        </select>
      </div>

      {/* General Calendar Preferences */}
      <div>
        <h3
          style={{
            "font-size": "13px",
            "font-weight": 600,
            color: "var(--color-text-primary, #1A1A1A)",
            "margin-bottom": "8px",
          }}
        >
          Preferences
        </h3>
        <div
          style={{ display: "flex", "flex-direction": "column", gap: "12px" }}
        >
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "max-width": "360px",
            }}
          >
            <span
              style={{
                "font-size": "13px",
                color: "var(--color-text-body, #1A1A1A)",
              }}
            >
              Start week on
            </span>
            <select
              value={weekStart()}
              onChange={(e) => handleWeekStartChange(e.currentTarget.value)}
              style={{
                height: "28px",
                padding: "0 8px",
                border: "1px solid var(--color-border-row, #E0E0E0)",
                "border-radius": "6px",
                background: "var(--color-surface, #FFFFFF)",
                "font-size": "12.5px",
              }}
            >
              <option value="sunday">Sunday</option>
              <option value="monday">Monday</option>
            </select>
          </div>

          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "max-width": "360px",
            }}
          >
            <span
              style={{
                "font-size": "13px",
                color: "var(--color-text-body, #1A1A1A)",
              }}
            >
              Time format
            </span>
            <select
              value={timeFormat()}
              onChange={(e) => handleTimeFormatChange(e.currentTarget.value)}
              style={{
                height: "28px",
                padding: "0 8px",
                border: "1px solid var(--color-border-row, #E0E0E0)",
                "border-radius": "6px",
                background: "var(--color-surface, #FFFFFF)",
                "font-size": "12.5px",
              }}
            >
              <option value="12h">12-hour (1:00 PM)</option>
              <option value="24h">24-hour (13:00)</option>
            </select>
          </div>

          {/* Timezone Correctness (Roadmap 4.3) */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "max-width": "360px",
            }}
          >
            <span
              style={{
                "font-size": "13px",
                color: "var(--color-text-body, #1A1A1A)",
              }}
            >
              Primary timezone
            </span>
            <select
              value={settings()?.primaryTimezone || "local"}
              onChange={(e) => {
                const val = e.currentTarget.value;
                void updateSettings({
                  primaryTimezone: val === "local" ? null : val,
                });
              }}
              style={{
                height: "28px",
                padding: "0 8px",
                border: "1px solid var(--color-border-row, #E0E0E0)",
                "border-radius": "6px",
                background: "var(--color-surface, #FFFFFF)",
                "font-size": "12.5px",
              }}
            >
              <option value="local">Local system time</option>
              <option value="America/New_York">
                America/New_York (Eastern)
              </option>
              <option value="America/Chicago">America/Chicago (Central)</option>
              <option value="America/Denver">America/Denver (Mountain)</option>
              <option value="America/Los_Angeles">
                America/Los_Angeles (Pacific)
              </option>
              <option value="Europe/London">Europe/London (GMT/BST)</option>
              <option value="Europe/Paris">Europe/Paris (CET/CEST)</option>
              <option value="Asia/Tokyo">Asia/Tokyo (JST)</option>
              <option value="UTC">UTC</option>
            </select>
          </div>

          <div
            style={{
              display: "flex",
              "align-items": "center",
              "justify-content": "space-between",
              "max-width": "360px",
            }}
          >
            <span
              style={{
                "font-size": "13px",
                color: "var(--color-text-body, #1A1A1A)",
              }}
            >
              Secondary timezone
            </span>
            <select
              value={settings()?.secondaryTimezone || "UTC"}
              onChange={(e) => {
                const val = e.currentTarget.value;
                void updateSettings({
                  secondaryTimezone: val,
                });
              }}
              style={{
                height: "28px",
                padding: "0 8px",
                border: "1px solid var(--color-border-row, #E0E0E0)",
                "border-radius": "6px",
                background: "var(--color-surface, #FFFFFF)",
                "font-size": "12.5px",
              }}
            >
              <option value="UTC">UTC</option>
              <option value="America/New_York">
                America/New_York (Eastern)
              </option>
              <option value="America/Chicago">America/Chicago (Central)</option>
              <option value="America/Denver">America/Denver (Mountain)</option>
              <option value="America/Los_Angeles">
                America/Los_Angeles (Pacific)
              </option>
              <option value="Europe/London">Europe/London (GMT/BST)</option>
              <option value="Europe/Paris">Europe/Paris (CET/CEST)</option>
              <option value="Asia/Tokyo">Asia/Tokyo (JST)</option>
              <option value="Australia/Sydney">Australia/Sydney (AEST)</option>
            </select>
          </div>

          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "8px",
              "max-width": "360px",
            }}
          >
            <input
              type="checkbox"
              id="show-secondary-tz"
              checked={settings()?.showSecondaryTimezone ?? false}
              onChange={(e) => {
                void updateSettings({
                  showSecondaryTimezone: e.currentTarget.checked,
                });
              }}
            />
            <label
              for="show-secondary-tz"
              style={{
                "font-size": "13px",
                color: "var(--color-text-body, #1A1A1A)",
                cursor: "pointer",
              }}
            >
              Show secondary timezone in week & day views
            </label>
          </div>
        </div>
      </div>
    </div>
  );
}

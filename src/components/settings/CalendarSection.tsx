import { createEffect, createSignal, For } from "solid-js";
import { useAccounts } from "../../lib/mail";
import { QuillCalendarDataSource } from "../../lib/calendarAdapter";
import type { Calendar } from "@rcalendar/ui";
import "../Settings.css";

const WEEK_START_KEY = "quill_calendar_week_start";
const TIME_FORMAT_KEY = "quill_calendar_time_format";
const DEFAULT_CAL_KEY = "quill_calendar_default_cal";

export function CalendarSection() {
  const accounts = useAccounts();
  const dataSource = new QuillCalendarDataSource();

  const [calendars, setCalendars] = createSignal<Calendar[]>([]);
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

  createEffect(() => {
    accounts();
    void loadCalendars();
  });

  const handleToggleCalendar = async (calId: string, enabled: boolean) => {
    await dataSource.setCalendarEnabled(calId, enabled);
    await loadCalendars();
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
    <div class="settings-calendar" style={{ display: "flex", "flex-direction": "column", gap: "20px" }}>
      {/* Calendars per Account */}
      <div>
        <h3 style={{ "font-size": "13px", "font-weight": 600, color: "var(--color-text-primary, #1A1A1A)", "margin-bottom": "8px" }}>
          Calendars & Visibility
        </h3>
        <p style={{ "font-size": "12px", color: "var(--color-text-body-soft, #666666)", "margin-bottom": "12px" }}>
          Toggle which calendars appear in your month, week, and agenda views.
        </p>

        <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
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
                <span style={{ "font-size": "13px", "font-weight": 500, flex: 1, color: "var(--color-text-primary, #1A1A1A)" }}>
                  {cal.name}
                </span>
                <label style={{ display: "flex", "align-items": "center", gap: "6px", "font-size": "12px", cursor: "pointer" }}>
                  <input
                    type="checkbox"
                    checked={cal.enabled}
                    onChange={(e) => handleToggleCalendar(cal.id, e.currentTarget.checked)}
                  />
                  <span>Show</span>
                </label>
              </div>
            )}
          </For>
        </div>
      </div>

      {/* Default Calendar */}
      <div>
        <h3 style={{ "font-size": "13px", "font-weight": 600, color: "var(--color-text-primary, #1A1A1A)", "margin-bottom": "8px" }}>
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
        <h3 style={{ "font-size": "13px", "font-weight": 600, color: "var(--color-text-primary, #1A1A1A)", "margin-bottom": "8px" }}>
          Preferences
        </h3>
        <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
          <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between", "max-width": "360px" }}>
            <span style={{ "font-size": "13px", color: "var(--color-text-body, #1A1A1A)" }}>Start week on</span>
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

          <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between", "max-width": "360px" }}>
            <span style={{ "font-size": "13px", color: "var(--color-text-body, #1A1A1A)" }}>Time format</span>
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
        </div>
      </div>
    </div>
  );
}

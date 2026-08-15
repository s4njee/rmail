import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { Calendar, EditScope, Event, EventDraft } from "../types/calendar";
import { toDateKey } from "../headless/dateUtils";

export interface EventEditorModalProps {
  isOpen: boolean;
  event: Event | null; // null for new event
  initialDate?: Date;
  calendars: Calendar[];
  onSave: (draft: EventDraft, id?: string, scope?: EditScope, targetDate?: string) => void;
  onDelete?: (id: string, scope?: EditScope, targetDate?: string) => void;
  onClose: () => void;
}

const WEEKDAY_KEYS = [
  { label: "M", value: "MO" },
  { label: "T", value: "TU" },
  { label: "W", value: "WE" },
  { label: "T", value: "TH" },
  { label: "F", value: "FR" },
  { label: "S", value: "SA" },
  { label: "S", value: "SU" },
];

export const EventEditorModal: Component<EventEditorModalProps> = (props) => {
  const [title, setTitle] = createSignal("");
  const [calendarId, setCalendarId] = createSignal("");
  const [dateStr, setDateStr] = createSignal("");
  const [startTime, setStartTime] = createSignal("10:00");
  const [endTime, setEndTime] = createSignal("11:00");
  const [allDay, setAllDay] = createSignal(false);
  const [repeatFreq, setRepeatFreq] = createSignal("none"); // 'none' | 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY'
  const [selectedDays, setSelectedDays] = createSignal<string[]>([]);
  const [endsMode, setEndsMode] = createSignal<"never" | "until" | "count">("never");
  const [untilDate, setUntilDate] = createSignal("");
  const [occurrenceCount, setOccurrenceCount] = createSignal(5);
  const [tz, setTz] = createSignal("local");
  const [location, setLocation] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [travelTime, setTravelTime] = createSignal<number>(0);
  const [color, setColor] = createSignal("");
  const [showFindTime, setShowFindTime] = createSignal(false);
  const [scope, setScope] = createSignal<EditScope>("this");

  const readonlyCalendar = () =>
    !!props.calendars.find((c) => c.id === calendarId())?.readOnly;

  const EVENT_COLORS = ["#3b5bdb", "#0f766e", "#b4451f", "#e8590c", "#7048e8", "#e03131"];

  const detectedVideo = () => {
    const combined = `${location()} \n ${notes()}`;
    if (/zoom\.us\/j/i.test(combined))
      return { provider: "Zoom", url: combined.match(/https?:\/\/[^\s"']+/)?.[0] || "" };
    if (/meet\.google\.com/i.test(combined))
      return { provider: "Google Meet", url: combined.match(/https?:\/\/[^\s"']+/)?.[0] || "" };
    if (/teams\.microsoft\.com|teams\.live\.com/i.test(combined))
      return { provider: "Microsoft Teams", url: combined.match(/https?:\/\/[^\s"']+/)?.[0] || "" };
    if (/webex\.com/i.test(combined))
      return { provider: "Webex", url: combined.match(/https?:\/\/[^\s"']+/)?.[0] || "" };
    if (/meet\.jit\.si/i.test(combined))
      return { provider: "Jitsi", url: combined.match(/https?:\/\/[^\s"']+/)?.[0] || "" };
    return null;
  };

  createEffect(() => {
    if (!props.isOpen) return;

    if (props.event) {
      const e = props.event;
      setTitle(e.title);
      setCalendarId(e.calendarId);
      setTz(e.tz || "local");
      setTravelTime(e.travelTimeMinutes || 0);
      const start = new Date(e.startsAt);
      const end = new Date(e.endsAt);
      setDateStr(toDateKey(start));
      const sH = String(start.getHours()).padStart(2, "0");
      const sM = String(start.getMinutes()).padStart(2, "0");
      const eH = String(end.getHours()).padStart(2, "0");
      const eM = String(end.getMinutes()).padStart(2, "0");
      setStartTime(`${sH}:${sM}`);
      setEndTime(`${eH}:${eM}`);
      setAllDay(e.allDay);
      setLocation(e.location || "");
      setNotes(e.notes || "");
      setColor(e.color || "");

      if (e.rrule) {
        if (e.rrule.includes("FREQ=DAILY")) setRepeatFreq("DAILY");
        else if (e.rrule.includes("FREQ=WEEKLY")) setRepeatFreq("WEEKLY");
        else if (e.rrule.includes("FREQ=MONTHLY")) setRepeatFreq("MONTHLY");
        else if (e.rrule.includes("FREQ=YEARLY")) setRepeatFreq("YEARLY");

        const matchDays = e.rrule.match(/BYDAY=([^;]+)/);
        if (matchDays) {
          setSelectedDays(matchDays[1].split(","));
        } else {
          setSelectedDays([]);
        }

        const matchCount = e.rrule.match(/COUNT=(\d+)/);
        const matchUntil = e.rrule.match(/UNTIL=([0-9T]+)/);
        if (matchCount) {
          setEndsMode("count");
          setOccurrenceCount(Number(matchCount[1]));
        } else if (matchUntil) {
          setEndsMode("until");
          const raw = matchUntil[1];
          if (raw.length >= 8) {
            setUntilDate(`${raw.slice(0, 4)}-${raw.slice(4, 6)}-${raw.slice(6, 8)}`);
          }
        } else {
          setEndsMode("never");
        }
      } else {
        setRepeatFreq("none");
        setSelectedDays([]);
        setEndsMode("never");
      }
    } else {
      const d = props.initialDate || new Date();
      setTitle("");
      // P1.4: quick-create starts at the clicked/dragged time (1 hour long) on
      // the first enabled, editable calendar.
      const calChoice =
        props.calendars.find((c) => c.enabled && !c.readOnly)?.id ||
        props.calendars.find((c) => c.enabled)?.id ||
        props.calendars[0]?.id ||
        "";
      setCalendarId(calChoice);
      setTz("local");
      setDateStr(toDateKey(d));
      const startMin = d.getHours() * 60 + d.getMinutes();
      const fmt = (m: number) => {
        const h = Math.floor(m / 60);
        const min = m % 60;
        return `${String(h).padStart(2, "0")}:${String(min).padStart(2, "0")}`;
      };
      setStartTime(fmt(startMin));
      setEndTime(fmt(startMin + 60));
      setAllDay(false);
      setRepeatFreq("none");
      setSelectedDays([]);
      setEndsMode("never");
      setUntilDate("");
      setOccurrenceCount(5);
      setLocation("");
      setNotes("");
      setTravelTime(0);
      setColor("#3b5bdb");
      setShowFindTime(false);
    }
  });

  const toggleDay = (day: string) => {
    const curr = selectedDays();
    if (curr.includes(day)) {
      setSelectedDays(curr.filter((d) => d !== day));
    } else {
      setSelectedDays([...curr, day]);
    }
  };

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onClose();
    } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      handleSave();
    }
  };

  const handleSave = () => {
    if (!title().trim()) return;
    if (readonlyCalendar()) return; // P1.4: read-only calendars can't be edited

    let startsAt: string;
    let endsAt: string;

    if (allDay()) {
      startsAt = `${dateStr()}T00:00:00Z`;
      endsAt = `${dateStr()}T23:59:59Z`;
    } else {
      startsAt = `${dateStr()}T${startTime()}:00Z`;
      endsAt = `${dateStr()}T${endTime()}:00Z`;
    }

    let rrule: string | null = null;
    if (repeatFreq() !== "none") {
      const parts = [`FREQ=${repeatFreq()}`];
      if (repeatFreq() === "WEEKLY" && selectedDays().length > 0) {
        parts.push(`BYDAY=${selectedDays().join(",")}`);
      }
      if (endsMode() === "count" && occurrenceCount() > 0) {
        parts.push(`COUNT=${occurrenceCount()}`);
      } else if (endsMode() === "until" && untilDate()) {
        const u = untilDate().replace(/-/g, "");
        parts.push(`UNTIL=${u}T235959Z`);
      }
      rrule = parts.join(";");
    }

    const draft: EventDraft = {
      calendarId: calendarId() || props.calendars[0]?.id || "",
      title: title().trim(),
      location: location().trim() || null,
      notes: notes().trim() || null,
      startsAt,
      endsAt,
      allDay: allDay(),
      tz: tz() === "local" ? null : tz(),
      rrule,
      travelTimeMinutes: travelTime() > 0 ? travelTime() : null,
      color: color() || null,
    };

    props.onSave(draft, props.event?.id, scope(), dateStr());
    props.onClose();
  };

  const handleDelete = () => {
    if (props.event) {
      props.onDelete?.(props.event.id, scope(), dateStr());
      props.onClose();
    }
  };

  return (
    <Show when={props.isOpen}>
      {/* Scrim */}
      <div
        onClick={props.onClose}
        style={{
          position: "fixed",
          inset: "52px 0 0 0",
          background: "var(--al-scrim, rgba(0,0,0,0.34))",
          "z-index": 100,
        }}
      />

      {/* Modal Sheet */}
      <div
        onKeyDown={handleKeyDown}
        style={{
          position: "fixed",
          left: "50%",
          top: "96px",
          transform: "translateX(-50%)",
          width: "576px",
          "max-height": "calc(100vh - 120px)",
          background: "var(--al-surface, #FFFFFF)",
          "border-radius": "14px",
          "box-shadow": "var(--al-shadow-modal, 0 40px 80px -20px rgba(0,0,0,0.5))",
          overflow: "hidden",
          display: "flex",
          "flex-direction": "column",
          "z-index": 101,
          "font-family": "var(--al-font-ui)",
          color: "var(--al-ink, #1A1A1A)",
        }}
      >
        {/* Head */}
        <div
          style={{
            padding: "22px 26px 18px",
            "border-bottom": "1px solid var(--al-grid, #EBEBEB)",
          }}
        >
          <div
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "9.5px",
              "letter-spacing": "0.12em",
              color: "var(--al-ink-7, #A0A0A0)",
              "margin-bottom": "12px",
            }}
          >
            {props.event ? "EDIT EVENT" : "NEW EVENT"}
          </div>
          <input
            type="text"
            placeholder="Event title"
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            autofocus
            style={{
              width: "100%",
              "font-size": "26px",
              "font-weight": 500,
              "letter-spacing": "-0.025em",
              color: "var(--al-ink, #1A1A1A)",
              "padding-bottom": "8px",
              border: "none",
              "border-bottom": "1.5px solid var(--al-accent, #1F6FEB)",
              outline: "none",
              background: "transparent",
              "font-family": "inherit",
            }}
          />
        </div>

        {/* Body */}
        <div
          style={{
            padding: "20px 26px",
            display: "flex",
            "flex-direction": "column",
            gap: "16px",
            "overflow-y": "auto",
          }}
        >
          {/* Calendar row */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              CALENDAR
            </span>
            <select
              value={calendarId()}
              onChange={(e) => setCalendarId(e.currentTarget.value)}
              style={{
                height: "34px",
                padding: "0 12px",
                border: "1px solid var(--al-border, #E0E0E0)",
                "border-radius": "8px",
                flex: 1,
                "font-size": "13px",
                background: "#FFFFFF",
                outline: "none",
              }}
            >
              <For each={props.calendars}>
                {(cal) => <option value={cal.id}>{cal.name}</option>}
              </For>
            </select>
          </div>

          {/* When row */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              WHEN
            </span>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "8px",
                flex: 1,
                "flex-wrap": "wrap",
              }}
            >
              <input
                type="date"
                value={dateStr()}
                onInput={(e) => setDateStr(e.currentTarget.value)}
                style={{
                  height: "34px",
                  padding: "0 8px",
                  border: "1px solid var(--al-border, #E0E0E0)",
                  "border-radius": "8px",
                  "font-family": "var(--al-font-mono)",
                  "font-size": "12.5px",
                }}
              />
              <Show when={!allDay()}>
                <input
                  type="time"
                  value={startTime()}
                  onInput={(e) => setStartTime(e.currentTarget.value)}
                  style={{
                    height: "34px",
                    padding: "0 8px",
                    border: "1px solid var(--al-border, #E0E0E0)",
                    "border-radius": "8px",
                    "font-family": "var(--al-font-mono)",
                    "font-size": "12.5px",
                  }}
                />
                <span style={{ color: "var(--al-ink-7, #A0A0A0)" }}>→</span>
                <input
                  type="time"
                  value={endTime()}
                  onInput={(e) => setEndTime(e.currentTarget.value)}
                  style={{
                    height: "34px",
                    padding: "0 8px",
                    border: "1px solid var(--al-border, #E0E0E0)",
                    "border-radius": "8px",
                    "font-family": "var(--al-font-mono)",
                    "font-size": "12.5px",
                  }}
                />
              </Show>
              <label
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "6px",
                  "font-size": "12px",
                  color: "var(--al-ink-5, #777777)",
                  cursor: "pointer",
                  "margin-left": "auto",
                }}
              >
                <input
                  type="checkbox"
                  checked={allDay()}
                  onChange={(e) => setAllDay(e.currentTarget.checked)}
                />
                All day
              </label>

              <Show when={!allDay()}>
                <button
                  type="button"
                  onClick={() => setShowFindTime(!showFindTime())}
                  style={{
                    height: "26px",
                    padding: "0 8px",
                    "border-radius": "5px",
                    border: "1px solid var(--al-border, #E0E0E0)",
                    background: showFindTime() ? "var(--al-accent-tint, #EBF3FE)" : "#FFFFFF",
                    color: showFindTime()
                      ? "var(--al-accent, #1F6FEB)"
                      : "var(--al-ink-4, #555555)",
                    "font-size": "11px",
                    cursor: "pointer",
                  }}
                >
                  ⚡ Find a time
                </button>
              </Show>
            </div>

            {/* Interactive Candidate Slot Strip (Roadmap 4.5) */}
            <Show when={showFindTime() && !allDay()}>
              <div
                style={{
                  margin: "8px 0 4px 108px",
                  padding: "10px 12px",
                  background: "var(--al-surface-2, #F8F9FA)",
                  border: "1px solid var(--al-border-soft, #EBEBEB)",
                  "border-radius": "8px",
                  display: "flex",
                  "flex-direction": "column",
                  gap: "6px",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    "justify-content": "space-between",
                  }}
                >
                  <span
                    style={{
                      "font-size": "11px",
                      "font-weight": 500,
                      color: "var(--al-ink-3, #444)",
                    }}
                  >
                    Recommended slots for {dateStr()}:
                  </span>
                  <span
                    style={{
                      "font-family": "var(--al-font-mono)",
                      "font-size": "9.5px",
                      color: "var(--al-accent, #1F6FEB)",
                    }}
                  >
                    ● Free
                  </span>
                </div>
                <div style={{ display: "flex", "flex-wrap": "wrap", gap: "6px" }}>
                  <For
                    each={["09:00", "10:00", "11:00", "13:00", "14:00", "15:00", "16:00", "17:00"]}
                  >
                    {(slot) => {
                      const h = Number(slot.split(":")[0]);
                      const endH = String(h + 1).padStart(2, "0");
                      const endSlot = `${endH}:00`;
                      const isCurrent = () => startTime() === slot;

                      return (
                        <button
                          type="button"
                          onClick={() => {
                            setStartTime(slot);
                            setEndTime(endSlot);
                          }}
                          style={{
                            padding: "4px 8px",
                            "border-radius": "5px",
                            border: isCurrent()
                              ? "1.5px solid var(--al-accent, #1F6FEB)"
                              : "1px solid var(--al-border, #D0D7DE)",
                            background: isCurrent() ? "var(--al-accent-tint, #EBF3FE)" : "#FFFFFF",
                            "font-family": "var(--al-font-mono)",
                            "font-size": "11px",
                            color: isCurrent()
                              ? "var(--al-accent, #1F6FEB)"
                              : "var(--al-ink, #1A1A1A)",
                            cursor: "pointer",
                          }}
                        >
                          {slot} - {endSlot}
                        </button>
                      );
                    }}
                  </For>
                </div>
              </div>
            </Show>
          </div>

          {/* Repeats row */}
          <div style={{ display: "flex", "align-items": "flex-start", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
                "margin-top": "8px",
              }}
            >
              REPEATS
            </span>
            <div style={{ display: "flex", "flex-direction": "column", gap: "8px", flex: 1 }}>
              <select
                value={repeatFreq()}
                onChange={(e) => setRepeatFreq(e.currentTarget.value)}
                style={{
                  height: "34px",
                  padding: "0 12px",
                  border: "1px solid var(--al-border, #E0E0E0)",
                  "border-radius": "8px",
                  "font-size": "13px",
                  background: "#FFFFFF",
                }}
              >
                <option value="none">Does not repeat</option>
                <option value="DAILY">Daily</option>
                <option value="WEEKLY">Weekly</option>
                <option value="MONTHLY">Monthly</option>
                <option value="YEARLY">Yearly</option>
              </select>

              <Show when={repeatFreq() === "WEEKLY"}>
                <div style={{ display: "flex", gap: "6px" }}>
                  <For each={WEEKDAY_KEYS}>
                    {(item) => {
                      const on = () => selectedDays().includes(item.value);
                      return (
                        <button
                          type="button"
                          onClick={() => toggleDay(item.value)}
                          style={{
                            width: "34px",
                            height: "30px",
                            "border-radius": "7px",
                            "font-family": "var(--al-font-mono)",
                            "font-size": "11px",
                            border: on()
                              ? "1px solid var(--al-accent, #1F6FEB)"
                              : "1px solid var(--al-border, #E0E0E0)",
                            background: on() ? "var(--al-accent, #1F6FEB)" : "#FFFFFF",
                            color: on() ? "#FFFFFF" : "var(--al-ink-6, #888888)",
                            cursor: "pointer",
                          }}
                        >
                          {item.label}
                        </button>
                      );
                    }}
                  </For>
                </div>
              </Show>

              {/* Ends option when repeating */}
              <Show when={repeatFreq() !== "none"}>
                <div
                  style={{
                    display: "flex",
                    "flex-direction": "column",
                    gap: "6px",
                    "padding-top": "4px",
                  }}
                >
                  <span
                    style={{
                      "font-family": "var(--al-font-mono)",
                      "font-size": "9.5px",
                      color: "var(--al-ink-7, #A0A0A0)",
                    }}
                  >
                    ENDS
                  </span>
                  <div
                    style={{
                      display: "flex",
                      "align-items": "center",
                      gap: "12px",
                      "font-size": "12.5px",
                    }}
                  >
                    <label
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "4px",
                        cursor: "pointer",
                      }}
                    >
                      <input
                        type="radio"
                        name="ends-mode"
                        checked={endsMode() === "never"}
                        onChange={() => setEndsMode("never")}
                      />
                      Never
                    </label>
                    <label
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "4px",
                        cursor: "pointer",
                      }}
                    >
                      <input
                        type="radio"
                        name="ends-mode"
                        checked={endsMode() === "until"}
                        onChange={() => setEndsMode("until")}
                      />
                      On date
                    </label>
                    <label
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "4px",
                        cursor: "pointer",
                      }}
                    >
                      <input
                        type="radio"
                        name="ends-mode"
                        checked={endsMode() === "count"}
                        onChange={() => setEndsMode("count")}
                      />
                      After count
                    </label>
                  </div>

                  <Show when={endsMode() === "until"}>
                    <input
                      type="date"
                      value={untilDate()}
                      onInput={(e) => setUntilDate(e.currentTarget.value)}
                      style={{
                        height: "32px",
                        padding: "0 8px",
                        border: "1px solid var(--al-border, #E0E0E0)",
                        "border-radius": "7px",
                        "font-family": "var(--al-font-mono)",
                        "font-size": "12px",
                        width: "160px",
                      }}
                    />
                  </Show>

                  <Show when={endsMode() === "count"}>
                    <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
                      <input
                        type="number"
                        min="1"
                        max="365"
                        value={occurrenceCount()}
                        onInput={(e) => setOccurrenceCount(Number(e.currentTarget.value) || 1)}
                        style={{
                          height: "32px",
                          padding: "0 8px",
                          border: "1px solid var(--al-border, #E0E0E0)",
                          "border-radius": "7px",
                          "font-family": "var(--al-font-mono)",
                          "font-size": "12px",
                          width: "80px",
                        }}
                      />
                      <span style={{ "font-size": "12px", color: "var(--al-ink-5, #777777)" }}>
                        occurrences
                      </span>
                    </div>
                  </Show>
                </div>
              </Show>
            </div>
          </div>

          {/* Remind Me row */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              REMIND ME
            </span>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "7px",
                flex: 1,
                "flex-wrap": "wrap",
              }}
            >
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "7px",
                  height: "30px",
                  padding: "0 11px",
                  "border-radius": "15px",
                  background: "var(--al-accent-tint, #E4EBF8)",
                }}
              >
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "11px",
                    color: "var(--al-accent, #1F6FEB)",
                  }}
                >
                  10 min before
                </span>
              </div>
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "7px",
                  height: "30px",
                  padding: "0 11px",
                  "border-radius": "15px",
                  background: "var(--al-accent-tint, #E4EBF8)",
                }}
              >
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "11px",
                    color: "var(--al-accent, #1F6FEB)",
                  }}
                >
                  at 08:00 same day
                </span>
              </div>
            </div>
          </div>

          {/* Timezone row */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              TIMEZONE
            </span>
            <select
              value={tz()}
              onChange={(e) => setTz(e.currentTarget.value)}
              style={{
                height: "34px",
                padding: "0 12px",
                border: "1px solid var(--al-border, #E0E0E0)",
                "border-radius": "8px",
                flex: 1,
                "font-size": "13px",
                background: "#FFFFFF",
              }}
            >
              <option value="local">Local system time</option>
              <option value="America/New_York">America/New_York (Eastern)</option>
              <option value="America/Chicago">America/Chicago (Central)</option>
              <option value="America/Denver">America/Denver (Mountain)</option>
              <option value="America/Los_Angeles">America/Los_Angeles (Pacific)</option>
              <option value="America/Anchorage">America/Anchorage (Alaska)</option>
              <option value="Pacific/Honolulu">Pacific/Honolulu (Hawaii)</option>
              <option value="America/Sao_Paulo">America/Sao_Paulo (BRT)</option>
              <option value="Europe/London">Europe/London (GMT/BST)</option>
              <option value="Europe/Paris">Europe/Paris (CET/CEST)</option>
              <option value="Europe/Berlin">Europe/Berlin (CET/CEST)</option>
              <option value="Asia/Dubai">Asia/Dubai (GST)</option>
              <option value="Asia/Kolkata">Asia/Kolkata (IST)</option>
              <option value="Asia/Singapore">Asia/Singapore (SGT)</option>
              <option value="Asia/Tokyo">Asia/Tokyo (JST)</option>
              <option value="Australia/Sydney">Australia/Sydney (AEST/AEDT)</option>
              <option value="Pacific/Auckland">Pacific/Auckland (NZST/NZDT)</option>
              <option value="UTC">UTC</option>
            </select>
          </div>

          {/* Travel Time row (Roadmap 4.5) */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              TRAVEL TIME
            </span>
            <select
              value={travelTime()}
              onChange={(e) => setTravelTime(Number(e.currentTarget.value))}
              style={{
                height: "34px",
                padding: "0 12px",
                border: "1px solid var(--al-border, #E0E0E0)",
                "border-radius": "8px",
                "font-size": "13px",
                background: "#FFFFFF",
                flex: 1,
              }}
            >
              <option value="0">None</option>
              <option value="15">15 minutes before</option>
              <option value="30">30 minutes before</option>
              <option value="45">45 minutes before</option>
              <option value="60">1 hour before</option>
            </select>
          </div>

          {/* P1.4 per-event color override */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              COLOR
            </span>
            <div style={{ display: "flex", gap: "8px", "flex-wrap": "wrap", flex: 1 }}>
              <button
                type="button"
                title="Use the calendar color"
                aria-label="Clear color override"
                onClick={() => setColor("")}
                style={{
                  width: "22px",
                  height: "22px",
                  "border-radius": "50%",
                  border:
                    color() === ""
                      ? "2px solid var(--al-accent, #1F6FEB)"
                      : "1px solid var(--al-border, #E0E0E0)",
                  background: "linear-gradient(135deg, #bbb 0 45%, #fff 45% 55%, #bbb 55%)",
                  cursor: "pointer",
                }}
              />
              <For each={EVENT_COLORS}>
                {(c) => (
                  <button
                    type="button"
                    onClick={() => setColor(c)}
                    aria-label={`Use color ${c}`}
                    style={{
                      width: "22px",
                      height: "22px",
                      "border-radius": "50%",
                      background: c,
                      border:
                        color() === c
                          ? "2px solid var(--al-accent, #1F6FEB)"
                          : "1px solid var(--al-border, #E0E0E0)",
                      cursor: "pointer",
                    }}
                  />
                )}
              </For>
            </div>
          </div>

          {/* Where row */}
          <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
              }}
            >
              WHERE
            </span>
            <div style={{ display: "flex", "align-items": "center", gap: "8px", flex: 1 }}>
              <input
                type="text"
                placeholder="Add location or video link"
                value={location()}
                onInput={(e) => setLocation(e.currentTarget.value)}
                style={{
                  height: "34px",
                  padding: "0 12px",
                  border: "1px solid var(--al-border, #E0E0E0)",
                  "border-radius": "8px",
                  flex: 1,
                  "font-size": "13px",
                }}
              />
              <Show when={location().trim() && !location().startsWith("http")}>
                <a
                  href={`https://maps.apple.com/?q=${encodeURIComponent(location().trim())}`}
                  target="_blank"
                  rel="noreferrer"
                  style={{
                    height: "32px",
                    padding: "0 10px",
                    "border-radius": "6px",
                    border: "1px solid var(--al-border, #E0E0E0)",
                    background: "#FFFFFF",
                    "font-size": "11.5px",
                    "text-decoration": "none",
                    color: "var(--al-accent, #1F6FEB)",
                    display: "flex",
                    "align-items": "center",
                    gap: "4px",
                    flex: "none",
                  }}
                >
                  📍 Maps
                </a>
              </Show>
            </div>
          </div>

          {/* Video Call Detection Row (Roadmap 4.5) */}
          <Show when={detectedVideo()}>
            {(video) => (
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "16px",
                  margin: "-4px 0 0 108px",
                }}
              >
                <a
                  href={video().url}
                  target="_blank"
                  rel="noreferrer"
                  style={{
                    display: "inline-flex",
                    "align-items": "center",
                    gap: "6px",
                    padding: "6px 12px",
                    background: "var(--al-accent, #1F6FEB)",
                    color: "#FFFFFF",
                    "border-radius": "6px",
                    "font-size": "12px",
                    "font-weight": 500,
                    "text-decoration": "none",
                  }}
                >
                  📹 Join {video().provider}
                </a>
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "10.5px",
                    color: "var(--al-ink-7, #888)",
                  }}
                >
                  One-click video call detected
                </span>
              </div>
            )}
          </Show>

          {/* Notes row */}
          <div style={{ display: "flex", "align-items": "flex-start", gap: "16px" }}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "10px",
                "letter-spacing": "0.08em",
                color: "var(--al-ink-7, #A0A0A0)",
                width: "92px",
                flex: "none",
                "margin-top": "8px",
              }}
            >
              NOTES
            </span>
            <textarea
              placeholder="Add notes or description"
              value={notes()}
              onInput={(e) => setNotes(e.currentTarget.value)}
              rows={3}
              style={{
                padding: "8px 12px",
                border: "1px solid var(--al-border, #E0E0E0)",
                "border-radius": "8px",
                flex: 1,
                "font-size": "13px",
                "font-family": "inherit",
                "min-height": "64px",
                resize: "vertical",
              }}
            />
          </div>

          {/* Scoped Edit selection (if editing a recurring series) */}
          <Show when={props.event && props.event.rrule}>
            <div
              style={{
                display: "flex",
                "align-items": "center",
                gap: "16px",
                "padding-top": "4px",
              }}
            >
              <span
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "10px",
                  "letter-spacing": "0.08em",
                  color: "var(--al-ink-7, #A0A0A0)",
                  width: "92px",
                  flex: "none",
                }}
              >
                SCOPE
              </span>
              <div style={{ display: "flex", gap: "8px" }}>
                <label
                  style={{
                    "font-size": "12.5px",
                    display: "flex",
                    "align-items": "center",
                    gap: "4px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="radio"
                    name="edit-scope"
                    checked={scope() === "this"}
                    onChange={() => setScope("this")}
                  />
                  This event
                </label>
                <label
                  style={{
                    "font-size": "12.5px",
                    display: "flex",
                    "align-items": "center",
                    gap: "4px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="radio"
                    name="edit-scope"
                    checked={scope() === "future"}
                    onChange={() => setScope("future")}
                  />
                  This and following
                </label>
                <label
                  style={{
                    "font-size": "12.5px",
                    display: "flex",
                    "align-items": "center",
                    gap: "4px",
                    cursor: "pointer",
                  }}
                >
                  <input
                    type="radio"
                    name="edit-scope"
                    checked={scope() === "all"}
                    onChange={() => setScope("all")}
                  />
                  All events
                </label>
              </div>
            </div>
          </Show>
        </div>

        {/* Foot */}
        <div
          style={{
            padding: "16px 26px",
            "border-top": "1px solid var(--al-grid, #EBEBEB)",
            background: "var(--al-surface-2, #FBFBFB)",
            display: "flex",
            "align-items": "center",
          }}
        >
          <Show when={props.event}>
            <button
              type="button"
              onClick={handleDelete}
              style={{
                background: "none",
                border: "none",
                color: "var(--al-cal-classes, #C2410C)",
                "font-size": "12.5px",
                cursor: "pointer",
                padding: "0 4px",
              }}
            >
              Delete
            </button>
          </Show>

          <div style={{ flex: 1 }} />

          {/* P1.4 read-only calendar */}
          <Show when={readonlyCalendar()}>
            <span
              style={{
                "font-size": "12px",
                color: "var(--al-cal-classes, #C2410C)",
                "margin-right": "8px",
              }}
            >
              This calendar is read-only.
            </span>
          </Show>

          <div style={{ display: "flex", gap: "10px" }}>
            <button
              type="button"
              onClick={props.onClose}
              style={{
                height: "34px",
                padding: "0 16px",
                border: "1px solid var(--al-border, #E0E0E0)",
                "border-radius": "8px",
                background: "#FFFFFF",
                "font-size": "13px",
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleSave}
              disabled={readonlyCalendar()}
              style={{
                height: "34px",
                padding: "0 17px",
                "border-radius": "8px",
                background: readonlyCalendar()
                  ? "var(--al-ink-5, #999)"
                  : "var(--al-accent, #1F6FEB)",
                color: "#FFFFFF",
                border: "none",
                "font-size": "13px",
                "font-weight": 500,
                cursor: readonlyCalendar() ? "not-allowed" : "pointer",
                display: "flex",
                "align-items": "center",
                gap: "8px",
              }}
            >
              <span>Save event</span>
              <span
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "10.5px",
                  opacity: 0.75,
                }}
              >
                ⌘↵
              </span>
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

import { Component, createEffect, createMemo, createSignal, For, Show } from "solid-js";
import {
  Attendee,
  AttendeeRole,
  AttendeeStatus,
  AvailableSlot,
  Calendar,
  EditScope,
  Event,
  EventDraft,
  Reminder,
} from "../types/calendar";
import { toDateKey } from "../headless/dateUtils";
import { buildEventDraft } from "../headless/eventDraft";
import { ALERT_PRESETS, formatAlertOffset } from "../headless/alerts";
import { ATTENDEE_STATUS_LABELS, summarizeAttendees } from "../headless/attendees";

/** A single alert configured in the editor. */
export interface EditorAlert {
  id?: string;
  offsetMinutes: number | null;
  absoluteAt: string | null;
}

/** A single invitee configured in the editor. */
export interface EditorAttendee {
  id?: string;
  email: string;
  displayName?: string | null;
  role: AttendeeRole;
  status: AttendeeStatus;
  rsvp?: boolean;
  isOrganizer?: boolean;
}

export interface EventEditorModalProps {
  isOpen: boolean;
  event: Event | null; // null for new event
  initialDate?: Date;
  calendars: Calendar[];
  /** Existing reminders when editing an event. */
  reminders?: Reminder[];
  /** Existing attendees when editing an event. */
  attendees?: Attendee[];
  /** Offset (minutes) to pre-fill for a newly created event, if any. */
  defaultAlertOffset?: number | null;
  /** Invitee autocomplete from previously used addresses. */
  onSuggestAttendees?: (query: string) => Promise<Attendee[]>;
  /** Real free/busy lookup for the Find a time strip. */
  onFindAvailableSlots?: (date: string, durationMinutes: number) => Promise<AvailableSlot[]>;
  onSave: (
    draft: EventDraft,
    alerts: EditorAlert[],
    invitees: EditorAttendee[],
    id?: string,
    scope?: EditScope,
    targetDate?: string,
  ) => void;
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
  const [alerts, setAlerts] = createSignal<EditorAlert[]>([]);
  const [addAlertValue, setAddAlertValue] = createSignal("");
  const [showCustomAlert, setShowCustomAlert] = createSignal(false);
  const [customMinutes, setCustomMinutes] = createSignal(15);
  const [invitees, setInvitees] = createSignal<EditorAttendee[]>([]);
  const [inviteeEmail, setInviteeEmail] = createSignal("");
  const [inviteeName, setInviteeName] = createSignal("");
  const [busy, setBusy] = createSignal(true);
  const [suggestions, setSuggestions] = createSignal<Attendee[]>([]);
  const [availableSlots, setAvailableSlots] = createSignal<AvailableSlot[]>([]);
  const [findTimeLoading, setFindTimeLoading] = createSignal(false);

  const inviteeSummary = createMemo(() =>
    summarizeAttendees(
      invitees().map((a) => ({
        id: a.id ?? a.email,
        eventId: props.event?.id ?? "",
        email: a.email,
        displayName: a.displayName,
        role: a.role,
        status: a.status,
        rsvp: a.rsvp ?? false,
        isOrganizer: a.isOrganizer ?? false,
      })),
    ),
  );

  const meetingDurationMinutes = () => {
    const [sh, sm] = startTime().split(":").map(Number);
    const [eh, em] = endTime().split(":").map(Number);
    return Math.max(15, eh * 60 + em - (sh * 60 + sm));
  };

  createEffect(() => {
    if (!showFindTime() || allDay() || !props.onFindAvailableSlots) {
      return;
    }
    const date = dateStr();
    const duration = meetingDurationMinutes();
    setFindTimeLoading(true);
    void props
      .onFindAvailableSlots(date, duration)
      .then((slots) => setAvailableSlots(slots))
      .catch(() => setAvailableSlots([]))
      .finally(() => setFindTimeLoading(false));
  });

  const readonlyCalendar = () => !!props.calendars.find((c) => c.id === calendarId())?.readOnly;

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
      setAlerts(
        (props.reminders || []).map((r) => ({
          id: r.id,
          offsetMinutes: r.offsetMinutes ?? null,
          absoluteAt: r.absoluteAt ?? null,
        })),
      );
      setInvitees(
        (props.attendees || []).map<EditorAttendee>((a) => ({
          id: a.id,
          email: a.email,
          displayName: a.displayName ?? null,
          role: a.role,
          status: a.status,
          rsvp: a.rsvp,
          isOrganizer: a.isOrganizer,
        })),
      );
      setInviteeEmail("");
      setInviteeName("");
      setBusy(e.busy !== false);
      setSuggestions([]);
      setAvailableSlots([]);

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
      setAlerts(
        props.defaultAlertOffset != null
          ? [{ offsetMinutes: props.defaultAlertOffset, absoluteAt: null }]
          : [],
      );
      setInvitees([]);
      setInviteeEmail("");
      setInviteeName("");
      setBusy(true);
      setSuggestions([]);
      setAvailableSlots([]);
      setAddAlertValue("");
      setShowCustomAlert(false);
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

  const addOffsetAlert = (offsetMinutes: number) => {
    const curr = alerts();
    if (curr.some((a) => a.offsetMinutes === offsetMinutes && a.absoluteAt == null)) return;
    setAlerts([...curr, { offsetMinutes, absoluteAt: null }]);
  };

  const addCustomAlert = () => {
    const mins = customMinutes();
    if (mins > 0) addOffsetAlert(-mins);
    setCustomMinutes(15);
    setShowCustomAlert(false);
  };

  const removeAlert = (index: number) => {
    setAlerts(alerts().filter((_, i) => i !== index));
  };

  const handleAddAlertChange = (value: string) => {
    setAddAlertValue("");
    if (value === "custom") {
      setShowCustomAlert(true);
      return;
    }
    if (value !== "") {
      addOffsetAlert(Number(value));
    }
  };

  const addInvitee = () => {
    const email = inviteeEmail().trim();
    if (!email) return;
    if (!invitees().some((a) => a.email.toLowerCase() === email.toLowerCase())) {
      setInvitees([
        ...invitees(),
        {
          email,
          displayName: inviteeName().trim() || null,
          role: "required",
          status: "needs_action",
          rsvp: true,
          isOrganizer: false,
        },
      ]);
    }
    setInviteeEmail("");
    setInviteeName("");
    setSuggestions([]);
  };

  const pickSuggestion = (person: Attendee) => {
    if (!invitees().some((a) => a.email.toLowerCase() === person.email.toLowerCase())) {
      setInvitees([
        ...invitees(),
        {
          id: undefined,
          email: person.email,
          displayName: person.displayName ?? null,
          role: person.role,
          status: "needs_action",
          rsvp: true,
          isOrganizer: false,
        },
      ]);
    }
    setInviteeEmail("");
    setInviteeName("");
    setSuggestions([]);
  };

  const onInviteeEmailInput = (value: string) => {
    setInviteeEmail(value);
    const q = value.trim();
    if (q.length < 1 || !props.onSuggestAttendees) {
      setSuggestions([]);
      return;
    }
    void props.onSuggestAttendees(q).then((rows) => {
      const taken = new Set(invitees().map((a) => a.email.toLowerCase()));
      setSuggestions(rows.filter((r) => !taken.has(r.email.toLowerCase())).slice(0, 6));
    });
  };

  const removeInvitee = (index: number) => {
    setInvitees(invitees().filter((_, i) => i !== index));
  };

  const setInviteeStatus = (index: number, status: AttendeeStatus) => {
    setInvitees(invitees().map((a, i) => (i === index ? { ...a, status } : a)));
  };

  const setInviteeRole = (index: number, role: AttendeeRole) => {
    setInvitees(invitees().map((a, i) => (i === index ? { ...a, role } : a)));
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

    const draft: EventDraft | null = buildEventDraft({
      title: title(),
      calendarId: calendarId(),
      dateStr: dateStr(),
      startTime: startTime(),
      endTime: endTime(),
      allDay: allDay(),
      repeatFreq: repeatFreq(),
      selectedDays: selectedDays(),
      endsMode: endsMode(),
      untilDate: untilDate(),
      occurrenceCount: occurrenceCount(),
      tz: tz(),
      location: location(),
      notes: notes(),
      travelTime: travelTime(),
      color: color(),
      fallbackCalendarId: props.calendars[0]?.id || "",
      busy: busy(),
    });
    if (!draft) return;

    props.onSave(draft, alerts(), invitees(), props.event?.id, scope(), dateStr());
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
              <label
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "6px",
                  "font-size": "12px",
                  color: "var(--al-ink-5, #777777)",
                  cursor: "pointer",
                }}
              >
                <input
                  type="checkbox"
                  checked={busy()}
                  onChange={(e) => setBusy(e.currentTarget.checked)}
                />
                Busy
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
                  <Show when={findTimeLoading()}>
                    <span style={{ "font-size": "11px", color: "var(--al-ink-5, #777777)" }}>
                      Looking for openings…
                    </span>
                  </Show>
                  <Show when={!findTimeLoading() && availableSlots().length === 0}>
                    <span style={{ "font-size": "11px", color: "var(--al-ink-5, #777777)" }}>
                      No free slots of {meetingDurationMinutes()} min on this day.
                    </span>
                  </Show>
                  <For each={availableSlots()}>
                    {(slot) => {
                      const start = new Date(slot.start);
                      const end = new Date(slot.end);
                      const fmt = (d: Date) =>
                        `${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
                      const startLabel = fmt(start);
                      const endLabel = fmt(end);
                      const isCurrent = () => startTime() === startLabel && endTime() === endLabel;

                      return (
                        <button
                          type="button"
                          onClick={() => {
                            setStartTime(startLabel);
                            setEndTime(endLabel);
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
                          {startLabel} – {endLabel}
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

          {/* Invitees row */}
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
              INVITEES
            </span>
            <div style={{ display: "flex", "flex-direction": "column", gap: "8px", flex: 1 }}>
              <Show when={invitees().length === 0}>
                <span style={{ "font-size": "12px", color: "var(--al-ink-5, #777777)" }}>
                  No attendees
                </span>
              </Show>
              <Show when={inviteeSummary().total > 0}>
                <span style={{ "font-size": "11px", color: "var(--al-ink-5, #777777)" }}>
                  {inviteeSummary().accepted} accepted · {inviteeSummary().pending} pending ·{" "}
                  {inviteeSummary().declined} declined
                  {inviteeSummary().tentative > 0 ? ` · ${inviteeSummary().tentative} maybe` : ""}
                </span>
              </Show>
              <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
                <For each={invitees()}>
                  {(invitee, index) => (
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "8px",
                        padding: "5px 10px",
                        border: "1px solid var(--al-border, #E0E0E0)",
                        "border-radius": "8px",
                        background: "var(--al-accent-tint, #E4EBF8)",
                      }}
                    >
                      <span
                        style={{
                          flex: 1,
                          "font-size": "12.5px",
                          color: "var(--al-ink-2, #222222)",
                          overflow: "hidden",
                          "text-overflow": "ellipsis",
                          "white-space": "nowrap",
                        }}
                        title={invitee.email}
                      >
                        {invitee.displayName || invitee.email}
                        {invitee.displayName ? (
                          <span style={{ color: "var(--al-ink-5, #777777)" }}>
                            {` · ${invitee.email}`}
                          </span>
                        ) : null}
                        {invitee.isOrganizer ? (
                          <span style={{ color: "var(--al-accent, #1F6FEB)" }}> · organizer</span>
                        ) : null}
                      </span>
                      <div
                        style={{ display: "flex", gap: "4px" }}
                        role="group"
                        aria-label="RSVP status"
                      >
                        <For
                          each={
                            [
                              ["needs_action", "Pending"],
                              ["accepted", "Yes"],
                              ["tentative", "Maybe"],
                              ["declined", "No"],
                            ] as [AttendeeStatus, string][]
                          }
                        >
                          {([value, label]) => (
                            <button
                              type="button"
                              aria-pressed={invitee.status === value}
                              title={ATTENDEE_STATUS_LABELS[value]}
                              onClick={() => setInviteeStatus(index(), value)}
                              style={{
                                height: "22px",
                                padding: "0 7px",
                                border:
                                  invitee.status === value
                                    ? "1px solid var(--al-accent, #1F6FEB)"
                                    : "1px solid var(--al-border, #E0E0E0)",
                                "border-radius": "11px",
                                background:
                                  invitee.status === value
                                    ? "var(--al-accent-tint, #E4EBF8)"
                                    : "#FFFFFF",
                                color:
                                  invitee.status === value
                                    ? "var(--al-accent, #1F6FEB)"
                                    : "var(--al-ink-5, #777777)",
                                "font-size": "10px",
                                cursor: "pointer",
                              }}
                            >
                              {label}
                            </button>
                          )}
                        </For>
                      </div>
                      <select
                        aria-label="Role"
                        value={invitee.role}
                        onChange={(e) =>
                          setInviteeRole(index(), e.currentTarget.value as AttendeeRole)
                        }
                        style={{
                          height: "26px",
                          border: "1px solid var(--al-border, #E0E0E0)",
                          "border-radius": "6px",
                          "font-size": "11px",
                          background: "#FFFFFF",
                        }}
                      >
                        <option value="required">Required</option>
                        <option value="optional">Optional</option>
                        <option value="chair">Chair</option>
                        <option value="non_participant">Observer</option>
                      </select>
                      <button
                        type="button"
                        aria-label="Remove invitee"
                        onClick={() => removeInvitee(index())}
                        style={{
                          background: "none",
                          border: "none",
                          padding: "0",
                          cursor: "pointer",
                          color: "var(--al-accent, #1F6FEB)",
                          "font-size": "13px",
                          "line-height": 1,
                        }}
                      >
                        ×
                      </button>
                    </div>
                  )}
                </For>
              </div>
              <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
                <input
                  type="text"
                  placeholder="name (optional)"
                  value={inviteeName()}
                  onInput={(e) => setInviteeName(e.currentTarget.value)}
                  style={{
                    height: "30px",
                    padding: "0 10px",
                    border: "1px solid var(--al-border, #E0E0E0)",
                    "border-radius": "8px",
                    "font-size": "12.5px",
                    width: "130px",
                  }}
                />
                <input
                  type="text"
                  placeholder="email address"
                  value={inviteeEmail()}
                  onInput={(e) => onInviteeEmailInput(e.currentTarget.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      addInvitee();
                    }
                    if (e.key === "ArrowDown" && suggestions().length > 0) {
                      e.preventDefault();
                      pickSuggestion(suggestions()[0]);
                    }
                  }}
                  style={{
                    height: "30px",
                    padding: "0 10px",
                    border: "1px solid var(--al-border, #E0E0E0)",
                    "border-radius": "8px",
                    "font-size": "12.5px",
                    flex: 1,
                  }}
                />
                <button
                  type="button"
                  onClick={addInvitee}
                  style={{
                    height: "30px",
                    padding: "0 12px",
                    border: "1px solid var(--al-accent, #1F6FEB)",
                    "border-radius": "8px",
                    background: "var(--al-accent-tint, #E4EBF8)",
                    color: "var(--al-accent, #1F6FEB)",
                    "font-size": "12px",
                    cursor: "pointer",
                  }}
                >
                  Add
                </button>
              </div>
              <Show when={suggestions().length > 0}>
                <div
                  style={{
                    border: "1px solid var(--al-border, #E0E0E0)",
                    "border-radius": "8px",
                    background: "#FFFFFF",
                    overflow: "hidden",
                  }}
                >
                  <For each={suggestions()}>
                    {(person) => (
                      <button
                        type="button"
                        onClick={() => pickSuggestion(person)}
                        style={{
                          display: "block",
                          width: "100%",
                          "text-align": "left",
                          padding: "8px 10px",
                          border: "none",
                          background: "transparent",
                          cursor: "pointer",
                          "font-size": "12.5px",
                        }}
                      >
                        {person.displayName || person.email}
                        <Show when={person.displayName}>
                          <span style={{ color: "var(--al-ink-5, #777777)" }}>
                            {` · ${person.email}`}
                          </span>
                        </Show>
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          </div>

          {/* Remind Me row */}
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
              ALERT
            </span>
            <div
              style={{
                display: "flex",
                "flex-direction": "column",
                gap: "8px",
                flex: 1,
              }}
            >
              <div style={{ display: "flex", "flex-wrap": "wrap", gap: "7px" }}>
                <For each={alerts()}>
                  {(alert, index) => (
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "7px",
                        height: "30px",
                        padding: "0 10px",
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
                        {alert.absoluteAt
                          ? `at ${new Date(alert.absoluteAt).toLocaleString()}`
                          : formatAlertOffset(alert.offsetMinutes)}
                      </span>
                      <button
                        type="button"
                        aria-label="Remove alert"
                        onClick={() => removeAlert(index())}
                        style={{
                          background: "none",
                          border: "none",
                          padding: "0",
                          cursor: "pointer",
                          color: "var(--al-accent, #1F6FEB)",
                          "font-size": "13px",
                          "line-height": 1,
                        }}
                      >
                        ×
                      </button>
                    </div>
                  )}
                </For>
                <Show when={alerts().length === 0}>
                  <span
                    style={{
                      "font-size": "12px",
                      color: "var(--al-ink-5, #777777)",
                      "align-self": "center",
                    }}
                  >
                    None
                  </span>
                </Show>
              </div>

              <select
                value={addAlertValue()}
                onChange={(e) => handleAddAlertChange(e.currentTarget.value)}
                style={{
                  height: "30px",
                  padding: "0 10px",
                  border: "1px solid var(--al-border, #E0E0E0)",
                  "border-radius": "8px",
                  "font-size": "12.5px",
                  background: "#FFFFFF",
                  width: "180px",
                }}
              >
                <option value="">Add alert…</option>
                <For each={ALERT_PRESETS.filter((p) => p.offsetMinutes != null)}>
                  {(p) => <option value={String(p.offsetMinutes)}>{p.label}</option>}
                </For>
                <option value="custom">Custom…</option>
              </select>

              <Show when={showCustomAlert()}>
                <div style={{ display: "flex", "align-items": "center", gap: "8px" }}>
                  <input
                    type="number"
                    min="1"
                    max="10080"
                    value={customMinutes()}
                    onInput={(e) => setCustomMinutes(Number(e.currentTarget.value) || 1)}
                    style={{
                      height: "30px",
                      padding: "0 8px",
                      border: "1px solid var(--al-border, #E0E0E0)",
                      "border-radius": "7px",
                      "font-family": "var(--al-font-mono)",
                      "font-size": "12px",
                      width: "80px",
                    }}
                  />
                  <span style={{ "font-size": "12px", color: "var(--al-ink-5, #777777)" }}>
                    minutes before
                  </span>
                  <button
                    type="button"
                    onClick={addCustomAlert}
                    style={{
                      height: "30px",
                      padding: "0 12px",
                      border: "1px solid var(--al-accent, #1F6FEB)",
                      "border-radius": "8px",
                      background: "var(--al-accent-tint, #E4EBF8)",
                      color: "var(--al-accent, #1F6FEB)",
                      "font-size": "12px",
                      cursor: "pointer",
                    }}
                  >
                    Add
                  </button>
                </div>
              </Show>
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

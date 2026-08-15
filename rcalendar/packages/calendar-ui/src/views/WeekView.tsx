import { Component, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { Calendar, OccurrenceItem } from "../types/calendar";
import {
  addDays,
  formatTime24,
  isSameDay,
  MONTH_NAMES_SHORT,
  startOfWeek,
  toDateKey,
  WEEKDAYS_SHORT,
} from "../headless/dateUtils";
import {
  computeNowLinePosition,
  findConflictingEventIds,
  GridConfig,
  isWithinWorkingHours,
  positionEventsForDay,
} from "../headless/layout";
import {
  computeDragToCreateRange,
  computeMovedRange,
  computeResizedEnd,
} from "../headless/dragEngine";

export interface WeekViewProps {
  focusedDate: Date;
  selectedDate: Date;
  onSelectDate: (d: Date) => void;
  onFocusedDateChange: (d: Date) => void;
  occurrences: OccurrenceItem[];
  calendars: Calendar[];
  onEventClick?: (item: OccurrenceItem) => void;
  onSlotClick?: (date: Date, hour: number) => void;
  onEventMove?: (item: OccurrenceItem, newStart: Date, newEnd: Date) => void;
  onEventResize?: (item: OccurrenceItem, newEnd: Date) => void;
  onRangeCreate?: (startsAt: Date, endsAt: Date) => void;
  primaryTz?: string | null;
  secondaryTz?: string | null;
  showSecondaryTz?: boolean;
  /** Shade the grid outside this window (hours 0-23). Default 9-17. */
  workingHours?: { start: number; end: number };
  /** Mark overlapping events with a conflict badge (default true). */
  conflictDetection?: boolean;
}

const GRID_CONFIG: GridConfig = {
  startHour: 7,
  endHour: 21,
  rowPitch: 45,
  minHeight: 20,
};

export const WeekView: Component<WeekViewProps> = (props) => {
  const [currentTime, setCurrentTime] = createSignal(new Date());

  // Drag interaction states
  const [draggingItem, setDraggingItem] = createSignal<{
    item: OccurrenceItem;
    startY: number;
    deltaY: number;
  } | null>(null);

  const [resizingItem, setResizingItem] = createSignal<{
    item: OccurrenceItem;
    startY: number;
    deltaY: number;
  } | null>(null);

  const [creatingRange, setCreatingRange] = createSignal<{
    day: Date;
    startY: number;
    currentY: number;
  } | null>(null);

  // Minute-interval timer for live now-line
  onMount(() => {
    const timer = setInterval(() => {
      setCurrentTime(new Date());
    }, 60000);
    onCleanup(() => clearInterval(timer));
  });

  const weekStart = createMemo(() => {
    return startOfWeek(props.focusedDate, 0); // Sunday start
  });

  const weekDays = createMemo(() => {
    const start = weekStart();
    const days: Date[] = [];
    for (let i = 0; i < 7; i++) {
      days.push(addDays(start, i));
    }
    return days;
  });

  const headerTitle = createMemo(() => {
    const start = weekDays()[0];
    const end = weekDays()[6];
    const sM = MONTH_NAMES_SHORT[start.getMonth()];
    const eM = MONTH_NAMES_SHORT[end.getMonth()];
    const y = start.getFullYear();

    if (start.getMonth() === end.getMonth()) {
      return `${start.getDate()} – ${end.getDate()} ${sM} ${y}`;
    }
    return `${start.getDate()} ${sM} – ${end.getDate()} ${eM} ${y}`;
  });

  const calendarMap = createMemo(() => {
    const map = new Map<string, Calendar>();
    for (const c of props.calendars) {
      map.set(c.id, c);
    }
    return map;
  });

  const allDayEventsByDay = createMemo(() => {
    const map = new Map<string, OccurrenceItem[]>();
    for (const item of props.occurrences) {
      if (item.occurrence.allDay || item.event.allDay) {
        const start = new Date(item.occurrence.startsAt);
        const key = toDateKey(start);
        const list = map.get(key) || [];
        list.push(item);
        map.set(key, list);
      }
    }
    return map;
  });

  const scheduledHours = createMemo(() => {
    let totalMs = 0;
    for (const item of props.occurrences) {
      if (!item.occurrence.allDay && !item.event.allDay) {
        const s = new Date(item.occurrence.startsAt).getTime();
        const e = new Date(item.occurrence.endsAt).getTime();
        totalMs += Math.max(0, e - s);
      }
    }
    return Math.round(totalMs / 3600000);
  });

  const nowLinePos = createMemo(() => {
    return computeNowLinePosition(currentTime(), GRID_CONFIG);
  });

  const hoursList = createMemo(() => {
    const list: { hour: number; label: string }[] = [];
    for (let h = GRID_CONFIG.startHour; h <= GRID_CONFIG.endHour; h++) {
      const label = `${String(h).padStart(2, "0")}:00`;
      list.push({ hour: h, label });
    }
    return list;
  });

  // Global mouse handlers for drag move/resize/create
  const handleMouseMove = (e: MouseEvent) => {
    const d = draggingItem();
    if (d) {
      setDraggingItem({ ...d, deltaY: e.clientY - d.startY });
      return;
    }
    const r = resizingItem();
    if (r) {
      setResizingItem({ ...r, deltaY: e.clientY - r.startY });
      return;
    }
    const c = creatingRange();
    if (c) {
      const target = e.currentTarget as HTMLElement;
      const rect = target.getBoundingClientRect();
      setCreatingRange({ ...c, currentY: e.clientY - rect.top });
    }
  };

  const handleMouseUp = () => {
    const d = draggingItem();
    if (d && Math.abs(d.deltaY) > 5) {
      const start = new Date(d.item.occurrence.startsAt);
      const end = new Date(d.item.occurrence.endsAt);
      const { newStart, newEnd } = computeMovedRange(start, end, d.deltaY, {
        rowPitch: GRID_CONFIG.rowPitch,
      });
      props.onEventMove?.(d.item, newStart, newEnd);
    }
    setDraggingItem(null);

    const r = resizingItem();
    if (r && Math.abs(r.deltaY) > 5) {
      const start = new Date(r.item.occurrence.startsAt);
      const end = new Date(r.item.occurrence.endsAt);
      const newEnd = computeResizedEnd(start, end, r.deltaY, {
        rowPitch: GRID_CONFIG.rowPitch,
      });
      props.onEventResize?.(r.item, newEnd);
    }
    setResizingItem(null);

    const c = creatingRange();
    if (c && Math.abs(c.currentY - c.startY) > 10) {
      const { startsAt, endsAt } = computeDragToCreateRange(c.day, c.startY, c.currentY, {
        rowPitch: GRID_CONFIG.rowPitch,
        startHour: GRID_CONFIG.startHour,
      });
      props.onRangeCreate?.(startsAt, endsAt);
    }
    setCreatingRange(null);
  };

  const handlePrev = () => {
    props.onFocusedDateChange(addDays(props.focusedDate, -7));
  };

  const handleNext = () => {
    props.onFocusedDateChange(addDays(props.focusedDate, 7));
  };

  const handleToday = () => {
    const today = new Date();
    props.onFocusedDateChange(today);
    props.onSelectDate(today);
  };

  return (
    <div
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      style={{
        flex: 1,
        display: "flex",
        "flex-direction": "column",
        "min-width": 0,
        background: "var(--al-surface, #FFFFFF)",
        "font-family": "var(--al-font-ui)",
        color: "var(--al-ink, #1A1A1A)",
        height: "100%",
        overflow: "hidden",
        "user-select": draggingItem() || resizingItem() || creatingRange() ? "none" : "auto",
      }}
    >
      {/* Header */}
      <div
        style={{
          height: "78px",
          flex: "none",
          display: "flex",
          "align-items": "center",
          gap: "18px",
          padding: "0 26px",
          "border-bottom": "1px solid var(--al-border-soft, #E5E5E5)",
        }}
      >
        <span
          style={{
            "font-size": "34px",
            "font-weight": 500,
            "letter-spacing": "-0.03em",
            color: "var(--al-ink, #1A1A1A)",
            "line-height": 1,
          }}
        >
          {headerTitle()}
        </span>

        <div style={{ flex: 1 }} />

        <span
          style={{
            "font-family": "var(--al-font-mono)",
            "font-size": "11.5px",
            color: "var(--al-ink-6, #888888)",
          }}
        >
          {scheduledHours()}h scheduled
        </span>

        {/* Stepper */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            height: "30px",
            border: "1px solid var(--al-border, #E0E0E0)",
            "border-radius": "8px",
            overflow: "hidden",
          }}
        >
          <button
            type="button"
            onClick={handlePrev}
            style={{
              width: "32px",
              height: "100%",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              "font-family": "var(--al-font-mono)",
              "font-size": "13px",
              color: "var(--al-ink-5, #777777)",
              background: "none",
              border: "none",
              cursor: "pointer",
            }}
          >
            ‹
          </button>
          <div style={{ width: "1px", height: "100%", background: "var(--al-border, #E0E0E0)" }} />
          <button
            type="button"
            onClick={handleToday}
            style={{
              padding: "0 13px",
              height: "100%",
              display: "flex",
              "align-items": "center",
              "font-size": "12.5px",
              "font-weight": 500,
              color: "var(--al-ink, #1A1A1A)",
              background: "none",
              border: "none",
              cursor: "pointer",
            }}
          >
            Today
          </button>
          <div style={{ width: "1px", height: "100%", background: "var(--al-border, #E0E0E0)" }} />
          <button
            type="button"
            onClick={handleNext}
            style={{
              width: "32px",
              height: "100%",
              display: "flex",
              "align-items": "center",
              "justify-content": "center",
              "font-family": "var(--al-font-mono)",
              "font-size": "13px",
              color: "var(--al-ink-5, #777777)",
              background: "none",
              border: "none",
              cursor: "pointer",
            }}
          >
            ›
          </button>
        </div>
      </div>
      {/* Column Headers (52px) */}
      <div
        style={{
          height: "52px",
          flex: "none",
          display: "flex",
          "border-bottom": "1px solid var(--al-border-soft, #E5E5E5)",
        }}
      >
        <div
          style={{
            width: props.showSecondaryTz && props.secondaryTz ? "92px" : "62px",
            flex: "none",
            "border-right": "1px solid var(--al-grid, #EBEBEB)",
            display: "flex",
            "align-items": "flex-end",
            "justify-content": "flex-end",
            "padding-right": "8px",
            "padding-bottom": "6px",
          }}
        >
          <Show when={props.showSecondaryTz && props.secondaryTz}>
            <span
              style={{
                "font-family": "var(--al-font-mono)",
                "font-size": "8.5px",
                color: "var(--al-ink-7, #888888)",
                "letter-spacing": "0.04em",
              }}
            >
              {props.secondaryTz?.split("/").pop()?.replace("_", " ")}
            </span>
          </Show>
        </div>
        <For each={weekDays()}>
          {(day, i) => {
            const today = new Date();
            const isToday = () => isSameDay(day, today);
            const isSel = () => isSameDay(day, props.selectedDate);

            return (
              <div
                onClick={() => props.onSelectDate(day)}
                style={{
                  flex: 1,
                  "min-width": 0,
                  "border-right": "1px solid var(--al-grid, #EBEBEB)",
                  display: "flex",
                  "flex-direction": "column",
                  "align-items": "center",
                  "justify-content": "center",
                  gap: "2px",
                  background: isToday()
                    ? "var(--al-today-wash, #FBFCFE)"
                    : isSel()
                      ? "var(--al-surface-2, #FBFBFB)"
                      : "#FFFFFF",
                  cursor: "pointer",
                }}
              >
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "9.5px",
                    "letter-spacing": "0.1em",
                    color: isToday() ? "var(--al-accent, #1F6FEB)" : "var(--al-ink-7, #A0A0A0)",
                    "text-transform": "uppercase",
                  }}
                >
                  {WEEKDAYS_SHORT[i()]}
                </span>
                <span
                  style={{
                    "font-size": "19px",
                    "font-weight": 500,
                    "letter-spacing": "-0.02em",
                    color: isToday() ? "var(--al-accent, #1F6FEB)" : "var(--al-ink, #1A1A1A)",
                  }}
                >
                  {day.getDate()}
                </span>
              </div>
            );
          }}
        </For>
      </div>

      {/* All-Day Band (40px) */}
      <div
        style={{
          height: "40px",
          flex: "none",
          display: "flex",
          "border-bottom": "1px solid var(--al-border-soft, #E5E5E5)",
          background: "#FFFFFF",
        }}
      >
        <div
          style={{
            width: props.showSecondaryTz && props.secondaryTz ? "92px" : "62px",
            flex: "none",
            "border-right": "1px solid var(--al-grid, #EBEBEB)",
            display: "flex",
            "align-items": "center",
            "justify-content": "flex-end",
            "padding-right": "9px",
          }}
        >
          <span
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "9px",
              color: "var(--al-ink-7, #A0A0A0)",
              "letter-spacing": "0.04em",
            }}
          >
            ALL-DAY
          </span>
        </div>
        <For each={weekDays()}>
          {(day) => {
            const dayAllDay = () => allDayEventsByDay().get(toDateKey(day)) || [];
            return (
              <div
                style={{
                  flex: 1,
                  "min-width": 0,
                  "border-right": "1px solid var(--al-grid, #EBEBEB)",
                  padding: "4px",
                  display: "flex",
                  "flex-direction": "column",
                  gap: "2px",
                  overflow: "hidden",
                }}
              >
                <For each={dayAllDay()}>
                  {(item) => {
                    const cal = () => calendarMap().get(item.event.calendarId);
                    const color = () => cal()?.color || "var(--al-accent, #1F6FEB)";

                    return (
                      <div
                        onClick={() => props.onEventClick?.(item)}
                        style={{
                          height: "17px",
                          "border-radius": "3px",
                          background: "var(--al-surface-event, #F3F4F6)",
                          display: "flex",
                          "align-items": "center",
                          padding: "0 5px",
                          gap: "4px",
                          overflow: "hidden",
                          cursor: "pointer",
                        }}
                      >
                        <div
                          style={{
                            width: "2.5px",
                            height: "11px",
                            "border-radius": "2px",
                            flex: "none",
                            background: color(),
                          }}
                        />
                        <span
                          style={{
                            "font-size": "11px",
                            color: "var(--al-ink-event, #262626)",
                            "white-space": "nowrap",
                            overflow: "hidden",
                            "text-overflow": "ellipsis",
                          }}
                        >
                          {item.event.title}
                        </span>
                      </div>
                    );
                  }}
                </For>
              </div>
            );
          }}
        </For>
      </div>

      {/* Time Grid */}
      <div
        style={{
          flex: 1,
          display: "flex",
          "min-height": 0,
          position: "relative",
          "overflow-y": "auto",
        }}
      >
        {/* Hour Gutter */}
        <div
          style={{
            width: props.showSecondaryTz && props.secondaryTz ? "92px" : "62px",
            flex: "none",
            "border-right": "1px solid var(--al-grid, #EBEBEB)",
            position: "relative",
          }}
        >
          <For each={hoursList()}>
            {(h) => {
              const secTime = () => {
                if (!props.showSecondaryTz || !props.secondaryTz) return null;
                try {
                  const d = new Date(props.focusedDate);
                  d.setHours(h.hour, 0, 0, 0);
                  return new Intl.DateTimeFormat("en-US", {
                    timeZone: props.secondaryTz,
                    hour: "2-digit",
                    minute: "2-digit",
                    hour12: false,
                  }).format(d);
                } catch {
                  return null;
                }
              };

              return (
                <div
                  style={{
                    height: `${GRID_CONFIG.rowPitch}px`,
                    display: "flex",
                    "align-items": "flex-start",
                    "justify-content": "flex-end",
                    "padding-right": "8px",
                    gap: "6px",
                  }}
                >
                  <Show when={secTime()}>
                    <span
                      style={{
                        "font-family": "var(--al-font-mono)",
                        "font-size": "9px",
                        color: "var(--al-ink-7, #888888)",
                        transform: "translateY(-5px)",
                      }}
                      title={props.secondaryTz || undefined}
                    >
                      {secTime()}
                    </span>
                  </Show>
                  <span
                    style={{
                      "font-family": "var(--al-font-mono)",
                      "font-size": "10px",
                      color: "var(--al-ink-8, #B3B3B3)",
                      transform: "translateY(-5px)",
                    }}
                  >
                    {h.label}
                  </span>
                </div>
              );
            }}
          </For>
        </div>

        {/* Day Columns */}
        <For each={weekDays()}>
          {(day) => {
            const today = new Date();
            const isToday = () => isSameDay(day, today);
            const dayEvents = () => positionEventsForDay(props.occurrences, day, GRID_CONFIG);
            const dayConflicts = () => findConflictingEventIds(props.occurrences, day);
            const workingHours = () => props.workingHours ?? { start: 9, end: 17 };
            const showConflicts = () => props.conflictDetection !== false;

            const handleColumnMouseDown = (e: MouseEvent) => {
              if (
                e.target !== e.currentTarget &&
                !(e.target as HTMLElement).classList.contains("hour-cell")
              )
                return;
              const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
              const startY = e.clientY - rect.top;
              setCreatingRange({ day, startY, currentY: startY });
            };

            return (
              <div
                onMouseDown={handleColumnMouseDown}
                style={{
                  flex: 1,
                  "min-width": 0,
                  "border-right": "1px solid var(--al-grid, #EBEBEB)",
                  position: "relative",
                  background: isToday() ? "var(--al-today-wash-2, #FDFDFE)" : "transparent",
                }}
              >
                {/* Horizontal hour lines (out-of-working-hours are shaded) */}
                <For each={hoursList()}>
                  {(h) => {
                    const hourDate = new Date(
                      day.getFullYear(),
                      day.getMonth(),
                      day.getDate(),
                      h.hour,
                    );
                    const outside = !isWithinWorkingHours(hourDate, workingHours());
                    return (
                      <div
                        class="hour-cell"
                        onClick={() => props.onSlotClick?.(day, h.hour)}
                        style={{
                          height: `${GRID_CONFIG.rowPitch - 1}px`,
                          "border-bottom": "1px solid var(--al-grid-hour, #F2F2F2)",
                          cursor: "pointer",
                          background: outside
                            ? "var(--al-outside-hours, #F6F7F9)"
                            : "transparent",
                        }}
                      />
                    );
                  }}
                </For>

                {/* Event blocks */}
                <For each={dayEvents()}>
                  {(pe) => {
                    const cal = () => calendarMap().get(pe.item.event.calendarId);
                    const color = () =>
                      pe.item.event.color || cal()?.color || "#1F6FEB";
                    const conflicted = () =>
                      showConflicts() && dayConflicts().has(pe.item.event.id);
                    const tint = () => {
                      const c = color();
                      return c.startsWith("#") ? `${c}1A` : "rgba(31,111,235,0.10)";
                    };

                    const isBeingDragged = () => draggingItem()?.item.event.id === pe.item.event.id;
                    const isBeingResized = () => resizingItem()?.item.event.id === pe.item.event.id;

                    const effectiveTop = () => {
                      if (isBeingDragged()) {
                        return pe.top + (draggingItem()?.deltaY || 0);
                      }
                      return pe.top;
                    };

                    const effectiveHeight = () => {
                      if (isBeingResized()) {
                        return Math.max(20, pe.height + (resizingItem()?.deltaY || 0));
                      }
                      return pe.height;
                    };

                    const timeStr = () => {
                      const s = new Date(pe.item.occurrence.startsAt);
                      const e = new Date(pe.item.occurrence.endsAt);
                      return `${formatTime24(s)} – ${formatTime24(e)}`;
                    };

                    const handleBlockMouseDown = (e: MouseEvent) => {
                      if ((e.target as HTMLElement).classList.contains("resize-handle")) return;
                      e.stopPropagation();
                      setDraggingItem({ item: pe.item, startY: e.clientY, deltaY: 0 });
                    };

                    const handleResizeMouseDown = (e: MouseEvent) => {
                      e.stopPropagation();
                      setResizingItem({ item: pe.item, startY: e.clientY, deltaY: 0 });
                    };

                    return (
                      <>
                        <Show
                          when={
                            pe.item.event.travelTimeMinutes && pe.item.event.travelTimeMinutes > 0
                          }
                        >
                          {(_) => {
                            const travelHeight =
                              (pe.item.event.travelTimeMinutes! / 60) * GRID_CONFIG.rowPitch;
                            return (
                              <div
                                style={{
                                  position: "absolute",
                                  top: `${effectiveTop() - travelHeight}px`,
                                  height: `${travelHeight}px`,
                                  left: `calc(${pe.leftPercent}% + 2px)`,
                                  width: `calc(${pe.widthPercent}% - 5px)`,
                                  background:
                                    "repeating-linear-gradient(45deg, transparent, transparent 4px, rgba(0,0,0,0.04) 4px, rgba(0,0,0,0.04) 8px)",
                                  border: "1px dashed var(--al-border, #D0D7DE)",
                                  "border-bottom": "none",
                                  "border-radius": "4px 4px 0 0",
                                  "font-family": "var(--al-font-mono)",
                                  "font-size": "9px",
                                  color: "var(--al-ink-7, #888)",
                                  display: "flex",
                                  "align-items": "center",
                                  "padding-left": "4px",
                                  "pointer-events": "none",
                                  "z-index": 5,
                                }}
                              >
                                🚗 {pe.item.event.travelTimeMinutes}m
                              </div>
                            );
                          }}
                        </Show>
                        <div
                          onMouseDown={handleBlockMouseDown}
                          onClick={(e) => {
                            e.stopPropagation();
                            if (!draggingItem() && !resizingItem()) {
                              props.onEventClick?.(pe.item);
                            }
                          }}
                          style={{
                            position: "absolute",
                            top: `${effectiveTop()}px`,
                            height: `${effectiveHeight()}px`,
                            left: `calc(${pe.leftPercent}% + 2px)`,
                            width: `calc(${pe.widthPercent}% - 5px)`,
                            padding: "5px 7px",
                            "border-radius": "6px",
                            background: tint(),
                            "border-left": `2.5px solid ${color()}`,
                            "box-shadow": "var(--al-shadow-event-week, 0 1px 2px rgba(0,0,0,0.05))",
                            overflow: "hidden",
                            cursor: isBeingDragged() ? "grabbing" : "grab",
                            "z-index": isBeingDragged() || isBeingResized() ? 30 : 10,
                            opacity: isBeingDragged() ? 0.85 : 1,
                            display: "flex",
                            "flex-direction": "column",
                            gap: "1px",
                          }}
                        >
                          <span
                            style={{
                              "font-size": "11.5px",
                              "font-weight": 500,
                              "line-height": 1.25,
                              color: "var(--al-ink-event, #232323)",
                              "white-space": "nowrap",
                              overflow: "hidden",
                              "text-overflow": "ellipsis",
                            }}
                          >
                            {pe.item.event.title}
                          </span>
                          <span
                            style={{
                              "font-family": "var(--al-font-mono)",
                              "font-size": "9.5px",
                              color: color(),
                            }}
                          >
                            {timeStr()}
                          </span>
                          <Show when={conflicted()}>
                            <span
                              title="Overlaps another event"
                              style={{
                                "font-size": "9.5px",
                                color: "#C2410C",
                                "font-weight": 600,
                              }}
                            >
                              ⚠ overlap
                            </span>
                          </Show>

                          {/* Edge resize handle at bottom */}
                          <div
                            class="resize-handle"
                            onMouseDown={handleResizeMouseDown}
                            style={{
                              position: "absolute",
                              bottom: 0,
                              left: 0,
                              right: 0,
                              height: "6px",
                              cursor: "ns-resize",
                            }}
                          />
                        </div>
                      </>
                    );
                  }}
                </For>

                {/* Drag to create preview box */}
                <Show when={creatingRange() && isSameDay(creatingRange()!.day, day)}>
                  <div
                    style={{
                      position: "absolute",
                      top: `${Math.min(creatingRange()!.startY, creatingRange()!.currentY)}px`,
                      height: `${Math.abs(creatingRange()!.currentY - creatingRange()!.startY)}px`,
                      left: "4px",
                      right: "4px",
                      "border-radius": "6px",
                      background: "var(--al-accent-tint, #E4EBF8)",
                      border: "1.5px dashed var(--al-accent, #1F6FEB)",
                      "pointer-events": "none",
                      "z-index": 25,
                    }}
                  />
                </Show>

                {/* Now-line */}
                <Show when={isToday() && nowLinePos() !== null}>
                  <div
                    style={{
                      position: "absolute",
                      left: 0,
                      right: 0,
                      top: `${nowLinePos()}px`,
                      height: "1.5px",
                      background: "var(--al-accent, #1F6FEB)",
                      "z-index": 20,
                      "pointer-events": "none",
                    }}
                  >
                    <div
                      style={{
                        position: "absolute",
                        left: "-4px",
                        top: "-3.5px",
                        width: "8px",
                        height: "8px",
                        "border-radius": "50%",
                        background: "var(--al-accent, #1F6FEB)",
                      }}
                    />
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};

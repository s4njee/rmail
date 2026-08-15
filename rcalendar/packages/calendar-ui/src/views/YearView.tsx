import { Component, createMemo, For, Show } from "solid-js";
import { Calendar, OccurrenceItem, ViewMode } from "../types/calendar";
import { isSameDay, MONTH_NAMES, toDateKey, WEEKDAYS_SHORT } from "../headless/dateUtils";

export interface YearViewProps {
  focusedDate: Date;
  selectedDate: Date;
  onSelectDate: (d: Date) => void;
  onFocusedDateChange: (d: Date) => void;
  occurrences: OccurrenceItem[];
  calendars: Calendar[];
  onNavigateView?: (view: ViewMode) => void;
}

export const YearView: Component<YearViewProps> = (props) => {
  const currentYear = createMemo(() => props.focusedDate.getFullYear());

  const handlePrevYear = () => {
    const d = new Date(props.focusedDate);
    d.setFullYear(d.getFullYear() - 1);
    props.onFocusedDateChange(d);
  };

  const handleNextYear = () => {
    const d = new Date(props.focusedDate);
    d.setFullYear(d.getFullYear() + 1);
    props.onFocusedDateChange(d);
  };

  const handleToday = () => {
    const now = new Date();
    props.onFocusedDateChange(now);
    props.onSelectDate(now);
  };

  // Map occurrences by date key (YYYY-MM-DD) for fast lookup
  const occurrencesByDate = createMemo(() => {
    const map = new Map<string, OccurrenceItem[]>();
    for (const item of props.occurrences) {
      const s = new Date(item.occurrence.startsAt);
      const key = toDateKey(s);
      const list = map.get(key) || [];
      list.push(item);
      map.set(key, list);
    }
    return map;
  });

  const totalYearEvents = createMemo(() => {
    const yr = currentYear();
    let count = 0;
    for (const item of props.occurrences) {
      const d = new Date(item.occurrence.startsAt);
      if (d.getFullYear() === yr) {
        count++;
      }
    }
    return count;
  });

  // 12 months array (0..11)
  const months = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        flex: 1,
        height: "100%",
        background: "var(--al-surface, #FFFFFF)",
        overflow: "hidden",
        "font-family": "var(--al-font-ui, system-ui, sans-serif)",
        color: "var(--al-ink, #1A1A1A)",
      }}
    >
      {/* Year View Header */}
      <div
        style={{
          height: "48px",
          flex: "none",
          display: "flex",
          "align-items": "center",
          "justify-content": "space-between",
          padding: "0 24px",
          "border-bottom": "1px solid var(--al-border-soft, #EBEBEB)",
          background: "#FFFFFF",
        }}
      >
        <div style={{ display: "flex", "align-items": "center", gap: "16px" }}>
          <span
            style={{
              "font-size": "22px",
              "font-weight": 600,
              "letter-spacing": "-0.02em",
              color: "var(--al-ink, #1A1A1A)",
            }}
          >
            {currentYear()}
          </span>
          <div style={{ display: "flex", "align-items": "center", gap: "4px" }}>
            <button
              type="button"
              onClick={handlePrevYear}
              style={{
                width: "28px",
                height: "28px",
                "border-radius": "6px",
                border: "1px solid var(--al-border, #E0E0E0)",
                background: "#FFFFFF",
                cursor: "pointer",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                color: "var(--al-ink-2, #333333)",
              }}
              title="Previous Year"
            >
              ‹
            </button>
            <button
              type="button"
              onClick={handleNextYear}
              style={{
                width: "28px",
                height: "28px",
                "border-radius": "6px",
                border: "1px solid var(--al-border, #E0E0E0)",
                background: "#FFFFFF",
                cursor: "pointer",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                color: "var(--al-ink-2, #333333)",
              }}
              title="Next Year"
            >
              ›
            </button>
            <button
              type="button"
              onClick={handleToday}
              style={{
                height: "28px",
                padding: "0 10px",
                "border-radius": "6px",
                border: "1px solid var(--al-border, #E0E0E0)",
                background: "#FFFFFF",
                cursor: "pointer",
                "font-size": "12px",
                "font-weight": 500,
                color: "var(--al-ink-2, #333333)",
              }}
            >
              Today
            </button>
          </div>
        </div>

        <div style={{ display: "flex", "align-items": "center", gap: "12px" }}>
          <span
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "11px",
              color: "var(--al-ink-7, #888888)",
            }}
          >
            {totalYearEvents()} events scheduled in {currentYear()}
          </span>
        </div>
      </div>

      {/* 12 Months Grid */}
      <div
        style={{
          flex: 1,
          overflow: "auto",
          padding: "24px",
          display: "grid",
          "grid-template-columns": "repeat(auto-fit, minmax(220px, 1fr))",
          gap: "24px",
          "align-content": "start",
        }}
      >
        <For each={months}>
          {(monthIndex) => {
            const yr = currentYear();
            const firstDay = new Date(yr, monthIndex, 1);
            const daysInMonth = new Date(yr, monthIndex + 1, 0).getDate();
            const startDayOfWeek = firstDay.getDay(); // 0 = Sunday

            const monthName = MONTH_NAMES[monthIndex];

            // Build grid cells (42 cells = 6 rows x 7 cols)
            const days: { dayNumber: number | null; date: Date | null }[] = [];
            for (let i = 0; i < startDayOfWeek; i++) {
              days.push({ dayNumber: null, date: null });
            }
            for (let d = 1; d <= daysInMonth; d++) {
              days.push({ dayNumber: d, date: new Date(yr, monthIndex, d) });
            }
            while (days.length % 7 !== 0 || days.length < 35) {
              days.push({ dayNumber: null, date: null });
            }

            return (
              <div
                style={{
                  background: "#FFFFFF",
                  border: "1px solid var(--al-border-soft, #EBEBEB)",
                  "border-radius": "10px",
                  padding: "14px",
                  display: "flex",
                  "flex-direction": "column",
                  gap: "10px",
                  "box-shadow": "0 1px 3px rgba(0,0,0,0.02)",
                }}
              >
                {/* Month Title */}
                <div
                  style={{
                    display: "flex",
                    "align-items": "center",
                    "justify-content": "space-between",
                  }}
                >
                  <span
                    style={{
                      "font-size": "14.5px",
                      "font-weight": 600,
                      "letter-spacing": "-0.01em",
                      color: "var(--al-ink, #1A1A1A)",
                    }}
                  >
                    {monthName}
                  </span>
                </div>

                {/* Weekday headers */}
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "repeat(7, 1fr)",
                    "text-align": "center",
                    gap: "2px",
                  }}
                >
                  <For each={WEEKDAYS_SHORT}>
                    {(w) => (
                      <span
                        style={{
                          "font-family": "var(--al-font-mono)",
                          "font-size": "9.5px",
                          color: "var(--al-ink-7, #A0A0A0)",
                        }}
                      >
                        {w.charAt(0)}
                      </span>
                    )}
                  </For>
                </div>

                {/* Days Grid */}
                <div
                  style={{
                    display: "grid",
                    "grid-template-columns": "repeat(7, 1fr)",
                    gap: "2px",
                  }}
                >
                  <For each={days}>
                    {(cell) => {
                      if (!cell.date || cell.dayNumber === null) {
                        return <div style={{ height: "26px" }} />;
                      }

                      const today = new Date();
                      const isToday = isSameDay(cell.date, today);
                      const isSelected = isSameDay(cell.date, props.selectedDate);
                      const key = toDateKey(cell.date);
                      const dayEvents = () => occurrencesByDate().get(key) || [];
                      const hasEvents = () => dayEvents().length > 0;

                      const handleClick = () => {
                        if (!cell.date) return;
                        props.onSelectDate(cell.date);
                        props.onFocusedDateChange(cell.date);
                        if (props.onNavigateView) {
                          props.onNavigateView("Month");
                        }
                      };

                      return (
                        <div
                          onClick={handleClick}
                          style={{
                            height: "26px",
                            display: "flex",
                            "flex-direction": "column",
                            "align-items": "center",
                            "justify-content": "center",
                            "border-radius": "6px",
                            cursor: "pointer",
                            position: "relative",
                            background: isToday
                              ? "var(--al-today-wash, #EBF3FE)"
                              : isSelected
                                ? "var(--al-surface-2, #F4F4F4)"
                                : hasEvents()
                                  ? "rgba(31, 111, 235, 0.04)"
                                  : "transparent",
                            transition: "background 100ms ease",
                          }}
                          title={
                            hasEvents()
                              ? `${dayEvents().length} event(s): ${dayEvents()
                                  .map((e) => e.event.title)
                                  .join(", ")}`
                              : undefined
                          }
                        >
                          <span
                            style={{
                              "font-size": "11.5px",
                              "font-weight": isToday || isSelected ? 600 : 400,
                              color: isToday
                                ? "var(--al-accent, #1F6FEB)"
                                : isSelected
                                  ? "var(--al-ink, #1A1A1A)"
                                  : "var(--al-ink-2, #333333)",
                            }}
                          >
                            {cell.dayNumber}
                          </span>

                          {/* Event Density Indicator */}
                          <Show when={hasEvents()}>
                            <div
                              style={{
                                width: "4px",
                                height: "4px",
                                "border-radius": "50%",
                                background: isToday
                                  ? "var(--al-accent, #1F6FEB)"
                                  : "var(--al-accent-2, #3b5bdb)",
                                "margin-top": "-1px",
                              }}
                            />
                          </Show>
                        </div>
                      );
                    }}
                  </For>
                </div>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};

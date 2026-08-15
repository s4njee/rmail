import { Component, createMemo, For } from "solid-js";
import { Calendar, Task } from "../types/calendar";
import { addMonths, buildMonthGrid, MONTH_NAMES, WEEKDAYS_LETTER } from "../headless/dateUtils";

export interface SidebarProps {
  focusedDate: Date;
  selectedDate: Date;
  onSelectDate: (d: Date) => void;
  onFocusedDateChange: (d: Date) => void;
  calendars: Calendar[];
  onToggleCalendar: (calendarId: string, enabled: boolean) => void;
  tasks: Task[];
  onToggleTask: (taskId: string) => void;
  onAddTask?: () => void;
  syncSummary?: string;
  lastSyncedText?: string;
  onSettingsClick?: () => void;
  /** Optional right-click hook on a calendar row (hosts render their own menu). */
  onCalendarContextMenu?: (calendar: Calendar, event: MouseEvent) => void;
  /** Fill the parent container (embedded host layout) instead of the fixed
   * 264px standalone column. Used by hosts that embed the sidebar inside their
   * own chrome. */
  fill?: boolean;
}

export const Sidebar: Component<SidebarProps> = (props) => {
  const miniGrid = createMemo(() => {
    const year = props.focusedDate.getFullYear();
    const month = props.focusedDate.getMonth();
    return buildMonthGrid(year, month, props.selectedDate, new Date(), 0);
  });

  const monthTitle = createMemo(() => {
    const m = MONTH_NAMES[props.focusedDate.getMonth()];
    const y = props.focusedDate.getFullYear();
    return `${m} ${y}`;
  });

  const handlePrevMonth = () => {
    props.onFocusedDateChange(addMonths(props.focusedDate, -1));
  };

  const handleNextMonth = () => {
    props.onFocusedDateChange(addMonths(props.focusedDate, 1));
  };

  return (
    <aside
      style={{
        width: props.fill ? "100%" : "264px",
        flex: props.fill ? "1" : "none",
        height: "100%",
        display: "flex",
        "flex-direction": "column",
        background: props.fill
          ? "transparent"
          : "var(--al-sidebar, #F4F4F4)",
        "border-right": props.fill
          ? "none"
          : "1px solid var(--al-border, #E0E0E0)",
        "font-family": "var(--al-font-ui)",
        color: "var(--al-ink, #1A1A1A)",
        "user-select": "none",
        overflow: "hidden",
      }}
    >
      {/* 1. Mini Month */}
      <div style={{ padding: "18px 18px 14px" }}>
        <div
          style={{
            display: "flex",
            "align-items": "baseline",
            "justify-content": "space-between",
            "margin-bottom": "12px",
          }}
        >
          <span style={{ "font-size": "14px", "font-weight": 600, "letter-spacing": "-0.01em" }}>
            {monthTitle()}
          </span>
          <div
            style={{
              display: "flex",
              gap: "12px",
              color: "var(--al-ink-7, #A0A0A0)",
              "font-family": "var(--al-font-mono)",
              "font-size": "12px",
            }}
          >
            <button
              type="button"
              onClick={handlePrevMonth}
              style={{
                background: "none",
                border: "none",
                cursor: "pointer",
                color: "inherit",
                padding: "0 2px",
              }}
            >
              ‹
            </button>
            <button
              type="button"
              onClick={handleNextMonth}
              style={{
                background: "none",
                border: "none",
                cursor: "pointer",
                color: "inherit",
                padding: "0 2px",
              }}
            >
              ›
            </button>
          </div>
        </div>

        {/* Weekday headers */}
        <div
          style={{
            display: "grid",
            "grid-template-columns": "repeat(7, 1fr)",
            gap: "1px 0",
            "font-family": "var(--al-font-mono)",
            "font-size": "9.5px",
            color: "var(--al-ink-7, #A0A0A0)",
            "letter-spacing": "0.06em",
            "margin-bottom": "4px",
          }}
        >
          <For each={WEEKDAYS_LETTER}>
            {(letter) => <div style={{ "text-align": "center" }}>{letter}</div>}
          </For>
        </div>

        {/* 42-cell grid */}
        <div style={{ display: "grid", "grid-template-columns": "repeat(7, 1fr)", gap: "2px" }}>
          <For each={miniGrid()}>
            {(cell) => {
              const bg = () => {
                if (cell.isToday) return "var(--al-accent, #1F6FEB)";
                if (cell.isSelected) return "var(--al-accent-tint, #E4EBF8)";
                return "transparent";
              };
              const fg = () => {
                if (cell.isToday) return "#FFFFFF";
                if (!cell.isCurrentMonth) return "var(--al-ink-9, #BFBFBF)";
                return "var(--al-ink-2, #424242)";
              };
              const fw = () => (cell.isToday || cell.isSelected ? 500 : 400);

              return (
                <button
                  type="button"
                  onClick={() => props.onSelectDate(cell.date)}
                  style={{
                    height: "27px",
                    display: "flex",
                    "align-items": "center",
                    "justify-content": "center",
                    "border-radius": "6px",
                    "font-family": "var(--al-font-mono)",
                    "font-size": "11.5px",
                    color: fg(),
                    background: bg(),
                    "font-weight": fw(),
                    border: "none",
                    cursor: "pointer",
                    padding: 0,
                  }}
                >
                  {cell.dayNumber}
                </button>
              );
            }}
          </For>
        </div>
      </div>

      <div style={{ height: "1px", background: "var(--al-border, #E0E0E0)", margin: "0 18px" }} />

      {/* 2. Calendars Section */}
      <div style={{ padding: "16px 18px", "max-height": "220px", "overflow-y": "auto" }}>
        <div
          style={{
            "font-family": "var(--al-font-mono)",
            "font-size": "9.5px",
            "letter-spacing": "0.12em",
            color: "var(--al-ink-7, #A0A0A0)",
            "margin-bottom": "10px",
          }}
        >
          CALENDARS
        </div>
        <div style={{ display: "flex", "flex-direction": "column", gap: "9px" }}>
          <For each={props.calendars}>
            {(cal) => (
              <button
                type="button"
                onClick={() => props.onToggleCalendar(cal.id, !cal.enabled)}
                onContextMenu={(e) => {
                  if (props.onCalendarContextMenu) {
                    e.preventDefault();
                    props.onCalendarContextMenu(cal, e);
                  }
                }}
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "9px",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                  padding: 0,
                  "text-align": "left",
                  width: "100%",
                }}
              >
                <div
                  style={{
                    width: "12px",
                    height: "12px",
                    "border-radius": "3.5px",
                    flex: "none",
                    background: cal.enabled ? cal.color : "transparent",
                    border: `1.5px solid ${cal.color}`,
                    opacity: cal.enabled ? 1 : 0.5,
                  }}
                />
                <span
                  style={{
                    "font-size": "12.5px",
                    color: cal.enabled ? "var(--al-ink, #1A1A1A)" : "var(--al-ink-7, #A0A0A0)",
                  }}
                >
                  {cal.name}
                </span>
                <div style={{ flex: 1 }} />
                <span
                  style={{
                    "font-family": "var(--al-font-mono)",
                    "font-size": "10.5px",
                    color: "var(--al-ink-9, #BFBFBF)",
                  }}
                >
                  {cal.eventCount}
                </span>
              </button>
            )}
          </For>
        </div>
      </div>

      <div style={{ height: "1px", background: "var(--al-border, #E0E0E0)", margin: "0 18px" }} />

      {/* 3. Tasks Section */}
      <div style={{ padding: "16px 18px", flex: 1, "min-height": 0, "overflow-y": "auto" }}>
        <div
          style={{
            display: "flex",
            "align-items": "center",
            "justify-content": "space-between",
            "margin-bottom": "10px",
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
            TASKS
          </span>
          <button
            type="button"
            onClick={() => props.onAddTask?.()}
            style={{
              background: "none",
              border: "none",
              cursor: "pointer",
              "font-family": "var(--al-font-mono)",
              "font-size": "11px",
              color: "var(--al-ink-7, #A0A0A0)",
              padding: "0 2px",
            }}
          >
            +
          </button>
        </div>
        <div style={{ display: "flex", "flex-direction": "column", gap: "11px" }}>
          <For each={props.tasks}>
            {(task) => {
              const isDone = () => !!task.completedAt;
              const isOverdue = () => {
                if (isDone() || !task.dueAt) return false;
                return new Date(task.dueAt).getTime() < Date.now();
              };

              const ringColor = () => {
                if (isDone()) return "var(--al-ink-9, #BFBFBF)";
                if (isOverdue()) return "var(--al-cal-classes, #C2410C)";
                return "var(--al-ink-8, #B3B3B3)";
              };

              const metaColor = () => {
                if (isOverdue()) return "var(--al-cal-classes, #C2410C)";
                return "var(--al-ink-7, #A0A0A0)";
              };

              const dueText = () => {
                if (isDone()) return "Done";
                if (!task.dueAt) return "No due date";
                const due = new Date(task.dueAt);
                return `Due ${due.toLocaleDateString(undefined, { month: "short", day: "numeric" })}`;
              };

              return (
                <div style={{ display: "flex", "align-items": "flex-start", gap: "9px" }}>
                  <button
                    type="button"
                    onClick={() => props.onToggleTask(task.id)}
                    style={{
                      width: "13px",
                      height: "13px",
                      "margin-top": "2px",
                      "border-radius": "50%",
                      flex: "none",
                      border: `1.5px solid ${ringColor()}`,
                      background: isDone() ? "var(--al-ink-9, #BFBFBF)" : "transparent",
                      cursor: "pointer",
                      padding: 0,
                    }}
                  />
                  <div
                    style={{
                      display: "flex",
                      "flex-direction": "column",
                      gap: "2px",
                      "min-width": 0,
                    }}
                  >
                    <span
                      style={{
                        "font-size": "12.5px",
                        "line-height": 1.3,
                        color: isDone() ? "var(--al-ink-7, #A0A0A0)" : "var(--al-ink, #1A1A1A)",
                        "text-decoration": isDone() ? "line-through" : "none",
                        "word-break": "break-word",
                      }}
                    >
                      {task.title}
                    </span>
                    <span
                      style={{
                        "font-family": "var(--al-font-mono)",
                        "font-size": "10px",
                        color: metaColor(),
                      }}
                    >
                      {dueText()}
                    </span>
                  </div>
                </div>
              );
            }}
          </For>
        </div>
      </div>

      {/* 4. Sync Footer */}
      <div
        style={{
          padding: "13px 18px",
          "border-top": "1px solid var(--al-border, #E0E0E0)",
          display: "flex",
          "align-items": "center",
          gap: "9px",
        }}
      >
        <div
          style={{
            width: "7px",
            height: "7px",
            "border-radius": "50%",
            background: "var(--al-cal-work, #0F766E)",
            flex: "none",
          }}
        />
        <div style={{ display: "flex", "flex-direction": "column", gap: "1px" }}>
          <span style={{ "font-size": "11.5px", "font-weight": 500 }}>
            {props.syncSummary || "Local store synced"}
          </span>
          <span
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "9.5px",
              color: "var(--al-ink-7, #A0A0A0)",
            }}
          >
            {props.lastSyncedText || "offline store ready"}
          </span>
        </div>
        <div style={{ flex: 1 }} />
        <button
          type="button"
          onClick={() => props.onSettingsClick?.()}
          style={{
            background: "none",
            border: "none",
            cursor: "pointer",
            padding: "4px",
            display: "flex",
            "flex-direction": "column",
            gap: "2.5px",
            "align-items": "center",
          }}
        >
          <div
            style={{
              width: "3px",
              height: "3px",
              "border-radius": "50%",
              background: "var(--al-ink-7, #A0A0A0)",
            }}
          />
          <div
            style={{
              width: "3px",
              height: "3px",
              "border-radius": "50%",
              background: "var(--al-ink-7, #A0A0A0)",
            }}
          />
          <div
            style={{
              width: "3px",
              height: "3px",
              "border-radius": "50%",
              background: "var(--al-ink-7, #A0A0A0)",
            }}
          />
        </button>
      </div>
    </aside>
  );
};

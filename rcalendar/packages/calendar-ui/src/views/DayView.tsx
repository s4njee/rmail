import { Component, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { Calendar, OccurrenceItem, Task } from '../types/calendar';
import {
  addDays,
  formatTime24,
  getWeekNumber,
  isSameDay,
  MONTH_NAMES,
  WEEKDAYS,
} from '../headless/dateUtils';
import {
  computeNowLinePosition,
  GridConfig,
  positionEventsForDay,
} from '../headless/layout';
import {
  computeDragToCreateRange,
  computeMovedRange,
  computeResizedEnd,
} from '../headless/dragEngine';

export interface DayViewProps {
  focusedDate: Date;
  selectedDate: Date;
  onSelectDate: (d: Date) => void;
  onFocusedDateChange: (d: Date) => void;
  occurrences: OccurrenceItem[];
  calendars: Calendar[];
  tasks: Task[];
  onToggleTask?: (taskId: string) => void;
  onEventClick?: (item: OccurrenceItem) => void;
  onSlotClick?: (date: Date, hour: number) => void;
  onAddToDay?: (date: Date) => void;
  onEventMove?: (item: OccurrenceItem, newStart: Date, newEnd: Date) => void;
  onEventResize?: (item: OccurrenceItem, newEnd: Date) => void;
  onRangeCreate?: (startsAt: Date, endsAt: Date) => void;
}

const GRID_CONFIG: GridConfig = {
  startHour: 7,
  endHour: 21,
  rowPitch: 49,
  minHeight: 28,
};

export const DayView: Component<DayViewProps> = (props) => {
  const [currentTime, setCurrentTime] = createSignal(new Date());

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
    startY: number;
    currentY: number;
  } | null>(null);

  onMount(() => {
    const timer = setInterval(() => {
      setCurrentTime(new Date());
    }, 60000);
    onCleanup(() => clearInterval(timer));
  });

  const dayNumber = createMemo(() => props.focusedDate.getDate());
  const weekdayName = createMemo(() => WEEKDAYS[props.focusedDate.getDay()]);
  const monthName = createMemo(() => MONTH_NAMES[props.focusedDate.getMonth()]);
  const year = createMemo(() => props.focusedDate.getFullYear());
  const weekNum = createMemo(() => getWeekNumber(props.focusedDate));

  const calendarMap = createMemo(() => {
    const map = new Map<string, Calendar>();
    for (const c of props.calendars) {
      map.set(c.id, c);
    }
    return map;
  });

  const dayEvents = createMemo(() => {
    return positionEventsForDay(props.occurrences, props.focusedDate, GRID_CONFIG);
  });

  const dueTodayTasks = createMemo(() => {
    return props.tasks.filter((t) => {
      if (!t.dueAt) return false;
      return isSameDay(new Date(t.dueAt), props.focusedDate);
    });
  });

  const repeatingEvents = createMemo(() => {
    return props.occurrences.filter((o) => !!o.event.rrule);
  });

  const nowLinePos = createMemo(() => {
    if (!isSameDay(props.focusedDate, currentTime())) return null;
    return computeNowLinePosition(currentTime(), GRID_CONFIG);
  });

  const hoursList = createMemo(() => {
    const list: { hour: number; label: string }[] = [];
    for (let h = GRID_CONFIG.startHour; h <= GRID_CONFIG.endHour; h++) {
      const label = `${String(h).padStart(2, '0')}:00`;
      list.push({ hour: h, label });
    }
    return list;
  });

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
      const { startsAt, endsAt } = computeDragToCreateRange(
        props.focusedDate,
        c.startY,
        c.currentY,
        { rowPitch: GRID_CONFIG.rowPitch, startHour: GRID_CONFIG.startHour }
      );
      props.onRangeCreate?.(startsAt, endsAt);
    }
    setCreatingRange(null);
  };

  const handlePrev = () => {
    const prev = addDays(props.focusedDate, -1);
    props.onFocusedDateChange(prev);
    props.onSelectDate(prev);
  };

  const handleNext = () => {
    const next = addDays(props.focusedDate, 1);
    props.onFocusedDateChange(next);
    props.onSelectDate(next);
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
        display: 'flex',
        'min-width': 0,
        background: 'var(--al-surface, #FFFFFF)',
        'font-family': 'var(--al-font-ui)',
        color: 'var(--al-ink, #1A1A1A)',
        height: '100%',
        overflow: 'hidden',
        'user-select': (draggingItem() || resizingItem() || creatingRange()) ? 'none' : 'auto',
      }}
    >
      {/* Main Day Pane */}
      <div style={{ flex: 1, display: 'flex', 'flex-direction': 'column', 'min-width': 0 }}>
        {/* Header (106px) */}
        <div
          style={{
            height: '106px',
            flex: 'none',
            display: 'flex',
            'align-items': 'center',
            gap: '20px',
            padding: '0 26px',
            'border-bottom': '1px solid var(--al-border-soft, #E5E5E5)',
          }}
        >
          <span style={{ 'font-size': '76px', 'font-weight': 400, 'letter-spacing': '-0.045em', color: 'var(--al-ink, #1A1A1A)', 'line-height': 1 }}>
            {dayNumber()}
          </span>
          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '3px' }}>
            <span style={{ 'font-size': '22px', 'font-weight': 500, 'letter-spacing': '-0.02em', color: 'var(--al-ink, #1A1A1A)', 'line-height': 1 }}>
              {weekdayName()}
            </span>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '12px', color: 'var(--al-ink-7, #A0A0A0)' }}>
              {monthName()} {year()} · week {weekNum()}
            </span>
          </div>

          <div style={{ flex: 1 }} />

          {/* Stepper */}
          <div
            style={{
              display: 'flex',
              'align-items': 'center',
              height: '30px',
              border: '1px solid var(--al-border, #E0E0E0)',
              'border-radius': '8px',
              overflow: 'hidden',
            }}
          >
            <button
              type="button"
              onClick={handlePrev}
              style={{ width: '32px', height: '100%', display: 'flex', 'align-items': 'center', 'justify-content': 'center', 'font-family': 'var(--al-font-mono)', 'font-size': '13px', color: 'var(--al-ink-5, #777777)', background: 'none', border: 'none', cursor: 'pointer' }}
            >
              ‹
            </button>
            <div style={{ width: '1px', height: '100%', background: 'var(--al-border, #E0E0E0)' }} />
            <button
              type="button"
              onClick={handleToday}
              style={{ padding: '0 13px', height: '100%', display: 'flex', 'align-items': 'center', 'font-size': '12.5px', 'font-weight': 500, color: 'var(--al-ink, #1A1A1A)', background: 'none', border: 'none', cursor: 'pointer' }}
            >
              Today
            </button>
            <div style={{ width: '1px', height: '100%', background: 'var(--al-border, #E0E0E0)' }} />
            <button
              type="button"
              onClick={handleNext}
              style={{ width: '32px', height: '100%', display: 'flex', 'align-items': 'center', 'justify-content': 'center', 'font-family': 'var(--al-font-mono)', 'font-size': '13px', color: 'var(--al-ink-5, #777777)', background: 'none', border: 'none', cursor: 'pointer' }}
            >
              ›
            </button>
          </div>
        </div>

        {/* Time Grid (49px pitch) */}
        <div style={{ flex: 1, display: 'flex', 'min-height': 0, overflow: 'hidden' }}>
          {/* Gutter */}
          <div style={{ width: '76px', flex: 'none', 'border-right': '1px solid var(--al-grid, #EBEBEB)' }}>
            <For each={hoursList()}>
              {(h) => (
                <div style={{ height: `${GRID_CONFIG.rowPitch}px`, display: 'flex', 'align-items': 'flex-start', 'justify-content': 'flex-end', 'padding-right': '12px' }}>
                  <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11px', color: 'var(--al-ink-8, #B3B3B3)', transform: 'translateY(-6px)' }}>
                    {h.label}
                  </span>
                </div>
              )}
            </For>
          </div>

          {/* Grid column */}
          <div
            onMouseDown={(e) => {
              if (e.target !== e.currentTarget && !(e.target as HTMLElement).classList.contains('hour-cell')) return;
              const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
              const startY = e.clientY - rect.top;
              setCreatingRange({ startY, currentY: startY });
            }}
            style={{ flex: 1, 'min-width': 0, position: 'relative', 'overflow-y': 'auto' }}
          >
            <For each={hoursList()}>
              {(h) => (
                <div
                  class="hour-cell"
                  onClick={() => props.onSlotClick?.(props.focusedDate, h.hour)}
                  style={{
                    height: `${GRID_CONFIG.rowPitch - 1}px`,
                    'border-bottom': '1px solid var(--al-grid-hour, #F2F2F2)',
                    cursor: 'pointer',
                  }}
                />
              )}
            </For>

            {/* Event blocks */}
            <For each={dayEvents()}>
              {(pe) => {
                const cal = () => calendarMap().get(pe.item.event.calendarId);
                const color = () => cal()?.color || '#1F6FEB';
                const tint = () => {
                  const c = color();
                  return c.startsWith('#') ? `${c}1A` : 'rgba(31,111,235,0.10)';
                };

                const isBeingDragged = () => draggingItem()?.item.event.id === pe.item.event.id;
                const isBeingResized = () => resizingItem()?.item.event.id === pe.item.event.id;

                const effectiveTop = () => {
                  if (isBeingDragged()) return pe.top + (draggingItem()?.deltaY || 0);
                  return pe.top;
                };

                const effectiveHeight = () => {
                  if (isBeingResized()) return Math.max(28, pe.height + (resizingItem()?.deltaY || 0));
                  return pe.height;
                };

                const timeStr = () => {
                  const s = new Date(pe.item.occurrence.startsAt);
                  const e = new Date(pe.item.occurrence.endsAt);
                  return `${formatTime24(s)} – ${formatTime24(e)}`;
                };

                const metaBadge = () => {
                  if (pe.item.event.rrule) return 'REPEATS';
                  return null;
                };

                const handleBlockMouseDown = (e: MouseEvent) => {
                  if ((e.target as HTMLElement).classList.contains('resize-handle')) return;
                  e.stopPropagation();
                  setDraggingItem({ item: pe.item, startY: e.clientY, deltaY: 0 });
                };

                const handleResizeMouseDown = (e: MouseEvent) => {
                  e.stopPropagation();
                  setResizingItem({ item: pe.item, startY: e.clientY, deltaY: 0 });
                };

                return (
                  <div
                    onMouseDown={handleBlockMouseDown}
                    onClick={(e) => {
                      e.stopPropagation();
                      if (!draggingItem() && !resizingItem()) {
                        props.onEventClick?.(pe.item);
                      }
                    }}
                    style={{
                      position: 'absolute',
                      top: `${effectiveTop()}px`,
                      height: `${effectiveHeight()}px`,
                      left: `calc(${pe.leftPercent}% + 8px)`,
                      width: `calc(${pe.widthPercent}% - 28px)`,
                      padding: '12px 16px',
                      'border-radius': '9px',
                      background: tint(),
                      'border-left': `3px solid ${color()}`,
                      'box-shadow': 'var(--al-shadow-event-day, 0 1px 3px rgba(0,0,0,0.06))',
                      overflow: 'hidden',
                      cursor: isBeingDragged() ? 'grabbing' : 'grab',
                      'z-index': isBeingDragged() || isBeingResized() ? 30 : 10,
                      opacity: isBeingDragged() ? 0.85 : 1,
                    }}
                  >
                    <div style={{ display: 'flex', 'align-items': 'flex-start', gap: '14px', overflow: 'hidden' }}>
                      <div style={{ display: 'flex', 'flex-direction': 'column', gap: '3px', 'min-width': 0 }}>
                        <span style={{ 'font-size': '15px', 'font-weight': 500, 'letter-spacing': '-0.01em', color: 'var(--al-ink-event, #232323)', 'line-height': 1.2 }}>
                          {pe.item.event.title}
                        </span>
                        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11px', color: color() }}>
                          {timeStr()}
                        </span>
                        <Show when={pe.item.event.location}>
                          <span style={{ 'font-size': '12px', color: 'var(--al-ink-4, #666666)' }}>
                            {pe.item.event.location}
                          </span>
                        </Show>
                      </div>

                      <div style={{ flex: 1 }} />

                      <Show when={metaBadge()}>
                        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.06em', color: color(), flex: 'none' }}>
                          {metaBadge()}
                        </span>
                      </Show>
                    </div>

                    {/* Edge resize handle */}
                    <div
                      class="resize-handle"
                      onMouseDown={handleResizeMouseDown}
                      style={{
                        position: 'absolute',
                        bottom: 0,
                        left: 0,
                        right: 0,
                        height: '6px',
                        cursor: 'ns-resize',
                      }}
                    />
                  </div>
                );
              }}
            </For>

            {/* Drag to create preview */}
            <Show when={creatingRange()}>
              <div
                style={{
                  position: 'absolute',
                  top: `${Math.min(creatingRange()!.startY, creatingRange()!.currentY)}px`,
                  height: `${Math.abs(creatingRange()!.currentY - creatingRange()!.startY)}px`,
                  left: '8px',
                  right: '28px',
                  'border-radius': '9px',
                  background: 'var(--al-accent-tint, #E4EBF8)',
                  border: '1.5px dashed var(--al-accent, #1F6FEB)',
                  'pointer-events': 'none',
                  'z-index': 25,
                }}
              />
            </Show>

            {/* Now-line */}
            <Show when={nowLinePos() !== null}>
              <div
                style={{
                  position: 'absolute',
                  left: 0,
                  right: 0,
                  top: `${nowLinePos()}px`,
                  height: '1.5px',
                  background: 'var(--al-accent, #1F6FEB)',
                  'z-index': 20,
                  'pointer-events': 'none',
                }}
              >
                <div style={{ position: 'absolute', left: '-4px', top: '-3.5px', width: '8px', height: '8px', 'border-radius': '50%', background: 'var(--al-accent, #1F6FEB)' }} />
              </div>
            </Show>
          </div>
        </div>
      </div>

      {/* Right Rail (300px) */}
      <div
        style={{
          width: '300px',
          flex: 'none',
          'border-left': '1px solid var(--al-border-soft, #E5E5E5)',
          background: 'var(--al-surface-2, #FBFBFB)',
          display: 'flex',
          'flex-direction': 'column',
          'overflow-y': 'auto',
        }}
      >
        {/* DUE TODAY */}
        <div style={{ padding: '20px 20px 14px' }}>
          <div style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.12em', color: 'var(--al-ink-7, #A0A0A0)', 'margin-bottom': '12px' }}>
            DUE TODAY
          </div>
          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '10px' }}>
            <For each={dueTodayTasks()} fallback={<span style={{ 'font-size': '12px', color: 'var(--al-ink-7, #A0A0A0)' }}>No tasks due today</span>}>
              {(task) => {
                const isDone = () => !!task.completedAt;
                return (
                  <div style={{ display: 'flex', 'align-items': 'flex-start', gap: '9px', padding: '10px 11px', background: '#FFFFFF', border: '1px solid var(--al-border-soft, #E5E5E5)', 'border-radius': '8px' }}>
                    <button
                      type="button"
                      onClick={() => props.onToggleTask?.(task.id)}
                      style={{
                        width: '13px',
                        height: '13px',
                        'margin-top': '2px',
                        'border-radius': '50%',
                        border: '1.5px solid var(--al-cal-classes, #C2410C)',
                        background: isDone() ? 'var(--al-ink-9, #BFBFBF)' : 'transparent',
                        flex: 'none',
                        cursor: 'pointer',
                        padding: 0,
                      }}
                    />
                    <div style={{ display: 'flex', 'flex-direction': 'column', gap: '2px' }}>
                      <span style={{ 'font-size': '12.5px', 'font-weight': 500, 'text-decoration': isDone() ? 'line-through' : 'none' }}>
                        {task.title}
                      </span>
                    </div>
                  </div>
                );
              }}
            </For>
          </div>
        </div>

        <div style={{ height: '1px', background: 'var(--al-border-soft, #E5E5E5)', margin: '0 20px' }} />

        {/* REPEATS */}
        <div style={{ padding: '18px 20px' }}>
          <div style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.12em', color: 'var(--al-ink-7, #A0A0A0)', 'margin-bottom': '12px' }}>
            REPEATS
          </div>
          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '11px' }}>
            <For each={repeatingEvents()} fallback={<span style={{ 'font-size': '12px', color: 'var(--al-ink-7, #A0A0A0)' }}>No repeating events</span>}>
              {(item) => (
                <div style={{ display: 'flex', 'flex-direction': 'column', gap: '2px' }}>
                  <span style={{ 'font-size': '12.5px' }}>{item.event.title}</span>
                  <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', color: 'var(--al-ink-7, #A0A0A0)' }}>
                    {item.event.rrule}
                  </span>
                </div>
              )}
            </For>
          </div>
        </div>

        <div style={{ flex: 1 }} />

        {/* Footer */}
        <div style={{ padding: '16px 20px', 'border-top': '1px solid var(--al-border-soft, #E5E5E5)' }}>
          <button
            type="button"
            onClick={() => props.onAddToDay?.(props.focusedDate)}
            style={{
              display: 'flex',
              'align-items': 'center',
              'justify-content': 'center',
              width: '100%',
              height: '32px',
              border: '1px dashed var(--al-dashed, #CACACA)',
              'border-radius': '8px',
              'font-size': '12px',
              color: 'var(--al-ink-5, #777777)',
              background: 'none',
              cursor: 'pointer',
            }}
          >
            Add to this day
          </button>
        </div>
      </div>
    </div>
  );
};

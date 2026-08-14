import { Component, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { Calendar, OccurrenceItem } from '../types/calendar';
import {
  addDays,
  formatTime24,
  isSameDay,
  MONTH_NAMES_SHORT,
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

export interface ThreeDayViewProps {
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
}

const GRID_CONFIG: GridConfig = {
  startHour: 7,
  endHour: 21,
  rowPitch: 47,
  minHeight: 24,
};

export const ThreeDayView: Component<ThreeDayViewProps> = (props) => {
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
    day: Date;
    startY: number;
    currentY: number;
  } | null>(null);

  onMount(() => {
    const timer = setInterval(() => {
      setCurrentTime(new Date());
    }, 60000);
    onCleanup(() => clearInterval(timer));
  });

  const threeDays = createMemo(() => {
    return [
      props.focusedDate,
      addDays(props.focusedDate, 1),
      addDays(props.focusedDate, 2),
    ];
  });

  const headerTitle = createMemo(() => {
    const days = threeDays();
    const start = days[0];
    const end = days[2];
    const sWk = WEEKDAYS[start.getDay()].slice(0, 3);
    const eWk = WEEKDAYS[end.getDay()].slice(0, 3);
    const m = MONTH_NAMES_SHORT[start.getMonth()];
    const y = start.getFullYear();

    return `${sWk} ${start.getDate()} – ${eWk} ${end.getDate()} ${m} ${y}`;
  });

  const calendarMap = createMemo(() => {
    const map = new Map<string, Calendar>();
    for (const c of props.calendars) {
      map.set(c.id, c);
    }
    return map;
  });

  const nowLinePos = createMemo(() => {
    return computeNowLinePosition(currentTime(), GRID_CONFIG);
  });

  const currentTimeText = createMemo(() => {
    return formatTime24(currentTime());
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
        c.day,
        c.startY,
        c.currentY,
        { rowPitch: GRID_CONFIG.rowPitch, startHour: GRID_CONFIG.startHour }
      );
      props.onRangeCreate?.(startsAt, endsAt);
    }
    setCreatingRange(null);
  };

  const handlePrev = () => {
    props.onFocusedDateChange(addDays(props.focusedDate, -3));
  };

  const handleNext = () => {
    props.onFocusedDateChange(addDays(props.focusedDate, 3));
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
        'flex-direction': 'column',
        'min-width': 0,
        background: 'var(--al-surface, #FFFFFF)',
        'font-family': 'var(--al-font-ui)',
        color: 'var(--al-ink, #1A1A1A)',
        height: '100%',
        overflow: 'hidden',
        'user-select': (draggingItem() || resizingItem() || creatingRange()) ? 'none' : 'auto',
      }}
    >
      {/* Header */}
      <div
        style={{
          height: '78px',
          flex: 'none',
          display: 'flex',
          'align-items': 'center',
          gap: '18px',
          padding: '0 26px',
          'border-bottom': '1px solid var(--al-border-soft, #E5E5E5)',
        }}
      >
        <span style={{ 'font-size': '34px', 'font-weight': 500, 'letter-spacing': '-0.03em', color: 'var(--al-ink, #1A1A1A)', 'line-height': 1 }}>
          {headerTitle()}
        </span>

        <div style={{ flex: 1 }} />

        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11.5px', color: 'var(--al-ink-6, #888888)' }}>
          next 3 days
        </span>

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

      {/* Column Headers (56px) */}
      <div style={{ height: '56px', flex: 'none', display: 'flex', 'border-bottom': '1px solid var(--al-border-soft, #E5E5E5)' }}>
        <div style={{ width: '70px', flex: 'none', 'border-right': '1px solid var(--al-grid, #EBEBEB)' }} />
        <For each={threeDays()}>
          {(day) => {
            const today = new Date();
            const isToday = () => isSameDay(day, today);
            const isSel = () => isSameDay(day, props.selectedDate);
            const events = () => positionEventsForDay(props.occurrences, day, GRID_CONFIG);

            return (
              <div
                onClick={() => props.onSelectDate(day)}
                style={{
                  flex: 1,
                  'min-width': 0,
                  'border-right': '1px solid var(--al-grid, #EBEBEB)',
                  display: 'flex',
                  'align-items': 'center',
                  gap: '10px',
                  'padding-left': '16px',
                  background: isToday() ? 'var(--al-today-wash, #FBFCFE)' : isSel() ? 'var(--al-surface-2, #FBFBFB)' : '#FFFFFF',
                  cursor: 'pointer',
                }}
              >
                <span
                  style={{
                    'font-size': '24px',
                    'font-weight': 500,
                    'letter-spacing': '-0.02em',
                    color: isToday() ? 'var(--al-accent, #1F6FEB)' : 'var(--al-ink, #1A1A1A)',
                  }}
                >
                  {day.getDate()}
                </span>
                <div style={{ display: 'flex', 'flex-direction': 'column' }}>
                  <span style={{ 'font-size': '12px', 'font-weight': 500, color: isToday() ? 'var(--al-accent, #1F6FEB)' : 'var(--al-ink, #1A1A1A)' }}>
                    {WEEKDAYS[day.getDay()]}
                  </span>
                  <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', color: 'var(--al-ink-7, #A0A0A0)' }}>
                    {events().length} events
                  </span>
                </div>
              </div>
            );
          }}
        </For>
      </div>

      {/* Time Grid */}
      <div style={{ flex: 1, display: 'flex', 'min-height': 0, position: 'relative', 'overflow-y': 'auto' }}>
        {/* Gutter */}
        <div style={{ width: '70px', flex: 'none', 'border-right': '1px solid var(--al-grid, #EBEBEB)' }}>
          <For each={hoursList()}>
            {(h) => (
              <div style={{ height: `${GRID_CONFIG.rowPitch}px`, display: 'flex', 'align-items': 'flex-start', 'justify-content': 'flex-end', 'padding-right': '11px' }}>
                <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10.5px', color: 'var(--al-ink-8, #B3B3B3)', transform: 'translateY(-6px)' }}>
                  {h.label}
                </span>
              </div>
            )}
          </For>
        </div>

        {/* 3 Day Columns */}
        <For each={threeDays()}>
          {(day) => {
            const today = new Date();
            const isToday = () => isSameDay(day, today);
            const dayEvents = () => positionEventsForDay(props.occurrences, day, GRID_CONFIG);

            const handleColumnMouseDown = (e: MouseEvent) => {
              if (e.target !== e.currentTarget && !(e.target as HTMLElement).classList.contains('hour-cell')) return;
              const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
              const startY = e.clientY - rect.top;
              setCreatingRange({ day, startY, currentY: startY });
            };

            return (
              <div
                onMouseDown={handleColumnMouseDown}
                style={{
                  flex: 1,
                  'min-width': 0,
                  'border-right': '1px solid var(--al-grid, #EBEBEB)',
                  position: 'relative',
                  background: isToday() ? 'var(--al-today-wash-2, #FDFDFE)' : 'transparent',
                }}
              >
                {/* Horizontal lines */}
                <For each={hoursList()}>
                  {(h) => (
                    <div
                      class="hour-cell"
                      onClick={() => props.onSlotClick?.(day, h.hour)}
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
                      if (isBeingResized()) return Math.max(24, pe.height + (resizingItem()?.deltaY || 0));
                      return pe.height;
                    };

                    const timeStr = () => {
                      const s = new Date(pe.item.occurrence.startsAt);
                      const e = new Date(pe.item.occurrence.endsAt);
                      return `${formatTime24(s)} – ${formatTime24(e)}`;
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
                          left: `calc(${pe.leftPercent}% + 3px)`,
                          width: `calc(${pe.widthPercent}% - 6px)`,
                          padding: '9px 12px',
                          'border-radius': '6px',
                          background: tint(),
                          'border-left': `2.5px solid ${color()}`,
                          'box-shadow': 'var(--al-shadow-event-week, 0 1px 2px rgba(0,0,0,0.05))',
                          overflow: 'hidden',
                          cursor: isBeingDragged() ? 'grabbing' : 'grab',
                          'z-index': isBeingDragged() || isBeingResized() ? 30 : 10,
                          opacity: isBeingDragged() ? 0.85 : 1,
                          display: 'flex',
                          'flex-direction': 'column',
                          gap: '2px',
                        }}
                      >
                        <span style={{ 'font-size': '13px', 'font-weight': 500, 'line-height': 1.25, color: 'var(--al-ink-event, #232323)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}>
                          {pe.item.event.title}
                        </span>
                        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', color: color() }}>
                          {timeStr()}
                        </span>
                        <Show when={pe.item.event.location}>
                          <span style={{ 'font-size': '11px', color: 'var(--al-ink-4, #666666)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}>
                            {pe.item.event.location}
                          </span>
                        </Show>

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

                {/* Drag to create preview box */}
                <Show when={creatingRange() && isSameDay(creatingRange()!.day, day)}>
                  <div
                    style={{
                      position: 'absolute',
                      top: `${Math.min(creatingRange()!.startY, creatingRange()!.currentY)}px`,
                      height: `${Math.abs(creatingRange()!.currentY - creatingRange()!.startY)}px`,
                      left: '4px',
                      right: '4px',
                      'border-radius': '6px',
                      background: 'var(--al-accent-tint, #E4EBF8)',
                      border: '1.5px dashed var(--al-accent, #1F6FEB)',
                      'pointer-events': 'none',
                      'z-index': 25,
                    }}
                  />
                </Show>

                {/* Now-line with time badge */}
                <Show when={isToday() && nowLinePos() !== null}>
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
                    <div
                      style={{
                        position: 'absolute',
                        right: '6px',
                        top: '-8px',
                        'font-family': 'var(--al-font-mono)',
                        'font-size': '9.5px',
                        color: 'var(--al-accent, #1F6FEB)',
                        background: '#FFFFFF',
                        padding: '1px 4px',
                        'border-radius': '3px',
                        'box-shadow': '0 1px 2px rgba(0,0,0,0.1)',
                      }}
                    >
                      {currentTimeText()}
                    </div>
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

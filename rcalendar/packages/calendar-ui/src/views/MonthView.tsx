import { Component, createMemo, For, Show } from 'solid-js';
import { Calendar, OccurrenceItem } from '../types/calendar';
import {
  addMonths,
  buildMonthGrid,
  formatTime24,
  getWeekNumber,
  MONTH_NAMES,
  toDateKey,
  WEEKDAYS_SHORT,
} from '../headless/dateUtils';

export interface MonthViewProps {
  focusedDate: Date;
  selectedDate: Date;
  onSelectDate: (d: Date) => void;
  onFocusedDateChange: (d: Date) => void;
  occurrences: OccurrenceItem[];
  calendars: Calendar[];
  onEventClick?: (item: OccurrenceItem) => void;
  onCellClick?: (date: Date) => void;
}

export const MonthView: Component<MonthViewProps> = (props) => {
  const monthGrid = createMemo(() => {
    const y = props.focusedDate.getFullYear();
    const m = props.focusedDate.getMonth();
    return buildMonthGrid(y, m, props.selectedDate, new Date(), 0);
  });

  const monthTitle = createMemo(() => {
    return MONTH_NAMES[props.focusedDate.getMonth()];
  });

  const yearTitle = createMemo(() => {
    return props.focusedDate.getFullYear();
  });

  const weekNum = createMemo(() => {
    return getWeekNumber(props.focusedDate);
  });

  const activeCalCount = createMemo(() => {
    return props.calendars.filter((c) => c.enabled).length;
  });

  // Map of dateString (YYYY-MM-DD) -> OccurrenceItem[]
  const occurrencesByDate = createMemo(() => {
    const map = new Map<string, OccurrenceItem[]>();
    for (const item of props.occurrences) {
      const start = new Date(item.occurrence.startsAt);
      const key = toDateKey(start);
      const list = map.get(key) || [];
      list.push(item);
      map.set(key, list);
    }
    return map;
  });

  const calendarMap = createMemo(() => {
    const map = new Map<string, Calendar>();
    for (const c of props.calendars) {
      map.set(c.id, c);
    }
    return map;
  });

  const handlePrev = () => {
    props.onFocusedDateChange(addMonths(props.focusedDate, -1));
  };

  const handleNext = () => {
    props.onFocusedDateChange(addMonths(props.focusedDate, 1));
  };

  const handleToday = () => {
    const today = new Date();
    props.onFocusedDateChange(today);
    props.onSelectDate(today);
  };

  return (
    <div
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
      }}
    >
      {/* Month Header */}
      <div
        style={{
          height: '88px',
          flex: 'none',
          display: 'flex',
          'align-items': 'center',
          gap: '18px',
          padding: '0 26px',
          'border-bottom': '1px solid var(--al-border-soft, #E5E5E5)',
        }}
      >
        <div style={{ display: 'flex', 'align-items': 'baseline', gap: '10px' }}>
          <span style={{ 'font-size': '42px', 'font-weight': 500, 'letter-spacing': '-0.03em', color: 'var(--al-ink, #1A1A1A)', 'line-height': 1 }}>
            {monthTitle()}
          </span>
          <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '20px', color: 'var(--al-ink-7, #A0A0A0)' }}>
            {yearTitle()}
          </span>
        </div>

        <div style={{ width: '1px', height: '26px', background: 'var(--al-border-soft, #E5E5E5)' }} />

        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11.5px', color: 'var(--al-ink-6, #888888)' }}>
          week {weekNum()} · {activeCalCount()} calendars shown
        </span>

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

      {/* Weekday Strip */}
      <div
        style={{
          height: '30px',
          flex: 'none',
          display: 'grid',
          'grid-template-columns': 'repeat(7, 1fr)',
          'border-bottom': '1px solid var(--al-border-soft, #E5E5E5)',
          background: 'var(--al-surface-3, #FCFCFC)',
        }}
      >
        <For each={WEEKDAYS_SHORT}>
          {(day) => (
            <div style={{ display: 'flex', 'align-items': 'center', 'padding-left': '9px', 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.12em', color: 'var(--al-ink-7, #A0A0A0)', 'text-transform': 'uppercase' }}>
              {day}
            </div>
          )}
        </For>
      </div>

      {/* 6x7 Month Grid */}
      <div
        style={{
          flex: 1,
          display: 'grid',
          'grid-template-columns': 'repeat(7, 1fr)',
          'grid-template-rows': 'repeat(6, 1fr)',
          'min-height': 0,
        }}
      >
        <For each={monthGrid()}>
          {(cell) => {
            const dayEvents = () => occurrencesByDate().get(cell.dateString) || [];
            const maxChips = 3;
            const chips = () => dayEvents().slice(0, maxChips);
            const moreCount = () => Math.max(0, dayEvents().length - maxChips);

            const bg = () => {
              if (cell.isToday) return 'var(--al-today-wash, #FBFCFE)';
              if (!cell.isCurrentMonth) return 'var(--al-surface-3, #FCFCFC)';
              return 'var(--al-surface, #FFFFFF)';
            };

            const numBg = () => {
              if (cell.isToday) return 'var(--al-accent, #1F6FEB)';
              return 'transparent';
            };

            const numFg = () => {
              if (cell.isToday) return '#FFFFFF';
              if (!cell.isCurrentMonth) return 'var(--al-ink-9, #BFBFBF)';
              return 'var(--al-ink, #1A1A1A)';
            };

            return (
              <div
                onClick={() => {
                  props.onSelectDate(cell.date);
                  props.onCellClick?.(cell.date);
                }}
                style={{
                  'border-right': '1px solid var(--al-grid, #EBEBEB)',
                  'border-bottom': '1px solid var(--al-grid, #EBEBEB)',
                  padding: '7px 7px 0',
                  display: 'flex',
                  'flex-direction': 'column',
                  gap: '3px',
                  'min-height': 0,
                  overflow: 'hidden',
                  background: bg(),
                  cursor: 'pointer',
                }}
              >
                {/* Date header */}
                <div style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'margin-bottom': '1px' }}>
                  <div
                    style={{
                      'min-width': '21px',
                      height: '21px',
                      padding: '0 4px',
                      display: 'flex',
                      'align-items': 'center',
                      'justify-content': 'center',
                      'border-radius': '6px',
                      'font-family': 'var(--al-font-mono)',
                      'font-size': '12px',
                      background: numBg(),
                      color: numFg(),
                      'font-weight': cell.isToday ? 500 : 400,
                    }}
                  >
                    {cell.dayNumber}
                  </div>
                </div>

                {/* Event chips */}
                <For each={chips()}>
                  {(item) => {
                    const cal = () => calendarMap().get(item.event.calendarId);
                    const color = () => cal()?.color || '#1F6FEB';
                    const tint = () => {
                      const c = color();
                      return c.startsWith('#') ? `${c}1A` : 'rgba(31,111,235,0.10)';
                    };

                    const timeLabel = () => {
                      if (item.occurrence.allDay || item.event.allDay) return '•';
                      const start = new Date(item.occurrence.startsAt);
                      return formatTime24(start);
                    };

                    return (
                      <div
                        onClick={(e) => {
                          e.stopPropagation();
                          props.onEventClick?.(item);
                        }}
                        style={{
                          display: 'flex',
                          'align-items': 'center',
                          gap: '5px',
                          padding: '3px 6px',
                          'border-radius': '5px',
                          background: tint(),
                          overflow: 'hidden',
                          cursor: 'pointer',
                        }}
                      >
                        <div style={{ width: '2.5px', height: '11px', 'border-radius': '2px', flex: 'none', background: color() }} />
                        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9px', flex: 'none', color: color() }}>
                          {timeLabel()}
                        </span>
                        <span style={{ 'font-size': '11px', color: 'var(--al-ink-event, #262626)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis' }}>
                          {item.event.title}
                        </span>
                      </div>
                    );
                  }}
                </For>

                {/* +N more */}
                <Show when={moreCount() > 0}>
                  <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', color: 'var(--al-ink-7, #A0A0A0)', 'padding-left': '6px' }}>
                    +{moreCount()} more
                  </span>
                </Show>
              </div>
            );
          }}
        </For>
      </div>
    </div>
  );
};

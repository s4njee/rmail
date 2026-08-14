import { Component, createMemo, createSignal, For, Show } from 'solid-js';
import { Calendar, OccurrenceItem, Task } from '../types/calendar';
import {
  addDays,
  formatTime24,
  isSameDay,
  MONTH_NAMES_SHORT,
  toDateKey,
  WEEKDAYS,
} from '../headless/dateUtils';

export interface AgendaViewProps {
  focusedDate: Date;
  selectedDate: Date;
  onSelectDate: (d: Date) => void;
  onFocusedDateChange: (d: Date) => void;
  occurrences: OccurrenceItem[];
  calendars: Calendar[];
  tasks: Task[];
  onToggleTask?: (taskId: string) => void;
  onEventClick?: (item: OccurrenceItem) => void;
}

interface AgendaDayGroup {
  date: Date;
  dateKey: string;
  isToday: boolean;
  dayNumber: number;
  weekday: string;
  month: string;
  items: {
    type: 'event' | 'task';
    timeStr: string;
    title: string;
    location?: string | null;
    calName: string;
    color: string;
    rawEvent?: OccurrenceItem;
    rawTask?: Task;
  }[];
}

export const AgendaView: Component<AgendaViewProps> = (props) => {
  const [filterMode, setFilterMode] = createSignal<'events' | 'all'>('events');

  const calendarMap = createMemo(() => {
    const map = new Map<string, Calendar>();
    for (const c of props.calendars) {
      map.set(c.id, c);
    }
    return map;
  });

  const dayGroups = createMemo(() => {
    const today = new Date();
    const startDate = props.focusedDate < today ? props.focusedDate : today;
    const daysCount = 30; // 30 days ahead

    const groups: AgendaDayGroup[] = [];
    for (let i = 0; i < daysCount; i++) {
      const d = addDays(startDate, i);
      const key = toDateKey(d);
      const isToday = isSameDay(d, today);

      // Find events for day
      const dayOccurrences = props.occurrences.filter((item) => {
        const start = new Date(item.occurrence.startsAt);
        return toDateKey(start) === key;
      });

      // Find tasks for day
      const dayTasks = props.tasks.filter((t) => {
        if (!t.dueAt) return false;
        const due = new Date(t.dueAt);
        return toDateKey(due) === key;
      });

      const items: AgendaDayGroup['items'] = [];

      for (const occ of dayOccurrences) {
        const cal = calendarMap().get(occ.event.calendarId);
        const color = cal?.color || '#1F6FEB';
        let timeStr = 'All day';
        if (!occ.occurrence.allDay && !occ.event.allDay) {
          const s = new Date(occ.occurrence.startsAt);
          const e = new Date(occ.occurrence.endsAt);
          timeStr = `${formatTime24(s)} – ${formatTime24(e)}`;
        }
        items.push({
          type: 'event',
          timeStr,
          title: occ.event.title,
          location: occ.event.location,
          calName: cal?.name || 'Calendar',
          color,
          rawEvent: occ,
        });
      }

      if (filterMode() === 'all') {
        for (const task of dayTasks) {
          const cal = calendarMap().get(task.calendarId);
          const color = cal?.color || '#C2410C';
          const due = task.dueAt ? new Date(task.dueAt) : null;
          const timeStr = due ? formatTime24(due) : 'Due today';
          items.push({
            type: 'task',
            timeStr,
            title: task.title,
            calName: cal?.name || 'Task',
            color,
            rawTask: task,
          });
        }
      }

      if (items.length > 0) {
        groups.push({
          date: d,
          dateKey: key,
          isToday,
          dayNumber: d.getDate(),
          weekday: WEEKDAYS[d.getDay()],
          month: MONTH_NAMES_SHORT[d.getMonth()].toUpperCase(),
          items,
        });
      }
    }

    return groups;
  });

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
      {/* Header (78px) */}
      <div
        style={{
          height: '78px',
          flex: 'none',
          display: 'flex',
          'align-items': 'center',
          gap: '18px',
          padding: '0 34px',
          'border-bottom': '1px solid var(--al-border-soft, #E5E5E5)',
        }}
      >
        <div style={{ display: 'flex', 'align-items': 'baseline', gap: '10px' }}>
          <span style={{ 'font-size': '34px', 'font-weight': 500, 'letter-spacing': '-0.03em', color: 'var(--al-ink, #1A1A1A)', 'line-height': 1 }}>
            Agenda
          </span>
          <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '15px', color: 'var(--al-ink-7, #A0A0A0)' }}>
            from today
          </span>
        </div>

        <div style={{ flex: 1 }} />

        {/* Filter Segmented Control */}
        <div
          style={{
            display: 'flex',
            'align-items': 'center',
            gap: '2px',
            padding: '3px',
            background: 'var(--al-segment-track, #EDEDED)',
            'border-radius': '9px',
          }}
        >
          <button
            type="button"
            onClick={() => setFilterMode('events')}
            style={{
              padding: '5px 11px',
              'border-radius': '6px',
              'font-size': '12px',
              'font-weight': 500,
              border: 'none',
              cursor: 'pointer',
              background: filterMode() === 'events' ? 'var(--al-surface, #FFFFFF)' : 'transparent',
              color: filterMode() === 'events' ? 'var(--al-ink, #1A1A1A)' : 'var(--al-ink-5, #777777)',
              'box-shadow': filterMode() === 'events' ? 'var(--al-shadow-segment-active, 0 1px 2px rgba(0,0,0,0.10))' : 'none',
            }}
          >
            Events
          </button>
          <button
            type="button"
            onClick={() => setFilterMode('all')}
            style={{
              padding: '5px 11px',
              'border-radius': '6px',
              'font-size': '12px',
              'font-weight': 500,
              border: 'none',
              cursor: 'pointer',
              background: filterMode() === 'all' ? 'var(--al-surface, #FFFFFF)' : 'transparent',
              color: filterMode() === 'all' ? 'var(--al-ink, #1A1A1A)' : 'var(--al-ink-5, #777777)',
              'box-shadow': filterMode() === 'all' ? 'var(--al-shadow-segment-active, 0 1px 2px rgba(0,0,0,0.10))' : 'none',
            }}
          >
            Events + tasks
          </button>
        </div>
      </div>

      {/* Agenda list */}
      <div style={{ flex: 1, 'overflow-y': 'auto', padding: '0 34px' }}>
        <For
          each={dayGroups()}
          fallback={
            <div style={{ padding: '60px 0', 'text-align': 'center', color: 'var(--al-ink-7, #A0A0A0)', 'font-size': '14px' }}>
              No upcoming commitments
            </div>
          }
        >
          {(group) => {
            const numFg = () => (group.isToday ? 'var(--al-accent, #1F6FEB)' : 'var(--al-ink, #1A1A1A)');

            return (
              <div
                style={{
                  display: 'flex',
                  gap: '28px',
                  padding: '22px 0',
                  'border-bottom': '1px solid var(--al-grid, #EBEBEB)',
                }}
              >
                {/* Left Date Marker */}
                <div style={{ width: '132px', flex: 'none', display: 'flex', 'align-items': 'flex-start', gap: '12px' }}>
                  <span style={{ 'font-size': '46px', 'font-weight': 400, 'letter-spacing': '-0.045em', 'line-height': 0.82, color: numFg() }}>
                    {group.dayNumber}
                  </span>
                  <div style={{ display: 'flex', 'flex-direction': 'column', gap: '2px', 'padding-top': '3px' }}>
                    <span style={{ 'font-size': '12.5px', 'font-weight': 500, color: numFg() }}>
                      {group.weekday}
                    </span>
                    <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.06em', color: 'var(--al-ink-7, #A0A0A0)' }}>
                      {group.month}
                    </span>
                  </div>
                </div>

                {/* Right Items */}
                <div style={{ flex: 1, 'min-width': 0, display: 'flex', 'flex-direction': 'column', gap: '2px' }}>
                  <For each={group.items}>
                    {(item, idx) => {
                      const isZebra = () => idx() % 2 === 1;
                      const rowBg = () => (isZebra() ? 'var(--al-surface-2, #FBFBFB)' : 'transparent');

                      return (
                        <div
                          onClick={() => {
                            if (item.rawEvent) props.onEventClick?.(item.rawEvent);
                          }}
                          style={{
                            display: 'flex',
                            'align-items': 'center',
                            gap: '16px',
                            padding: '8px 10px',
                            'border-radius': '7px',
                            background: rowBg(),
                            cursor: item.type === 'event' ? 'pointer' : 'default',
                          }}
                        >
                          <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11.5px', color: 'var(--al-ink-5, #777777)', width: '104px', flex: 'none' }}>
                            {item.timeStr}
                          </span>

                          <Show
                            when={item.type === 'event'}
                            fallback={
                              <button
                                type="button"
                                onClick={(e) => {
                                  e.stopPropagation();
                                  if (item.rawTask) props.onToggleTask?.(item.rawTask.id);
                                }}
                                style={{
                                  width: '13px',
                                  height: '13px',
                                  'border-radius': '50%',
                                  border: `1.5px solid ${item.color}`,
                                  background: item.rawTask?.completedAt ? 'var(--al-ink-9, #BFBFBF)' : 'transparent',
                                  flex: 'none',
                                  cursor: 'pointer',
                                  padding: 0,
                                }}
                              />
                            }
                          >
                            <div style={{ width: '3px', height: '16px', 'border-radius': '2px', flex: 'none', background: item.color }} />
                          </Show>

                          <span style={{ 'font-size': '14px', color: 'var(--al-ink, #1A1A1A)', 'white-space': 'nowrap', overflow: 'hidden', 'text-overflow': 'ellipsis', 'text-decoration': item.rawTask?.completedAt ? 'line-through' : 'none' }}>
                            {item.title}
                          </span>

                          <div style={{ flex: 1 }} />

                          <Show when={item.location}>
                            <span style={{ 'font-size': '11.5px', color: 'var(--al-ink-6, #888888)', flex: 'none' }}>
                              {item.location}
                            </span>
                          </Show>

                          <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.06em', color: item.color, width: '74px', 'text-align': 'right', flex: 'none' }}>
                            {item.calName}
                          </span>
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

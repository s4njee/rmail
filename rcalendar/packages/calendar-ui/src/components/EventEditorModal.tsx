import { Component, createEffect, createSignal, For, Show } from 'solid-js';
import { Calendar, EditScope, Event, EventDraft } from '../types/calendar';
import { toDateKey } from '../headless/dateUtils';

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
  { label: 'M', value: 'MO' },
  { label: 'T', value: 'TU' },
  { label: 'W', value: 'WE' },
  { label: 'T', value: 'TH' },
  { label: 'F', value: 'FR' },
  { label: 'S', value: 'SA' },
  { label: 'S', value: 'SU' },
];

export const EventEditorModal: Component<EventEditorModalProps> = (props) => {
  const [title, setTitle] = createSignal('');
  const [calendarId, setCalendarId] = createSignal('');
  const [dateStr, setDateStr] = createSignal('');
  const [startTime, setStartTime] = createSignal('10:00');
  const [endTime, setEndTime] = createSignal('11:00');
  const [allDay, setAllDay] = createSignal(false);
  const [repeatFreq, setRepeatFreq] = createSignal('none'); // 'none' | 'DAILY' | 'WEEKLY' | 'MONTHLY' | 'YEARLY'
  const [selectedDays, setSelectedDays] = createSignal<string[]>([]);
  const [endsMode, setEndsMode] = createSignal<'never' | 'until' | 'count'>('never');
  const [untilDate, setUntilDate] = createSignal('');
  const [occurrenceCount, setOccurrenceCount] = createSignal(5);
  const [tz, setTz] = createSignal('local');
  const [location, setLocation] = createSignal('');
  const [notes, setNotes] = createSignal('');
  const [scope, setScope] = createSignal<EditScope>('this');

  createEffect(() => {
    if (!props.isOpen) return;

    if (props.event) {
      const e = props.event;
      setTitle(e.title);
      setCalendarId(e.calendarId);
      setTz(e.tz || 'local');
      const start = new Date(e.startsAt);
      const end = new Date(e.endsAt);
      setDateStr(toDateKey(start));
      const sH = String(start.getHours()).padStart(2, '0');
      const sM = String(start.getMinutes()).padStart(2, '0');
      const eH = String(end.getHours()).padStart(2, '0');
      const eM = String(end.getMinutes()).padStart(2, '0');
      setStartTime(`${sH}:${sM}`);
      setEndTime(`${eH}:${eM}`);
      setAllDay(e.allDay);
      setLocation(e.location || '');
      setNotes(e.notes || '');

      if (e.rrule) {
        if (e.rrule.includes('FREQ=DAILY')) setRepeatFreq('DAILY');
        else if (e.rrule.includes('FREQ=WEEKLY')) setRepeatFreq('WEEKLY');
        else if (e.rrule.includes('FREQ=MONTHLY')) setRepeatFreq('MONTHLY');
        else if (e.rrule.includes('FREQ=YEARLY')) setRepeatFreq('YEARLY');

        const matchDays = e.rrule.match(/BYDAY=([^;]+)/);
        if (matchDays) {
          setSelectedDays(matchDays[1].split(','));
        } else {
          setSelectedDays([]);
        }

        const matchCount = e.rrule.match(/COUNT=(\d+)/);
        const matchUntil = e.rrule.match(/UNTIL=([0-9T]+)/);
        if (matchCount) {
          setEndsMode('count');
          setOccurrenceCount(Number(matchCount[1]));
        } else if (matchUntil) {
          setEndsMode('until');
          const raw = matchUntil[1];
          if (raw.length >= 8) {
            setUntilDate(`${raw.slice(0, 4)}-${raw.slice(4, 6)}-${raw.slice(6, 8)}`);
          }
        } else {
          setEndsMode('never');
        }
      } else {
        setRepeatFreq('none');
        setSelectedDays([]);
        setEndsMode('never');
      }
    } else {
      const d = props.initialDate || new Date();
      setTitle('');
      setCalendarId(props.calendars[0]?.id || '');
      setTz('local');
      setDateStr(toDateKey(d));
      setStartTime('10:00');
      setEndTime('11:00');
      setAllDay(false);
      setRepeatFreq('none');
      setSelectedDays([]);
      setEndsMode('never');
      setUntilDate('');
      setOccurrenceCount(5);
      setLocation('');
      setNotes('');
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
    if (e.key === 'Escape') {
      props.onClose();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
      handleSave();
    }
  };

  const handleSave = () => {
    if (!title().trim()) return;

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
    if (repeatFreq() !== 'none') {
      const parts = [`FREQ=${repeatFreq()}`];
      if (repeatFreq() === 'WEEKLY' && selectedDays().length > 0) {
        parts.push(`BYDAY=${selectedDays().join(',')}`);
      }
      if (endsMode() === 'count' && occurrenceCount() > 0) {
        parts.push(`COUNT=${occurrenceCount()}`);
      } else if (endsMode() === 'until' && untilDate()) {
        const u = untilDate().replace(/-/g, '');
        parts.push(`UNTIL=${u}T235959Z`);
      }
      rrule = parts.join(';');
    }

    const draft: EventDraft = {
      calendarId: calendarId() || props.calendars[0]?.id || '',
      title: title().trim(),
      location: location().trim() || null,
      notes: notes().trim() || null,
      startsAt,
      endsAt,
      allDay: allDay(),
      tz: tz() === 'local' ? null : tz(),
      rrule,
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
          position: 'fixed',
          inset: '52px 0 0 0',
          background: 'var(--al-scrim, rgba(0,0,0,0.34))',
          'z-index': 100,
        }}
      />

      {/* Modal Sheet */}
      <div
        onKeyDown={handleKeyDown}
        style={{
          position: 'fixed',
          left: '50%',
          top: '96px',
          transform: 'translateX(-50%)',
          width: '576px',
          'max-height': 'calc(100vh - 120px)',
          background: 'var(--al-surface, #FFFFFF)',
          'border-radius': '14px',
          'box-shadow': 'var(--al-shadow-modal, 0 40px 80px -20px rgba(0,0,0,0.5))',
          overflow: 'hidden',
          display: 'flex',
          'flex-direction': 'column',
          'z-index': 101,
          'font-family': 'var(--al-font-ui)',
          color: 'var(--al-ink, #1A1A1A)',
        }}
      >
        {/* Head */}
        <div style={{ padding: '22px 26px 18px', 'border-bottom': '1px solid var(--al-grid, #EBEBEB)' }}>
          <div style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.12em', color: 'var(--al-ink-7, #A0A0A0)', 'margin-bottom': '12px' }}>
            {props.event ? 'EDIT EVENT' : 'NEW EVENT'}
          </div>
          <input
            type="text"
            placeholder="Event title"
            value={title()}
            onInput={(e) => setTitle(e.currentTarget.value)}
            autofocus
            style={{
              width: '100%',
              'font-size': '26px',
              'font-weight': 500,
              'letter-spacing': '-0.025em',
              color: 'var(--al-ink, #1A1A1A)',
              'padding-bottom': '8px',
              border: 'none',
              'border-bottom': '1.5px solid var(--al-accent, #1F6FEB)',
              outline: 'none',
              background: 'transparent',
              'font-family': 'inherit',
            }}
          />
        </div>

        {/* Body */}
        <div style={{ padding: '20px 26px', display: 'flex', 'flex-direction': 'column', gap: '16px', 'overflow-y': 'auto' }}>
          {/* Calendar row */}
          <div style={{ display: 'flex', 'align-items': 'center', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none' }}>
              CALENDAR
            </span>
            <select
              value={calendarId()}
              onChange={(e) => setCalendarId(e.currentTarget.value)}
              style={{
                height: '34px',
                padding: '0 12px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                flex: 1,
                'font-size': '13px',
                background: '#FFFFFF',
                outline: 'none',
              }}
            >
              <For each={props.calendars}>
                {(cal) => <option value={cal.id}>{cal.name}</option>}
              </For>
            </select>
          </div>

          {/* When row */}
          <div style={{ display: 'flex', 'align-items': 'center', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none' }}>
              WHEN
            </span>
            <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', flex: 1, 'flex-wrap': 'wrap' }}>
              <input
                type="date"
                value={dateStr()}
                onInput={(e) => setDateStr(e.currentTarget.value)}
                style={{
                  height: '34px',
                  padding: '0 8px',
                  border: '1px solid var(--al-border, #E0E0E0)',
                  'border-radius': '8px',
                  'font-family': 'var(--al-font-mono)',
                  'font-size': '12.5px',
                }}
              />
              <Show when={!allDay()}>
                <input
                  type="time"
                  value={startTime()}
                  onInput={(e) => setStartTime(e.currentTarget.value)}
                  style={{
                    height: '34px',
                    padding: '0 8px',
                    border: '1px solid var(--al-border, #E0E0E0)',
                    'border-radius': '8px',
                    'font-family': 'var(--al-font-mono)',
                    'font-size': '12.5px',
                  }}
                />
                <span style={{ color: 'var(--al-ink-7, #A0A0A0)' }}>→</span>
                <input
                  type="time"
                  value={endTime()}
                  onInput={(e) => setEndTime(e.currentTarget.value)}
                  style={{
                    height: '34px',
                    padding: '0 8px',
                    border: '1px solid var(--al-border, #E0E0E0)',
                    'border-radius': '8px',
                    'font-family': 'var(--al-font-mono)',
                    'font-size': '12.5px',
                  }}
                />
              </Show>
              <label style={{ display: 'flex', 'align-items': 'center', gap: '6px', 'font-size': '12px', color: 'var(--al-ink-5, #777777)', cursor: 'pointer', 'margin-left': 'auto' }}>
                <input
                  type="checkbox"
                  checked={allDay()}
                  onChange={(e) => setAllDay(e.currentTarget.checked)}
                />
                All day
              </label>
            </div>
          </div>

          {/* Repeats row */}
          <div style={{ display: 'flex', 'align-items': 'flex-start', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none', 'margin-top': '8px' }}>
              REPEATS
            </span>
            <div style={{ display: 'flex', 'flex-direction': 'column', gap: '8px', flex: 1 }}>
              <select
                value={repeatFreq()}
                onChange={(e) => setRepeatFreq(e.currentTarget.value)}
                style={{
                  height: '34px',
                  padding: '0 12px',
                  border: '1px solid var(--al-border, #E0E0E0)',
                  'border-radius': '8px',
                  'font-size': '13px',
                  background: '#FFFFFF',
                }}
              >
                <option value="none">Does not repeat</option>
                <option value="DAILY">Daily</option>
                <option value="WEEKLY">Weekly</option>
                <option value="MONTHLY">Monthly</option>
                <option value="YEARLY">Yearly</option>
              </select>

              <Show when={repeatFreq() === 'WEEKLY'}>
                <div style={{ display: 'flex', gap: '6px' }}>
                  <For each={WEEKDAY_KEYS}>
                    {(item) => {
                      const on = () => selectedDays().includes(item.value);
                      return (
                        <button
                          type="button"
                          onClick={() => toggleDay(item.value)}
                          style={{
                            width: '34px',
                            height: '30px',
                            'border-radius': '7px',
                            'font-family': 'var(--al-font-mono)',
                            'font-size': '11px',
                            border: on() ? '1px solid var(--al-accent, #1F6FEB)' : '1px solid var(--al-border, #E0E0E0)',
                            background: on() ? 'var(--al-accent, #1F6FEB)' : '#FFFFFF',
                            color: on() ? '#FFFFFF' : 'var(--al-ink-6, #888888)',
                            cursor: 'pointer',
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
              <Show when={repeatFreq() !== 'none'}>
                <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px', 'padding-top': '4px' }}>
                  <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', color: 'var(--al-ink-7, #A0A0A0)' }}>
                    ENDS
                  </span>
                  <div style={{ display: 'flex', 'align-items': 'center', gap: '12px', 'font-size': '12.5px' }}>
                    <label style={{ display: 'flex', 'align-items': 'center', gap: '4px', cursor: 'pointer' }}>
                      <input
                        type="radio"
                        name="ends-mode"
                        checked={endsMode() === 'never'}
                        onChange={() => setEndsMode('never')}
                      />
                      Never
                    </label>
                    <label style={{ display: 'flex', 'align-items': 'center', gap: '4px', cursor: 'pointer' }}>
                      <input
                        type="radio"
                        name="ends-mode"
                        checked={endsMode() === 'until'}
                        onChange={() => setEndsMode('until')}
                      />
                      On date
                    </label>
                    <label style={{ display: 'flex', 'align-items': 'center', gap: '4px', cursor: 'pointer' }}>
                      <input
                        type="radio"
                        name="ends-mode"
                        checked={endsMode() === 'count'}
                        onChange={() => setEndsMode('count')}
                      />
                      After count
                    </label>
                  </div>

                  <Show when={endsMode() === 'until'}>
                    <input
                      type="date"
                      value={untilDate()}
                      onInput={(e) => setUntilDate(e.currentTarget.value)}
                      style={{
                        height: '32px',
                        padding: '0 8px',
                        border: '1px solid var(--al-border, #E0E0E0)',
                        'border-radius': '7px',
                        'font-family': 'var(--al-font-mono)',
                        'font-size': '12px',
                        width: '160px',
                      }}
                    />
                  </Show>

                  <Show when={endsMode() === 'count'}>
                    <div style={{ display: 'flex', 'align-items': 'center', gap: '8px' }}>
                      <input
                        type="number"
                        min="1"
                        max="365"
                        value={occurrenceCount()}
                        onInput={(e) => setOccurrenceCount(Number(e.currentTarget.value) || 1)}
                        style={{
                          height: '32px',
                          padding: '0 8px',
                          border: '1px solid var(--al-border, #E0E0E0)',
                          'border-radius': '7px',
                          'font-family': 'var(--al-font-mono)',
                          'font-size': '12px',
                          width: '80px',
                        }}
                      />
                      <span style={{ 'font-size': '12px', color: 'var(--al-ink-5, #777777)' }}>occurrences</span>
                    </div>
                  </Show>
                </div>
              </Show>
            </div>
          </div>

          {/* Remind Me row */}
          <div style={{ display: 'flex', 'align-items': 'center', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none' }}>
              REMIND ME
            </span>
            <div style={{ display: 'flex', 'align-items': 'center', gap: '7px', flex: 1, 'flex-wrap': 'wrap' }}>
              <div style={{ display: 'flex', 'align-items': 'center', gap: '7px', height: '30px', padding: '0 11px', 'border-radius': '15px', background: 'var(--al-accent-tint, #E4EBF8)' }}>
                <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11px', color: 'var(--al-accent, #1F6FEB)' }}>
                  10 min before
                </span>
              </div>
              <div style={{ display: 'flex', 'align-items': 'center', gap: '7px', height: '30px', padding: '0 11px', 'border-radius': '15px', background: 'var(--al-accent-tint, #E4EBF8)' }}>
                <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11px', color: 'var(--al-accent, #1F6FEB)' }}>
                  at 08:00 same day
                </span>
              </div>
            </div>
          </div>

          {/* Timezone row */}
          <div style={{ display: 'flex', 'align-items': 'center', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none' }}>
              TIMEZONE
            </span>
            <select
              value={tz()}
              onChange={(e) => setTz(e.currentTarget.value)}
              style={{
                height: '34px',
                padding: '0 12px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                flex: 1,
                'font-size': '13px',
                background: '#FFFFFF',
              }}
            >
              <option value="local">Local system time</option>
              <option value="America/New_York">America/New_York (Eastern)</option>
              <option value="America/Chicago">America/Chicago (Central)</option>
              <option value="America/Los_Angeles">America/Los_Angeles (Pacific)</option>
              <option value="Europe/London">Europe/London (GMT/BST)</option>
              <option value="Asia/Tokyo">Asia/Tokyo (JST)</option>
              <option value="UTC">UTC</option>
            </select>
          </div>

          {/* Where row */}
          <div style={{ display: 'flex', 'align-items': 'center', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none' }}>
              WHERE
            </span>
            <input
              type="text"
              placeholder="Add location"
              value={location()}
              onInput={(e) => setLocation(e.currentTarget.value)}
              style={{
                height: '34px',
                padding: '0 12px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                flex: 1,
                'font-size': '13px',
              }}
            />
          </div>

          {/* Notes row */}
          <div style={{ display: 'flex', 'align-items': 'flex-start', gap: '16px' }}>
            <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none', 'margin-top': '8px' }}>
              NOTES
            </span>
            <textarea
              placeholder="Add notes or description"
              value={notes()}
              onInput={(e) => setNotes(e.currentTarget.value)}
              rows={3}
              style={{
                padding: '8px 12px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                flex: 1,
                'font-size': '13px',
                'font-family': 'inherit',
                'min-height': '64px',
                resize: 'vertical',
              }}
            />
          </div>

          {/* Scoped Edit selection (if editing a recurring series) */}
          <Show when={props.event && props.event.rrule}>
            <div style={{ display: 'flex', 'align-items': 'center', gap: '16px', 'padding-top': '4px' }}>
              <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)', width: '92px', flex: 'none' }}>
                SCOPE
              </span>
              <div style={{ display: 'flex', gap: '8px' }}>
                <label style={{ 'font-size': '12.5px', display: 'flex', 'align-items': 'center', gap: '4px', cursor: 'pointer' }}>
                  <input
                    type="radio"
                    name="edit-scope"
                    checked={scope() === 'this'}
                    onChange={() => setScope('this')}
                  />
                  This event
                </label>
                <label style={{ 'font-size': '12.5px', display: 'flex', 'align-items': 'center', gap: '4px', cursor: 'pointer' }}>
                  <input
                    type="radio"
                    name="edit-scope"
                    checked={scope() === 'future'}
                    onChange={() => setScope('future')}
                  />
                  This and following
                </label>
                <label style={{ 'font-size': '12.5px', display: 'flex', 'align-items': 'center', gap: '4px', cursor: 'pointer' }}>
                  <input
                    type="radio"
                    name="edit-scope"
                    checked={scope() === 'all'}
                    onChange={() => setScope('all')}
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
            padding: '16px 26px',
            'border-top': '1px solid var(--al-grid, #EBEBEB)',
            background: 'var(--al-surface-2, #FBFBFB)',
            display: 'flex',
            'align-items': 'center',
          }}
        >
          <Show when={props.event}>
            <button
              type="button"
              onClick={handleDelete}
              style={{
                background: 'none',
                border: 'none',
                color: 'var(--al-cal-classes, #C2410C)',
                'font-size': '12.5px',
                cursor: 'pointer',
                padding: '0 4px',
              }}
            >
              Delete
            </button>
          </Show>

          <div style={{ flex: 1 }} />

          <div style={{ display: 'flex', gap: '10px' }}>
            <button
              type="button"
              onClick={props.onClose}
              style={{
                height: '34px',
                padding: '0 16px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                background: '#FFFFFF',
                'font-size': '13px',
                cursor: 'pointer',
              }}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleSave}
              style={{
                height: '34px',
                padding: '0 17px',
                'border-radius': '8px',
                background: 'var(--al-accent, #1F6FEB)',
                color: '#FFFFFF',
                border: 'none',
                'font-size': '13px',
                'font-weight': 500,
                cursor: 'pointer',
                display: 'flex',
                'align-items': 'center',
                gap: '8px',
              }}
            >
              <span>Save event</span>
              <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10.5px', opacity: 0.75 }}>
                ⌘↵
              </span>
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
};

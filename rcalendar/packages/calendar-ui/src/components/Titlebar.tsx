import { Component, For } from 'solid-js';
import { ViewMode } from '../types/calendar';

export interface TitlebarProps {
  activeView: ViewMode;
  onViewChange: (view: ViewMode) => void;
  onNewEvent: () => void;
  onSearchClick?: () => void;
  onMinimize?: () => void;
  onMaximize?: () => void;
  onClose?: () => void;
}

const VIEWS: ViewMode[] = ['Month', 'Week', '3-day', 'Day', 'Agenda'];

export const Titlebar: Component<TitlebarProps> = (props) => {
  return (
    <header
      data-tauri-drag-region
      style={{
        height: '52px',
        flex: 'none',
        display: 'flex',
        'align-items': 'center',
        gap: '14px',
        padding: '0 14px',
        background: 'var(--al-chrome, #FAFAFA)',
        'border-bottom': '1px solid var(--al-border, #E0E0E0)',
        'font-family': 'var(--al-font-ui)',
        color: 'var(--al-ink, #1A1A1A)',
        'user-select': 'none',
      }}
    >
      {/* Wordmark */}
      <div style={{ display: 'flex', 'align-items': 'center', gap: '8px', 'padding-right': '4px' }}>
        <div style={{ width: '16px', height: '16px', 'border-radius': '4px', background: 'var(--al-ink, #1A1A1A)' }} />
        <span style={{ 'font-size': '14px', 'font-weight': 600, 'letter-spacing': '-0.01em' }}>Almanac</span>
      </div>

      {/* View switcher */}
      <div
        style={{
          display: 'flex',
          'align-items': 'center',
          gap: '2px',
          padding: '3px',
          background: 'var(--al-segment-track-titlebar, #EBEBEB)',
          'border-radius': '9px',
        }}
      >
        <For each={VIEWS}>
          {(view) => {
            const isActive = () => props.activeView === view;
            return (
              <button
                type="button"
                onClick={() => props.onViewChange(view)}
                style={{
                  padding: '5px 11px',
                  'border-radius': '6px',
                  'font-size': '12.5px',
                  'font-weight': 500,
                  border: 'none',
                  cursor: 'pointer',
                  background: isActive() ? 'var(--al-surface, #FFFFFF)' : 'transparent',
                  color: isActive() ? 'var(--al-ink, #1A1A1A)' : 'var(--al-ink-5, #777777)',
                  'box-shadow': isActive() ? 'var(--al-shadow-segment-active, 0 1px 2px rgba(0,0,0,0.10))' : 'none',
                  transition: 'background 120ms ease, color 120ms ease',
                }}
              >
                {view}
              </button>
            );
          }}
        </For>
      </div>

      {/* Spacer for drag region */}
      <div data-tauri-drag-region style={{ flex: 1, height: '100%' }} />

      {/* Search box */}
      <button
        type="button"
        onClick={() => props.onSearchClick?.()}
        style={{
          display: 'flex',
          'align-items': 'center',
          gap: '8px',
          width: '230px',
          height: '30px',
          padding: '0 10px',
          border: '1px solid var(--al-border, #E0E0E0)',
          'border-radius': '8px',
          background: 'var(--al-surface, #FFFFFF)',
          cursor: 'pointer',
          'text-align': 'left',
        }}
      >
        <div style={{ width: '11px', height: '11px', border: '1.5px solid var(--al-ink-7, #A0A0A0)', 'border-radius': '50%' }} />
        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '11.5px', color: 'var(--al-ink-7, #A0A0A0)' }}>
          Search or type a date
        </span>
        <div style={{ flex: 1 }} />
        <span style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10.5px', color: 'var(--al-ink-9, #BFBFBF)' }}>
          ⌘K
        </span>
      </button>

      {/* + New event */}
      <button
        type="button"
        onClick={() => props.onNewEvent()}
        style={{
          display: 'flex',
          'align-items': 'center',
          gap: '7px',
          height: '30px',
          padding: '0 12px',
          'border-radius': '8px',
          background: 'var(--al-accent, #1F6FEB)',
          color: '#FFFFFF',
          'font-size': '12.5px',
          'font-weight': 500,
          border: 'none',
          cursor: 'pointer',
        }}
      >
        <span style={{ 'font-size': '14px', 'line-height': 1 }}>+</span>
        <span>New event</span>
      </button>

      <div style={{ width: '1px', height: '20px', background: 'var(--al-border, #E0E0E0)', margin: '0 2px' }} />

      {/* Window controls */}
      <div style={{ display: 'flex', 'align-items': 'center', gap: '14px', color: 'var(--al-ink-6, #888888)' }}>
        <button
          type="button"
          aria-label="Minimize"
          onClick={() => props.onMinimize?.()}
          style={{ background: 'none', border: 'none', padding: '4px', cursor: 'pointer', display: 'flex', 'align-items': 'center', color: 'inherit' }}
        >
          <div style={{ width: '11px', height: '1.5px', background: 'currentColor' }} />
        </button>
        <button
          type="button"
          aria-label="Maximize"
          onClick={() => props.onMaximize?.()}
          style={{ background: 'none', border: 'none', padding: '4px', cursor: 'pointer', display: 'flex', 'align-items': 'center', color: 'inherit' }}
        >
          <div style={{ width: '10px', height: '10px', border: '1.5px solid currentColor', 'border-radius': '2px' }} />
        </button>
        <button
          type="button"
          aria-label="Close"
          onClick={() => props.onClose?.()}
          style={{ background: 'none', border: 'none', padding: '4px', cursor: 'pointer', display: 'flex', 'align-items': 'center', color: 'inherit' }}
        >
          <div style={{ width: '12px', height: '12px', position: 'relative' }}>
            <div style={{ position: 'absolute', top: '5px', left: 0, width: '12px', height: '1.5px', background: 'currentColor', transform: 'rotate(45deg)' }} />
            <div style={{ position: 'absolute', top: '5px', left: 0, width: '12px', height: '1.5px', background: 'currentColor', transform: 'rotate(-45deg)' }} />
          </div>
        </button>
      </div>
    </header>
  );
};

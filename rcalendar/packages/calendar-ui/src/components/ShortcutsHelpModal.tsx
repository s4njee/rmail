import { Component, For, Show } from 'solid-js';

export interface ShortcutsHelpModalProps {
  isOpen: boolean;
  onClose: () => void;
}

const SHORTCUT_GROUPS = [
  {
    title: 'NAVIGATION',
    items: [
      { key: 't', desc: 'Go to today' },
      { key: 'j / k', desc: 'Next / previous period' },
      { key: '← / →', desc: 'Move focused date' },
      { key: '1 – 5', desc: 'Switch view (Month, Week, 3-day, Day, Agenda)' },
    ],
  },
  {
    title: 'ACTIONS',
    items: [
      { key: 'c or n', desc: 'Create new event' },
      { key: '⌘K', desc: 'Search events, tasks, or jump to date' },
      { key: '⌘↵', desc: 'Save in editor' },
      { key: 'Esc', desc: 'Close open sheet / modal' },
      { key: '?', desc: 'Open this shortcuts guide' },
    ],
  },
];

export const ShortcutsHelpModal: Component<ShortcutsHelpModalProps> = (props) => {
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

      {/* Sheet */}
      <div
        style={{
          position: 'fixed',
          left: '50%',
          top: '96px',
          transform: 'translateX(-50%)',
          width: '512px',
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
        <div style={{ padding: '20px 24px 16px', 'border-bottom': '1px solid var(--al-grid, #EBEBEB)', display: 'flex', 'align-items': 'center', 'justify-content': 'space-between' }}>
          <span style={{ 'font-size': '18px', 'font-weight': 600 }}>Keyboard Shortcuts</span>
          <button
            type="button"
            onClick={props.onClose}
            style={{ background: 'none', border: 'none', cursor: 'pointer', 'font-family': 'var(--al-font-mono)', 'font-size': '11px', color: 'var(--al-ink-7, #A0A0A0)' }}
          >
            ESC
          </button>
        </div>

        <div style={{ padding: '20px 24px', display: 'flex', 'flex-direction': 'column', gap: '20px' }}>
          <For each={SHORTCUT_GROUPS}>
            {(group) => (
              <div>
                <div style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.12em', color: 'var(--al-ink-7, #A0A0A0)', 'margin-bottom': '10px' }}>
                  {group.title}
                </div>
                <div style={{ display: 'flex', 'flex-direction': 'column', gap: '8px' }}>
                  <For each={group.items}>
                    {(item) => (
                      <div style={{ display: 'flex', 'align-items': 'center', 'justify-content': 'space-between' }}>
                        <span style={{ 'font-size': '13px', color: 'var(--al-ink, #1A1A1A)' }}>{item.desc}</span>
                        <kbd
                          style={{
                            padding: '3px 8px',
                            'border-radius': '5px',
                            background: 'var(--al-segment-track, #EDEDED)',
                            border: '1px solid var(--al-border, #E0E0E0)',
                            'font-family': 'var(--al-font-mono)',
                            'font-size': '11px',
                            color: 'var(--al-ink-2, #424242)',
                          }}
                        >
                          {item.key}
                        </kbd>
                      </div>
                    )}
                  </For>
                </div>
              </div>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
};

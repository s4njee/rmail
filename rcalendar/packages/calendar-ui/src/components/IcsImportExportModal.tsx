import { Component, createSignal, For, Show } from 'solid-js';
import { Calendar } from '../types/calendar';

export interface IcsImportExportModalProps {
  isOpen: boolean;
  onClose: () => void;
  calendars: Calendar[];
  onExport: (calendarId?: string) => Promise<string>;
  onImport: (calendarId: string, icsContent: string) => Promise<void>;
}

export const IcsImportExportModal: Component<IcsImportExportModalProps> = (props) => {
  const [tab, setTab] = createSignal<'export' | 'import'>('export');
  const [selectedCalId, setSelectedCalId] = createSignal<string>('all');
  const [importCalId, setImportCalId] = createSignal<string>('');
  const [exportedText, setExportedText] = createSignal<string>('');
  const [importText, setImportText] = createSignal<string>('');
  const [statusMsg, setStatusMsg] = createSignal<string>('');
  const [loading, setLoading] = createSignal(false);

  const handleExport = async () => {
    setLoading(true);
    setStatusMsg('');
    try {
      const calId = selectedCalId() === 'all' ? undefined : selectedCalId();
      const ics = await props.onExport(calId);
      setExportedText(ics);
      setStatusMsg('Calendar exported successfully!');
    } catch (err) {
      console.error(err);
      setStatusMsg(`Export failed: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = () => {
    if (exportedText()) {
      navigator.clipboard.writeText(exportedText());
      setStatusMsg('Copied .ics content to clipboard!');
    }
  };

  const handleDownload = () => {
    if (!exportedText()) return;
    const blob = new Blob([exportedText()], { type: 'text/calendar;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'calendar.ics';
    a.click();
    URL.revokeObjectURL(url);
    setStatusMsg('Downloaded calendar.ics');
  };

  const handleImport = async () => {
    if (!importText().trim()) {
      setStatusMsg('Please paste or upload .ics content first.');
      return;
    }
    const calId = importCalId() || props.calendars[0]?.id;
    if (!calId) {
      setStatusMsg('No calendar available.');
      return;
    }

    setLoading(true);
    setStatusMsg('');
    try {
      await props.onImport(calId, importText());
      setStatusMsg('Events imported successfully!');
      setImportText('');
      setTimeout(() => {
        props.onClose();
      }, 1200);
    } catch (err) {
      console.error(err);
      setStatusMsg(`Import failed: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleFileUpload = (e: Event) => {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (evt) => {
      const content = evt.target?.result as string;
      setImportText(content || '');
    };
    reader.readAsText(file);
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

      {/* Sheet */}
      <div
        style={{
          position: 'fixed',
          left: '50%',
          top: '96px',
          transform: 'translateX(-50%)',
          width: '576px',
          'max-height': 'calc(100vh - 140px)',
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
        <div style={{ padding: '20px 24px 14px', 'border-bottom': '1px solid var(--al-grid, #EBEBEB)', display: 'flex', 'align-items': 'center', 'justify-content': 'space-between' }}>
          <div style={{ display: 'flex', gap: '8px' }}>
            <button
              type="button"
              onClick={() => {
                setTab('export');
                setStatusMsg('');
              }}
              style={{
                padding: '6px 12px',
                'border-radius': '6px',
                'font-size': '13px',
                'font-weight': tab() === 'export' ? 600 : 400,
                background: tab() === 'export' ? 'var(--al-accent-tint, #E4EBF8)' : 'transparent',
                color: tab() === 'export' ? 'var(--al-accent, #1F6FEB)' : 'var(--al-ink-5, #777777)',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              Export .ics
            </button>
            <button
              type="button"
              onClick={() => {
                setTab('import');
                setStatusMsg('');
              }}
              style={{
                padding: '6px 12px',
                'border-radius': '6px',
                'font-size': '13px',
                'font-weight': tab() === 'import' ? 600 : 400,
                background: tab() === 'import' ? 'var(--al-accent-tint, #E4EBF8)' : 'transparent',
                color: tab() === 'import' ? 'var(--al-accent, #1F6FEB)' : 'var(--al-ink-5, #777777)',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              Import .ics
            </button>
          </div>
          <button
            type="button"
            onClick={props.onClose}
            style={{ background: 'none', border: 'none', cursor: 'pointer', 'font-family': 'var(--al-font-mono)', 'font-size': '11px', color: 'var(--al-ink-7, #A0A0A0)' }}
          >
            ESC
          </button>
        </div>

        {/* Body */}
        <div style={{ padding: '20px 24px', display: 'flex', 'flex-direction': 'column', gap: '16px', 'overflow-y': 'auto' }}>
          <Show when={tab() === 'export'}>
            <div style={{ display: 'flex', 'flex-direction': 'column', gap: '12px' }}>
              <div style={{ display: 'flex', 'align-items': 'center', gap: '12px' }}>
                <span style={{ 'font-size': '13px', color: 'var(--al-ink-5, #777777)', width: '120px' }}>Calendar:</span>
                <select
                  value={selectedCalId()}
                  onChange={(e) => setSelectedCalId(e.currentTarget.value)}
                  style={{
                    height: '34px',
                    padding: '0 12px',
                    border: '1px solid var(--al-border, #E0E0E0)',
                    'border-radius': '8px',
                    flex: 1,
                    'font-size': '13px',
                  }}
                >
                  <option value="all">All enabled calendars</option>
                  <For each={props.calendars}>
                    {(cal) => <option value={cal.id}>{cal.name}</option>}
                  </For>
                </select>
                <button
                  type="button"
                  onClick={handleExport}
                  disabled={loading()}
                  style={{
                    height: '34px',
                    padding: '0 16px',
                    'border-radius': '8px',
                    background: 'var(--al-accent, #1F6FEB)',
                    color: '#FFFFFF',
                    border: 'none',
                    'font-size': '13px',
                    'font-weight': 500,
                    cursor: 'pointer',
                  }}
                >
                  {loading() ? 'Exporting...' : 'Export'}
                </button>
              </div>

              <Show when={exportedText()}>
                <div style={{ display: 'flex', 'flex-direction': 'column', gap: '8px' }}>
                  <textarea
                    value={exportedText()}
                    readonly
                    rows={8}
                    style={{
                      padding: '8px 12px',
                      border: '1px solid var(--al-border, #E0E0E0)',
                      'border-radius': '8px',
                      'font-family': 'var(--al-font-mono)',
                      'font-size': '11px',
                      resize: 'none',
                      background: 'var(--al-surface-2, #FBFBFB)',
                    }}
                  />
                  <div style={{ display: 'flex', gap: '10px' }}>
                    <button
                      type="button"
                      onClick={handleCopy}
                      style={{
                        padding: '6px 14px',
                        border: '1px solid var(--al-border, #E0E0E0)',
                        'border-radius': '7px',
                        background: '#FFFFFF',
                        'font-size': '12.5px',
                        cursor: 'pointer',
                      }}
                    >
                      Copy to clipboard
                    </button>
                    <button
                      type="button"
                      onClick={handleDownload}
                      style={{
                        padding: '6px 14px',
                        border: '1px solid var(--al-border, #E0E0E0)',
                        'border-radius': '7px',
                        background: '#FFFFFF',
                        'font-size': '12.5px',
                        cursor: 'pointer',
                      }}
                    >
                      Download .ics file
                    </button>
                  </div>
                </div>
              </Show>
            </div>
          </Show>

          <Show when={tab() === 'import'}>
            <div style={{ display: 'flex', 'flex-direction': 'column', gap: '12px' }}>
              <div style={{ display: 'flex', 'align-items': 'center', gap: '12px' }}>
                <span style={{ 'font-size': '13px', color: 'var(--al-ink-5, #777777)', width: '120px' }}>Target calendar:</span>
                <select
                  value={importCalId()}
                  onChange={(e) => setImportCalId(e.currentTarget.value)}
                  style={{
                    height: '34px',
                    padding: '0 12px',
                    border: '1px solid var(--al-border, #E0E0E0)',
                    'border-radius': '8px',
                    flex: 1,
                    'font-size': '13px',
                  }}
                >
                  <For each={props.calendars}>
                    {(cal) => <option value={cal.id}>{cal.name}</option>}
                  </For>
                </select>
              </div>

              <div>
                <input type="file" accept=".ics,text/calendar" onChange={handleFileUpload} style={{ 'font-size': '12.5px' }} />
              </div>

              <textarea
                placeholder="Or paste RFC 5545 .ics text here (BEGIN:VCALENDAR...)"
                value={importText()}
                onInput={(e) => setImportText(e.currentTarget.value)}
                rows={6}
                style={{
                  padding: '8px 12px',
                  border: '1px solid var(--al-border, #E0E0E0)',
                  'border-radius': '8px',
                  'font-family': 'var(--al-font-mono)',
                  'font-size': '11px',
                  resize: 'vertical',
                }}
              />

              <div style={{ display: 'flex', 'justify-content': 'flex-end' }}>
                <button
                  type="button"
                  onClick={handleImport}
                  disabled={loading()}
                  style={{
                    height: '34px',
                    padding: '0 16px',
                    'border-radius': '8px',
                    background: 'var(--al-accent, #1F6FEB)',
                    color: '#FFFFFF',
                    border: 'none',
                    'font-size': '13px',
                    'font-weight': 500,
                    cursor: 'pointer',
                  }}
                >
                  {loading() ? 'Importing...' : 'Import events'}
                </button>
              </div>
            </div>
          </Show>

          <Show when={statusMsg()}>
            <div style={{ 'font-size': '12.5px', color: 'var(--al-accent, #1F6FEB)', 'font-weight': 500 }}>
              {statusMsg()}
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

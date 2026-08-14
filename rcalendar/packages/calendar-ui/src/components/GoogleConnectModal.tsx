import { Component, createSignal, Show } from 'solid-js';

export interface GoogleConnectModalProps {
  isOpen: boolean;
  onClose: () => void;
  onConnect: (email: string, token: string) => Promise<void>;
}

export const GoogleConnectModal: Component<GoogleConnectModalProps> = (props) => {
  const [email, setEmail] = createSignal('');
  const [token, setToken] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [errorMsg, setErrorMsg] = createSignal('');

  const handleConnect = async (e: Event) => {
    e.preventDefault();
    if (!email().trim()) {
      setErrorMsg('Please enter your Google account email.');
      return;
    }

    setLoading(true);
    setErrorMsg('');
    try {
      const activeToken = token().trim() || 'mock_google_oauth_token';
      await props.onConnect(email().trim(), activeToken);
      props.onClose();
    } catch (err) {
      console.error('Failed to connect Google account:', err);
      setErrorMsg(`Connection error: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const handleUseDemo = () => {
    setEmail('casey.student@gmail.com');
    setToken('mock_google_oauth_token');
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

      {/* Modal Card */}
      <div
        style={{
          position: 'fixed',
          left: '50%',
          top: '120px',
          transform: 'translateX(-50%)',
          width: '520px',
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
        <div style={{ padding: '22px 26px 16px', 'border-bottom': '1px solid var(--al-grid, #EBEBEB)', display: 'flex', 'align-items': 'center', 'justify-content': 'space-between' }}>
          <div>
            <div style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '9.5px', 'letter-spacing': '0.12em', color: 'var(--al-ink-7, #A0A0A0)', 'margin-bottom': '4px' }}>
              CALENDAR INTEGRATION
            </div>
            <div style={{ 'font-size': '20px', 'font-weight': 500, 'letter-spacing': '-0.02em' }}>
              Connect Google Calendar
            </div>
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
        <form onSubmit={handleConnect} style={{ padding: '20px 26px', display: 'flex', 'flex-direction': 'column', gap: '16px' }}>
          <div style={{ 'font-size': '13px', color: 'var(--al-ink-5, #777777)', 'line-height': 1.5 }}>
            Synchronize events bidirectionally with Google Calendar. Events will stay available offline in your local SQLite store.
          </div>

          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px' }}>
            <label style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)' }}>
              GOOGLE ACCOUNT EMAIL
            </label>
            <input
              type="email"
              placeholder="e.g. your.name@gmail.com"
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
              required
              style={{
                height: '36px',
                padding: '0 12px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                'font-size': '13px',
                outline: 'none',
              }}
            />
          </div>

          <div style={{ display: 'flex', 'flex-direction': 'column', gap: '6px' }}>
            <div style={{ display: 'flex', 'justify-content': 'space-between', 'align-items': 'center' }}>
              <label style={{ 'font-family': 'var(--al-font-mono)', 'font-size': '10px', 'letter-spacing': '0.08em', color: 'var(--al-ink-7, #A0A0A0)' }}>
                OAUTH / ACCESS TOKEN (OPTIONAL)
              </label>
              <button
                type="button"
                onClick={handleUseDemo}
                style={{
                  background: 'none',
                  border: 'none',
                  'font-size': '11.5px',
                  color: 'var(--al-accent, #1F6FEB)',
                  cursor: 'pointer',
                  padding: 0,
                }}
              >
                Fill demo account
              </button>
            </div>
            <input
              type="password"
              placeholder="OAuth Token (leave empty for demo sync)"
              value={token()}
              onInput={(e) => setToken(e.currentTarget.value)}
              style={{
                height: '36px',
                padding: '0 12px',
                border: '1px solid var(--al-border, #E0E0E0)',
                'border-radius': '8px',
                'font-size': '13px',
                outline: 'none',
                'font-family': 'var(--al-font-mono)',
              }}
            />
          </div>

          <Show when={errorMsg()}>
            <div style={{ 'font-size': '12.5px', color: 'var(--al-cal-classes, #C2410C)' }}>
              {errorMsg()}
            </div>
          </Show>

          {/* Foot */}
          <div style={{ display: 'flex', 'align-items': 'center', 'justify-content': 'flex-end', gap: '10px', 'padding-top': '8px' }}>
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
              type="submit"
              disabled={loading()}
              style={{
                height: '34px',
                padding: '0 18px',
                'border-radius': '8px',
                background: 'var(--al-accent, #1F6FEB)',
                color: '#FFFFFF',
                border: 'none',
                'font-size': '13px',
                'font-weight': 500,
                cursor: 'pointer',
              }}
            >
              {loading() ? 'Connecting...' : 'Connect & Sync'}
            </button>
          </div>
        </form>
      </div>
    </Show>
  );
};

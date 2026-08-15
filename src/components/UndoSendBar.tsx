import { Show } from "solid-js";
import { sendPendingNow, undoPendingSend, useComposer } from "../lib/compose";
import "./UndoSendBar.css";

export function UndoSendBar() {
  const { pendingSend } = useComposer();

  const recipientSnippet = () => {
    const ps = pendingSend();
    if (!ps) return "";
    const to = ps.outgoing.to;
    if (to.length === 1) return to[0];
    if (to.length > 1) return `${to[0]} +${to.length - 1}`;
    return "message";
  };

  const percent = () => {
    const ps = pendingSend();
    if (!ps || ps.totalSeconds <= 0) return 0;
    return Math.max(
      0,
      Math.min(
        100,
        ((ps.totalSeconds - ps.secondsRemaining) / ps.totalSeconds) * 100,
      ),
    );
  };

  return (
    <Show when={pendingSend()}>
      <div class="undo-send-bar" role="status" aria-live="polite">
        <div
          class="undo-send-bar__progress"
          style={{ width: `${percent()}%` }}
        />
        <div class="undo-send-bar__content">
          <span class="undo-send-bar__icon" aria-hidden="true">
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <circle cx="12" cy="12" r="10" />
              <polyline points="12 6 12 12 16 14" />
            </svg>
          </span>
          <span class="undo-send-bar__text">
            Sending to{" "}
            <span class="undo-send-bar__recipient">{recipientSnippet()}</span>{" "}
            in <strong>{pendingSend()!.secondsRemaining}s</strong>…
          </span>
          <div class="undo-send-bar__actions">
            <button
              type="button"
              class="undo-send-bar__btn undo-send-bar__btn--undo"
              onClick={undoPendingSend}
            >
              Undo
            </button>
            <button
              type="button"
              class="undo-send-bar__btn undo-send-bar__btn--now"
              onClick={sendPendingNow}
            >
              Send now
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}

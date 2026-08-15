import { createEffect, createSignal, Show } from "solid-js";
import { formatClock, formatFullDate } from "../lib/format";
import type { CalendarInvite } from "../lib/ipc/CalendarInvite";
import { rsvpInvite } from "../lib/tauri";
import { detectVideoCall, getMapUrl } from "../lib/videoCall";
import "./InviteCard.css";

interface InviteCardProps {
  invite: CalendarInvite;
  accountId: number;
  messageId: number;
}

export function InviteCard(props: InviteCardProps) {
  const [partstat, setPartstat] = createSignal("NEEDS-ACTION");
  const [isSubmitting, setIsSubmitting] = createSignal(false);

  createEffect(() => {
    setPartstat(props.invite.userPartstat);
  });

  const startDate = () => new Date(props.invite.startMs);
  const monthStr = () =>
    startDate().toLocaleDateString([], { month: "short" }).toUpperCase();
  const dayStr = () => startDate().getDate();
  const weekdayStr = () =>
    startDate().toLocaleDateString([], { weekday: "short" });

  const timeRangeStr = () => {
    if (props.invite.allDay) {
      return "All day";
    }
    return `${formatClock(props.invite.startMs)} – ${formatClock(props.invite.endMs)}`;
  };

  const videoCall = () => detectVideoCall(props.invite.location);
  const mapUrl = () => getMapUrl(props.invite.location);

  const handleRsvp = async (status: "ACCEPTED" | "TENTATIVE" | "DECLINED") => {
    setIsSubmitting(true);
    setPartstat(status);
    try {
      await rsvpInvite(props.accountId, props.messageId, status);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div
      class="invite-card"
      classList={{
        "invite-card--cancelled": props.invite.method === "CANCEL",
        "invite-card--accepted": partstat() === "ACCEPTED",
      }}
    >
      <div class="invite-card__date-block">
        <span class="invite-card__month">{monthStr()}</span>
        <span class="invite-card__day">{dayStr()}</span>
        <span class="invite-card__weekday">{weekdayStr()}</span>
      </div>

      <div class="invite-card__content">
        <div class="invite-card__header">
          <span class="invite-card__method-badge">
            {props.invite.method === "CANCEL"
              ? "Cancelled"
              : props.invite.method === "REPLY"
                ? "RSVP Reply"
                : props.invite.method === "COUNTER"
                  ? "Counter Proposal"
                  : "Invitation"}
          </span>
          <Show when={props.invite.method === "REQUEST"}>
            <span
              class="invite-card__status-pill"
              classList={{
                "invite-card__status-pill--accepted": partstat() === "ACCEPTED",
                "invite-card__status-pill--declined": partstat() === "DECLINED",
                "invite-card__status-pill--tentative":
                  partstat() === "TENTATIVE",
              }}
            >
              {partstat() === "ACCEPTED"
                ? "✓ Accepted"
                : partstat() === "DECLINED"
                  ? "✕ Declined"
                  : partstat() === "TENTATIVE"
                    ? "? Tentative"
                    : "Response Needed"}
            </span>
          </Show>
        </div>

        <h2 class="invite-card__title">{props.invite.title}</h2>

        <div class="invite-card__meta">
          <div class="invite-card__meta-item">
            <span class="invite-card__meta-label">When:</span>
            <span class="invite-card__meta-value">
              {formatFullDate(props.invite.startMs)} ({timeRangeStr()})
            </span>
          </div>

          <Show when={props.invite.location}>
            <div class="invite-card__meta-item">
              <span class="invite-card__meta-label">Where:</span>
              <span class="invite-card__meta-value">
                {props.invite.location}
                <Show when={mapUrl()}>
                  {(url) => (
                    <a
                      href={url()}
                      target="_blank"
                      rel="noreferrer"
                      style={{
                        "margin-left": "8px",
                        "font-size": "11px",
                        color: "var(--color-accent, #1F6FEB)",
                        "text-decoration": "none",
                      }}
                    >
                      📍 Maps
                    </a>
                  )}
                </Show>
              </span>
            </div>
          </Show>

          {/* Join Video Call Action */}
          <Show when={videoCall()}>
            {(vc) => (
              <div
                class="invite-card__meta-item"
                style={{ "margin-top": "4px" }}
              >
                <span class="invite-card__meta-label">Call:</span>
                <span class="invite-card__meta-value">
                  <a
                    href={vc().url}
                    target="_blank"
                    rel="noreferrer"
                    class="btn btn--sm btn--primary"
                    style={{
                      display: "inline-flex",
                      "align-items": "center",
                      gap: "4px",
                      "text-decoration": "none",
                      padding: "2px 8px",
                      "font-size": "11.5px",
                    }}
                  >
                    📹 {vc().label}
                  </a>
                </span>
              </div>
            )}
          </Show>

          <div class="invite-card__meta-item">
            <span class="invite-card__meta-label">Organizer:</span>
            <span class="invite-card__meta-value">
              {props.invite.organizerName ?? props.invite.organizerEmail}
            </span>
          </div>
        </div>

        <Show when={props.invite.method === "REQUEST"}>
          <div class="invite-card__actions">
            <button
              type="button"
              class="btn btn--sm invite-btn"
              classList={{
                "btn--primary": partstat() === "ACCEPTED",
                "btn--secondary": partstat() !== "ACCEPTED",
              }}
              disabled={isSubmitting()}
              onClick={() => void handleRsvp("ACCEPTED")}
            >
              Accept
            </button>
            <button
              type="button"
              class="btn btn--sm btn--secondary invite-btn"
              classList={{ "invite-btn--active": partstat() === "TENTATIVE" }}
              disabled={isSubmitting()}
              onClick={() => void handleRsvp("TENTATIVE")}
            >
              Tentative
            </button>
            <button
              type="button"
              class="btn btn--sm btn--secondary invite-btn"
              classList={{ "invite-btn--active": partstat() === "DECLINED" }}
              disabled={isSubmitting()}
              onClick={() => void handleRsvp("DECLINED")}
            >
              Decline
            </button>
          </div>
        </Show>

        <Show when={props.invite.method === "CANCEL"}>
          <div class="invite-card__cancelled-banner">
            This event has been cancelled by the organizer.
          </div>
        </Show>

        <Show when={props.invite.method === "COUNTER"}>
          <div class="invite-card__counter-banner">
            Counter-proposal received from attendee (read-only).
          </div>
        </Show>
      </div>
    </div>
  );
}

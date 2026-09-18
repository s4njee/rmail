import { Component, Show } from "solid-js";
import { AttendeeSummary } from "../headless/attendees";

/** A compact "✓ 2/3" attendee-acceptance pill shown on event blocks. */
export const AttendeeBadge: Component<{ summary: AttendeeSummary }> = (props) => {
  const label = () =>
    props.summary.total > 0 ? `✓ ${props.summary.accepted}/${props.summary.total}` : "";
  return (
    <Show when={props.summary.total > 0}>
      <span
        title={`${props.summary.accepted} accepted, ${props.summary.pending} pending, ${props.summary.declined} declined`}
        style={{
          "font-family": "var(--al-font-mono)",
          "font-size": "9.5px",
          color: "var(--al-accent, #1F6FEB)",
          "font-weight": 600,
          "white-space": "nowrap",
        }}
      >
        {label()}
      </span>
    </Show>
  );
};

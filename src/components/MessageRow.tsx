import type { JSX } from "solid-js";
import { formatRelativeTime } from "../lib/format";
import type { Account } from "../lib/ipc/Account";
import type { MessageRow as MessageRowData } from "../lib/ipc/MessageRow";
import "./MessageRow.css";

type MessageRowProps = {
  row: MessageRowData;
  account: Account | undefined;
  selected: boolean;
  onSelect: (id: number) => void;
};

// A 72px message row (Epic 6.2). The selection language is per-treatment: a
// 3px full-height rail (Hairline) or an 8px status dot (Banded). Read/unread
// colors come from CSS tokens; the account color is data, set inline as a
// custom property so the rail/dot can pick it up in pure CSS.
export function MessageRow(props: MessageRowProps) {
  return (
    <div
      class="list-row"
      role="option"
      id={`mail-opt-${props.row.id}`}
      aria-selected={props.selected}
      classList={{
        "is-selected": props.selected,
        "is-unread": props.row.unread,
      }}
      style={
        {
          "--account-color": props.account?.color ?? "transparent",
        } as JSX.CSSProperties
      }
      onClick={() => props.onSelect(props.row.id)}
    >
      <span class="list-row__rail" aria-hidden="true" />
      <span class="list-row__dot" aria-hidden="true" />
      <span class="list-row__content">
        <span class="list-row__top">
          <span class="list-row__sender">{props.row.sender_name}</span>
          <span class="list-row__time tabular">
            {formatRelativeTime(props.row.received_at_ms)}
          </span>
        </span>
        <span class="list-row__subject">{props.row.subject}</span>
        <span class="list-row__snippet">{props.row.snippet}</span>
      </span>
    </div>
  );
}

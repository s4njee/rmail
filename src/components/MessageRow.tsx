import type { JSX } from "solid-js";
import { formatRelativeTime } from "../lib/format";
import type { Account } from "../lib/ipc/Account";
import type { MessageRow as MessageRowData } from "../lib/ipc/MessageRow";
import "./MessageRow.css";

type MessageRowProps = {
  row: MessageRowData;
  account: Account | undefined;
  selected: boolean;
  multiSelected: boolean;
  onSelect: (id: number) => void;
  onToggleSelect: (id: number) => void;
  onSelectRange: (id: number) => void;
  onContextMenu: (e: MouseEvent) => void;
  onDragStart: (e: DragEvent) => void;
};

// A 72px message row (Epic 6.2). The selection language is per-treatment: a
// 3px full-height rail (Hairline) or an 8px status dot (Banded). Read/unread
// colors come from CSS tokens; the account color is data, set inline as a
// custom property so the rail/dot can pick it up in pure CSS.
//
// P1.1 multi-select: a plain click selects (single), Ctrl/Cmd+click toggles
// membership in the bulk set, Shift+click selects the range from the anchor.
export function MessageRow(props: MessageRowProps) {
  const handleClick = (e: MouseEvent) => {
    if (e.ctrlKey || e.metaKey) {
      props.onToggleSelect(props.row.id);
    } else if (e.shiftKey) {
      props.onSelectRange(props.row.id);
    } else {
      props.onSelect(props.row.id);
    }
  };

  return (
    <div
      class="list-row"
      role="option"
      id={`mail-opt-${props.row.id}`}
      aria-selected={props.selected}
      classList={{
        "is-selected": props.selected,
        "is-multi": props.multiSelected,
        "is-unread": props.row.unread,
      }}
      style={
        {
          "--account-color": props.account?.color ?? "transparent",
        } as JSX.CSSProperties
      }
      onClick={handleClick}
      onContextMenu={(e) => props.onContextMenu(e)}
      draggable
      onDragStart={(e) => props.onDragStart(e)}
    >
      <span class="list-row__rail" aria-hidden="true" />
      <span class="list-row__dot" aria-hidden="true" />
      <span class="list-row__multi" aria-hidden="true">
        {props.multiSelected ? "✓" : ""}
      </span>
      <span class="list-row__content">
        <span class="list-row__top">
          <span class="list-row__sender-wrap">
            {props.row.answered && (
              <span
                class="list-row__reply-icon"
                title="Replied"
                aria-label="Replied"
              >
                ↩
              </span>
            )}
            {props.row.forwarded && (
              <span
                class="list-row__forward-icon"
                title="Forwarded"
                aria-label="Forwarded"
              >
                ↪
              </span>
            )}
            <span class="list-row__sender">{props.row.sender_name}</span>
            {props.row.thread_count > 1 && (
              <span class="list-row__thread-count">
                {props.row.thread_count}
              </span>
            )}
          </span>
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

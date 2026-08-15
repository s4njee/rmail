import { createSignal, For, Show } from "solid-js";
import "./SearchHelp.css";

// P1.3: a small "?" next to the search field listing the search operators.
const OPERATORS: [string, string][] = [
  ["from:", "sender address or name"],
  ["to: · cc:", "recipient address or name"],
  ["subject:", "subject text"],
  ["has:attachment", "messages with attachments"],
  ["is:unread · is:read", "read state"],
  ["is:starred · is:unstarred", "starred state"],
  ["before: · after:", "date (YYYY-MM-DD, today, yesterday, …)"],
  ["in:", "folder"],
  ["account:", "account address"],
  ["calendar:", "calendar name (events)"],
];

export function SearchHelp() {
  const [open, setOpen] = createSignal(false);
  return (
    <span class="search-help">
      <button
        type="button"
        class="search-help__toggle"
        aria-label="Search operators"
        aria-expanded={open()}
        onClick={() => setOpen(!open())}
      >
        ?
      </button>
      <Show when={open()}>
        <div class="search-help__pop" role="tooltip">
          <For each={OPERATORS}>
            {([op, desc]) => (
              <div class="search-help__row">
                <code class="mono">{op}</code>
                <span>{desc}</span>
              </div>
            )}
          </For>
        </div>
      </Show>
    </span>
  );
}

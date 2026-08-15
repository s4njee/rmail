import { Component, createEffect, createSignal, For, Show } from "solid-js";
import { Event, SearchResults } from "../types/calendar";

export interface SearchModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSearch: (query: string) => Promise<SearchResults>;
  onSelectEvent: (event: Event) => void;
  onSelectDate: (date: Date) => void;
}

export const SearchModal: Component<SearchModalProps> = (props) => {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<SearchResults>({ events: [], tasks: [] });
  const [loading, setLoading] = createSignal(false);

  createEffect(async () => {
    if (!props.isOpen) {
      setQuery("");
      setResults({ events: [], tasks: [] });
      return;
    }

    const q = query().trim();
    if (!q) {
      setResults({ events: [], tasks: [] });
      return;
    }

    setLoading(true);
    try {
      const res = await props.onSearch(q);
      setResults(res);
    } catch (err) {
      console.error("Search error:", err);
    } finally {
      setLoading(false);
    }
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") {
      props.onClose();
    }
  };

  const handleDateJump = (dateStr: string) => {
    const [y, m, d] = dateStr.split("-").map(Number);
    props.onSelectDate(new Date(y, m - 1, d));
    props.onClose();
  };

  return (
    <Show when={props.isOpen}>
      {/* Scrim */}
      <div
        onClick={props.onClose}
        style={{
          position: "fixed",
          inset: "52px 0 0 0",
          background: "var(--al-scrim, rgba(0,0,0,0.34))",
          "z-index": 100,
        }}
      />

      {/* Sheet */}
      <div
        onKeyDown={handleKeyDown}
        style={{
          position: "fixed",
          left: "50%",
          top: "96px",
          transform: "translateX(-50%)",
          width: "576px",
          "max-height": "calc(100vh - 140px)",
          background: "var(--al-surface, #FFFFFF)",
          "border-radius": "14px",
          "box-shadow": "var(--al-shadow-modal, 0 40px 80px -20px rgba(0,0,0,0.5))",
          overflow: "hidden",
          display: "flex",
          "flex-direction": "column",
          "z-index": 101,
          "font-family": "var(--al-font-ui)",
          color: "var(--al-ink, #1A1A1A)",
        }}
      >
        {/* Search Input Bar */}
        <div
          style={{
            display: "flex",
            "align-items": "center",
            gap: "12px",
            padding: "16px 20px",
            "border-bottom": "1px solid var(--al-grid, #EBEBEB)",
          }}
        >
          <div
            style={{
              width: "14px",
              height: "14px",
              border: "2px solid var(--al-ink-7, #A0A0A0)",
              "border-radius": "50%",
              flex: "none",
            }}
          />
          <input
            type="text"
            placeholder="Search events, tasks, or type a date ('tomorrow', 'aug 13')..."
            value={query()}
            onInput={(e) => setQuery(e.currentTarget.value)}
            autofocus
            style={{
              flex: 1,
              "font-size": "16px",
              border: "none",
              outline: "none",
              background: "transparent",
              "font-family": "inherit",
              color: "var(--al-ink, #1A1A1A)",
            }}
          />
          <span
            style={{
              "font-family": "var(--al-font-mono)",
              "font-size": "11px",
              color: "var(--al-ink-7, #A0A0A0)",
            }}
          >
            ESC
          </span>
        </div>

        {/* Results List */}
        <div
          style={{
            padding: "16px 20px",
            "overflow-y": "auto",
            display: "flex",
            "flex-direction": "column",
            gap: "16px",
          }}
        >
          {/* Matched date jump */}
          <Show when={results().matchedDate}>
            <div>
              <div
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "9.5px",
                  "letter-spacing": "0.12em",
                  color: "var(--al-ink-7, #A0A0A0)",
                  "margin-bottom": "8px",
                }}
              >
                JUMP TO DATE
              </div>
              <button
                type="button"
                onClick={() => handleDateJump(results().matchedDate!)}
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "8px",
                  width: "100%",
                  padding: "9px 12px",
                  background: "var(--al-accent-tint, #E4EBF8)",
                  border: "none",
                  "border-radius": "8px",
                  cursor: "pointer",
                  "font-size": "13px",
                  color: "var(--al-accent, #1F6FEB)",
                  "font-weight": 500,
                  "text-align": "left",
                }}
              >
                <span>📅 Go to {results().matchedDate}</span>
              </button>
            </div>
          </Show>

          {/* Events */}
          <Show when={results().events.length > 0}>
            <div>
              <div
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "9.5px",
                  "letter-spacing": "0.12em",
                  color: "var(--al-ink-7, #A0A0A0)",
                  "margin-bottom": "8px",
                }}
              >
                EVENTS ({results().events.length})
              </div>
              <div style={{ display: "flex", "flex-direction": "column", gap: "4px" }}>
                <For each={results().events}>
                  {(evt) => {
                    const start = new Date(evt.startsAt);
                    const dateText = start.toLocaleDateString(undefined, {
                      month: "short",
                      day: "numeric",
                    });
                    return (
                      <button
                        type="button"
                        onClick={() => {
                          props.onSelectEvent(evt);
                          props.onClose();
                        }}
                        style={{
                          display: "flex",
                          "align-items": "center",
                          gap: "12px",
                          padding: "8px 12px",
                          "border-radius": "7px",
                          background: "none",
                          border: "none",
                          cursor: "pointer",
                          "text-align": "left",
                        }}
                      >
                        <div
                          style={{
                            width: "3px",
                            height: "14px",
                            "border-radius": "2px",
                            background: "var(--al-accent, #1F6FEB)",
                            flex: "none",
                          }}
                        />
                        <span
                          style={{ "font-size": "13px", color: "var(--al-ink, #1A1A1A)", flex: 1 }}
                        >
                          {evt.title}
                        </span>
                        <span
                          style={{
                            "font-family": "var(--al-font-mono)",
                            "font-size": "11px",
                            color: "var(--al-ink-7, #A0A0A0)",
                          }}
                        >
                          {dateText}
                        </span>
                      </button>
                    );
                  }}
                </For>
              </div>
            </div>
          </Show>

          {/* Tasks */}
          <Show when={results().tasks.length > 0}>
            <div>
              <div
                style={{
                  "font-family": "var(--al-font-mono)",
                  "font-size": "9.5px",
                  "letter-spacing": "0.12em",
                  color: "var(--al-ink-7, #A0A0A0)",
                  "margin-bottom": "8px",
                }}
              >
                TASKS ({results().tasks.length})
              </div>
              <div style={{ display: "flex", "flex-direction": "column", gap: "4px" }}>
                <For each={results().tasks}>
                  {(task) => (
                    <div
                      style={{
                        display: "flex",
                        "align-items": "center",
                        gap: "10px",
                        padding: "6px 12px",
                      }}
                    >
                      <div
                        style={{
                          width: "10px",
                          height: "10px",
                          "border-radius": "50%",
                          border: "1.5px solid var(--al-cal-classes, #C2410C)",
                          flex: "none",
                        }}
                      />
                      <span style={{ "font-size": "13px", color: "var(--al-ink, #1A1A1A)" }}>
                        {task.title}
                      </span>
                    </div>
                  )}
                </For>
              </div>
            </div>
          </Show>

          <Show
            when={
              query().trim() &&
              !loading() &&
              results().events.length === 0 &&
              results().tasks.length === 0 &&
              !results().matchedDate
            }
          >
            <div
              style={{
                padding: "24px 0",
                "text-align": "center",
                color: "var(--al-ink-7, #A0A0A0)",
                "font-size": "13px",
              }}
            >
              No matches found for "{query()}"
            </div>
          </Show>
        </div>
      </div>
    </Show>
  );
};

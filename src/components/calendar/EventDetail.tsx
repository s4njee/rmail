import { createEffect, createSignal, For, Show } from "solid-js";
import { loadEvents, removeEvent, saveEvent, useSelectedEvent } from "../../lib/calendar";
import { duplicateEvent } from "../../lib/tauri";
import { detectVideoCall, getMapUrl } from "../../lib/videoCall";
import "./EventDetail.css";

const EVENT_COLORS = ["#3b5bdb", "#0f766e", "#b4451f", "#e8590c", "#7048e8", "#e03131"];

function formatRange(startMs: number, endMs: number): string {
  const start = new Date(startMs);
  const end = new Date(endMs);
  const day = start.toLocaleDateString([], {
    weekday: "short",
    month: "short",
    day: "numeric",
  });
  const time = (d: Date) =>
    d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  return `${day}, ${time(start)} – ${time(end)}`;
}

// Event detail in a reading-pane-styled pane (Epic 14.5) — the event's
// subject/time take the reading-pane scale, with inline editing and delete.
export function EventDetail() {
  const selected = useSelectedEvent();
  const [title, setTitle] = createSignal("");
  const [location, setLocation] = createSignal("");
  const [notes, setNotes] = createSignal("");
  const [travelTime, setTravelTime] = createSignal<number | null>(null);
  const [color, setColor] = createSignal("");

  createEffect(() => {
    const event = selected();
    if (event) {
      setTitle(event.title);
      setLocation(event.location ?? "");
      setNotes(event.notes ?? "");
      setTravelTime(event.travel_time_minutes ?? null);
      setColor(event.color ?? "");
    }
  });

  const videoCall = () => detectVideoCall(location(), notes());
  const mapUrl = () => getMapUrl(location());

  const save = () => {
    const event = selected();
    if (!event) return;
    void saveEvent({
      ...event,
      title: title(),
      location: location() || null,
      notes: notes() || null,
      travel_time_minutes: travelTime(),
      color: color() || null,
    });
  };

  const duplicate = async () => {
    const event = selected();
    if (!event) return;
    await duplicateEvent(event.id);
    const now = Date.now();
    await loadEvents(now - 90 * 86400000, now + 90 * 86400000);
  };

  return (
    <aside class="event-detail" aria-label="Event details">
      <Show
        when={selected()}
        fallback={
          <div class="event-detail__empty">Select an event to see it here.</div>
        }
      >
        {(event) => (
          <>
            <h2 class="event-detail__title">{event().title}</h2>
            <div class="event-detail__time">
              {formatRange(event().start_ms, event().end_ms)}
            </div>

            {/* Prominent Video Call Join Action */}
            <Show when={videoCall()}>
              {(vc) => (
                <div
                  class="event-detail__video-action"
                  style={{ margin: "12px 0 6px 0" }}
                >
                  <a
                    href={vc().url}
                    target="_blank"
                    rel="noreferrer"
                    class="btn btn--primary"
                    style={{
                      display: "inline-flex",
                      "align-items": "center",
                      gap: "6px",
                      "text-decoration": "none",
                      width: "100%",
                      "justify-content": "center",
                    }}
                  >
                    📹 {vc().label}
                  </a>
                </div>
              )}
            </Show>

            <label class="event-detail__field">
              <span>Title</span>
              <input
                type="text"
                value={title()}
                onInput={(e) => setTitle(e.currentTarget.value)}
              />
            </label>
            <label class="event-detail__field">
              <div
                style={{
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "space-between",
                }}
              >
                <span>Location</span>
                <Show when={mapUrl()}>
                  {(url) => (
                    <a
                      href={url()}
                      target="_blank"
                      rel="noreferrer"
                      style={{
                        "font-size": "11px",
                        color: "var(--color-accent, #1F6FEB)",
                        "text-decoration": "none",
                      }}
                    >
                      📍 Open in Maps
                    </a>
                  )}
                </Show>
              </div>
              <input
                type="text"
                value={location()}
                onInput={(e) => setLocation(e.currentTarget.value)}
              />
            </label>

            <label class="event-detail__field">
              <span>Travel Time (minutes)</span>
              <input
                type="number"
                min="0"
                step="15"
                placeholder="0"
                value={travelTime() ?? ""}
                onInput={(e) => {
                  const val = Number(e.currentTarget.value);
                  setTravelTime(val > 0 ? val : null);
                }}
              />
            </label>

            <label class="event-detail__field">
              <span>Notes</span>
              <textarea
                value={notes()}
                onInput={(e) => setNotes(e.currentTarget.value)}
              />
            </label>

            {/* P1.4 per-event color override */}
            <div class="event-detail__color-row">
              <span class="event-detail__color-label">Color</span>
              <div class="event-detail__color-swatches">
                <button
                  type="button"
                  class="event-detail__swatch"
                  classList={{ active: color() === "" }}
                  title="Use the calendar color"
                  aria-label="Clear color override"
                  onClick={() => setColor("")}
                  style={{
                    background:
                      "linear-gradient(135deg, #bbb 0 45%, #fff 45% 55%, #bbb 55%)",
                  }}
                />
                <For each={EVENT_COLORS}>
                  {(c) => (
                    <button
                      type="button"
                      class="event-detail__swatch"
                      classList={{ active: color() === c }}
                      aria-label={`Use color ${c}`}
                      onClick={() => setColor(c)}
                      style={{ background: c }}
                    />
                  )}
                </For>
              </div>
            </div>

            <div class="event-detail__actions">
              <button type="button" class="btn btn--primary" onClick={save}>
                Save
              </button>
              <button
                type="button"
                class="btn btn--secondary"
                onClick={() => window.print()}
                title="Print this event (⌘P)"
              >
                Print
              </button>
              <button
                type="button"
                class="btn btn--secondary"
                onClick={() => void duplicate()}
              >
                Duplicate
              </button>
              <button
                type="button"
                class="btn btn--secondary"
                onClick={() => void removeEvent(event().id)}
              >
                Delete
              </button>
            </div>
          </>
        )}
      </Show>
    </aside>
  );
}

import { createEffect, createSignal, Show } from "solid-js";
import { removeEvent, saveEvent, useSelectedEvent } from "../../lib/calendar";
import "./EventDetail.css";

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

  createEffect(() => {
    const event = selected();
    if (event) {
      setTitle(event.title);
      setLocation(event.location ?? "");
      setNotes(event.notes ?? "");
    }
  });

  const save = () => {
    const event = selected();
    if (!event) return;
    void saveEvent({
      ...event,
      title: title(),
      location: location() || null,
      notes: notes() || null,
    });
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

            <label class="event-detail__field">
              <span>Title</span>
              <input
                type="text"
                value={title()}
                onInput={(e) => setTitle(e.currentTarget.value)}
              />
            </label>
            <label class="event-detail__field">
              <span>Location</span>
              <input
                type="text"
                value={location()}
                onInput={(e) => setLocation(e.currentTarget.value)}
              />
            </label>
            <label class="event-detail__field">
              <span>Notes</span>
              <textarea
                value={notes()}
                onInput={(e) => setNotes(e.currentTarget.value)}
              />
            </label>

            <div class="event-detail__actions">
              <button type="button" class="btn btn--primary" onClick={save}>
                Save
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

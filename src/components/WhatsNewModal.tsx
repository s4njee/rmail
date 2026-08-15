import { For } from "solid-js";
import { notesFor } from "../lib/updates";
import { Modal } from "./Modal";
import "./WhatsNewModal.css";

// "What's new" shown on first launch after an update (E2.2). Lists curated
// release notes for the version the user just upgraded to.
export function WhatsNewModal(props: { version: string; onClose: () => void }) {
  const notes = () => notesFor(props.version) ?? [];
  return (
    <Modal title={`What's new in Quill ${props.version}`} onClose={props.onClose}>
      <ul class="whats-new__list">
        <For each={notes()}>
          {(note) => <li class="whats-new__item">{note}</li>}
        </For>
      </ul>
      <div class="whats-new__footer">
        <button
          type="button"
          class="btn btn--primary"
          onClick={() => props.onClose()}
        >
          Got it
        </button>
      </div>
    </Modal>
  );
}

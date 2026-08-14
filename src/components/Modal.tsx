import { onMount } from "solid-js";
import type { JSX } from "solid-js";
import "./Modal.css";

type ModalProps = {
  title: string;
  onClose: () => void;
  children: JSX.Element;
};

// A modal overlay (used by the add-account form, Epic 10.4). Escape or a
// backdrop click closes it; the dialog receives focus on open. Escape is
// stopped from reaching the global keymap, so it closes the modal and not the
// view beneath it.
export function Modal(props: ModalProps) {
  let ref: HTMLDivElement | undefined;
  onMount(() => ref?.focus());

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      props.onClose();
    }
  };

  return (
    <div
      class="modal-backdrop"
      onClick={(event) => {
        if (event.target === event.currentTarget) props.onClose();
      }}
    >
      <div
        ref={ref}
        class="modal"
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        tabindex="-1"
        onKeyDown={onKeyDown}
      >
        <header class="modal__header">
          <h2 class="modal__title">{props.title}</h2>
          <button
            type="button"
            class="modal__close"
            aria-label="Close"
            onClick={() => props.onClose()}
          >
            ×
          </button>
        </header>
        <div class="modal__body">{props.children}</div>
      </div>
    </div>
  );
}

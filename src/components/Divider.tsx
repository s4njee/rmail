import "./Divider.css";

export type DividerSide = "sidebar" | "list";

type DividerProps = {
  side: DividerSide;
  /** Called with the horizontal drag delta (positive = right) on each move. */
  onResize: (delta: number) => void;
  /** Called when the drag (or a keyboard step) finishes. */
  onResizeEnd: () => void;
};

const ARROW_STEP = 8;

// Resizable pane divider (Epic 4.2): ≥4px hit area, col-resize cursor,
// widths clamped by the caller, no text selection while dragging. Persistence
// happens on release (onResizeEnd).
export function Divider(props: DividerProps) {
  let startX = 0;

  const beginDrag = (event: PointerEvent) => {
    if (event.button !== 0) return;
    event.preventDefault();
    startX = event.clientX;
    document.body.classList.add("is-resizing");

    const move = (ev: PointerEvent) => props.onResize(ev.clientX - startX);
    const up = () => {
      document.body.classList.remove("is-resizing");
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      props.onResizeEnd();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      props.onResize(-ARROW_STEP);
      props.onResizeEnd();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      props.onResize(ARROW_STEP);
      props.onResizeEnd();
    }
  };

  return (
    <div
      class="divider"
      role="separator"
      aria-orientation="vertical"
      data-side={props.side}
      tabindex={0}
      onPointerDown={beginDrag}
      onKeyDown={onKeyDown}
    />
  );
}

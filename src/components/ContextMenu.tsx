import { For, onCleanup, onMount, Show } from "solid-js";
import { closeContextMenu, useContextMenu } from "../lib/context-menu";
import "./ContextMenu.css";

// A lightweight right-click menu (context-menu state in lib/context-menu).
// Positioned at the cursor, clamped to the viewport; closes on outside mousedown,
// Escape, or scroll. Rendered once at the App root.
export function ContextMenu() {
  const menu = useContextMenu();

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape") closeContextMenu();
  };
  const onPointerDown = (e: MouseEvent) => {
    if (!(e.target as HTMLElement)?.closest(".context-menu")) closeContextMenu();
  };
  const onScroll = () => closeContextMenu();

  onMount(() => {
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("blur", closeContextMenu);
    window.addEventListener("scroll", onScroll, true);
    onCleanup(() => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("blur", closeContextMenu);
      window.removeEventListener("scroll", onScroll, true);
    });
  });

  const pos = () => {
    const m = menu();
    if (!m) return null;
    const w = 180; // approximate menu width for clamping
    const h = 40 + m.items.length * 28;
    return {
      left: Math.min(m.x, Math.max(0, window.innerWidth - w)),
      top: Math.min(m.y, Math.max(0, window.innerHeight - h)),
    };
  };

  return (
    <Show when={menu()}>
      {(m) => (
        <div
          class="context-menu"
          role="menu"
          style={{
            left: `${pos()?.left ?? m().x}px`,
            top: `${pos()?.top ?? m().y}px`,
          }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <For each={m().items}>
            {(item) => (
              <button
                type="button"
                class="context-menu__item"
                classList={{ "context-menu__item--danger": item.danger }}
                role="menuitem"
                onClick={() => {
                  closeContextMenu();
                  item.onSelect();
                }}
              >
                {item.label}
              </button>
            )}
          </For>
        </div>
      )}
    </Show>
  );
}

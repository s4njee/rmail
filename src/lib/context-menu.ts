import { createSignal } from "solid-js";

// Shared context-menu state (right-click on account/calendar rows). The
// ContextMenu component is rendered once at the App root and reads this; any
// handler calls `openContextMenu` with items at the cursor position.

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  /** Render the item in the danger color (delete/remove). */
  danger?: boolean;
}

export interface ContextMenuState {
  x: number;
  y: number;
  items: ContextMenuItem[];
}

const [menu, setMenu] = createSignal<ContextMenuState | null>(null);

export function useContextMenu(): () => ContextMenuState | null {
  return menu;
}

export function openContextMenu(items: ContextMenuItem[], x: number, y: number): void {
  setMenu({ x, y, items });
}

export function closeContextMenu(): void {
  setMenu(null);
}

import { For, Show } from "solid-js";
import {
  childrenOf,
  folderCount,
  isMailbox,
} from "../lib/folders";
import type { Folder } from "../lib/ipc/Folder";
import "./Sidebar.css";

export function FolderTree(props: {
  folders: Folder[];
  accountId: number;
  parentId?: number | null;
  depth?: number;
  selectedId: number | null;
  onSelect: (folder: Folder) => void;
  onToggle: (folder: Folder) => void;
  onContextMenu?: (folder: Folder, event: MouseEvent) => void;
  onDragOver?: (folder: Folder, event: DragEvent) => void;
  onDrop?: (folder: Folder, event: DragEvent) => void;
}) {
  const depth = () => props.depth ?? 0;
  const nodes = () =>
    childrenOf(props.folders, props.parentId ?? null, props.accountId);

  return (
    <For each={nodes()}>
      {(folder) => {
        const kids = () => childrenOf(props.folders, folder.id, props.accountId);
        const hasKids = () => kids().length > 0;
        const rolled = () => hasKids() && !folder.expanded;
        const count = () => folderCount(folder, rolled());
        return (
          <>
            <button
              type="button"
              class="sidebar__row sidebar__row--tree"
              classList={{
                "is-selected": props.selectedId === folder.id,
                "is-unsubscribed": !folder.subscribed,
                "is-disabled-sync": !folder.enabled,
              }}
              style={{ "--tree-depth": String(depth()) }}
              aria-current={props.selectedId === folder.id ? "true" : undefined}
              aria-expanded={hasKids() ? folder.expanded : undefined}
              title={folder.path}
              onClick={() => {
                if (folder.selectable) props.onSelect(folder);
              }}
              onContextMenu={(e) => props.onContextMenu?.(folder, e)}
              onDragOver={(e) => props.onDragOver?.(folder, e)}
              onDrop={(e) => props.onDrop?.(folder, e)}
            >
              <span
                class="sidebar__twistie"
                classList={{
                  "is-hidden": !hasKids(),
                  "is-open": folder.expanded,
                }}
                aria-hidden="true"
                onClick={(e) => {
                  e.stopPropagation();
                  if (hasKids()) props.onToggle(folder);
                }}
              >
                ▸
              </span>
              <span class="sidebar__dot" aria-hidden="true" />
              <span class="sidebar__row-text">{folder.name}</span>
              <Show when={folder.favourite}>
                <span class="sidebar__fav" aria-label="Favourite">
                  ★
                </span>
              </Show>
              <Show when={count()}>
                {(c) => (
                  <span
                    class="sidebar__count tabular"
                    classList={{ "is-unread": c().unread }}
                  >
                    {c().text}
                  </span>
                )}
              </Show>
            </button>
            <Show when={folder.expanded && hasKids()}>
              <FolderTree
                folders={props.folders}
                accountId={props.accountId}
                parentId={folder.id}
                depth={depth() + 1}
                selectedId={props.selectedId}
                onSelect={props.onSelect}
                onToggle={props.onToggle}
                onContextMenu={props.onContextMenu}
                onDragOver={props.onDragOver}
                onDrop={props.onDrop}
              />
            </Show>
          </>
        );
      }}
    </For>
  );
}

export function FolderDestList(props: {
  folders: Folder[];
  onPick: (folder: Folder) => void;
}) {
  const dests = () => props.folders.filter(isMailbox);
  return (
    <For each={dests()}>
      {(folder) => (
        <button
          type="button"
          role="menuitem"
          class="bulk-move__item"
          onClick={() => props.onPick(folder)}
        >
          {folder.account_id != null && folder.path !== folder.name
            ? folder.path
            : folder.name}
        </button>
      )}
    </For>
  );
}

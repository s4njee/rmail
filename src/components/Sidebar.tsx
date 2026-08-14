import { For, Show } from "solid-js";
import { formatBytes } from "../lib/format";
import {
  selectAccount,
  selectFolder,
  useAccounts,
  useFilter,
  useFolders,
} from "../lib/mail";
import { effectiveSidebarWidth } from "../lib/panes";
import {
  connectivityText,
  useConnectivity,
  useFootprintBytes,
} from "../lib/store-events";
import { useTheme } from "../lib/theme";
import { switchSection, useSection } from "../lib/ui";
import "./Sidebar.css";

// The sidebar (Epic 5): wordmark, folder rows with live counts, account rows,
// and the footer (Hairline connectivity status) / "On this device" card
// (Banded footprint). Folder/account clicks set the filter the message list
// consumes. Structural differences (footer vs card, folder dot presence)
// follow Epic 2.2 — one branch on useTheme() each.
export function Sidebar() {
  const theme = useTheme();
  const folders = useFolders();
  const accounts = useAccounts();
  const filter = useFilter();
  const connectivity = useConnectivity();
  const footprintBytes = useFootprintBytes();
  const section = useSection();

  const isFolderActive = (id: number) => {
    const current = filter();
    return current.kind === "folder" && current.folderId === id;
  };
  const isAccountActive = (id: number) => {
    const current = filter();
    return current.kind === "account" && current.accountId === id;
  };

  return (
    <aside
      class="sidebar"
      aria-label="Sidebar"
      style={{ width: `${effectiveSidebarWidth(theme)}px` }}
    >
      <div class="sidebar__wordmark">Quill</div>

      {/* Mail / Calendar section switch (Epic 14.6) — extrapolated. */}
      <div class="sidebar__section-switch" role="tablist" aria-label="Section">
        <button
          type="button"
          class="sidebar__section-tab"
          classList={{ "is-selected": section() === "mail" }}
          role="tab"
          aria-selected={section() === "mail"}
          onClick={() => switchSection("mail")}
        >
          Mail
        </button>
        <button
          type="button"
          class="sidebar__section-tab"
          classList={{ "is-selected": section() === "calendar" }}
          role="tab"
          aria-selected={section() === "calendar"}
          onClick={() => switchSection("calendar")}
        >
          Calendar
        </button>
      </div>

      <div class="sidebar__navs">
        <h2 class="sidebar__label">Unified</h2>
        <nav class="sidebar__folders" aria-label="Folders">
          <For each={folders()}>
            {(folder) => (
              <button
                type="button"
                class="sidebar__row"
                classList={{ "is-selected": isFolderActive(folder.id) }}
                aria-current={isFolderActive(folder.id) ? "true" : undefined}
                onClick={() => selectFolder(folder.id)}
              >
                <span class="sidebar__dot" aria-hidden="true" />
                <span class="sidebar__row-text">{folder.name}</span>
                <Show when={folder.total_count > 0}>
                  <span class="sidebar__count tabular">
                    {folder.total_count}
                  </span>
                </Show>
              </button>
            )}
          </For>
        </nav>

        <h2 class="sidebar__label sidebar__label--accounts">Accounts</h2>
        <nav class="sidebar__accounts" aria-label="Accounts">
          <For each={accounts()}>
            {(account) => (
              <button
                type="button"
                class="sidebar__account-row"
                classList={{ "is-selected": isAccountActive(account.id) }}
                aria-current={isAccountActive(account.id) ? "true" : undefined}
                onClick={() => selectAccount(account.id)}
              >
                <span
                  class="sidebar__account-dot"
                  style={{ background: account.color }}
                  aria-hidden="true"
                />
                <span class="sidebar__account-address">{account.address}</span>
              </button>
            )}
          </For>
        </nav>
      </div>

      {theme() === "banded" ? (
        <div class="sidebar-card">
          <div class="sidebar-card__title">On this device</div>
          <div class="sidebar-card__line">
            {formatBytes(footprintBytes())} mail cache
          </div>
        </div>
      ) : (
        <div
          class="sidebar-footer"
          role="status"
          data-connectivity={connectivity().state}
        >
          <span class="sidebar-footer__dot" aria-hidden="true" />
          <span>{connectivityText(connectivity())}</span>
        </div>
      )}
    </aside>
  );
}

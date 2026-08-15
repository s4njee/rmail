import { createEffect, createSignal, For, Show } from "solid-js";
import type { NotificationSettings } from "../../lib/ipc/NotificationSettings";
import { useAccounts, useFolders } from "../../lib/mail";
import { syncDockBadge } from "../../lib/notifications";
import { updateSettings, useSettings } from "../../lib/settings";
import "../Settings.css";

const DEFAULT_NOTIFICATIONS: NotificationSettings = {
  enabled: true,
  sound: true,
  dockBadge: true,
  quietHoursEnabled: false,
  quietHoursStart: "22:00",
  quietHoursEnd: "08:00",
  knownContactsOnly: false,
  defaultAlarmMinutes: 15,
  perAccount: [],
};

export function NotificationsSection() {
  const settings = useSettings();
  const accounts = useAccounts();
  const folders = useFolders();

  const [notif, setNotif] = createSignal<NotificationSettings>(
    DEFAULT_NOTIFICATIONS,
  );

  // Initialize once settings resolve. `onMount` reads settings() untracked, so
  // if Settings opens before initSettings() resolves, the form keeps defaults
  // and the next toggle persists them over the stored config. The hydrated
  // guard stops later settings updates from clobbering in-progress edits.
  const [hydrated, setHydrated] = createSignal(false);
  createEffect(() => {
    if (hydrated()) return;
    const s = settings()?.notifications;
    if (s) {
      setNotif(s);
      setHydrated(true);
    }
  });

  const save = async (next: NotificationSettings) => {
    setNotif(next);
    await updateSettings({ notifications: next });
    // Toggling any notification option must not clear the dock badge — pass the
    // real inbox unread count so an enabled badge stays accurate.
    const inboxUnread =
      folders().find((f) => f.kind === "inbox")?.unread_count ?? 0;
    syncDockBadge(inboxUnread, next);
  };

  const toggleAccount = (accountId: number, enabled: boolean) => {
    const current = notif();
    const existing = current.perAccount.find((a) => a.accountId === accountId);
    let nextList = [...current.perAccount];
    if (existing) {
      nextList = nextList.map((a) =>
        a.accountId === accountId ? { ...a, enabled } : a,
      );
    } else {
      nextList.push({
        accountId,
        enabled,
        folders: ["Inbox"],
      });
    }
    void save({ ...current, perAccount: nextList });
  };

  const isAccountEnabled = (accountId: number) => {
    const found = notif().perAccount.find((a) => a.accountId === accountId);
    return found ? found.enabled : true;
  };

  return (
    <div class="notifications-settings">
      <div class="settings-row">
        <label class="general-option__label">
          <input
            type="checkbox"
            checked={notif().enabled}
            onChange={(e) =>
              void save({ ...notif(), enabled: e.currentTarget.checked })
            }
          />
          <span class="general-option__text">
            <span class="general-option__title">
              Enable desktop notifications
            </span>
            <span class="general-option__desc">
              Show native notifications when new emails arrive
            </span>
          </span>
        </label>
      </div>

      <Show when={notif().enabled}>
        <div class="settings-row">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={notif().dockBadge}
              onChange={(e) =>
                void save({ ...notif(), dockBadge: e.currentTarget.checked })
              }
            />
            <span class="general-option__text">
              <span class="general-option__title">
                Show unread count on dock / taskbar
              </span>
              <span class="general-option__desc">
                Display badge indicator with the number of unread messages
              </span>
            </span>
          </label>
        </div>

        <div class="settings-row">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={notif().sound}
              onChange={(e) =>
                void save({ ...notif(), sound: e.currentTarget.checked })
              }
            />
            <span class="general-option__text">
              <span class="general-option__title">Play notification sound</span>
              <span class="general-option__desc">
                Play an alert sound when a notification arrives
              </span>
            </span>
          </label>
        </div>

        <div class="settings-row">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={notif().knownContactsOnly}
              onChange={(e) =>
                void save({
                  ...notif(),
                  knownContactsOnly: e.currentTarget.checked,
                })
              }
            />
            <span class="general-option__text">
              <span class="general-option__title">
                Notify only for people I know
              </span>
              <span class="general-option__desc">
                Suppress notifications from unfamiliar senders or mailing lists
              </span>
            </span>
          </label>
        </div>

        <div class="settings-row notif-alarm-row">
          <div class="general-option__text">
            <span class="general-option__title">Default calendar reminder</span>
            <span class="general-option__desc">
              Notify before scheduled event start time
            </span>
          </div>
          <select
            class="notif-time-input"
            value={notif().defaultAlarmMinutes ?? "none"}
            onChange={(e) => {
              const val = e.currentTarget.value;
              void save({
                ...notif(),
                defaultAlarmMinutes: val === "none" ? null : Number(val),
              });
            }}
          >
            <option value="none">None</option>
            <option value="0">At time of event</option>
            <option value="5">5 minutes before</option>
            <option value="10">10 minutes before</option>
            <option value="15">15 minutes before</option>
            <option value="30">30 minutes before</option>
            <option value="60">1 hour before</option>
          </select>
        </div>

        <div class="settings-row notif-quiet-hours">
          <label class="general-option__label">
            <input
              type="checkbox"
              checked={notif().quietHoursEnabled}
              onChange={(e) =>
                void save({
                  ...notif(),
                  quietHoursEnabled: e.currentTarget.checked,
                })
              }
            />
            <span class="general-option__text">
              <span class="general-option__title">Quiet hours</span>
              <span class="general-option__desc">
                Mute notifications during scheduled hours
              </span>
            </span>
          </label>

          <Show when={notif().quietHoursEnabled}>
            <div class="notif-quiet-schedule">
              <span class="notif-schedule-label">From</span>
              <input
                type="time"
                class="notif-time-input"
                value={notif().quietHoursStart}
                onChange={(e) =>
                  void save({
                    ...notif(),
                    quietHoursStart: e.currentTarget.value,
                  })
                }
              />
              <span class="notif-schedule-label">to</span>
              <input
                type="time"
                class="notif-time-input"
                value={notif().quietHoursEnd}
                onChange={(e) =>
                  void save({
                    ...notif(),
                    quietHoursEnd: e.currentTarget.value,
                  })
                }
              />
            </div>
          </Show>
        </div>

        <div class="notif-accounts-header">
          <span class="notif-accounts-title">Per-Account Notifications</span>
        </div>

        <div class="notif-accounts-list">
          <For each={accounts()}>
            {(acc) => (
              <div class="settings-row notif-account-row">
                <label class="general-option__label">
                  <input
                    type="checkbox"
                    checked={isAccountEnabled(acc.id)}
                    onChange={(e) =>
                      toggleAccount(acc.id, e.currentTarget.checked)
                    }
                  />
                  <span class="general-option__text">
                    <span class="general-option__title">{acc.address}</span>
                    <span class="general-option__desc">
                      {acc.protocol} ·{" "}
                      {folders().find((f) => f.kind === "inbox")?.name ??
                        "Inbox"}
                    </span>
                  </span>
                </label>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

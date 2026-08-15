import { invoke } from "@tauri-apps/api/core";
import type { MessageRow } from "./ipc/MessageRow";
import type { NotificationSettings } from "./ipc/NotificationSettings";
import { isTauri } from "./tauri";

export function isQuietHours(startStr: string, endStr: string): boolean {
  if (!startStr || !endStr) return false;
  const now = new Date();
  const currentMinutes = now.getHours() * 60 + now.getMinutes();

  const [startH, startM] = startStr.split(":").map(Number);
  const [endH, endM] = endStr.split(":").map(Number);

  const startMinutes = (startH || 0) * 60 + (startM || 0);
  const endMinutes = (endH || 0) * 60 + (endM || 0);

  if (startMinutes <= endMinutes) {
    return currentMinutes >= startMinutes && currentMinutes < endMinutes;
  }
  // Wraparound overnight (e.g. 22:00 to 08:00)
  return currentMinutes >= startMinutes || currentMinutes < endMinutes;
}

export function shouldNotifyMessage(
  msg: MessageRow,
  settings: NotificationSettings,
  knownAddresses: string[] = [],
): boolean {
  if (!settings.enabled) return false;

  if (
    settings.quietHoursEnabled &&
    isQuietHours(settings.quietHoursStart, settings.quietHoursEnd)
  ) {
    return false;
  }

  if (settings.knownContactsOnly) {
    const isKnown = knownAddresses.some(
      (addr) => addr.toLowerCase() === msg.sender_address.toLowerCase(),
    );
    if (!isKnown) return false;
  }

  const accountSetting = settings.perAccount.find(
    (a) => a.accountId === msg.account_id,
  );
  if (accountSetting) {
    if (!accountSetting.enabled) return false;
    if (
      accountSetting.folders.length > 0 &&
      !accountSetting.folders.includes(msg.folder)
    ) {
      return false;
    }
  }

  return true;
}

export async function setDockBadge(count: number | null): Promise<void> {
  if (isTauri()) {
    await invoke<void>("set_dock_badge", { count });
  }
}

export async function showNotification(
  title: string,
  body: string,
): Promise<void> {
  if (isTauri()) {
    await invoke<void>("show_notification", { title, body });
  } else if (
    "Notification" in window &&
    Notification.permission === "granted"
  ) {
    new Notification(title, { body });
  }
}

export function syncDockBadge(
  unreadCount: number,
  settings: NotificationSettings | undefined,
): void {
  if (!settings || !settings.dockBadge) {
    void setDockBadge(null);
    return;
  }
  void setDockBadge(unreadCount > 0 ? unreadCount : null);
}

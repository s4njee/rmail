// Humanize byte counts for the footprint readout (Epic 4.3 / 11.2).
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${Math.round(value)} ${units[unit]}`;
}

/** Local wall-clock, `HH:MM`, for the connectivity readouts. */
export function formatClock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

/** The mock's list timestamps: today → `11:38`, yesterday → `Yest`, else the
 * weekday (`Tue`, `Mon`). */
export function formatRelativeTime(ms: number): string {
  const date = new Date(ms);
  const now = new Date();
  if (date.toDateString() === now.toDateString()) {
    return date.toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    });
  }
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) return "Yest";
  return date.toLocaleDateString([], { weekday: "short" });
}

/** The reading-pane date: `Aug 13, 11:38`. */
export function formatFullDate(ms: number): string {
  const date = new Date(ms);
  const day = date.toLocaleDateString([], { month: "short", day: "numeric" });
  const time = date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
  return `${day}, ${time}`;
}

/** Avatar initials: up to 2 glyphs from the first two name words, uppercase,
 * code-point safe for non-ASCII names (Epic 7.1). */
export function avatarInitials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((word) => Array.from(word)[0]?.toUpperCase() ?? "")
    .join("");
}

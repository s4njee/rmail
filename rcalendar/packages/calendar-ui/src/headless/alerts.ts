/** Alert/reminder presets shared by the event editor and the settings screen. */

export interface AlertPreset {
  label: string;
  offsetMinutes: number | null;
}

/** Standard Apple-Calendar-style presets. Negative = before the event. */
export const ALERT_PRESETS: AlertPreset[] = [
  { label: "None", offsetMinutes: null },
  { label: "At time of event", offsetMinutes: 0 },
  { label: "5 minutes before", offsetMinutes: -5 },
  { label: "15 minutes before", offsetMinutes: -15 },
  { label: "30 minutes before", offsetMinutes: -30 },
  { label: "1 hour before", offsetMinutes: -60 },
  { label: "1 day before", offsetMinutes: -1440 },
  { label: "1 week before", offsetMinutes: -10080 },
];

/** Human-readable label for an offset value, even non-preset ones. */
export function formatAlertOffset(offsetMinutes: number | null | undefined): string {
  if (offsetMinutes == null) return "None";
  const preset = ALERT_PRESETS.find((p) => p.offsetMinutes === offsetMinutes);
  if (preset) return preset.label;
  if (offsetMinutes === 0) return "At time of event";
  if (offsetMinutes < 0) return `${-offsetMinutes} minutes before`;
  return `${offsetMinutes} minutes after`;
}

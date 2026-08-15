/**
 * Video call and map link detection utilities (Roadmap 4.5).
 */

export interface VideoCallInfo {
  provider:
    | "Google Meet"
    | "Zoom"
    | "Microsoft Teams"
    | "Webex"
    | "Jitsi"
    | "Whereby"
    | "FaceTime"
    | "Video Call";
  url: string;
  label: string;
}

const VIDEO_CALL_PATTERNS: {
  regex: RegExp;
  provider: VideoCallInfo["provider"];
}[] = [
  {
    regex: /https?:\/\/(?:[a-zA-Z0-9-]+\.)?zoom\.us\/j\/[0-9]+[^\s"']*/i,
    provider: "Zoom",
  },
  {
    regex: /https?:\/\/meet\.google\.com\/[a-z]{3}-[a-z]{4}-[a-z]{3}[^\s"']*/i,
    provider: "Google Meet",
  },
  {
    regex: /https?:\/\/teams\.microsoft\.com\/l\/meetup-join\/[^\s"']+/i,
    provider: "Microsoft Teams",
  },
  {
    regex: /https?:\/\/teams\.live\.com\/meet\/[^\s"']+/i,
    provider: "Microsoft Teams",
  },
  {
    regex: /https?:\/\/(?:[a-zA-Z0-9-]+\.)?webex\.com\/[^\s"']+/i,
    provider: "Webex",
  },
  {
    regex: /https?:\/\/meet\.jit\.si\/[a-zA-Z0-9_-]+[^\s"']*/i,
    provider: "Jitsi",
  },
  {
    regex: /https?:\/\/whereby\.com\/[a-zA-Z0-9_-]+[^\s"']*/i,
    provider: "Whereby",
  },
];

/**
 * Detect video conference URLs in location or notes.
 */
export function detectVideoCall(
  location?: string | null,
  notes?: string | null,
): VideoCallInfo | null {
  const combined = `${location || ""} \n ${notes || ""}`;

  for (const { regex, provider } of VIDEO_CALL_PATTERNS) {
    const match = combined.match(regex);
    if (match) {
      return {
        provider,
        url: match[0],
        label: `Join ${provider}`,
      };
    }
  }

  // Generic https:// meeting check if location is a raw URL
  if (location) {
    const trimmed = location.trim();
    if (trimmed.startsWith("https://") || trimmed.startsWith("http://")) {
      return {
        provider: "Video Call",
        url: trimmed,
        label: "Join Call",
      };
    }
  }

  return null;
}

/**
 * Return an Apple Maps / Web Maps search URL if the location looks like a physical address.
 */
export function getMapUrl(location?: string | null): string | null {
  if (!location) return null;
  const trimmed = location.trim();
  if (!trimmed) return null;

  // Don't treat URLs as physical map addresses
  if (
    trimmed.startsWith("http://") ||
    trimmed.startsWith("https://") ||
    trimmed.includes("zoom.us") ||
    trimmed.includes("meet.google")
  ) {
    return null;
  }

  // Virtual room names without street/city
  if (
    trimmed.toLowerCase().startsWith("virtual") ||
    trimmed.toLowerCase().startsWith("online")
  ) {
    return null;
  }

  return `https://maps.apple.com/?q=${encodeURIComponent(trimmed)}`;
}

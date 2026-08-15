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
 * received date (`Aug 13`). */
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
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
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

export type AttachmentCategory =
  | "pdf"
  | "image"
  | "archive"
  | "audio"
  | "video"
  | "code"
  | "document"
  | "spreadsheet"
  | "presentation"
  | "generic";

export interface AttachmentTypeInfo {
  category: AttachmentCategory;
  label: string;
  isPreviewable: boolean;
}

export function getAttachmentTypeInfo(filename: string): AttachmentTypeInfo {
  const ext = filename.split(".").pop()?.toLowerCase() || "";

  if (ext === "pdf") {
    return { category: "pdf", label: "PDF", isPreviewable: true };
  }
  if (
    ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"].includes(ext)
  ) {
    return { category: "image", label: "IMG", isPreviewable: true };
  }
  if (["zip", "tar", "gz", "tgz", "rar", "7z", "bz2", "xz"].includes(ext)) {
    return { category: "archive", label: "ZIP", isPreviewable: false };
  }
  if (["mp3", "wav", "m4a", "aac", "ogg", "flac"].includes(ext)) {
    return { category: "audio", label: "AUD", isPreviewable: true };
  }
  if (["mp4", "webm", "mov", "mkv", "avi"].includes(ext)) {
    return { category: "video", label: "VID", isPreviewable: true };
  }
  if (
    [
      "js",
      "ts",
      "jsx",
      "tsx",
      "rs",
      "py",
      "json",
      "html",
      "css",
      "md",
      "txt",
      "sh",
      "yaml",
      "yml",
      "toml",
    ].includes(ext)
  ) {
    return {
      category: "code",
      label: ext.toUpperCase().slice(0, 4),
      isPreviewable: true,
    };
  }
  if (["doc", "docx", "pages", "rtf", "odt"].includes(ext)) {
    return { category: "document", label: "DOC", isPreviewable: false };
  }
  if (["xls", "xlsx", "numbers", "csv", "tsv"].includes(ext)) {
    return {
      category: "spreadsheet",
      label: "XLS",
      isPreviewable: ["csv", "tsv"].includes(ext),
    };
  }
  if (["ppt", "pptx", "key"].includes(ext)) {
    return { category: "presentation", label: "PPT", isPreviewable: false };
  }

  return { category: "generic", label: "FILE", isPreviewable: false };
}

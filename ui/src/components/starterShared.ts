import { convertFileSrc } from "@tauri-apps/api/core";

export function toUiErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error !== null && "message" in error) {
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string" && message.trim()) {
      return message;
    }
  }
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  return fallback;
}

export function toThumbnailSrc(path: string | null | undefined): string | null {
  if (!path) {
    return null;
  }

  const lower = path.toLowerCase();
  if (
    lower.startsWith("http://") ||
    lower.startsWith("https://") ||
    lower.startsWith("data:") ||
    lower.startsWith("asset:")
  ) {
    return path;
  }

  const normalized = path.replace(/\\/g, "/");
  const withLeadingSlash = normalized.startsWith("/") ? normalized : `/${normalized}`;
  try {
    return convertFileSrc(withLeadingSlash);
  } catch {
    return encodeURI(`file://${withLeadingSlash}`);
  }
}

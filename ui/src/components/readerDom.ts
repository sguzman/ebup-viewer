import { convertFileSrc } from "@tauri-apps/api/core";

import type { HighlightColor } from "../types";
import { clamp, clamp01 } from "./readerShared";

export function toHexColor(color: HighlightColor): string {
  const r = Math.round(clamp01(color.r) * 255)
    .toString(16)
    .padStart(2, "0");
  const g = Math.round(clamp01(color.g) * 255)
    .toString(16)
    .padStart(2, "0");
  const b = Math.round(clamp01(color.b) * 255)
    .toString(16)
    .padStart(2, "0");
  return `#${r}${g}${b}`;
}

export function withHexColor(current: HighlightColor, hex: string): HighlightColor {
  const normalized = hex.replace("#", "");
  if (!/^[0-9a-fA-F]{6}$/.test(normalized)) {
    return current;
  }
  const r = Number.parseInt(normalized.slice(0, 2), 16) / 255;
  const g = Number.parseInt(normalized.slice(2, 4), 16) / 255;
  const b = Number.parseInt(normalized.slice(4, 6), 16) / 255;
  return {
    r: clamp01(r),
    g: clamp01(g),
    b: clamp01(b),
    a: clamp01(current.a)
  };
}

export function withAlpha(current: HighlightColor, alpha: number): HighlightColor {
  return {
    r: clamp01(current.r),
    g: clamp01(current.g),
    b: clamp01(current.b),
    a: clamp01(alpha)
  };
}

export function toReaderImageSrc(path: string): string {
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

export function scrollSentenceIntoView(
  container: HTMLElement,
  sentence: HTMLElement,
  center: boolean,
  behavior: ScrollBehavior
): void {
  const currentTop = container.scrollTop;
  let sentenceTop = sentence.offsetTop;
  let parent = sentence.offsetParent as HTMLElement | null;
  while (parent && parent !== container) {
    sentenceTop += parent.offsetTop;
    parent = parent.offsetParent as HTMLElement | null;
  }
  const sentenceHeight = sentence.offsetHeight;
  const sentenceBottom = sentenceTop + sentenceHeight;
  const viewportTop = currentTop;
  const viewportBottom = viewportTop + container.clientHeight;
  const maxTop = Math.max(0, container.scrollHeight - container.clientHeight);
  const padding = 16;

  let targetTop: number;
  if (center) {
    targetTop = sentenceTop - (container.clientHeight - sentenceHeight) / 2;
  } else if (sentenceTop < viewportTop + padding) {
    targetTop = sentenceTop - padding;
  } else if (sentenceBottom > viewportBottom - padding) {
    targetTop = sentenceBottom - container.clientHeight + padding;
  } else {
    return;
  }

  const clampedTop = clamp(targetTop, 0, maxTop);
  const targetTopPx = Math.round(clampedTop);
  if (Math.abs(targetTopPx - currentTop) < 1) {
    return;
  }
  container.scrollTo({ top: targetTopPx, behavior });
}

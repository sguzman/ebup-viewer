import ArrowBackIcon from "@mui/icons-material/ArrowBack";
import ChevronLeftIcon from "@mui/icons-material/ChevronLeft";
import ChevronRightIcon from "@mui/icons-material/ChevronRight";
import DarkModeOutlinedIcon from "@mui/icons-material/DarkModeOutlined";
import GpsFixedIcon from "@mui/icons-material/GpsFixed";
import LightModeOutlinedIcon from "@mui/icons-material/LightModeOutlined";
import SearchIcon from "@mui/icons-material/Search";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  Button,
  Card,
  CardContent,
  Divider,
  FormControl,
  InputLabel,
  MenuItem,
  Select,
  Slider,
  Stack,
  Switch,
  Tab,
  Tabs,
  TextField,
  Typography,
} from "@mui/material";
import { useCallback, useEffect, useMemo, useRef, useState, memo, type MouseEvent } from "react";

import { recordPerfMeasure, useRenderDebugCounter } from "../perf/debug";
import { buildHtmlSentenceAnchorMap } from "./htmlSync";
import { renderMarkdownToHtml } from "./markdownRender";
import { renderNativePrettyHtml } from "./prettyHtml";
import { computeReaderTypographyLayout } from "./readerTypography";
import { TtsPlayerWidget } from "./TtsPlayerWidget";
import type {
  FontFamily,
  FontWeight,
  HighlightColor,
  ReaderSettingsPatch,
  ReaderSnapshot,
  ThemeMode,
  TtsStateEvent
} from "../types";

interface ReaderShellProps {
  reader: ReaderSnapshot;
  busy: boolean;
  onCloseSession: () => Promise<void>;
  onPrevPage: () => Promise<void>;
  onNextPage: () => Promise<void>;
  onPrevSentence: () => Promise<void>;
  onNextSentence: () => Promise<void>;
  onSetPage: (page: number) => Promise<void>;
  onSentenceClick: (sentenceIdx: number) => Promise<void>;
  onToggleTextOnly: () => Promise<void>;
  onSearchQuery: (query: string) => Promise<void>;
  onSearchNext: () => Promise<void>;
  onSearchPrev: () => Promise<void>;
  onToggleTheme: () => Promise<void>;
  onToggleSettingsPanel: () => Promise<void>;
  onToggleStatsPanel: () => Promise<void>;
  onToggleTtsPanel: () => Promise<void>;
  onTtsPlay: () => Promise<void>;
  onTtsPause: () => Promise<void>;
  onTtsTogglePlayPause: () => Promise<void>;
  onTtsPlayFromPageStart: () => Promise<void>;
  onTtsPlayFromHighlight: () => Promise<void>;
  onTtsSeekNext: () => Promise<void>;
  onTtsSeekPrev: () => Promise<void>;
  onTtsRepeatSentence: () => Promise<void>;
  onTtsPrecomputePage: () => Promise<void>;
  onApplySettings: (patch: ReaderSettingsPatch) => Promise<void>;
  ttsStateEvent: TtsStateEvent | null;
}

interface NumericSettingControlProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  decimals?: number;
  testId?: string;
  onCommit: (value: number) => Promise<void>;
}

const FONT_FAMILY_OPTIONS: Array<{ value: FontFamily; label: string }> = [
  { value: "lexend", label: "Lexend" },
  { value: "sans", label: "Sans" },
  { value: "serif", label: "Serif" },
  { value: "monospace", label: "Monospace" },
  { value: "fira-code", label: "Fira Code" },
  { value: "atkinson-hyperlegible", label: "Atkinson Hyperlegible" },
  { value: "atkinson-hyperlegible-next", label: "Atkinson Hyperlegible Next" },
  { value: "lexica-ultralegible", label: "Lexica Ultralegible" },
  { value: "courier", label: "Courier" },
  { value: "frank-gothic", label: "Frank Gothic" },
  { value: "hermit", label: "Hermit" },
  { value: "hasklug", label: "Hasklug" },
  { value: "noto-sans", label: "Noto Sans" }
];

const FONT_WEIGHT_OPTIONS: Array<{ value: FontWeight; label: string }> = [
  { value: "light", label: "Light" },
  { value: "normal", label: "Normal" },
  { value: "bold", label: "Bold" }
];

function formatSeconds(seconds: number): string {
  const rounded = Math.max(0, Math.round(seconds));
  if (rounded >= 7 * 24 * 60 * 60) {
    const weeks = Math.floor(rounded / (7 * 24 * 60 * 60));
    const days = Math.floor((rounded % (7 * 24 * 60 * 60)) / (24 * 60 * 60));
    return days > 0 ? `${weeks}w ${days}d` : `${weeks}w`;
  }
  if (rounded >= 24 * 60 * 60) {
    const days = Math.floor(rounded / (24 * 60 * 60));
    const hours = Math.floor((rounded % (24 * 60 * 60)) / (60 * 60));
    return hours > 0 ? `${days}d ${hours}h` : `${days}d`;
  }
  if (rounded >= 60 * 60) {
    const hours = Math.floor(rounded / (60 * 60));
    const mins = Math.floor((rounded % (60 * 60)) / 60);
    return mins > 0 ? `${hours}h ${mins}m` : `${hours}h`;
  }
  const mins = Math.floor(rounded / 60);
  const secs = rounded % 60;
  return `${mins}m ${secs}s`;
}

function formatRemainingTime(
  seconds: number,
  mode: "adaptive" | "minutes-seconds"
): string {
  if (mode === "minutes-seconds") {
    const rounded = Math.max(0, Math.round(seconds));
    const mins = Math.floor(rounded / 60);
    const secs = rounded % 60;
    return `${mins}m ${secs}s`;
  }
  return formatSeconds(seconds);
}

function formatPercent(value: number): string {
  return `${value.toFixed(3)}%`;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function normalizeNumber(value: number, min: number, max: number, step: number, decimals: number): number {
  const clamped = clamp(value, min, max);
  if (step <= 0) {
    return Number(clamped.toFixed(decimals));
  }
  const snapped = min + Math.round((clamped - min) / step) * step;
  return Number(clamp(snapped, min, max).toFixed(decimals));
}

function almostEqual(a: number, b: number, decimals: number): boolean {
  const threshold = Math.max(1e-8, Math.pow(10, -decimals) / 2);
  return Math.abs(a - b) <= threshold;
}

function clamp01(value: number): number {
  return Math.min(1, Math.max(0, value));
}

function toHexColor(color: HighlightColor): string {
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

function withHexColor(current: HighlightColor, hex: string): HighlightColor {
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

function withAlpha(current: HighlightColor, alpha: number): HighlightColor {
  return {
    r: clamp01(current.r),
    g: clamp01(current.g),
    b: clamp01(current.b),
    a: clamp01(alpha)
  };
}

function toReaderImageSrc(path: string): string {
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

function scrollSentenceIntoView(
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

function NumericSettingControl({
  label,
  value,
  min,
  max,
  step,
  decimals = 2,
  testId,
  onCommit
}: NumericSettingControlProps) {
  const [inputValue, setInputValue] = useState(value.toFixed(decimals));
  const [sliderValue, setSliderValue] = useState(value);
  const [invalid, setInvalid] = useState(false);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    setInputValue(value.toFixed(decimals));
    setSliderValue(value);
    setInvalid(false);
  }, [decimals, value]);

  const parseValue = (raw: string): number | null => {
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) {
      return null;
    }
    if (parsed < min || parsed > max) {
      return null;
    }
    return parsed;
  };

  const commitNumber = async (candidate: number): Promise<void> => {
    const normalized = normalizeNumber(candidate, min, max, step, decimals);
    if (almostEqual(normalized, value, decimals)) {
      setInputValue(value.toFixed(decimals));
      setSliderValue(value);
      setInvalid(false);
      return;
    }
    setInputValue(normalized.toFixed(decimals));
    setSliderValue(normalized);
    setInvalid(false);
    await onCommit(normalized);
  };

  const commitRaw = async (raw: string): Promise<void> => {
    const parsed = parseValue(raw);
    if (parsed === null) {
      setInvalid(true);
      return;
    }
    await commitNumber(parsed);
  };

  return (
    <Stack spacing={0.75}>
      <Typography variant="caption" fontWeight={700}>
        {label}
      </Typography>
      <Stack direction="row" spacing={1.25} alignItems="center" sx={{ overflow: "visible" }}>
        <Slider
          value={sliderValue}
          min={min}
          max={max}
          step={step}
          onChange={(_, nextValue) => {
            if (typeof nextValue !== "number") {
              return;
            }
            setSliderValue(nextValue);
            setInputValue(nextValue.toFixed(decimals));
            setInvalid(false);
          }}
          onChangeCommitted={(_, nextValue) => {
            if (typeof nextValue !== "number") {
              return;
            }
            void commitNumber(nextValue);
          }}
          sx={{
            flex: 1,
            minWidth: 0,
            px: 1,
            overflow: "visible",
            "& .MuiSlider-thumb": {
              boxShadow: "none"
            }
          }}
        />
        <TextField
          inputRef={inputRef}
          size="small"
          value={inputValue}
          error={invalid}
          onChange={(event) => {
            const raw = event.target.value;
            setInputValue(raw);
            const parsed = parseValue(raw);
            setInvalid(parsed === null);
            if (parsed !== null) {
              setSliderValue(parsed);
            }
          }}
          onBlur={() => void commitRaw(inputValue)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              void commitRaw(inputValue);
            }
            if (event.key === "Escape") {
              event.preventDefault();
              setInputValue(value.toFixed(decimals));
              setSliderValue(value);
              setInvalid(false);
            }
          }}
          onWheel={(event) => {
            if (document.activeElement !== inputRef.current) {
              return;
            }
            event.preventDefault();
            const base = parseValue(inputValue) ?? value;
            const delta = event.deltaY < 0 ? step : -step;
            void commitNumber(base + delta);
          }}
          inputProps={{
            inputMode: "decimal",
            ...(testId ? { "data-testid": `${testId}-input` } : {})
          }}
          sx={{
            width: 98,
            "& .MuiInputBase-input": {
              color: invalid ? "error.main" : undefined
            }
          }}
        />
      </Stack>
    </Stack>
  );
}


export const ReaderShell = memo(function ReaderShell({
  reader,
  busy,
  onCloseSession,
  onPrevPage,
  onNextPage,
  onPrevSentence,
  onNextSentence,
  onSetPage,
  onSentenceClick,
  onToggleTextOnly,
  onSearchQuery,
  onSearchNext,
  onSearchPrev,
  onToggleTheme,
  onToggleSettingsPanel,
  onToggleStatsPanel,
  onToggleTtsPanel,
  onTtsPlay,
  onTtsPause,
  onTtsTogglePlayPause,
  onTtsPlayFromPageStart,
  onTtsPlayFromHighlight,
  onTtsSeekNext,
  onTtsSeekPrev,
  onTtsRepeatSentence,
  onTtsPrecomputePage,
  onApplySettings,
  ttsStateEvent
}: ReaderShellProps) {
  useRenderDebugCounter("ReaderShell");
  const [pageInput, setPageInput] = useState(String(reader.current_page + 1));
  const [searchInput, setSearchInput] = useState(reader.search_query);
  const [statsTab, setStatsTab] = useState<"page" | "global" | "session">("page");
  const sentenceRefs = useRef<Record<number, HTMLButtonElement | null>>({});
  const sentenceScrollRef = useRef<HTMLDivElement | null>(null);
  const [sessionNowMs, setSessionNowMs] = useState(Date.now());
  const sessionStartMsRef = useRef(Date.now());
  const listeningAccumulatedMsRef = useRef(0);
  const listeningStartedAtMsRef = useRef<number | null>(
    reader.tts.state === "playing" ? Date.now() : null
  );
  const sessionBaselineWordsRef = useRef(reader.stats.words_read_up_to_current_position);
  const sessionMaxWordsRef = useRef(reader.stats.words_read_up_to_current_position);
  const sessionBaselineSentencesRef = useRef(reader.stats.sentences_read_up_to_current_position);
  const sessionMaxSentencesRef = useRef(reader.stats.sentences_read_up_to_current_position);
  const sessionFinishedPagesRef = useRef<Set<number>>(new Set());
  const [sessionWordsRead, setSessionWordsRead] = useState(0);
  const [sessionSentencesRead, setSessionSentencesRead] = useState(0);
  const [sessionPagesFinished, setSessionPagesFinished] = useState(0);
  const nativeHtmlCacheRef = useRef<{ key: string; html: string }>({ key: "", html: "" });
  const nativeHtmlFrameRef = useRef<HTMLIFrameElement | null>(null);
  const prettyAnchorElementsRef = useRef<{
    html: Map<number, HTMLElement>;
    markdown: Map<number, HTMLElement>;
  }>({ html: new Map(), markdown: new Map() });
  const htmlSentenceAnchorCacheRef = useRef<{ key: string; map: number[] }>({ key: "", map: [] });
  const prettyAnchorLookupKeyRef = useRef<string>("");
  const prettyHighlightedNodeRef = useRef<HTMLElement | null>(null);
  const prettyLastAutoScrollAnchorRef = useRef<number | null>(null);
  const prettyLastAutoScrollPageRef = useRef<number | null>(null);
  const pendingScrollFrameRef = useRef<number | null>(null);

  useEffect(() => {
    setPageInput(String(reader.current_page + 1));
  }, [reader.current_page]);

  useEffect(() => {
    setSearchInput(reader.search_query);
  }, [reader.search_query]);

  const searchMatchSet = useMemo(() => new Set(reader.search_matches), [reader.search_matches]);

  const resolvePrettyAnchorIdx = useCallback(
    (idx: number): number | null => {
      if (reader.pretty_kind === "html") {
        const anchors = htmlSentenceAnchorCacheRef.current.map;
        let anchorIdx = anchors[idx] ?? null;
        if (anchorIdx === null || anchorIdx === undefined) {
          for (let offset = 1; offset < anchors.length; offset += 1) {
            const prev = idx - offset;
            const next = idx + offset;
            if (prev >= 0 && anchors[prev] !== null && anchors[prev] !== undefined) {
              anchorIdx = anchors[prev];
              break;
            }
            if (next < anchors.length && anchors[next] !== null && anchors[next] !== undefined) {
              anchorIdx = anchors[next];
              break;
            }
          }
        }
        if (anchorIdx !== null && anchorIdx !== undefined) {
          return anchorIdx;
        }
      }
      const anchors = reader.sentence_anchor_map;
      let anchorIdx = anchors[idx] ?? null;
      if (anchorIdx === null || anchorIdx === undefined) {
        for (let offset = 1; offset < anchors.length; offset += 1) {
          const prev = idx - offset;
          const next = idx + offset;
          if (prev >= 0 && anchors[prev] !== null && anchors[prev] !== undefined) {
            anchorIdx = anchors[prev];
            break;
          }
          if (next < anchors.length && anchors[next] !== null && anchors[next] !== undefined) {
            anchorIdx = anchors[next];
            break;
          }
        }
      }
      return anchorIdx ?? null;
    },
    [reader.pretty_kind, reader.sentence_anchor_map]
  );

  const getPrettyAnchorNode = useCallback(
    (anchorIdx: number): HTMLElement | null => {
      const key = reader.pretty_kind === "html" ? "html" : "markdown";
      return prettyAnchorElementsRef.current[key].get(anchorIdx) ?? null;
    },
    [reader.pretty_kind]
  );

  const scrollNativeHtmlAnchorIntoView = useCallback(
    (anchor: HTMLElement, behavior: ScrollBehavior): void => {
      anchor.scrollIntoView({
        behavior,
        block: reader.settings.center_spoken_sentence ? "center" : "nearest",
        inline: "nearest"
      });
    },
    [reader.settings.center_spoken_sentence]
  );

  const activePrettyAnchorIdx = useMemo(() => {
    const idx = reader.highlighted_sentence_idx;
    if (idx === null || idx === undefined) {
      return null;
    }
    return resolvePrettyAnchorIdx(idx);
  }, [reader.highlighted_sentence_idx, resolvePrettyAnchorIdx]);

  const alignHighlightedSentence = useCallback(
    (behavior: ScrollBehavior, force = false) => {
      const idx = reader.highlighted_sentence_idx;
      if (idx === null || idx === undefined) {
        return;
      }
      if (!force && !reader.settings.auto_scroll_tts) {
        return;
      }
      const container = sentenceScrollRef.current;
      if (!container) {
        return;
      }
      if (!reader.text_only_mode && reader.pretty_kind === "markdown" && reader.reading_markdown_page) {
        const anchorIdx = resolvePrettyAnchorIdx(idx);
        if (anchorIdx !== null && anchorIdx !== undefined) {
          if (
            !force &&
            prettyLastAutoScrollPageRef.current === reader.current_page &&
            prettyLastAutoScrollAnchorRef.current === anchorIdx
          ) {
            return;
          }
          const anchor = getPrettyAnchorNode(anchorIdx);
          if (anchor) {
            scrollSentenceIntoView(
              container,
              anchor,
              reader.settings.center_spoken_sentence,
              behavior
            );
            prettyLastAutoScrollPageRef.current = reader.current_page;
            prettyLastAutoScrollAnchorRef.current = anchorIdx;
            return;
          }
        }
      }
      if (!reader.text_only_mode && reader.pretty_kind === "html" && reader.reading_html_page) {
        const anchorIdx = resolvePrettyAnchorIdx(idx);
        if (anchorIdx !== null && anchorIdx !== undefined) {
          if (
            !force &&
            prettyLastAutoScrollPageRef.current === reader.current_page &&
            prettyLastAutoScrollAnchorRef.current === anchorIdx
          ) {
            return;
          }
          const anchor = getPrettyAnchorNode(anchorIdx);
          if (anchor) {
            scrollNativeHtmlAnchorIntoView(anchor, behavior);
            prettyLastAutoScrollPageRef.current = reader.current_page;
            prettyLastAutoScrollAnchorRef.current = anchorIdx;
            return;
          }
        }
      }
      const sentence = sentenceRefs.current[idx];
      if (!sentence) {
        return;
      }
      scrollSentenceIntoView(
        container,
        sentence,
        reader.settings.center_spoken_sentence,
        behavior
      );
    },
    [
      reader.highlighted_sentence_idx,
      reader.current_page,
      getPrettyAnchorNode,
      reader.settings.auto_scroll_tts,
      reader.settings.center_spoken_sentence,
      reader.pretty_kind,
      resolvePrettyAnchorIdx,
      scrollNativeHtmlAnchorIntoView,
      reader.reading_markdown_page,
      reader.reading_html_page,
      reader.sentences.length,
      reader.text_only_mode
    ]
  );

  const jumpToHighlightedSentence = useCallback(() => {
    alignHighlightedSentence("smooth", true);
  }, [alignHighlightedSentence]);

  const handlePrettyContentClick = useCallback((event: MouseEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement | null;
    const anchor = target?.closest("a");
    if (!anchor) {
      return;
    }
    const href = (anchor.getAttribute("href") ?? "").trim();
    if (!href.startsWith("#")) {
      return;
    }
    event.preventDefault();
    const container = sentenceScrollRef.current;
    if (!container) {
      return;
    }
    const id = href.slice(1);
    if (!id) {
      return;
    }
    const escapedId =
      typeof CSS !== "undefined" && typeof CSS.escape === "function" ? CSS.escape(id) : id;
    const element = container.querySelector(`#${escapedId}`) as HTMLElement | null;
    if (!element) {
      return;
    }
    scrollSentenceIntoView(container, element, false, "smooth");
  }, []);

  useEffect(() => {
    const idx = reader.highlighted_sentence_idx;
    if (idx === null || idx === undefined) {
      return;
    }
    if (!reader.settings.auto_scroll_tts) {
      return;
    }
    if (pendingScrollFrameRef.current !== null) {
      cancelAnimationFrame(pendingScrollFrameRef.current);
    }
    pendingScrollFrameRef.current = requestAnimationFrame(() => {
      pendingScrollFrameRef.current = null;
      const behavior: ScrollBehavior = reader.tts.state === "playing" ? "auto" : "smooth";
      alignHighlightedSentence(behavior);
    });
    return () => {
      if (pendingScrollFrameRef.current !== null) {
        cancelAnimationFrame(pendingScrollFrameRef.current);
        pendingScrollFrameRef.current = null;
      }
    };
  }, [
    alignHighlightedSentence,
    reader.current_page,
    reader.highlighted_sentence_idx,
    reader.settings.auto_scroll_tts,
    reader.settings.center_spoken_sentence,
    reader.tts.state,
    reader.settings.font_size,
    reader.settings.line_spacing,
    reader.settings.margin_horizontal,
    reader.settings.margin_vertical,
    reader.settings.word_spacing,
    reader.settings.letter_spacing
  ]);

  useEffect(() => {
    if (!reader.settings.auto_scroll_tts) {
      return;
    }
    if (!reader.text_only_mode && reader.pretty_kind === "html") {
      return;
    }
    const container = sentenceScrollRef.current;
    if (!container) {
      return;
    }

    const realign = () => {
      requestAnimationFrame(() => {
        alignHighlightedSentence("auto");
      });
    };

    const resizeObserver = new ResizeObserver(() => {
      realign();
    });
    resizeObserver.observe(container);
    window.addEventListener("resize", realign);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", realign);
    };
  }, [
    alignHighlightedSentence,
    reader.sentences.length,
    reader.settings.auto_scroll_tts,
    reader.pretty_kind,
    reader.text_only_mode,
    reader.settings.font_size,
    reader.settings.line_spacing,
    reader.settings.margin_horizontal,
    reader.settings.margin_vertical,
    reader.settings.word_spacing,
    reader.settings.letter_spacing
  ]);

  const applyPrettyHighlight = useCallback((): boolean => {
    if (reader.text_only_mode) {
      if (prettyHighlightedNodeRef.current) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
        prettyHighlightedNodeRef.current = null;
      }
      return false;
    }
    if (activePrettyAnchorIdx === null || activePrettyAnchorIdx === undefined) {
      if (prettyHighlightedNodeRef.current) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
        prettyHighlightedNodeRef.current = null;
      }
      return false;
    }
    const target = getPrettyAnchorNode(activePrettyAnchorIdx);
    if (target) {
      if (prettyHighlightedNodeRef.current && prettyHighlightedNodeRef.current !== target) {
        prettyHighlightedNodeRef.current.classList.remove("reader-pretty-highlight");
      }
      target.classList.add("reader-pretty-highlight");
      prettyHighlightedNodeRef.current = target;
      return true;
    }
    return false;
  }, [
    activePrettyAnchorIdx,
    getPrettyAnchorNode,
    reader.text_only_mode,
  ]);

  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      applyPrettyHighlight();
    });
    return () => cancelAnimationFrame(frame);
  }, [applyPrettyHighlight, activePrettyAnchorIdx, reader.current_page]);

  const panelTitle = useMemo(() => {
    if (reader.panels.show_settings) {
      return "Settings";
    }
    if (reader.panels.show_stats) {
      return "Stats";
    }
    if (reader.panels.show_tts) {
      return "TTS Options";
    }
    return null;
  }, [reader.panels.show_settings, reader.panels.show_stats, reader.panels.show_tts]);

  const readerTypography = useMemo(
    () => computeReaderTypographyLayout(reader.settings),
    [reader.settings]
  );
  const imageCandidatesKey = useMemo(
    () => reader.images.map((image) => `${image.raw_path}\t${image.local_path}`).join("\n"),
    [reader.images]
  );
  const readerImageCandidates = useMemo(
    () =>
      reader.images.map((image) => ({
        rawPath: image.raw_path,
        src: toReaderImageSrc(image.local_path)
      })),
    [reader.images]
  );
  const estimatedTotalWords = useMemo(() => {
    if (reader.stats.page_end_percent <= 0) {
      return reader.stats.words_read_up_to_page_end;
    }
    return Math.max(
      reader.stats.words_read_up_to_page_end,
      Math.round((reader.stats.words_read_up_to_page_end * 100) / reader.stats.page_end_percent)
    );
  }, [reader.stats.page_end_percent, reader.stats.words_read_up_to_page_end]);
  const estimatedTotalSentences = useMemo(() => {
    if (reader.stats.page_end_percent <= 0) {
      return reader.stats.sentences_read_up_to_page_end;
    }
    return Math.max(
      reader.stats.sentences_read_up_to_page_end,
      Math.round(
        (reader.stats.sentences_read_up_to_page_end * 100) / reader.stats.page_end_percent
      )
    );
  }, [reader.stats.page_end_percent, reader.stats.sentences_read_up_to_page_end]);
  const wordsReadOnPage = useMemo(
    () =>
      Math.max(
        0,
        reader.stats.words_read_up_to_current_position - reader.stats.words_read_up_to_page_start
      ),
    [reader.stats.words_read_up_to_current_position, reader.stats.words_read_up_to_page_start]
  );
  const sentencesReadOnPage = useMemo(
    () =>
      Math.max(
        0,
        reader.stats.sentences_read_up_to_current_position -
          reader.stats.sentences_read_up_to_page_start
      ),
    [
      reader.stats.sentences_read_up_to_current_position,
      reader.stats.sentences_read_up_to_page_start
    ]
  );
  const pageFinishedPct = useMemo(() => {
    if (reader.stats.page_word_count <= 0) {
      return 0;
    }
    return clamp((wordsReadOnPage / reader.stats.page_word_count) * 100, 0, 100);
  }, [reader.stats.page_word_count, wordsReadOnPage]);
  const sessionSecondsInApp = Math.floor((sessionNowMs - sessionStartMsRef.current) / 1000);
  const sessionSecondsListening = Math.floor(
    (listeningAccumulatedMsRef.current +
      (reader.tts.state === "playing" && listeningStartedAtMsRef.current !== null
        ? sessionNowMs - listeningStartedAtMsRef.current
        : 0)) /
      1000
  );
  const estimatedReadPages = useMemo(
    () =>
      Math.min(
        reader.stats.total_pages,
        Math.max(
          reader.stats.page_index,
          Math.floor((reader.stats.global_progress_pct / 100) * reader.stats.total_pages)
        )
      ),
    [reader.stats.global_progress_pct, reader.stats.page_index, reader.stats.total_pages]
  );
  const sessionGlobalPercentFinished = useMemo(() => {
    if (estimatedTotalWords <= 0) {
      return 0;
    }
    return clamp((sessionWordsRead / estimatedTotalWords) * 100, 0, 100);
  }, [estimatedTotalWords, sessionWordsRead]);
  const listeningMinutes = useMemo(
    () => sessionSecondsListening / 60,
    [sessionSecondsListening]
  );
  const sessionWordsPerMinute = useMemo(
    () => (listeningMinutes > 0 ? sessionWordsRead / listeningMinutes : 0),
    [listeningMinutes, sessionWordsRead]
  );
  const sessionSentencesPerMinute = useMemo(
    () => (listeningMinutes > 0 ? sessionSentencesRead / listeningMinutes : 0),
    [listeningMinutes, sessionSentencesRead]
  );
  const sessionPercentPerMinute = useMemo(
    () => (listeningMinutes > 0 ? sessionGlobalPercentFinished / listeningMinutes : 0),
    [listeningMinutes, sessionGlobalPercentFinished]
  );

  const hasHighlightSentence = reader.highlighted_sentence_idx !== null;
  const textModeLabel = reader.text_only_mode ? "Pretty Text" : "Text-only";
  const isPrettyTextMode = !reader.text_only_mode;
  const hasPrettyMarkdown =
    isPrettyTextMode && reader.pretty_kind === "markdown" && Boolean(reader.reading_markdown_page);
  const hasPrettyHtml =
    isPrettyTextMode && reader.pretty_kind === "html" && Boolean(reader.reading_html_page);
  const isBrowserTabPrettyHtml =
    hasPrettyHtml && reader.source_path.toLowerCase().endsWith(".lltab");
  const prettyUnavailable = isPrettyTextMode && !hasPrettyMarkdown && !hasPrettyHtml;
  const showSentenceList = reader.text_only_mode || prettyUnavailable;
  const ttsSentenceLabel = useMemo(
    () =>
      `Sentence ${reader.tts.current_sentence_idx !== null ? reader.tts.current_sentence_idx + 1 : 0}/${Math.max(1, reader.tts.sentence_count)}`,
    [reader.tts.current_sentence_idx, reader.tts.sentence_count]
  );
  const ttsProgressLabel = useMemo(
    () => `Progress ${reader.tts.progress_pct.toFixed(3)}% | ${reader.tts.state}`,
    [reader.tts.progress_pct, reader.tts.state]
  );
  const renderedMarkdownHtml = useMemo(() => {
    if (!hasPrettyMarkdown || !reader.reading_markdown_page) {
      return "";
    }
    return renderMarkdownToHtml(reader.reading_markdown_page, readerImageCandidates);
  }, [hasPrettyMarkdown, reader.reading_markdown_page, readerImageCandidates]);
  const renderedNativeHtml = useMemo(() => {
    if (!hasPrettyHtml || !reader.reading_html_page) {
      return "";
    }
    const key = `${reader.source_path}\n${imageCandidatesKey}\n${reader.reading_html_page}`;
    if (nativeHtmlCacheRef.current.key === key) {
      return nativeHtmlCacheRef.current.html;
    }
    const rendered = renderNativePrettyHtml(reader.reading_html_page, readerImageCandidates);
    nativeHtmlCacheRef.current = { key, html: rendered };
    return rendered;
  }, [
    hasPrettyHtml,
    imageCandidatesKey,
    reader.reading_html_page,
    reader.source_path,
    readerImageCandidates
  ]);
  const sentencesKey = useMemo(() => reader.sentences.join("\n"), [reader.sentences]);
  const sentenceAnchorHintKey = useMemo(
    () => reader.sentence_anchor_map.map((value) => (value ?? "null")).join(","),
    [reader.sentence_anchor_map]
  );
  const prettyLookupKey = useMemo(() => {
    if (hasPrettyHtml) {
      return `html:${reader.source_path}:${reader.current_page}:${renderedNativeHtml}`;
    }
    if (hasPrettyMarkdown) {
      return `markdown:${reader.source_path}:${reader.current_page}:${renderedMarkdownHtml}`;
    }
    return "";
  }, [
    hasPrettyHtml,
    hasPrettyMarkdown,
    reader.current_page,
    reader.source_path,
    renderedMarkdownHtml,
    renderedNativeHtml
  ]);

  useEffect(() => {
    if (!prettyLookupKey) {
      prettyAnchorElementsRef.current.html.clear();
      prettyAnchorElementsRef.current.markdown.clear();
      prettyAnchorLookupKeyRef.current = "";
      return;
    }
    if (prettyAnchorLookupKeyRef.current === prettyLookupKey) {
      return;
    }
    const kind = hasPrettyHtml ? "html" : hasPrettyMarkdown ? "markdown" : null;
    if (!kind) {
      return;
    }
    const attribute = kind === "html" ? "data-ll-html-anchor" : "data-ll-md-anchor";
    const root =
      kind === "html"
        ? nativeHtmlFrameRef.current?.contentDocument
        : sentenceScrollRef.current;
    if (!root) {
      return;
    }
    const anchors = Array.from(root.querySelectorAll(`[${attribute}]`)) as HTMLElement[];
    const nextMap = new Map<number, HTMLElement>();
    for (const element of anchors) {
      const raw = element.getAttribute(attribute);
      const parsed = raw === null ? Number.NaN : Number.parseInt(raw, 10);
      if (Number.isFinite(parsed)) {
        nextMap.set(parsed, element);
      }
    }
    prettyAnchorElementsRef.current[kind] = nextMap;
    if (kind === "html") {
      prettyAnchorElementsRef.current.markdown.clear();
    } else {
      prettyAnchorElementsRef.current.html.clear();
    }
    prettyAnchorLookupKeyRef.current = prettyLookupKey;
  }, [hasPrettyHtml, hasPrettyMarkdown, prettyLookupKey]);

  useEffect(() => {
    if (!hasPrettyHtml) {
      return;
    }
    const frame = nativeHtmlFrameRef.current;
    const doc = frame?.contentDocument;
    if (!frame || !doc) {
      return;
    }

    const rebuildHtmlAnchors = (): void => {
      const anchors = Array.from(doc.querySelectorAll("[data-ll-html-anchor]")) as HTMLElement[];
      const nextMap = new Map<number, HTMLElement>();
      for (const element of anchors) {
        const raw = element.getAttribute("data-ll-html-anchor");
        const parsed = raw === null ? Number.NaN : Number.parseInt(raw, 10);
        if (Number.isFinite(parsed)) {
          nextMap.set(parsed, element);
        }
      }
      prettyAnchorElementsRef.current.html = nextMap;
      prettyAnchorLookupKeyRef.current = prettyLookupKey;
    };

    const handleLoad = (): void => {
      rebuildHtmlAnchors();
    };

    const handleClick = (event: globalThis.MouseEvent): void => {
      const target = event.target as HTMLElement | null;
      const anchor = target?.closest("a");
      if (!anchor) {
        return;
      }
      const href = (anchor.getAttribute("href") ?? "").trim();
      if (!href || href.startsWith("#")) {
        return;
      }
      event.preventDefault();
      window.open(href, "_blank", "noopener,noreferrer");
    };

    frame.addEventListener("load", handleLoad);
    doc.addEventListener("click", handleClick);
    rebuildHtmlAnchors();

    return () => {
      frame.removeEventListener("load", handleLoad);
      doc.removeEventListener("click", handleClick);
    };
  }, [hasPrettyHtml, prettyLookupKey]);

  useEffect(() => {
    if (!hasPrettyHtml || !renderedNativeHtml) {
      htmlSentenceAnchorCacheRef.current = { key: "", map: [] };
      return;
    }
    const cacheKey = [
      reader.source_path,
      reader.current_page,
      renderedNativeHtml,
      sentencesKey,
      sentenceAnchorHintKey
    ].join("\n");
    if (htmlSentenceAnchorCacheRef.current.key === cacheKey) {
      return;
    }
    const anchors = Array.from(prettyAnchorElementsRef.current.html.values());
    if (anchors.length === 0 || reader.sentences.length === 0) {
      htmlSentenceAnchorCacheRef.current = { key: cacheKey, map: [] };
      return;
    }
    const startedAt = typeof performance !== "undefined" ? performance.now() : 0;
    const anchorTexts = anchors.map((element) => element.textContent ?? "");
    const { map, diagnostics } = buildHtmlSentenceAnchorMap(
      anchorTexts,
      reader.sentences,
      reader.sentence_anchor_map
    );
    if (import.meta.env.DEV) {
      console.debug("reader pretty html sync map", {
        sentences: reader.sentences.length,
        anchors: anchors.length,
        confidentMatches: diagnostics.confidentMatches,
        fallbackMatches: diagnostics.fallbackMatches,
        cappedLeaps: diagnostics.cappedLeaps
      });
    }
    recordPerfMeasure("ReaderShell.buildHtmlSentenceAnchorMap", startedAt);
    htmlSentenceAnchorCacheRef.current = { key: cacheKey, map };
  }, [
    hasPrettyHtml,
    reader.current_page,
    reader.sentences,
    reader.source_path,
    reader.sentence_anchor_map,
    renderedNativeHtml,
    sentenceAnchorHintKey,
    sentencesKey
  ]);
  const themeLabel = reader.settings.theme === "night" ? "Day" : "Night";
  const themeIcon =
    reader.settings.theme === "night" ? <LightModeOutlinedIcon /> : <DarkModeOutlinedIcon />;

  useEffect(() => {
    if (!reader.panels.show_stats) {
      return;
    }
    const tick = window.setInterval(() => {
      setSessionNowMs(Date.now());
    }, 1000);
    return () => window.clearInterval(tick);
  }, [reader.panels.show_stats]);

  useEffect(() => {
    const now = Date.now();
    if (reader.tts.state === "playing") {
      if (listeningStartedAtMsRef.current === null) {
        listeningStartedAtMsRef.current = now;
      }
    } else if (listeningStartedAtMsRef.current !== null) {
      listeningAccumulatedMsRef.current += now - listeningStartedAtMsRef.current;
      listeningStartedAtMsRef.current = null;
    }
  }, [reader.tts.state]);

  useEffect(() => {
    sessionStartMsRef.current = Date.now();
    listeningAccumulatedMsRef.current = 0;
    listeningStartedAtMsRef.current = reader.tts.state === "playing" ? Date.now() : null;
    sessionBaselineWordsRef.current = reader.stats.words_read_up_to_current_position;
    sessionMaxWordsRef.current = reader.stats.words_read_up_to_current_position;
    sessionBaselineSentencesRef.current = reader.stats.sentences_read_up_to_current_position;
    sessionMaxSentencesRef.current = reader.stats.sentences_read_up_to_current_position;
    sessionFinishedPagesRef.current = new Set();
    setSessionWordsRead(0);
    setSessionSentencesRead(0);
    setSessionPagesFinished(0);
    setSessionNowMs(Date.now());
    setStatsTab("page");
  }, [reader.source_path]);

  useEffect(() => {
    sessionMaxWordsRef.current = Math.max(
      sessionMaxWordsRef.current,
      reader.stats.words_read_up_to_current_position
    );
    setSessionWordsRead(
      Math.max(0, sessionMaxWordsRef.current - sessionBaselineWordsRef.current)
    );
    sessionMaxSentencesRef.current = Math.max(
      sessionMaxSentencesRef.current,
      reader.stats.sentences_read_up_to_current_position
    );
    setSessionSentencesRead(
      Math.max(0, sessionMaxSentencesRef.current - sessionBaselineSentencesRef.current)
    );
    if (pageFinishedPct >= 99.9) {
      sessionFinishedPagesRef.current.add(reader.current_page);
      setSessionPagesFinished(sessionFinishedPagesRef.current.size);
    }
  }, [
    pageFinishedPct,
    reader.current_page,
    reader.stats.sentences_read_up_to_current_position,
    reader.stats.words_read_up_to_current_position
  ]);

  return (
    <Card className="w-full max-w-[1700px] min-h-0 rounded-3xl border border-slate-200 shadow-sm lg:h-full">
      <CardContent className="h-full p-4 md:p-6" sx={{ position: "relative" }}>
        <Stack spacing={2} sx={{ height: "100%", minHeight: 0 }}>
          <Stack
            direction="row"
            alignItems="center"
            spacing={1}
            data-testid="reader-topbar"
            sx={{
              flexWrap: "nowrap",
              overflowX: "hidden",
              overflowY: "visible",
              whiteSpace: "nowrap",
              minHeight: 52,
              paddingTop: 0.5,
              paddingRight: 0.5,
              flexShrink: 0
            }}
          >
            <Button
              variant="outlined"
              startIcon={<ArrowBackIcon />}
              onClick={() => void onCloseSession()}
              disabled={busy}
              data-testid="reader-close-session-button"
              sx={{ flexShrink: 0 }}
            >
              Close Session
            </Button>
            <Divider flexItem orientation="vertical" />
            <Button
              variant="outlined"
              startIcon={<ChevronLeftIcon />}
              onClick={() => void onPrevPage()}
              disabled={busy || reader.current_page === 0}
              data-testid="reader-prev-page-button"
              sx={{ flexShrink: 0 }}
            >
              Prev Page
            </Button>
            <Button
              variant="outlined"
              endIcon={<ChevronRightIcon />}
              onClick={() => void onNextPage()}
              disabled={busy || reader.current_page + 1 >= reader.total_pages}
              data-testid="reader-next-page-button"
              sx={{ flexShrink: 0 }}
            >
              Next Page
            </Button>
            <Button
              variant="outlined"
              onClick={() => void onPrevSentence()}
              disabled={busy}
              data-testid="reader-prev-sentence-button"
              sx={{ flexShrink: 0 }}
            >
              Prev Sentence
            </Button>
            <Button
              variant="outlined"
              onClick={() => void onNextSentence()}
              disabled={busy}
              data-testid="reader-next-sentence-button"
              sx={{ flexShrink: 0 }}
            >
              Next Sentence
            </Button>
            <Button
              variant="outlined"
              startIcon={<GpsFixedIcon />}
              onClick={() => jumpToHighlightedSentence()}
              disabled={!hasHighlightSentence}
              data-testid="reader-jump-highlight-button"
              sx={{ flexShrink: 0 }}
            >
              Jump to Highlight
            </Button>
            <TextField
              size="small"
              value={pageInput}
              onChange={(event) => setPageInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  const parsed = Number(pageInput);
                  if (Number.isFinite(parsed)) {
                    const page = Math.max(1, Math.min(reader.total_pages, Math.floor(parsed)));
                    void onSetPage(page - 1);
                  }
                }
              }}
              sx={{ width: 92, flexShrink: 0 }}
              label="Page"
            />
            <Button
              variant="outlined"
              startIcon={themeIcon}
              onClick={() => void onToggleTheme()}
              disabled={busy}
              data-testid="reader-topbar-theme-toggle-button"
              sx={{ flexShrink: 0 }}
            >
              {themeLabel}
            </Button>
          </Stack>

          <Stack direction="row" spacing={1} alignItems="center" sx={{ flexShrink: 0 }}>
            <SearchIcon fontSize="small" />
            <TextField
              size="small"
              fullWidth
              label="Search (regex supported)"
              value={searchInput}
              data-testid="reader-search-input"
              inputProps={{ "data-reader-search-input": "1" }}
              onChange={(event) => setSearchInput(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  void onSearchQuery(searchInput);
                }
              }}
            />
            <Button
              variant="outlined"
              onClick={() => void onSearchQuery(searchInput)}
              data-testid="reader-search-apply-button"
            >
              Apply
            </Button>
            <Button
              variant="outlined"
              onClick={() => void onSearchPrev()}
              data-testid="reader-search-prev-button"
            >
              Prev
            </Button>
            <Button
              variant="outlined"
              onClick={() => void onSearchNext()}
              data-testid="reader-search-next-button"
            >
              Next
            </Button>
          </Stack>

          <Stack direction={{ xs: "column", lg: "row" }} spacing={2} sx={{ flex: 1, minHeight: 0 }}>
            <div className="min-h-0 flex flex-1 flex-col overflow-hidden rounded-2xl border border-slate-200">
              <div
                ref={sentenceScrollRef}
                className={
                  hasPrettyHtml && !hasPrettyMarkdown && !showSentenceList
                    ? "overflow-hidden overscroll-contain"
                    : "overflow-y-auto overscroll-contain"
                }
                data-testid="reader-sentence-scroll-container"
                style={{
                  height: reader.panels.show_tts ? "calc(100% - 118px)" : "100%",
                  paddingLeft: `${readerTypography.horizontalMarginPx}px`,
                  paddingRight: `${readerTypography.horizontalMarginPx}px`,
                  paddingTop: `${readerTypography.verticalMarginPx}px`,
                  paddingBottom: `${readerTypography.verticalMarginPx}px`,
                  scrollbarGutter: "stable",
                  background: isPrettyTextMode
                    ? "linear-gradient(180deg, rgba(255,255,255,0.7) 0%, rgba(248,250,252,0.8) 100%)"
                    : "transparent"
                }}
              >
                <Stack
                  spacing={0.75}
                  sx={{
                    minHeight: hasPrettyHtml && !hasPrettyMarkdown && !showSentenceList ? "100%" : undefined,
                    height: hasPrettyHtml && !hasPrettyMarkdown && !showSentenceList ? "100%" : undefined,
                  }}
                >
                  {hasPrettyHtml ? (
                    <div
                      style={{
                        width: "100%",
                        flex: showSentenceList || hasPrettyMarkdown ? undefined : 1,
                        minHeight: showSentenceList || hasPrettyMarkdown ? undefined : 0,
                      }}
                    >
                      <iframe
                        ref={nativeHtmlFrameRef}
                        className="reader-native-html-frame"
                        data-testid="reader-pretty-native-html"
                        data-reader-browser-tab={isBrowserTabPrettyHtml ? "1" : "0"}
                        sandbox="allow-same-origin allow-popups allow-popups-to-escape-sandbox"
                        srcDoc={renderedNativeHtml}
                        title={`Native HTML reader page ${reader.current_page + 1}`}
                      />
                    </div>
                  ) : null}
                  {hasPrettyMarkdown ? (
                    <div
                      style={{
                        maxWidth: "72ch",
                        marginInline: "auto",
                        padding: "10px 12px",
                        border: "1px solid rgba(148, 163, 184, 0.36)",
                        borderRadius: 12,
                        background: "rgba(255, 255, 255, 0.82)",
                        boxShadow: "0 1px 2px rgba(15, 23, 42, 0.06)",
                        color: "#1f2937"
                      }}
                    >
                      <div
                        className="reader-markdown-content"
                        data-testid="reader-pretty-markdown"
                        onClick={handlePrettyContentClick}
                        dangerouslySetInnerHTML={{ __html: renderedMarkdownHtml }}
                      />
                    </div>
                  ) : null}
                  {prettyUnavailable ? (
                    <Typography
                      variant="caption"
                      color="text.secondary"
                      data-testid="reader-pretty-markdown-fallback"
                    >
                      Pretty view unavailable for this source. Showing text fallback.
                    </Typography>
                  ) : null}
                  {showSentenceList
                    ? reader.sentences.map((sentence, idx) => {
                    const highlighted = reader.highlighted_sentence_idx === idx;
                    const searchMatch = searchMatchSet.has(idx);
                    const baseBorderColor = isPrettyTextMode
                      ? "rgba(148, 163, 184, 0.36)"
                      : "transparent";
                    const baseBackground = isPrettyTextMode
                      ? "rgba(255, 255, 255, 0.78)"
                      : "transparent";
                    return (
                      <button
                        key={`${reader.current_page}:${idx}`}
                        ref={(element) => {
                          sentenceRefs.current[idx] = element;
                        }}
                        type="button"
                        onClick={() => void onSentenceClick(idx)}
                        className="w-full rounded-lg border px-3 py-1.5 text-left transition-colors"
                        data-testid={`reader-sentence-${idx}`}
                        data-highlighted={highlighted ? "1" : "0"}
                        style={{
                          fontSize: `${readerTypography.fontSizePx}px`,
                          lineHeight: isPrettyTextMode
                            ? Math.max(readerTypography.lineSpacing, 1.55)
                            : readerTypography.lineSpacing,
                          wordSpacing: `${readerTypography.wordSpacingPx}px`,
                          letterSpacing: `${readerTypography.letterSpacingPx}px`,
                          borderColor: highlighted
                            ? "var(--reader-highlight-border)"
                            : searchMatch
                              ? "var(--reader-search-border)"
                              : baseBorderColor,
                          background: highlighted
                            ? "var(--reader-highlight-bg)"
                            : searchMatch
                              ? "var(--reader-search-bg)"
                              : baseBackground,
                          maxWidth: isPrettyTextMode ? "72ch" : "100%",
                          marginInline: isPrettyTextMode ? "auto" : undefined,
                          boxShadow: isPrettyTextMode
                            ? "0 1px 2px rgba(15, 23, 42, 0.06)"
                            : "none",
                          borderRadius: isPrettyTextMode ? 12 : 8,
                          color: isPrettyTextMode ? "#1f2937" : undefined
                        }}
                      >
                        {sentence}
                      </button>
                    );
                  })
                    : null}
                </Stack>
              </div>
              <TtsPlayerWidget
                visible={reader.panels.show_tts}
                busy={busy}
                isPlaying={reader.tts.state === "playing"}
                canPrevPage={reader.current_page > 0}
                canNextPage={reader.current_page + 1 < reader.total_pages}
                canPrevSentence={reader.tts.can_seek_prev}
                canNextSentence={reader.tts.can_seek_next}
                currentSentenceLabel={ttsSentenceLabel}
                progressLabel={ttsProgressLabel}
                onPrevPage={onPrevPage}
                onPrevSentence={onTtsSeekPrev}
                onTogglePlayPause={onTtsTogglePlayPause}
                onNextSentence={onTtsSeekNext}
                onNextPage={onNextPage}
              />
            </div>

            {panelTitle ? (
              <div className="w-full min-h-0 shrink-0 rounded-2xl border border-slate-200 p-3 lg:h-full lg:w-[380px]">
                <Stack spacing={1.25} sx={{ height: "100%", minHeight: 0 }}>
                  <Typography variant="subtitle1" fontWeight={700} sx={{ flexShrink: 0 }}>
                    <span data-testid="reader-panel-title">{panelTitle}</span>
                  </Typography>
                  <Divider sx={{ flexShrink: 0 }} />

                  <div
                    style={{
                      overflowY: "auto",
                      minHeight: 0,
                      overscrollBehavior: "contain",
                      paddingTop: 6,
                      paddingRight: 8,
                      scrollbarGutter: "stable"
                    }}
                  >
                    {reader.panels.show_settings ? (
                      <Stack spacing={1.5}>
                        <FormControl size="small">
                          <InputLabel id="setting-font-family-label">Font Family</InputLabel>
                          <Select
                            labelId="setting-font-family-label"
                            label="Font Family"
                            value={reader.settings.font_family}
                            onChange={(event) =>
                              void onApplySettings({
                                font_family: event.target.value as FontFamily
                              })
                            }
                            data-testid="setting-font-family"
                          >
                            {FONT_FAMILY_OPTIONS.map((option) => (
                              <MenuItem key={option.value} value={option.value}>
                                {option.label}
                              </MenuItem>
                            ))}
                          </Select>
                        </FormControl>

                        <FormControl size="small">
                          <InputLabel id="setting-font-weight-label">Font Weight</InputLabel>
                          <Select
                            labelId="setting-font-weight-label"
                            label="Font Weight"
                            value={reader.settings.font_weight}
                            onChange={(event) =>
                              void onApplySettings({
                                font_weight: event.target.value as FontWeight
                              })
                            }
                            data-testid="setting-font-weight"
                          >
                            {FONT_WEIGHT_OPTIONS.map((option) => (
                              <MenuItem key={option.value} value={option.value}>
                                {option.label}
                              </MenuItem>
                            ))}
                          </Select>
                        </FormControl>
                        <FormControl size="small">
                          <InputLabel id="setting-theme-label">Theme</InputLabel>
                          <Select
                            labelId="setting-theme-label"
                            label="Theme"
                            value={reader.settings.theme}
                            onChange={(event) =>
                              void onApplySettings({
                                theme: event.target.value as ThemeMode
                              })
                            }
                            data-testid="setting-theme"
                          >
                            <MenuItem value="day">Day</MenuItem>
                            <MenuItem value="night">Night</MenuItem>
                          </Select>
                        </FormControl>

                        <Stack spacing={1}>
                          <Typography variant="caption" fontWeight={700}>
                            Day Highlight
                          </Typography>
                          <Stack direction="row" spacing={1} alignItems="center">
                            <TextField
                              type="color"
                              size="small"
                              value={toHexColor(reader.settings.day_highlight)}
                              onChange={(event) =>
                                void onApplySettings({
                                  day_highlight: withHexColor(
                                    reader.settings.day_highlight,
                                    event.target.value
                                  )
                                })
                              }
                              inputProps={{ "data-testid": "setting-day-highlight-color" }}
                              sx={{ width: 76 }}
                            />
                            <NumericSettingControl
                              label="Day Highlight Alpha"
                              testId="setting-day-highlight-alpha"
                              value={reader.settings.day_highlight.a}
                              min={0}
                              max={1}
                              step={0.01}
                              decimals={2}
                              onCommit={async (next) => {
                                await onApplySettings({
                                  day_highlight: withAlpha(reader.settings.day_highlight, next)
                                });
                              }}
                            />
                          </Stack>
                        </Stack>

                        <Stack spacing={1}>
                          <Typography variant="caption" fontWeight={700}>
                            Night Highlight
                          </Typography>
                          <Stack direction="row" spacing={1} alignItems="center">
                            <TextField
                              type="color"
                              size="small"
                              value={toHexColor(reader.settings.night_highlight)}
                              onChange={(event) =>
                                void onApplySettings({
                                  night_highlight: withHexColor(
                                    reader.settings.night_highlight,
                                    event.target.value
                                  )
                                })
                              }
                              inputProps={{ "data-testid": "setting-night-highlight-color" }}
                              sx={{ width: 76 }}
                            />
                            <NumericSettingControl
                              label="Night Highlight Alpha"
                              testId="setting-night-highlight-alpha"
                              value={reader.settings.night_highlight.a}
                              min={0}
                              max={1}
                              step={0.01}
                              decimals={2}
                              onCommit={async (next) => {
                                await onApplySettings({
                                  night_highlight: withAlpha(reader.settings.night_highlight, next)
                                });
                              }}
                            />
                          </Stack>
                        </Stack>

                        <NumericSettingControl
                          label="Font Size"
                          testId="setting-font-size"
                          value={reader.settings.font_size}
                          min={12}
                          max={36}
                          step={1}
                          decimals={0}
                          onCommit={async (next) => {
                            await onApplySettings({ font_size: Math.round(next) });
                          }}
                        />
                        <NumericSettingControl
                          label="Lines Per Page"
                          testId="setting-lines-per-page"
                          value={reader.settings.lines_per_page}
                          min={8}
                          max={1000}
                          step={1}
                          decimals={0}
                          onCommit={async (next) => {
                            await onApplySettings({ lines_per_page: Math.round(next) });
                          }}
                        />
                        <NumericSettingControl
                          label="Horizontal Margin"
                          testId="setting-horizontal-margin"
                          value={reader.settings.margin_horizontal}
                          min={0}
                          max={600}
                          step={1}
                          decimals={0}
                          onCommit={async (next) => {
                            await onApplySettings({ margin_horizontal: Math.round(next) });
                          }}
                        />
                        <NumericSettingControl
                          label="Vertical Margin"
                          testId="setting-vertical-margin"
                          value={reader.settings.margin_vertical}
                          min={0}
                          max={240}
                          step={1}
                          decimals={0}
                          onCommit={async (next) => {
                            await onApplySettings({ margin_vertical: Math.round(next) });
                          }}
                        />
                        <NumericSettingControl
                          label="Line Spacing"
                          testId="setting-line-spacing"
                          value={reader.settings.line_spacing}
                          min={0.8}
                          max={3}
                          step={0.05}
                          decimals={2}
                          onCommit={async (next) => {
                            await onApplySettings({ line_spacing: next });
                          }}
                        />
                        <NumericSettingControl
                          label="Word Spacing"
                          testId="setting-word-spacing"
                          value={reader.settings.word_spacing}
                          min={0}
                          max={24}
                          step={1}
                          decimals={0}
                          onCommit={async (next) => {
                            await onApplySettings({ word_spacing: Math.round(next) });
                          }}
                        />
                        <NumericSettingControl
                          label="Letter Spacing"
                          testId="setting-letter-spacing"
                          value={reader.settings.letter_spacing}
                          min={0}
                          max={24}
                          step={1}
                          decimals={0}
                          onCommit={async (next) => {
                            await onApplySettings({ letter_spacing: Math.round(next) });
                          }}
                        />

                        <Stack direction="row" alignItems="center" justifyContent="space-between">
                          <Typography variant="caption" fontWeight={700}>
                            Auto Scroll
                          </Typography>
                          <Switch
                            checked={reader.settings.auto_scroll_tts}
                            onChange={(event) =>
                              void onApplySettings({ auto_scroll_tts: event.target.checked })
                            }
                          />
                        </Stack>
                        <Stack direction="row" alignItems="center" justifyContent="space-between">
                          <Typography variant="caption" fontWeight={700}>
                            Auto Center
                          </Typography>
                          <Switch
                            checked={reader.settings.center_spoken_sentence}
                            onChange={(event) =>
                              void onApplySettings({
                                center_spoken_sentence: event.target.checked
                              })
                            }
                          />
                        </Stack>
                      </Stack>
                    ) : null}

                    {reader.panels.show_stats ? (
                      <Stack spacing={1.2}>
                        <Tabs
                          value={statsTab}
                          onChange={(_, value: "page" | "global" | "session") =>
                            setStatsTab(value)
                          }
                          variant="scrollable"
                          allowScrollButtonsMobile
                          sx={{ minHeight: 32 }}
                        >
                          <Tab label="Current Page Stats" value="page" />
                          <Tab label="Global Stats" value="global" />
                          <Tab label="Current Session Stats" value="session" />
                        </Tabs>
                        {statsTab === "page" ? (
                          <Stack spacing={0.8}>
                            <Typography variant="body2">
                              Page index: {reader.stats.page_index} / {reader.stats.total_pages}
                            </Typography>
                            <Typography variant="body2">
                              Words on page: {reader.stats.page_word_count}
                            </Typography>
                            <Typography variant="body2">
                              Sentences on page: {reader.stats.page_sentence_count}
                            </Typography>
                            <Typography variant="body2">
                              Percent at start of page: {formatPercent(reader.stats.page_start_percent)}
                            </Typography>
                            <Typography variant="body2">
                              Percent at end of page: {formatPercent(reader.stats.page_end_percent)}
                            </Typography>
                            <Divider />
                            <Typography variant="body2" fontWeight={700}>
                              Page Progress
                            </Typography>
                            <Typography variant="body2">
                              TTS progress (page): {reader.stats.tts_progress_pct.toFixed(3)}%
                            </Typography>
                            <Typography variant="body2">
                              Page time remaining:{" "}
                              {formatRemainingTime(
                                reader.stats.page_time_remaining_secs,
                                reader.settings.time_remaining_display
                              )}
                            </Typography>
                            <Typography variant="body2">Words read on page: {wordsReadOnPage}</Typography>
                            <Typography variant="body2">
                              Sentences read on page: {sentencesReadOnPage}
                            </Typography>
                          </Stack>
                        ) : null}
                        {statsTab === "global" ? (
                          <Stack spacing={0.8}>
                            <Typography variant="body2" fontWeight={700}>
                              Global Stats
                            </Typography>
                            <Typography variant="body2">Total page count: {reader.stats.total_pages}</Typography>
                            <Typography variant="body2">Total words in book: {estimatedTotalWords}</Typography>
                            <Typography variant="body2">
                              Total sentences in book: {estimatedTotalSentences}
                            </Typography>
                            <Divider />
                            <Typography variant="body2" fontWeight={700}>
                              Global Progress
                            </Typography>
                            <Typography variant="body2">Total read pages: {estimatedReadPages}</Typography>
                            <Typography variant="body2">
                              Total read words: {reader.stats.words_read_up_to_current_position}
                            </Typography>
                            <Typography variant="body2">
                              Total read sentences: {reader.stats.sentences_read_up_to_current_position}
                            </Typography>
                            <Typography variant="body2">
                              TTS global progress: {reader.stats.global_progress_pct.toFixed(3)}%
                            </Typography>
                            <Typography variant="body2">
                              Book time remaining:{" "}
                              {formatRemainingTime(
                                reader.stats.book_time_remaining_secs,
                                reader.settings.time_remaining_display
                              )}
                            </Typography>
                          </Stack>
                        ) : null}
                        {statsTab === "session" ? (
                          <Stack spacing={0.8}>
                            <Typography variant="body2">
                              Time spent in app: {formatSeconds(sessionSecondsInApp)}
                            </Typography>
                            <Typography variant="body2">
                              Time spent listening to audio: {formatSeconds(sessionSecondsListening)}
                            </Typography>
                            <Typography variant="body2">Words read: {sessionWordsRead}</Typography>
                            <Typography variant="body2">Pages finished: {sessionPagesFinished}</Typography>
                            <Typography variant="body2">
                              Percent (global) finished: {sessionGlobalPercentFinished.toFixed(3)}%
                            </Typography>
                            <Typography variant="body2">
                              Percent (page) finished: {pageFinishedPct.toFixed(3)}%
                            </Typography>
                            <Divider />
                            <Typography variant="body2">
                              Words read per minute: {sessionWordsPerMinute.toFixed(2)}
                            </Typography>
                            <Typography variant="body2">
                              Sentences read per minute: {sessionSentencesPerMinute.toFixed(2)}
                            </Typography>
                            <Typography variant="body2">
                              Percent read per minute: {sessionPercentPerMinute.toFixed(4)}%
                            </Typography>
                            <Typography variant="body2">State: {reader.tts.state}</Typography>
                          </Stack>
                        ) : null}
                      </Stack>
                    ) : null}

                    {reader.panels.show_tts ? (
                      <Stack spacing={1.5}>
                        <Typography variant="caption" fontWeight={700}>
                          <span data-testid="reader-tts-state-summary">
                            State: {reader.tts.state} | Sentence:{" "}
                            {reader.tts.current_sentence_idx !== null
                              ? `${reader.tts.current_sentence_idx + 1}/${Math.max(1, reader.tts.sentence_count)}`
                              : `0/${Math.max(1, reader.tts.sentence_count)}`}
                          </span>
                        </Typography>
                        <Typography variant="caption" color="text.secondary">
                          <span data-testid="reader-tts-progress-label">
                            Progress: {reader.tts.progress_pct.toFixed(3)}%
                          </span>
                        </Typography>
                        {ttsStateEvent ? (
                          <Typography variant="caption" color="text.secondary">
                            Last TTS event #{ttsStateEvent.request_id}: {ttsStateEvent.action}
                          </Typography>
                        ) : null}
                        <Typography variant="caption" color="text.secondary">
                          Playback controls are shown in the player bar at the bottom of the reading pane.
                        </Typography>
                        <Divider />
                        <NumericSettingControl
                          label="Playback Speed"
                          testId="setting-tts-speed"
                          value={reader.settings.tts_speed}
                          min={0.25}
                          max={4}
                          step={0.05}
                          decimals={2}
                          onCommit={async (next) => {
                            await onApplySettings({ tts_speed: next });
                          }}
                        />
                        <NumericSettingControl
                          label="Volume"
                          testId="setting-tts-volume"
                          value={reader.settings.tts_volume}
                          min={0}
                          max={2}
                          step={0.05}
                          decimals={2}
                          onCommit={async (next) => {
                            await onApplySettings({ tts_volume: next });
                          }}
                        />
                        <NumericSettingControl
                          label="Pause After Sentence"
                          testId="setting-pause-after-sentence"
                          value={reader.settings.pause_after_sentence}
                          min={0}
                          max={3}
                          step={0.01}
                          decimals={2}
                          onCommit={async (next) => {
                            await onApplySettings({ pause_after_sentence: next });
                          }}
                        />
                      </Stack>
                    ) : null}
                  </div>
                </Stack>
              </div>
            ) : null}
          </Stack>
        </Stack>
      </CardContent>
    </Card>
  );
});

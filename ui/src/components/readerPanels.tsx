import ArrowBackIcon from "@mui/icons-material/ArrowBack";
import ChevronLeftIcon from "@mui/icons-material/ChevronLeft";
import ChevronRightIcon from "@mui/icons-material/ChevronRight";
import DarkModeOutlinedIcon from "@mui/icons-material/DarkModeOutlined";
import GpsFixedIcon from "@mui/icons-material/GpsFixed";
import LightModeOutlinedIcon from "@mui/icons-material/LightModeOutlined";
import SearchIcon from "@mui/icons-material/Search";
import {
  Button,
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
  Typography
} from "@mui/material";
import { useEffect, useRef, useState } from "react";

import type {
  FontFamily,
  FontWeight,
  HighlightColor,
  ReaderSettingsPatch,
  ReaderSnapshot,
  ThemeMode,
  TtsStateEvent
} from "../types";
import { toHexColor, withAlpha, withHexColor } from "./readerDom";
import { almostEqual, normalizeNumber } from "./readerShared";
import type { ReaderSessionStatsState } from "./useReaderSessionStats";

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

interface NumericSettingControlProps {
  decimals?: number;
  label: string;
  max: number;
  min: number;
  onCommit: (value: number) => Promise<void>;
  step: number;
  testId?: string;
  value: number;
}

export interface ReaderTopBarProps {
  busy: boolean;
  hasHighlightSentence: boolean;
  jumpToHighlightedSentence: () => void;
  onCloseSession: () => Promise<void>;
  onNextPage: () => Promise<void>;
  onNextSentence: () => Promise<void>;
  onPrevPage: () => Promise<void>;
  onPrevSentence: () => Promise<void>;
  onSetPage: (page: number) => Promise<void>;
  onToggleTheme: () => Promise<void>;
  pageInput: string;
  reader: ReaderSnapshot;
  setPageInput: (value: string) => void;
  themeLabel: string;
}

export interface ReaderSearchBarProps {
  onSearchNext: () => Promise<void>;
  onSearchPrev: () => Promise<void>;
  onSearchQuery: (query: string) => Promise<void>;
  searchInput: string;
  setSearchInput: (value: string) => void;
}

interface ReaderSettingsPanelProps {
  onApplySettings: (patch: ReaderSettingsPatch) => Promise<void>;
  reader: ReaderSnapshot;
}

interface ReaderStatsPanelProps {
  reader: ReaderSnapshot;
  stats: ReaderSessionStatsState;
}

interface ReaderTtsPanelProps {
  onApplySettings: (patch: ReaderSettingsPatch) => Promise<void>;
  reader: ReaderSnapshot;
  ttsStateEvent: TtsStateEvent | null;
}

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

function formatRemainingTime(seconds: number, mode: "adaptive" | "minutes-seconds"): string {
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
    if (!Number.isFinite(parsed) || parsed < min || parsed > max) {
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
            overflow: "visible",
            px: 1,
            "& .MuiSlider-thumb": { boxShadow: "none" }
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

function HighlightControls({
  color,
  label,
  onChange,
  testIdPrefix
}: {
  color: HighlightColor;
  label: string;
  onChange: (next: HighlightColor) => void;
  testIdPrefix: string;
}) {
  return (
    <Stack spacing={1}>
      <Typography variant="caption" fontWeight={700}>
        {label}
      </Typography>
      <Stack direction="row" spacing={1} alignItems="center">
        <TextField
          type="color"
          size="small"
          value={toHexColor(color)}
          onChange={(event) => onChange(withHexColor(color, event.target.value))}
          inputProps={{ "data-testid": `${testIdPrefix}-color` }}
          sx={{ width: 76 }}
        />
        <NumericSettingControl
          label={`${label} Alpha`}
          testId={`${testIdPrefix}-alpha`}
          value={color.a}
          min={0}
          max={1}
          step={0.01}
          decimals={2}
          onCommit={async (next) => {
            onChange(withAlpha(color, next));
          }}
        />
      </Stack>
    </Stack>
  );
}

export function ReaderTopBar({
  busy,
  hasHighlightSentence,
  jumpToHighlightedSentence,
  onCloseSession,
  onNextPage,
  onNextSentence,
  onPrevPage,
  onPrevSentence,
  onSetPage,
  onToggleTheme,
  pageInput,
  reader,
  setPageInput,
  themeLabel
}: ReaderTopBarProps) {
  const themeIcon =
    reader.settings.theme === "night" ? <LightModeOutlinedIcon /> : <DarkModeOutlinedIcon />;

  return (
    <Stack
      direction="row"
      alignItems="center"
      spacing={1}
      data-testid="reader-topbar"
      sx={{
        flexShrink: 0,
        flexWrap: "nowrap",
        minHeight: 52,
        overflowX: "hidden",
        overflowY: "visible",
        paddingRight: 0.5,
        paddingTop: 0.5,
        whiteSpace: "nowrap"
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
          if (event.key !== "Enter") {
            return;
          }
          const parsed = Number(pageInput);
          if (!Number.isFinite(parsed)) {
            return;
          }
          const page = Math.max(1, Math.min(reader.total_pages, Math.floor(parsed)));
          void onSetPage(page - 1);
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
  );
}

export function ReaderSearchBar({
  onSearchNext,
  onSearchPrev,
  onSearchQuery,
  searchInput,
  setSearchInput
}: ReaderSearchBarProps) {
  return (
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
      <Button variant="outlined" onClick={() => void onSearchPrev()} data-testid="reader-search-prev-button">
        Prev
      </Button>
      <Button variant="outlined" onClick={() => void onSearchNext()} data-testid="reader-search-next-button">
        Next
      </Button>
    </Stack>
  );
}

export function ReaderSettingsPanel({ onApplySettings, reader }: ReaderSettingsPanelProps) {
  return (
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

      <HighlightControls
        label="Day Highlight"
        color={reader.settings.day_highlight}
        testIdPrefix="setting-day-highlight"
        onChange={(next) => void onApplySettings({ day_highlight: next })}
      />

      <HighlightControls
        label="Night Highlight"
        color={reader.settings.night_highlight}
        testIdPrefix="setting-night-highlight"
        onChange={(next) => void onApplySettings({ night_highlight: next })}
      />

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
          onChange={(event) => void onApplySettings({ auto_scroll_tts: event.target.checked })}
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
      <Stack direction="row" alignItems="center" justifyContent="space-between" gap={2}>
        <Stack spacing={0.25}>
          <Typography variant="caption" fontWeight={700}>
            Show Original Text
          </Typography>
          <Typography variant="caption" color="text.secondary">
            Text-only view only. TTS still uses normalized text.
          </Typography>
        </Stack>
        <Switch
          checked={reader.settings.text_only_show_original_text}
          disabled={!reader.text_only_mode}
          onChange={(event) =>
            void onApplySettings({
              text_only_show_original_text: event.target.checked
            })
          }
        />
      </Stack>
    </Stack>
  );
}

export function ReaderStatsPanel({ reader, stats }: ReaderStatsPanelProps) {
  return (
    <Stack spacing={1.2}>
      <Tabs
        value={stats.statsTab}
        onChange={(_, value: "page" | "global" | "session") => stats.setStatsTab(value)}
        variant="scrollable"
        allowScrollButtonsMobile
        sx={{ minHeight: 32 }}
      >
        <Tab label="Current Page Stats" value="page" />
        <Tab label="Global Stats" value="global" />
        <Tab label="Current Session Stats" value="session" />
      </Tabs>
      {stats.statsTab === "page" ? (
        <Stack spacing={0.8}>
          <Typography variant="body2">
            Page index: {reader.stats.page_index} / {reader.stats.total_pages}
          </Typography>
          <Typography variant="body2">Words on page: {reader.stats.page_word_count}</Typography>
          <Typography variant="body2">Sentences on page: {reader.stats.page_sentence_count}</Typography>
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
          <Typography variant="body2">Words read on page: {stats.wordsReadOnPage}</Typography>
          <Typography variant="body2">Sentences read on page: {stats.sentencesReadOnPage}</Typography>
        </Stack>
      ) : null}
      {stats.statsTab === "global" ? (
        <Stack spacing={0.8}>
          <Typography variant="body2" fontWeight={700}>
            Global Stats
          </Typography>
          <Typography variant="body2">Total page count: {reader.stats.total_pages}</Typography>
          <Typography variant="body2">Total words in book: {stats.estimatedTotalWords}</Typography>
          <Typography variant="body2">
            Total sentences in book: {stats.estimatedTotalSentences}
          </Typography>
          <Divider />
          <Typography variant="body2" fontWeight={700}>
            Global Progress
          </Typography>
          <Typography variant="body2">Total read pages: {stats.estimatedReadPages}</Typography>
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
      {stats.statsTab === "session" ? (
        <Stack spacing={0.8}>
          <Typography variant="body2">
            Time spent in app: {formatSeconds(stats.sessionSecondsInApp)}
          </Typography>
          <Typography variant="body2">
            Time spent listening to audio: {formatSeconds(stats.sessionSecondsListening)}
          </Typography>
          <Typography variant="body2">Words read: {stats.sessionWordsRead}</Typography>
          <Typography variant="body2">Pages finished: {stats.sessionPagesFinished}</Typography>
          <Typography variant="body2">
            Percent (global) finished: {stats.sessionGlobalPercentFinished.toFixed(3)}%
          </Typography>
          <Typography variant="body2">
            Percent (page) finished: {stats.pageFinishedPct.toFixed(3)}%
          </Typography>
          <Divider />
          <Typography variant="body2">
            Words read per minute: {stats.sessionWordsPerMinute.toFixed(2)}
          </Typography>
          <Typography variant="body2">
            Sentences read per minute: {stats.sessionSentencesPerMinute.toFixed(2)}
          </Typography>
          <Typography variant="body2">
            Percent read per minute: {stats.sessionPercentPerMinute.toFixed(4)}%
          </Typography>
          <Typography variant="body2">State: {reader.tts.state}</Typography>
        </Stack>
      ) : null}
    </Stack>
  );
}

export function ReaderTtsPanel({ onApplySettings, reader, ttsStateEvent }: ReaderTtsPanelProps) {
  return (
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
  );
}

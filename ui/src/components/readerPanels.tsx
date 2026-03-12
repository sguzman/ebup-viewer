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
  disabled?: boolean;
  disabledReason?: string | null;
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

function formatPdfTokenLabel(value: string): string {
  return value.replaceAll("_", " ");
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
  disabled = false,
  disabledReason = null,
  onSearchNext,
  onSearchPrev,
  onSearchQuery,
  searchInput,
  setSearchInput
}: ReaderSearchBarProps) {
  return (
    <Stack spacing={0.5} sx={{ flexShrink: 0 }}>
      <Stack direction="row" spacing={1} alignItems="center">
        <SearchIcon fontSize="small" />
        <TextField
          size="small"
          fullWidth
          label="Search (regex supported)"
          value={searchInput}
          disabled={disabled}
          data-testid="reader-search-input"
          inputProps={{ "data-reader-search-input": "1" }}
          onChange={(event) => setSearchInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !disabled) {
              void onSearchQuery(searchInput);
            }
          }}
        />
        <Button
          variant="outlined"
          onClick={() => void onSearchQuery(searchInput)}
          data-testid="reader-search-apply-button"
          disabled={disabled}
        >
          Apply
        </Button>
        <Button
          variant="outlined"
          onClick={() => void onSearchPrev()}
          data-testid="reader-search-prev-button"
          disabled={disabled}
        >
          Prev
        </Button>
        <Button
          variant="outlined"
          onClick={() => void onSearchNext()}
          data-testid="reader-search-next-button"
          disabled={disabled}
        >
          Next
        </Button>
      </Stack>
      {disabledReason ? (
        <Typography variant="caption" color="text.secondary" data-testid="reader-search-disabled-reason">
          {disabledReason}
        </Typography>
      ) : null}
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
          {reader.pdf_classification ? (
            <>
              <Divider />
              <Typography variant="body2" fontWeight={700}>
                PDF Diagnostics
              </Typography>
              <Typography variant="body2">
                Document class: {formatPdfTokenLabel(reader.pdf_classification.document_class)}
              </Typography>
              <Typography variant="body2">
                OCR recommendation: {formatPdfTokenLabel(reader.pdf_classification.ocr_recommendation)}
              </Typography>
              <Typography variant="body2">
                Confidence: {reader.pdf_classification.confidence.toFixed(2)}
              </Typography>
              <Typography variant="body2">
                Sampled pages: {reader.pdf_classification.feature_summary.sampled_pages}
              </Typography>
              <Typography variant="body2">
                Image-heavy sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.image_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Mixed text/image sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.mixed_text_image_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Full-page raster sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.full_page_raster_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Hidden text-layer sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.hidden_text_layer_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Invisible-text sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.invisible_text_layer_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Duplicate-text sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.duplicate_text_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Stacked duplicate-text sampled pages:{" "}
                {formatPercent(reader.pdf_classification.feature_summary.stacked_duplicate_text_page_ratio * 100)}
              </Typography>
              <Typography variant="body2">
                Trust: block {reader.pdf_classification.trust_diagnostics.block_coherence.toFixed(2)} | coordinates{" "}
                {reader.pdf_classification.trust_diagnostics.coordinate_sanity.toFixed(2)} | reading order{" "}
                {reader.pdf_classification.trust_diagnostics.reading_order_stability.toFixed(2)}
              </Typography>
              <Typography variant="body2">
                Suppression needed:{" "}
                {reader.pdf_classification.trust_diagnostics.duplicate_text_suppression_needed ? "yes" : "no"}
                {" | "}Hidden text layer suspected:{" "}
                {reader.pdf_classification.trust_diagnostics.hidden_text_layer_suspected ? "yes" : "no"}
              </Typography>
              <Typography variant="body2">
                Invisible text suspected:{" "}
                {reader.pdf_classification.trust_diagnostics.invisible_text_suspected ? "yes" : "no"}
                {" | "}Stacked duplicate text suspected:{" "}
                {reader.pdf_classification.trust_diagnostics.stacked_duplicate_text_suspected ? "yes" : "no"}
              </Typography>
              <Typography variant="body2">
                OCR replace confidence: {reader.pdf_classification.trust_diagnostics.ocr_replace_confidence.toFixed(2)}
                {" | "}OCR augment confidence: {reader.pdf_classification.trust_diagnostics.ocr_augment_confidence.toFixed(2)}
                {" | "}Threshold met: {reader.pdf_classification.trust_diagnostics.ocr_confidence_threshold_met ? "yes" : "no"}
              </Typography>
              {reader.pdf_classification.reasons.length > 0 ? (
                <Typography variant="body2">
                  Why: {reader.pdf_classification.reasons.slice(0, 4).map(formatPdfTokenLabel).join("; ")}
                </Typography>
              ) : null}
              {reader.pdf_classification.trust_diagnostics.rationale.length > 0 ? (
                <Typography variant="body2">
                  Trust rationale:{" "}
                  {reader.pdf_classification.trust_diagnostics.rationale
                    .slice(0, 4)
                    .map(formatPdfTokenLabel)
                    .join("; ")}
                </Typography>
              ) : null}
              {reader.pdf_classification.page_classes.length > 0 ? (
                <Stack spacing={0.35}>
                  <Typography variant="body2" fontWeight={700}>
                    Sampled Page Classes
                  </Typography>
                  {reader.pdf_classification.page_classes.slice(0, 6).map((page) => (
                    <Typography key={`pdf-page-class-${page.page_index}`} variant="body2">
                      Page {page.page_index}: {formatPdfTokenLabel(page.class)} ({page.confidence.toFixed(2)}) | image coverage{" "}
                      {formatPercent(page.features.image_coverage_ratio * 100)} | block{" "}
                      {page.features.block_coherence.toFixed(2)} | order {page.features.reading_order_stability.toFixed(2)}
                      {page.features.invisible_text_suspected ? " | invisible text" : ""}
                      {page.features.stacked_duplicate_text_suspected ? " | stacked duplicate text" : ""}
                    </Typography>
                  ))}
                </Stack>
              ) : null}
              {reader.pdf_ocr_alignment ? (
                <>
                  <Typography variant="body2">
                    OCR geometry quality: {formatPdfTokenLabel(reader.pdf_ocr_alignment.quality_class)}
                  </Typography>
                  <Typography variant="body2">
                    OCR source kind: {formatPdfTokenLabel(reader.pdf_ocr_alignment.source_kind)}
                  </Typography>
                  <Typography variant="body2">
                    Coverage: {formatPercent(reader.pdf_ocr_alignment.coverage_ratio * 100)} | mapped{" "}
                    {reader.pdf_ocr_alignment.mapped_sentence_count}/{reader.pdf_ocr_alignment.sentence_count} | highlightable{" "}
                    {reader.pdf_ocr_alignment.highlightable_sentence_count}
                  </Typography>
                  <Typography variant="body2">
                    Sentence rects: {reader.pdf_ocr_alignment.rect_mapped_sentence_count} | line fallback{" "}
                    {reader.pdf_ocr_alignment.line_mapped_sentence_count} | block fallback{" "}
                    {reader.pdf_ocr_alignment.block_mapped_sentence_count} | page-only{" "}
                    {reader.pdf_ocr_alignment.page_only_sentence_count} | missing{" "}
                    {reader.pdf_ocr_alignment.unmappable_sentence_count}
                  </Typography>
                  <Typography variant="body2">
                    Deterministic rebuild: {reader.pdf_ocr_alignment.deterministic ? "yes" : "no"}
                    {" | "}Token lineage available: {reader.pdf_ocr_alignment.token_lineage_available ? "yes" : "no"}
                  </Typography>
                  <Typography variant="body2">
                    Alignment cache reuse: {reader.pdf_ocr_alignment.reused_alignment_count} reused |{" "}
                    {reader.pdf_ocr_alignment.rebuilt_alignment_count} rebuilt | page buckets{" "}
                    {reader.pdf_ocr_alignment.cached_page_bucket_count} | build{" "}
                    {reader.pdf_ocr_alignment.alignment_build_ms} ms
                  </Typography>
                  <Typography variant="body2">
                    Geometry blocks: {reader.pdf_ocr_alignment.geometry_block_count} | lines{" "}
                    {reader.pdf_ocr_alignment.geometry_line_count} | tokens{" "}
                    {reader.pdf_ocr_alignment.geometry_token_count}
                  </Typography>
                  <Typography variant="body2">
                    Page timings: {reader.pdf_ocr_alignment.page_timing_count} pages | max{" "}
                    {reader.pdf_ocr_alignment.max_page_build_ms} ms | chunk timings{" "}
                    {reader.pdf_ocr_alignment.chunk_timing_count} | max{" "}
                    {reader.pdf_ocr_alignment.max_chunk_build_ms} ms
                  </Typography>
                  <Typography variant="body2">
                    Cross-column alignments: {reader.pdf_ocr_alignment.cross_column_alignment_count}
                    {" | "}confident cross-column: {reader.pdf_ocr_alignment.cross_column_confident_alignment_count}
                  </Typography>
                  <Typography variant="body2">
                    Exact rate: {formatPercent(reader.pdf_ocr_alignment.exact_sentence_rate * 100)}
                    {" | "}degraded fallback rate: {formatPercent(reader.pdf_ocr_alignment.degraded_fallback_rate * 100)}
                    {" | "}page-only rate: {formatPercent(reader.pdf_ocr_alignment.page_only_rate * 100)}
                    {" | "}unmappable rate: {formatPercent(reader.pdf_ocr_alignment.unmappable_rate * 100)}
                  </Typography>
                  <Typography variant="body2">
                    OCR geometry note: {reader.pdf_ocr_alignment.explanation}
                  </Typography>
                  {reader.pdf_ocr_alignment.degraded_reasons.length > 0 ? (
                    <Typography variant="body2">
                      OCR degraded reasons:{" "}
                      {reader.pdf_ocr_alignment.degraded_reasons
                        .slice(0, 4)
                        .map(formatPdfTokenLabel)
                        .join("; ")}
                    </Typography>
                  ) : null}
                </>
              ) : null}
              {reader.pdf_ocr_pipeline ? (
                <>
                  <Typography variant="body2">
                    OCR engine policy: {formatPdfTokenLabel(reader.pdf_ocr_pipeline.engine_policy)}
                  </Typography>
                  <Typography variant="body2">
                    OCR enabled: {reader.pdf_ocr_pipeline.ocr_enabled ? "yes" : "no"}
                    {" | "}Reading order mode: {formatPdfTokenLabel(reader.pdf_ocr_pipeline.reading_order_mode)}
                  </Typography>
                  <Typography variant="body2">
                    OCR chunks: {reader.pdf_ocr_pipeline.chunk_count} | sampled pages{" "}
                    {reader.pdf_ocr_pipeline.sampled_pages} | total pages {reader.pdf_ocr_pipeline.page_count}
                  </Typography>
                  {reader.pdf_ocr_pipeline.fallback_decisions.length > 0 ? (
                    <Typography variant="body2">
                      OCR fallback decisions:{" "}
                      {reader.pdf_ocr_pipeline.fallback_decisions
                        .map(formatPdfTokenLabel)
                        .join("; ")}
                    </Typography>
                  ) : null}
                  {reader.pdf_ocr_pipeline.fallback_strategy_labels.length > 0 ? (
                    <Typography variant="body2">
                      OCR fallback strategies:{" "}
                      {reader.pdf_ocr_pipeline.fallback_strategy_labels
                        .slice(0, 5)
                        .map(formatPdfTokenLabel)
                        .join("; ")}
                    </Typography>
                  ) : null}
                </>
              ) : null}
            </>
          ) : null}
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

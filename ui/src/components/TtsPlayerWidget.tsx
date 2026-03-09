import KeyboardDoubleArrowLeftRoundedIcon from "@mui/icons-material/KeyboardDoubleArrowLeftRounded";
import KeyboardDoubleArrowRightRoundedIcon from "@mui/icons-material/KeyboardDoubleArrowRightRounded";
import PauseRoundedIcon from "@mui/icons-material/PauseRounded";
import PlayArrowRoundedIcon from "@mui/icons-material/PlayArrowRounded";
import SkipNextRoundedIcon from "@mui/icons-material/SkipNextRounded";
import SkipPreviousRoundedIcon from "@mui/icons-material/SkipPreviousRounded";
import { Box, IconButton, Paper, Stack, Tooltip, Typography } from "@mui/material";
import { memo, useMemo, type ReactNode } from "react";

interface TtsPlayerWidgetProps {
  visible: boolean;
  busy: boolean;
  isPlaying: boolean;
  disabledReason?: string | null;
  canPrevPage: boolean;
  canNextPage: boolean;
  canPrevSentence: boolean;
  canNextSentence: boolean;
  currentSentenceLabel: string;
  progressLabel: string;
  onPrevPage: () => Promise<void>;
  onPrevSentence: () => Promise<void>;
  onTogglePlayPause: () => Promise<void>;
  onNextSentence: () => Promise<void>;
  onNextPage: () => Promise<void>;
}

interface PlayerAction {
  key: string;
  label: string;
  icon: ReactNode;
  disabled: boolean;
  prominent?: boolean;
  onClick: () => Promise<void>;
}

export const TtsPlayerWidget = memo(function TtsPlayerWidget({
  visible,
  busy,
  isPlaying,
  disabledReason,
  canPrevPage,
  canNextPage,
  canPrevSentence,
  canNextSentence,
  currentSentenceLabel,
  progressLabel,
  onPrevPage,
  onPrevSentence,
  onTogglePlayPause,
  onNextSentence,
  onNextPage
}: TtsPlayerWidgetProps) {
  const controlsDisabled = busy || Boolean(disabledReason);
  const actions = useMemo<PlayerAction[]>(
    () => [
      {
        key: "prev-page",
        label: "Previous page",
        icon: <KeyboardDoubleArrowLeftRoundedIcon fontSize="medium" />,
        disabled: controlsDisabled || !canPrevPage,
        onClick: onPrevPage
      },
      {
        key: "prev-sentence",
        label: "Previous sentence",
        icon: <SkipPreviousRoundedIcon fontSize="medium" />,
        disabled: controlsDisabled || !canPrevSentence,
        onClick: onPrevSentence
      },
      {
        key: "play-pause",
        label: isPlaying ? "Pause" : "Play",
        icon: isPlaying ? (
          <PauseRoundedIcon sx={{ fontSize: 34 }} />
        ) : (
          <PlayArrowRoundedIcon sx={{ fontSize: 34 }} />
        ),
        disabled: controlsDisabled,
        prominent: true,
        onClick: onTogglePlayPause
      },
      {
        key: "next-sentence",
        label: "Next sentence",
        icon: <SkipNextRoundedIcon fontSize="medium" />,
        disabled: controlsDisabled || !canNextSentence,
        onClick: onNextSentence
      },
      {
        key: "next-page",
        label: "Next page",
        icon: <KeyboardDoubleArrowRightRoundedIcon fontSize="medium" />,
        disabled: controlsDisabled || !canNextPage,
        onClick: onNextPage
      }
    ],
    [
      controlsDisabled,
      canNextPage,
      canNextSentence,
      canPrevPage,
      canPrevSentence,
      isPlaying,
      onNextPage,
      onNextSentence,
      onPrevPage,
      onPrevSentence,
      onTogglePlayPause
    ]
  );

  if (!visible) {
    return null;
  }

  return (
    <Paper
      elevation={0}
      data-testid="reader-tts-player-widget"
      sx={{
        borderTop: "1px solid rgba(148, 163, 184, 0.28)",
        background:
          "linear-gradient(180deg, rgba(248,250,252,0.96) 0%, rgba(255,255,255,0.98) 100%)",
        px: { xs: 1, sm: 1.5 },
        py: 1.25,
        flexShrink: 0
      }}
    >
      <Stack spacing={1} alignItems="center">
        <Stack
          direction={{ xs: "column", sm: "row" }}
          spacing={{ xs: 0.35, sm: 1.5 }}
          alignItems="center"
          justifyContent="center"
          sx={{ width: "100%", minHeight: 22 }}
        >
          {disabledReason ? (
            <Typography
              variant="caption"
              color="warning.main"
              data-testid="reader-tts-player-disabled-reason"
            >
              {disabledReason}
            </Typography>
          ) : null}
          <Typography
            variant="caption"
            color="text.secondary"
            data-testid="reader-tts-player-sentence-label"
          >
            {currentSentenceLabel}
          </Typography>
          <Typography
            variant="caption"
            color="text.secondary"
            data-testid="reader-tts-player-progress-label"
          >
            {progressLabel}
          </Typography>
        </Stack>

        <Stack
          direction="row"
          spacing={{ xs: 0.5, sm: 1 }}
          alignItems="center"
          justifyContent="center"
          sx={{
            width: "100%",
            flexWrap: "nowrap"
          }}
        >
          {actions.map((action) => (
            <Box
              key={action.key}
              sx={{
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                minWidth: action.prominent ? { xs: 82, sm: 96 } : { xs: 64, sm: 72 }
              }}
            >
              <Tooltip title={action.label}>
                <span>
                  <IconButton
                    color={action.prominent ? "primary" : "default"}
                    onClick={() => void action.onClick()}
                    disabled={action.disabled}
                    aria-label={action.label}
                    data-testid={`reader-tts-player-${action.key}`}
                    data-prominent={action.prominent ? "1" : "0"}
                    sx={{
                      width: action.prominent ? 64 : 44,
                      height: action.prominent ? 64 : 44,
                      border: "1px solid rgba(148, 163, 184, 0.24)",
                      background: action.prominent
                        ? "linear-gradient(135deg, rgba(37,99,235,0.14) 0%, rgba(56,189,248,0.18) 100%)"
                        : "rgba(255,255,255,0.88)",
                      boxShadow: action.prominent
                        ? "0 10px 22px rgba(37, 99, 235, 0.16)"
                        : "0 1px 2px rgba(15, 23, 42, 0.06)",
                      transition: "background-color 120ms ease, transform 120ms ease",
                      "&:hover": {
                        background: action.prominent
                          ? "linear-gradient(135deg, rgba(37,99,235,0.18) 0%, rgba(56,189,248,0.22) 100%)"
                          : "rgba(241,245,249,0.96)",
                        transform: "translateY(-1px)"
                      },
                      "&.Mui-disabled": {
                        opacity: 0.42
                      }
                    }}
                  >
                    {action.icon}
                  </IconButton>
                </span>
              </Tooltip>
              <Typography
                variant="caption"
                color="text.secondary"
                sx={{
                  mt: 0.35,
                  fontSize: action.prominent ? "0.74rem" : "0.68rem",
                  textAlign: "center",
                  lineHeight: 1.15
                }}
              >
                {action.label}
              </Typography>
            </Box>
          ))}
        </Stack>
      </Stack>
    </Paper>
  );
});

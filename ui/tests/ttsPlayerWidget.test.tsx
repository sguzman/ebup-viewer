import { renderToStaticMarkup } from "react-dom/server";
import type { ComponentProps } from "react";
import { describe, expect, it } from "vitest";

import { TtsPlayerWidget } from "../src/components/TtsPlayerWidget";

function renderWidget(
  visible: boolean,
  overrides: Partial<ComponentProps<typeof TtsPlayerWidget>> = {}
) {
  return renderToStaticMarkup(
    <TtsPlayerWidget
      visible={visible}
      busy={false}
      isPlaying={false}
      disabledReason={null}
      canPrevPage={true}
      canNextPage={true}
      canPrevSentence={true}
      canNextSentence={true}
      currentSentenceLabel="Sentence 2/8"
      progressLabel="Progress 25.000% | playing"
      onPrevPage={async () => {}}
      onPrevSentence={async () => {}}
      onTogglePlayPause={async () => {}}
      onNextSentence={async () => {}}
      onNextPage={async () => {}}
      {...overrides}
    />
  );
}

describe("TtsPlayerWidget", () => {
  it("stays hidden when TTS controls are disabled", () => {
    const html = renderWidget(false);
    expect(html).toBe("");
  });

  it("renders the required controls in the required order", () => {
    const html = renderWidget(true);
    const order = [
      "reader-tts-player-prev-page",
      "reader-tts-player-prev-sentence",
      "reader-tts-player-play-pause",
      "reader-tts-player-next-sentence",
      "reader-tts-player-next-page"
    ];
    const positions = order.map((marker) => html.indexOf(marker));

    expect(positions.every((idx) => idx >= 0)).toBe(true);
    expect([...positions].sort((a, b) => a - b)).toEqual(positions);
  });

  it("marks the play pause button as the prominent action", () => {
    const html = renderWidget(true);
    expect(html).toContain('data-testid="reader-tts-player-play-pause"');
    expect(html).toContain('data-prominent="1"');
  });

  it("reflects disabled states on unavailable controls", () => {
    const html = renderWidget(true, {
      canPrevPage: false,
      canPrevSentence: false,
      canNextSentence: false,
      canNextPage: false
    });

    expect(html).toContain('data-testid="reader-tts-player-prev-page"');
    expect(html).toContain('data-testid="reader-tts-player-prev-sentence"');
    expect(html).toContain('data-testid="reader-tts-player-next-sentence"');
    expect(html).toContain('data-testid="reader-tts-player-next-page"');
    expect(html.match(/disabled=""/g)?.length ?? 0).toBeGreaterThanOrEqual(4);
  });

  it("disables all controls and renders a reason when playback is unavailable", () => {
    const html = renderWidget(true, {
      disabledReason: "TTS is disabled for this PDF until OCR produces usable text."
    });

    expect(html).toContain('data-testid="reader-tts-player-disabled-reason"');
    expect(html).toContain("TTS is disabled for this PDF until OCR produces usable text.");
    expect(html.match(/disabled=""/g)?.length ?? 0).toBeGreaterThanOrEqual(5);
  });
});

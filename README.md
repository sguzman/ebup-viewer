# ebup-viewer

Rust desktop EPUB reader with integrated TTS playback using Piper.

`ebup-viewer` loads an EPUB, paginates the extracted text, and provides synchronized sentence-level audio playback with highlighting, bookmarking, and per-book persisted settings.

## Current Status

- UI: `iced` (`wgpu` backend)
- EPUB parsing: `epub` + `html2text`
- TTS model runtime: `piper-rs`
- Audio playback: `rodio`
- Speed/pitch compensation at playback: `sonic-rs-sys`
- TTS parallelism: multi-process worker pool (`tts_threads`)

## Features

- EPUB text extraction from spine chapters
- Reader pagination with configurable lines-per-page
- Configurable typography and spacing controls
- Day/Night mode and sentence highlight colors
- Sentence-aware TTS playback controls
- Play from page start or current highlighted sentence
- Seek sentence forward/backward
- Auto-scroll to current spoken sentence
- Scroll/position bookmark resume
- Per-EPUB cached config and TTS WAV cache
- Global audio progress indicator (`TTS xx.x%`) based on sentence index across the whole book

## Architecture

Top-level modules:

- `src/main.rs`: startup, logging, config load/override, EPUB load, app launch, worker mode dispatch
- `src/app/`: GUI state/update/view
- `src/tts.rs`: TTS engine facade, worker-pool orchestration, cache lookups, playback + time-stretching
- `src/tts_worker.rs`: worker process entry (`--tts-worker`) that runs Piper synthesis and writes WAV files
- `src/epub_loader.rs`: EPUB text extraction
- `src/pagination.rs`: text-to-page chunking
- `src/cache.rs`: bookmark/config/audio cache paths and persistence
- `src/config/`: typed config model + TOML parsing/serialization
- `src/text_utils.rs`: sentence splitting utility

### App update split

`src/app/update/` is organized by domain:

- `core.rs`: reducer/effect dispatch/subscriptions
- `navigation.rs`: page navigation and pagination-related behavior
- `appearance.rs`: theme/typography/settings mutations
- `tts.rs`: playback lifecycle, batch prep, seek, timing ticks
- `scroll.rs`: scroll tracking, bookmark capture, jump-to-audio behavior

## Runtime Flow

1. `main` checks for `--tts-worker`; if present, runs worker loop and exits.
2. Main app initializes tracing and loads `conf/config.toml`.
3. If `.cache/<book-hash>/config.toml` exists, per-book overrides are loaded.
4. `log_level` and `tts_threads` are explicitly taken from base config to avoid stale cached values.
5. EPUB is loaded and converted to plain text.
6. GUI app starts with bookmark restore (`page`, `sentence_idx`/`sentence_text`, `scroll_y`).
7. On TTS start:
   - Sentences are split for current page.
   - Engine prepares WAV files from `sentence_idx..`.
   - Missing files are synthesized by worker processes and written into cache.
   - Playback starts through `rodio`.
8. While playing, `Tick` updates current sentence highlight using accumulated clip durations + configured pause.
9. At page end, playback auto-advances to next page if available.

## TTS Model and Parallelism

This project uses `piper-rs` with separate worker processes to avoid in-process phonemization/threading issues.

- `tts_threads = N` means up to `N` worker processes for sentence synthesis.
- Work is per sentence (round-robin dispatch across workers).
- Worker IPC is line-delimited JSON over stdin/stdout.
- Audio generation is always done at normal synthesis rate.
- Playback speed (`tts_speed`) is applied during playback, with Sonic time-stretching to reduce pitch distortion.

## Requirements

Tested on Linux. You need:

- Rust toolchain (`cargo`, `rustc`)
- C toolchain (`clang`/`cc`, linker)
- `cmake` (for vendored `espeak-rs-sys` build)
- ONNX Runtime library available to linker/runtime (`libonnxruntime`)
- ALSA development/runtime (`libasound`)
- Piper voice model files (`.onnx` + matching `.onnx.json`)
- eSpeak data directory (for phonemization data used by Piper)

Notes:

- Crate patch points `espeak-rs-sys` to `vendor/espeak-rs-sys`.
- `.cargo/config.toml` sets `CMAKE_ARGS = "-DUSE_LIBPCAUDIO=OFF"` to avoid pcaudiolib dependency.

## Build

```bash
cargo build --release
```

## Run

```bash
cargo run --release -- <path-to-book.epub>
```

Example:

```bash
cargo run --release -- res/pg64317-images-3.epub
```

## Configuration

Primary file: `conf/config.toml`

Sections:

- `[appearance]`
- `[reading_behavior]`
- `[ui]`
- `[logging]`
- `[tts]`

### Keys

| Key | Type | Purpose |
|---|---|---|
| `appearance.theme` | `day`/`night` | UI theme |
| `appearance.font_family` | enum | Reader font family |
| `appearance.font_weight` | `light`/`normal`/`bold` | Reader font weight |
| `appearance.font_size` | `u32` | Text size |
| `appearance.line_spacing` | `f32` | Line height multiplier |
| `appearance.word_spacing` | `u32` | Extra spacing between words |
| `appearance.letter_spacing` | `u32` | Extra spacing between letters |
| `appearance.lines_per_page` | `usize` | Pagination density |
| `appearance.margin_horizontal` | `u16` | Horizontal padding |
| `appearance.margin_vertical` | `u16` | Vertical padding |
| `appearance.day_highlight` | RGBA object | Current sentence color in day mode |
| `appearance.night_highlight` | RGBA object | Current sentence color in night mode |
| `reading_behavior.pause_after_sentence` | `f32` | Pause inserted between clips |
| `reading_behavior.auto_scroll_tts` | `bool` | Follow sentence while playing |
| `reading_behavior.center_spoken_sentence` | `bool` | Center-ish tracking behavior |
| `ui.show_tts` | `bool` | Show TTS controls |
| `ui.show_settings` | `bool` | Show settings panel |
| `logging.log_level` | `trace..error` | Runtime log verbosity |
| `tts.tts_model_path` | `string` | Path to Piper `.onnx` model |
| `tts.tts_espeak_path` | `string` | Root path containing `espeak-ng-data` |
| `tts.tts_speed` | `f32` | Playback speed multiplier |
| `tts.tts_threads` | `usize` | Worker process count for synthesis |

## Cache Layout

Cache root is `.cache/`.

Per EPUB, files are under `.cache/<sha256(epub-path)>/`:

- `bookmark.toml`: page/sentence/scroll resume state
- `config.toml`: per-book persisted UI + TTS settings
- `tts/tts-<hash>.wav`: sentence-level synthesized audio cache

Audio cache key hash uses model path + sentence text. Changing model path naturally invalidates cache keys.

## UI Controls

Reader controls:

- Previous/Next page
- Theme toggle
- Settings panel toggle
- TTS panel toggle
- Font size slider

TTS controls:

- Seek backward/forward sentence
- Play/Pause
- Play page from start
- Play from highlighted sentence
- Jump to current audio sentence
- Speed slider

Top bar status:

- `Page n of m`
- `TTS xx.x%` global audio cursor progress

## Logging

- Tracing is initialized at startup.
- Default filter falls back to `debug` if not configured.
- `logging.log_level` in config is applied at runtime.
- `RUST_LOG` can still influence initial startup filter.

## Troubleshooting

### Linker errors around `audio_object_*` / `create_audio_device_object`

If these appear, ensure vendored `espeak-rs-sys` is active and built with pcaudiolib disabled.

Check:

- `Cargo.toml` has `[patch.crates-io] espeak-rs-sys = { path = "vendor/espeak-rs-sys" }`
- `.cargo/config.toml` includes `CMAKE_ARGS = "-DUSE_LIBPCAUDIO=OFF"`

Then clean and rebuild:

```bash
cargo clean
cargo build --release
```

### `espeak-rs-sys` "unnecessary transmute" warnings

These are generated in bindgen output from dependency build artifacts and are non-fatal.

### Vulkan warning `Unrecognized present mode 1000361000`

This comes from `wgpu-hal`/driver interactions. If app behavior is normal, it is usually informational.

### `tts_threads` appears ignored

Current startup logic intentionally forces `tts_threads` from base `conf/config.toml` over cached per-book config to avoid stale values.

## Development

Useful commands:

```bash
cargo check
cargo fmt
cargo clippy --all-targets --all-features
```

If you change config schema:

- Update `src/config/models.rs`
- Update table mapping in `src/config/tables.rs`
- Update defaults in `src/config/defaults.rs`
- Update sample `conf/config.toml`

If you change TTS worker protocol:

- Keep request/response structs in sync between `src/tts.rs` and `src/tts_worker.rs`
- Validate worker mode manually with `cargo run -- --tts-worker ...` only for debugging

## License

See `LICENSE`.

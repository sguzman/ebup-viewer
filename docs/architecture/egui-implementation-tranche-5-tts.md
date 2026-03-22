# Implementation Tranche 5: Audio & TTS Integration

This document captures the tranche 5 work for audio and TTS integration.

## Deliverables
- Tie Piper/Rodio playback interfaces into the egui shortcut registry and command contracts so keyboard-driven TTS controls behave like the previous shell.
- Surface `reader.tts` state in the UI with play/pause, seek, and JumpToSentence instrumentation carrying `budget_plan=shell.performance_budget` plus `audio_command` metadata.
- Cache playback timing and sentence duration metadata, emit `tts.timeline` spans for auto-play and manual navigation, and expose diagnostics for audio budget decisions and anchor fallback counts.
- Document manual vs. automatic TTS scroll/anchor semantics so follow-on work knows when `shell.performance_budget` throttles audio-induced jumps.

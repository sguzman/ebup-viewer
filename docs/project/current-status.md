# LanternLeaf Current Status

Updated: 2026-09-06

This file contains verified or explicitly bounded evidence only. Roadmap checkboxes and historical parity claims are not accepted as current proof.

## Workspace / architecture

**VERIFIED PRESENT**

- Rust workspace contains root package plus `lanternleaf-core`, `lanternleaf-app`, and `lanternleaf-egui`.
- Root `src/main.rs` dispatches TTS worker mode and otherwise calls `lanternleaf_egui::app::run()`.
- The intended current GUI architecture is native egui.

## Windows build

**BROKEN**

A fresh Windows 11 `cargo check --workspace --verbose --jobs 16` reached native dependency compilation and then failed because bindgen 0.59.2 could not locate `clang.dll` / `libclang.dll`.

The exact dependency owner/chain is intentionally left to Goal 0001 to prove rather than guessed here.

## Egui shell

**PRESENT BUT UNVERIFIED**

The crate and entrypoint are present. Fresh Windows launch behavior has not yet been verified in the restart.

## EPUB / TXT / Markdown

**PRESENT BUT UNVERIFIED**

The repository contains prior implementations and documentation, but restart-era Windows runtime verification has not yet been performed.

## HTML

**PARTIAL / UNVERIFIED**

Historical roadmap material describes native HTML/EPUB rendering goals. Current user-facing completeness under egui has not yet been proven.

## Piper TTS

**PRESENT BUT UNVERIFIED ON THE RESTARTED WINDOWS ENVIRONMENT**

The project contains Piper/TTS worker infrastructure. Build/runtime verification is blocked by the current Windows native dependency issue.

## Native Windows TTS

**NOT IMPLEMENTED / NOT VERIFIED**

This is a restart priority.

## PDF rendering

**PARTIAL**

Native egui PDF modules and a dedicated native-PDF roadmap exist. The user reports PDF rendering was never fully finished.

## PDF text extraction and TTS/highlight synchronization

**PARTIAL / BROKEN-QUALITY AREA**

Substantial historical work exists, including prior commits about PDF TTS follow-scroll/highlight drift, but the user reports the native egui PDF rendering + TTS + highlight-sync work was never finished. Restart verification has not yet occurred.

## Calibre integration

**PRESENT BUT UNVERIFIED**

Recent commits include Calibre work, but restart-era runtime verification is pending.

## Browser/import integrations

**PRESENT IN DESIGN/HISTORY, UNVERIFIED**

Do not assume parity until exercised.

## Persistence / bookmarks / config / cache

**PRESENT BUT UNVERIFIED**

Substantial implementation exists. Fresh cross-platform validation is pending.

## Tests / CI

**UNVERIFIED FOR CURRENT WINDOWS BASELINE**

Goal 0001 will establish what currently runs and add/repair a useful Windows gate.

## Historical Tauri / React / WebView implementation

**HISTORICAL / OBSOLETE AS PRODUCTION TARGET**

It remains useful as behavioral evidence only.

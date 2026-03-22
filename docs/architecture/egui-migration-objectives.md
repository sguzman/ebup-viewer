# Egui Migration Objectives And Scope

This document captures the top-level objectives and scope for the egui migration.

## Primary objectives
- Replace the shipped Tauri + React/TypeScript desktop app with a native Rust `eframe` + `egui` desktop app.
- Keep the migration staged and parity-driven; do not perform a greenfield reset.
- End with a pure-Rust shipped desktop application and remove Tauri, TypeScript, React, Zustand, Vite, Vitest, and Playwright from production ownership.

## Scope to preserve during migration
- Starter/library flow.
- Reader shell/layout.
- EPUB/HTML/Markdown rendering.
- Native PDF rendering and sync.
- TTS runtime and playback controls.
- Settings, stats, and search.
- Browser-tab import.
- Calibre integration.
- Cache/config/bookmarks/recents.

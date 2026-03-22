# Egui Runtime Tracing Fields

This document lists the structured fields attached to command, effect, and event spans in the egui runtime so logging/observability consumers can rely on stable keys.

## Command spans (`app_command`)
- `request_id`: request identifier assigned by the runtime
- `action`: command action string (`AppCommand::action()`)
- `command`: command name
- `source_path`: source path for open commands
- `tab_id`: browser tab id for browser-tab commands
- `window_id`: browser window id for browser-tab commands
- `calibre_id`: calibre book id for calibre commands
- `trigger`: persistence trigger for persistence commands
- `log_level`: log level for runtime log-level commands
- `text_len`: clipboard text length for clipboard open commands
- `refresh`: refresh flag for browser-tab list commands
- `query_present`: whether a query string was supplied for browser-tab list commands

## Effect spans (`runtime_effect`)
- `request_id`: request identifier assigned by the runtime
- `effect`: effect name
- `owner`: effect owner enum
- `source_path`: source path for open commands
- `tab_id`: browser tab id for browser-tab commands
- `window_id`: browser window id for browser-tab commands
- `calibre_id`: calibre book id for calibre commands
- `trigger`: persistence trigger for persistence effects
- `text_len`: clipboard text length for clipboard open effects
- `refresh`: refresh flag for browser-tab list effects
- `query_present`: whether a query string was supplied for browser-tab list effects

## Event spans (`app_event`)
- `request_id`: request identifier for the originating command/effect
- `event`: event name
- `trigger`: persistence trigger for persistence results
- `outcome`: persistence outcome (`Completed`, `SkippedNoSession`, `Failed`)
- `scope`: operation scope when a command fails
- `error_code`: error code for failed commands

## Notes
- Fields are emitted only when relevant to the command/effect/event; unused fields are left empty.
- This file is the canonical reference for structured tracing keys during the migration.

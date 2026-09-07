# LanternLeaf Agent Guide

This file is the entry point for coding agents. The repository documentation is the system of record.

## Read before changing code

1. `docs/project/philosophy.md`
2. `docs/project/product-scope.md`
3. `docs/project/priorities.md`
4. `ARCHITECTURE.md`
5. `docs/project/current-status.md`
6. `docs/project/roles-and-workflow.md`
7. `docs/roadmaps/restart-master-roadmap-2026-09.md`
8. The single authorized macro-goal under `docs/work/ready/` or `docs/work/active/`

Historical Tauri/React/WebView documents and old roadmap checkboxes are context, not authority. Verified repository state and the current director-owned documents above outrank stale migration records.

## Roles

- **ChatGPT / director-architect-integrator** owns philosophy, product scope, priorities, architecture, roadmap ordering, goal definitions, review, integration, and updates to verified current status.
- **Codex / implementation worker** owns implementation explicitly delegated by the active macro-goal, relevant tests, validation, and a committed work report.
- **Human maintainer** owns operating the local checkout and local Windows/runtime/GUI/TTS testing when requested. The human is not the normal courier between ChatGPT and Codex.

## Authoritative architecture

The shipped desktop target is native Rust using `eframe` + `egui`.

- Do not resurrect Tauri, React, TypeScript, WebView, pdf.js, or browser-owned production rendering.
- Historical web code/docs may be consulted for behavior/parity evidence only.
- Rust owns canonical document/session/playback state.
- Platform-specific behavior belongs behind explicit abstractions and `cfg` boundaries.
- Windows and Linux are first-class desktop targets.

## Macro-goal protocol

Work state lives under `docs/work/`.

A **macro-goal** is larger than the old one-prompt/one-pass task style. It may intentionally contain several implementation, test, diagnosis, and repair passes and may contain multiple independent subtracks when their boundaries are explicit.

Codex should continue within the authorized goal until:

- every acceptance criterion is satisfied;
- a true architectural ambiguity requires director input; or
- continuing would violate an explicit non-goal.

Do not stop after the first compile failure merely to ask for another prompt if fixing the next directly-related blocker is already authorized by the goal.

At most one macro-goal is authorized in `docs/work/ready/` at a time.

## Goal-completion notification protocol

The human maintainer must not need to remember a second command merely to learn that a long-running macro-goal finished.

On Windows, Codex is responsible for launching the repository-owned goal watcher automatically for each macro-goal.

Once `scripts/codex-goal-notify.ps1` exists, Codex must:

1. move the authorized goal `ready -> active`;
2. start the watcher as an independent/detached Windows process for that goal ID/branch;
3. continue the macro-goal normally;
4. move the goal to `done/` or `blocked/` only at the real terminal state;
5. allow the watcher to emit the Windows desktop notification and exit;
6. then complete the ordinary report/push/checkout-restoration handoff.

The watcher is workflow UX, not a correctness gate:

- notification failure must never make an otherwise-correct product goal fail;
- Codex should record watcher startup/failure in the report when material;
- do not make the human manually start the watcher;
- do not notify on intermediate Codex turns/passes;
- notify only on the repository macro-goal reaching `done` or `blocked`.

Goal 0005 is the bootstrap goal that creates/tests this watcher. Because the script does not exist at the instant 0005 begins, Goal 0005 must implement it first and then launch it for its own remaining lifecycle if practical. Goal 0006 onward must launch it during normal goal activation.

## Scope discipline

- Implement only the authorized macro-goal.
- Respect every non-goal.
- Do not silently change philosophy, product scope, priorities, roadmap ordering, persistence formats, or public behavior.
- Do not perform opportunistic broad rewrites.
- Do not weaken/delete tests merely to get green.
- If a cross-subsystem design decision is genuinely open, preserve evidence and escalate instead of improvising.

## Implementation principles

- Rust stable; preserve edition 2024 where configured.
- Prefer explicit crate/module boundaries and single-responsibility modules.
- Existing god files are debt, not templates. Do not grow large hand-maintained files without explicit approval.
- Preserve and extend `tracing` diagnostics.
- Prefer deterministic local tests and add regression tests for concrete failures when practical.
- New dependencies require a concrete benefit and should use current compatible versions when justified.
- Native/platform dependencies must have reproducible setup and actionable diagnostics.
- Core reading/TTS behavior must not depend on a browser runtime.

## Work handoff

Unless a goal says otherwise, Codex should:

1. start from `main` and fast-forward it;
2. create/switch to the branch named by the goal, normally `codex/<goal-id>-<slug>`;
3. move the goal from `ready/` to `active/`;
4. launch the repository goal watcher on Windows when available;
5. execute all authorized passes;
6. run the required validation;
7. write `docs/work/reports/<goal-id>.md`;
8. move the goal to `done/` only if acceptance passes, otherwise `blocked/`;
9. commit and push the result;
10. switch the shared checkout back to `main` without merging the implementation branch;
11. verify `git branch --show-current`.

ChatGPT reviews the pushed branch directly and integrates accepted work into `main`.

Never claim Windows runtime behavior was verified unless it actually ran on Windows.

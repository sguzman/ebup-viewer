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

## Repository goal identity vs Codex Goal session

A repository macro-goal and a Codex Goal UI session are different lifecycles.

- The repository goal ID names the durable semantic work unit.
- A Codex Goal session is one disposable execution container for one attempt at that goal.
- Director review may reopen the same repository goal after a Codex Goal session has already terminated.
- In that case, start a **fresh Codex Goal session** while preserving the same repository goal ID.
- Do not create the next numbered macro-goal merely because the prior Codex Goal session ended.

For a director correction continuation, Codex must read the reopened goal and `docs/work/reviews/<goal-id>.md` from current `main`, continue the existing implementation branch/report lineage unless the review says otherwise, and preserve accepted work from the earlier attempt.

## Goal-completion notification protocol

The human maintainer must not need to remember a second command merely to learn that a long-running macro-goal finished.

On Windows, Codex is responsible for launching the repository-owned goal watcher automatically for each **execution attempt**. A reopened macro-goal gets a fresh watcher epoch even though the goal ID is unchanged.

Normal terminal-notification protocol:

1. after moving `ready -> active`, launch `scripts/codex-goal-notify.ps1` as an independent/detached Windows process for the current goal ID; for a correction continuation under a reused goal ID, pass `-Rearm` so stale terminal/ack state from the prior attempt is cleared before polling;
2. execute the goal normally;
3. write the report and move the goal to `done/` or `blocked/` only at its real terminal state;
4. commit and push the terminal implementation branch;
5. only after the push succeeds, call `scripts/signal-codex-goal-terminal.ps1` with the goal ID and terminal state;
6. let the signal helper wait briefly for the watcher acknowledgment;
7. restore the shared checkout to `main` even if notification delivery/acknowledgment failed.

The `.git/lanternleaf-goal-state/<id>.terminal` sentinel is the normal notification trigger. It is intentionally outside the checked-out tree so switching the shared checkout back to `main` cannot erase the terminal signal.

Because one repository goal may now have multiple execution attempts, the watcher is **exactly-once per attempt**, not exactly-once for the entire goal lifetime. On a correction continuation, `-Rearm` clears the previous attempt's ephemeral `.terminal` and `.ack` files before the new watcher begins. The human does not perform this reset manually.

The watcher may support repository-state fallback for diagnostics/tests, including slugged goal filenames such as `0006-non-pdf-reader-parity.md`, but ordinary Codex handoff must use the post-push sentinel protocol.

The watcher is workflow UX, not a correctness gate:

- notification failure must never make an otherwise-correct product goal fail;
- Codex should record watcher startup/signal/ack failure in the report when material;
- do not make the human manually start or signal the watcher;
- do not notify on intermediate Codex turns/passes;
- notify only after the terminal implementation branch has been pushed, so ChatGPT can inspect it immediately;
- a correction attempt under the same goal ID must not immediately notify from the prior attempt's stale sentinel/ack.

## Repo-native human development and QA

The Git checkout is the canonical human execution surface.

On Windows:

- `Scoopfile.json` is the Scoop-native declaration of Windows CLI dependencies; `deps.ps1` is the repository-owned bootstrap/import contract.
- `qa.ps1` is the repository-owned real-desktop QA entrypoint.
- `qa.ps1` may call `deps.ps1` automatically when dependencies are missing.
- local QA state, generated fixtures, cache, and logs belong under ignored `.qa/`.
- **never** instruct the human principal to download, unpack, or run a generated GitHub Actions/agent payload for development or manual QA.
- CI artifacts may exist as machine evidence or release/distribution outputs, but they are never a human development/manual-QA fallback.
- ordinary Windows CLI dependencies belong in `Scoopfile.json`; Rust belongs in `rust-toolchain.toml`; only Windows-native compiler/workload exceptions belong in bootstrap code.
- Scoop is the current Windows dependency convention. Nix/mise are not active LanternLeaf work and must not be added or scheduled without an explicit future director/principal policy change.

The intended human testing loop is:

`git pull -> .\qa.ps1 -> test -> report observations`.

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
4. launch the repository goal watcher on Windows;
5. execute all authorized passes;
6. run the required validation;
7. write `docs/work/reports/<goal-id>.md`;
8. move the goal to `done/` only if acceptance passes, otherwise `blocked/`;
9. commit and push the terminal result;
10. signal the pushed terminal state with `scripts/signal-codex-goal-terminal.ps1` (notification failure remains non-fatal);
11. switch the shared checkout back to `main` without merging the implementation branch;
12. verify `git branch --show-current`.

ChatGPT reviews the pushed branch directly and integrates accepted work into `main`.

Never claim Windows runtime behavior was verified unless it actually ran on Windows.

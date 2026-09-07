# 0005 — TTS correctness closure and automatic goal notifications

## Outcome

Close the correctness gaps found in director review of Goal 0004 and remove another piece of human workflow burden.

By the end of this one macro-goal:

1. active TTS backend/voice changes safely cancel/rebuild from canonical reader state;
2. Windows default-voice cache identity resolves to the actual effective voice ID;
3. Piper-specific initialization is isolated to Piper;
4. Windows sentence WAVs are explicitly proven decodable through the shared audio path;
5. hosted Windows CI records whether speech synthesis actually ran or was skipped for lack of voices;
6. the repository owns a tested Windows goal watcher;
7. Codex launches that watcher automatically, detached, with no extra human command;
8. Goal 0005 itself should use the watcher for its remaining lifecycle after the watcher is implemented, if practical.

Do not request a new prompt between these stages.

## Read first

- `AGENTS.md`
- `ARCHITECTURE.md`
- `docs/project/current-status.md`
- `docs/project/roles-and-workflow.md`
- `docs/work/reviews/0004.md`
- `docs/work/reports/0004.md`
- `docs/roadmaps/restart-master-roadmap-2026-09.md`

# Stage A — bootstrap automatic goal-completion notifications

This stage comes first so Goal 0005 itself can benefit from the watcher if possible.

## A1 — implement repository-owned watcher

Create:

`scripts/codex-goal-notify.ps1`

Requirements:

- Windows PowerShell/PowerShell-compatible;
- no paid/external service;
- no human setup step beyond normal repository prerequisites;
- watches one explicit macro-goal ID and repository root;
- distinguishes terminal `done` vs `blocked`;
- emits exactly one Windows desktop notification with:
  - LanternLeaf/project identity;
  - goal ID;
  - Completed or Blocked status;
- include an audible notification when Windows notification APIs allow it without intrusive behavior;
- exits after terminal notification;
- does not notify for intermediate Codex turns;
- is safe to run detached;
- does not modify product source/config;
- notification failure is non-fatal.

Preferred usage shape:

```powershell
scripts/codex-goal-notify.ps1 -GoalId 0005 -RepoRoot <repo>
```

The exact parameters may be refined for reliability.

## A2 — survive checkout restoration

Codex normally switches the shared checkout from its implementation branch back to `main` shortly after the terminal goal transition.

Design the watcher so that this cannot routinely cause a missed notification.

Acceptable strategies include:

- terminal-state acknowledgment before checkout restoration;
- branch-aware Git inspection;
- a watcher-owned sentinel/ack under a safe temporary location;
- another deterministic repo-owned mechanism.

Do not rely on a race-prone assumption that polling will always observe `done/` before the checkout switches.

## A3 — detached lifetime

Launch the watcher as an independent Windows process so it survives individual Codex turns/process changes.

Do not tie its lifetime to a single shell command that terminates when Codex returns.

## A4 — testability

Provide a deterministic no-toast/test mode or injectable notification sink so watcher logic can be tested without spamming the desktop.

Tests/probes must cover:

- done detection;
- blocked detection;
- exactly-once terminal behavior;
- no notification for active/ready;
- checkout-restoration-safe behavior;
- malformed/missing goal handling;
- watcher failure does not fail product validation.

Avoid adding a heavyweight test framework solely for this script unless justified.

## A5 — activate watcher for Goal 0005

After the script is implemented and its basic tests pass:

- launch it detached for Goal 0005;
- record whether watcher startup succeeded in `docs/work/reports/0005.md`;
- continue the same Codex Goal.

Do not stop and ask the human to launch it.

# Stage B — active backend/voice resynchronization correctness

## B1 — fix resync decision

Director finding:

`patch_has_tts_fields` recognizes `tts_backend` and `windows_voice_id`, but `should_sync_tts_after_reader_command` omits them.

Fix so backend/voice changes during active TTS cause the current runtime request to be cancelled/replaced and rebuilt from the canonical current reader position.

Preserve paused/idle semantics sensibly:

- do not invent playback when TTS is idle;
- do not jump the canonical sentence;
- when currently playing, new backend/voice must take effect on the rebuilt request;
- stale prepared/prefetched old-backend audio must not become active after the switch.

## B2 — regression tests

Add tests that prove, at minimum:

- a backend patch is treated as a runtime-resyncing TTS change;
- a voice patch is treated as a runtime-resyncing TTS change;
- current sentence/canonical cursor is preserved;
- old request cancellation/replacement occurs rather than silently continuing old backend state;
- idle settings changes do not start playback.

Use simulated runtime/test seams where practical; do not require physical audio.

# Stage C — effective Windows voice identity and cache correctness

## C1 — resolve actual effective voice

When Windows backend is selected:

- configured voice ID -> validate/resolve that voice;
- no configured ID -> resolve the current Windows default voice;
- missing configured ID -> actionable error.

Expose enough backend state so synthesis and cache identity use the **same resolved voice**.

Do not synthesize with one voice while hashing another identity.

## C2 — cache identity

Replace the ambiguous implicit identity `windows:default` with the actual effective Windows voice ID.

Cache identity must remain:

- backend-specific;
- voice/model-specific;
- normalized-sentence-specific;
- independent of playback-only speed and volume.

Add regression coverage that two distinct effective Windows voice identities cannot share the same sentence cache key.

If cache-key helper visibility must be refactored for testing, keep it narrowly scoped.

## C3 — OS default changes

Document/test the intended behavior:

- when the OS default voice changes and no explicit voice is configured, a new resolved voice ID produces a different cache identity;
- old cached WAVs may remain on disk but must not be reused under the new effective voice.

# Stage D — backend ownership cleanup

## D1 — Piper-only setup

Only initialize/sanitize/set Piper/eSpeak environment state when `TtsBackend::Piper` is selected.

Windows backend initialization must not depend on Piper paths.

## D2 — diagnostics

Remove misleading hardcoded Piper wording from backend-neutral runtime errors.

Logs/errors should identify the actual backend when useful.

Preserve `tracing`.

## D3 — egui modularity

Do not grow `crates/lanternleaf-egui/src/app/mod.rs` further for this work.

Extract the Windows voice catalog/settings helpers into a coherent module if that can be done without broad UI churn.

A small targeted extraction is authorized.

# Stage E — stronger Windows synthesis evidence

## E1 — decode synthesized WAV

Strengthen the Windows TTS integration probe so successful synthesis proves the file is readable by the same WAV/audio decoder assumptions used by LanternLeaf shared playback.

At minimum verify:

- file exists;
- nontrivial content;
- decoder opens it;
- sane channel/sample-rate metadata or nonzero duration/samples.

Do not require physical speaker output.

## E2 — hosted voice/synthesis observability

In Windows CI add a targeted probe step with uncaptured diagnostics, e.g. an appropriate `--nocapture` invocation.

The CI log/report must explicitly reveal:

- number of installed voices;
- whether a sentence was actually synthesized;
- if skipped, that the runner had no usable Windows voice.

Do not fail the whole product baseline solely because a hosted image contains no speech voices if deterministic API/unit coverage remains valid.

## E3 — preserve green required baseline

Required hosted Windows:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
```

must remain green.

Renderer capability remains a separate probe.

# Stage F — watcher terminal-state verification

Before final handoff:

- move Goal 0005 to `done/` or `blocked/` only at its real terminal state;
- ensure the watcher receives/derives that terminal state before checkout restoration can erase the working-tree view;
- record notification/watcher result in the report;
- notification failure does not change DONE/BLOCKED product status.

If practical, leave a small machine-readable watcher diagnostic/ack in a temporary/log location that is not committed as product state.

# Acceptance gates

DONE requires:

1. active backend switch resynchronizes TTS correctly;
2. active Windows voice switch resynchronizes TTS correctly;
3. canonical sentence/cursor ownership is preserved;
4. stale old-backend request/audio cannot silently continue after switch;
5. implicit Windows default resolves to an actual voice ID for cache identity;
6. configured missing voice remains actionable;
7. Piper-specific initialization no longer runs for Windows backend;
8. backend-neutral diagnostics no longer falsely call every engine Piper;
9. synthesized Windows WAV is explicitly decoder-readable;
10. hosted CI exposes voice-count/synthesis-vs-skip evidence;
11. required Windows check/build/normal-parallel tests remain green;
12. `scripts/codex-goal-notify.ps1` exists and is tested;
13. Codex automatically launches the watcher without a human command;
14. watcher detects both done and blocked exactly once;
15. watcher cannot routinely miss completion merely because Codex restores checkout to main;
16. notification failures are non-fatal;
17. Goal 0005 report/handoff is complete.

# Non-goals

Do not:

- redesign the TTS playback/session model;
- add Linux-native TTS;
- replace Piper;
- redesign the whole egui settings UI;
- begin broad non-PDF parity work;
- redesign PDF rendering/sync;
- change the egui renderer for GitHub Actions;
- require the human to start the watcher;
- introduce cloud notification infrastructure;
- add a background service/daemon that persists beyond the goal watcher lifecycle;
- make notification delivery a product correctness gate.

# Validation

At minimum:

```powershell
cargo check --workspace
cargo build --workspace
cargo test --workspace
git diff --check
```

Also run:

- targeted TTS runtime resync tests;
- targeted cache identity tests;
- Windows TTS probe with visible diagnostics;
- watcher deterministic test mode/probes for done and blocked.

Do not use global test single-threading.

# Repository handoff

Branch:

`codex/0005-tts-correctness-and-goal-notify`

Report:

`docs/work/reports/0005.md`

Required report sections:

## Result
## Goal Notification Watcher
## Watcher Reliability / Checkout Restoration
## Backend Resynchronization
## Windows Voice Identity / Cache
## Piper Isolation
## Windows WAV Decode Evidence
## Hosted Windows Voice Evidence
## Validation
## CI
## Remaining Human Runtime Verification
## Recommended Next Goal
## Git

Return the shared checkout to clean `main` after pushing.

# Human verification

Do not require the human to do anything during this goal.

The human's only routine action should remain starting the Codex Goal once.

If a real speaker/UI interaction is still needed after Goal 0005, record a minimal later verification scenario; the director will decide when to ask for it.

# Stop / escalation conditions

Stop and mark BLOCKED only if:

- safely resynchronizing backend/voice requires changing canonical reader/session ownership;
- resolving the effective Windows default voice cannot be made stable with WinRT without a new architecture decision;
- Windows speech output is incompatible with the shared decoder path;
- a reliable detached notification cannot be implemented without installing persistent system software or requiring human configuration;
- an unrelated product defect outside this bounded goal blocks required Windows tests.

Do not stop merely because Windows toast delivery is unavailable in the current shell/session. Prove the watcher logic and classify notification delivery as best-effort UX.

# Roles and Repository Workflow

The Git repository is the durable communication channel between the human maintainer, ChatGPT, and Codex. Important decisions, goals, reports, reviews, and evidence must survive outside chat history.

## Human maintainer

Owns:

- deciding when to run a goal;
- maintaining/pulling the local checkout;
- launching Codex locally;
- Windows/GUI/audio/device testing that cannot be performed remotely;
- reporting observed runtime behavior when asked.

Does not normally own:

- carrying architecture instructions from ChatGPT to Codex;
- carrying Codex patches/reports back to ChatGPT;
- reviewing diffs;
- merging implementation branches;
- maintaining philosophy/roadmaps;
- manually starting a goal-completion watcher.

The desired initial human loop is short:

`pull accepted main -> start one Codex Goal session -> give the standard one-line invocation -> leave it alone -> receive a terminal desktop notification after the branch is pushed -> tell ChatGPT the attempt finished/blocked`.

If director review requires correction after that Codex Goal session has terminated:

`pull correction on main -> start a fresh Codex Goal session -> continue the SAME repository goal ID/branch/report lineage -> receive the next attempt notification -> tell ChatGPT`.

A new Codex Goal session does **not** imply a new repository macro-goal number.

## Repo-native local execution

Ordinary human development/testing must stay inside the repository checkout.

Canonical Windows entrypoints:

- `Scoopfile.json` — Scoop-native Windows CLI dependency declaration.
- `.\deps.ps1` — idempotent bootstrap that installs Scoop if necessary, imports the Scoopfile, then verifies Rust/MSVC.
- `.\qa.ps1` — prepares isolated QA state/fixtures, enters the MSVC environment, builds, and launches LanternLeaf.

The human should not have to download CI artifacts, unpack alternate distributions, manually enter a Visual Studio shell, or remember separate dependency commands to perform development/manual testing. Generated CI/agent payload handoff is prohibited for this human workflow, not merely discouraged.

CI artifacts may remain as machine evidence or release/distribution outputs, but they are **not** a fallback human testing surface. Do not ask the human maintainer to download/unpack/run generated CI or agent payloads for development/manual QA.

## ChatGPT / director, architect, integrator

Owns:

- `AGENTS.md`;
- `ARCHITECTURE.md`;
- `docs/project/*`;
- active roadmap strategy/order;
- macro-goal definitions;
- architecture boundaries and non-goals;
- direct review of Codex branches/reports through GitHub;
- accepting/rejecting/revising implementation;
- integrating accepted work into `main`;
- updating verified current status after review.

ChatGPT may edit governance/architecture/roadmap documents directly. Broad code implementation should normally be delegated through an authorized macro-goal.

## Codex / implementation worker

Owns:

- executing the active macro-goal;
- making directly-required implementation changes across the explicitly authorized subtracks;
- running validation after each meaningful repair pass;
- adding relevant regression tests;
- writing a durable report;
- pushing its branch;
- returning the shared checkout to `main`;
- automatically launching the repository-owned terminal goal watcher on Windows once that watcher exists.

Codex does not own project philosophy, product scope, priority ordering, or cross-subsystem architecture unless the macro-goal explicitly delegates a bounded design decision.

## Macro-goals instead of tiny prompt loops

The prior one-small-prompt-per-pass workflow created too much human/agent turnaround.

LanternLeaf uses **macro-goals**.

A macro-goal is a durable semantic work unit, not a Codex Goal UI session. One macro-goal may span multiple worker sessions and director correction attempts.

A macro-goal:

- has one coherent outcome;
- may contain multiple passes;
- may contain several independent subtracks;
- authorizes Codex to diagnose -> implement -> test -> fix -> retest repeatedly without requesting a new prompt for every directly-related failure;
- ends only at an acceptance gate or an architectural blocker.

Example:

`Windows recovery baseline` may legitimately include dependency-chain diagnosis, build-system repair, additional Windows compile blockers, launch bootstrap fixes, tests, and CI hardening in one goal.

It must not silently expand into Windows TTS feature implementation or PDF redesign if those are explicit non-goals.

## Work tree

```text
docs/
  project/
  roadmaps/
  work/
    README.md
    queued/
    ready/
    active/
    blocked/
    done/
    reports/
    reviews/
```

Only the goal in `ready/` is authorized to begin.

## Goal lifecycle

1. ChatGPT writes/updates project docs and the next macro-goal on `main`.
2. Human pulls `main`, starts one Codex Goal session for that numbered repository macro-goal, and gives the standard repository invocation.
3. Codex moves `ready -> active`, creates `codex/<id>-<slug>`, and launches the detached Windows goal watcher for execution attempt A1.
4. Codex executes all authorized passes without requiring a new prompt for ordinary retries/fixes.
5. Codex writes `reports/<id>.md`, moves the goal to `done` or `blocked`, commits, and pushes the terminal branch.
6. After the push succeeds, Codex calls `scripts/signal-codex-goal-terminal.ps1` for that goal/state.
7. The detached watcher emits one terminal Windows notification for A1, acknowledges the signal, and exits; notification failure is non-fatal.
8. Codex restores the shared checkout to `main`.
9. ChatGPT reviews the already-pushed branch/report directly.
10. If accepted, ChatGPT integrates the candidate, updates current status/roadmaps, and queues the next repository goal.
11. If revision/correctable rejection is required, ChatGPT writes `reviews/<id>.md`, reopens the **same** goal to `ready/`, and preserves the goal ID plus branch/report lineage.
12. Because the prior Codex Goal session has terminalized, the human pulls the correction and starts a **fresh Codex Goal session** for the same repository goal.
13. The correction session reads current `main` governance/review, continues the existing branch, launches `scripts/codex-goal-notify.ps1 -Rearm`, executes attempt A2, appends report history, and terminalizes/pushes again.
14. Return to director review. Repeat attempts until accepted, blocked on a true unresolved decision, or the goal is explicitly abandoned/superseded.
15. Human runs runtime verification only when requested.

The post-push `.git/lanternleaf-goal-state/<id>.terminal` sentinel is the normal notification trigger. This makes the alert mean that the implementation is already remotely inspectable and prevents checkout restoration from racing the watcher.

Notification is best-effort workflow UX. It must not alter whether a product goal passes or fails.

`done` is terminal for the current execution attempt, not necessarily an absorbing terminal state for the repository macro-goal. Director correction may cycle the same goal `done -> ready -> active` in a new Codex Goal session.

## Parallelism inside a goal

Multiple things may be worked in the same goal when they support one outcome and have explicit boundaries.

Good parallel/subtrack combinations:

- build-toolchain diagnosis + codebase status inventory;
- Windows build repair + Windows CI update;
- separate unit tests for independent modules.

Bad combinations:

- Windows build recovery + new Windows TTS + PDF algorithm redesign;
- unrelated cleanup merely because files are nearby.

The director chooses these boundaries.

## Branch safety

Codex owns the temporary branch switch in the shared checkout:

`main -> codex/<goal> -> main`

Do not use destructive reset/clean/stash to force branch changes over unknown human work.

After pushing, Codex must verify the final branch is `main`.

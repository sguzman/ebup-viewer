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
- maintaining philosophy/roadmaps.

The desired human loop is short: pull accepted `main`, launch the authorized repo goal, perform any requested local runtime test, report the observation.

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
- returning the shared checkout to `main`.

Codex does not own project philosophy, product scope, priority ordering, or cross-subsystem architecture unless the macro-goal explicitly delegates a bounded design decision.

## Macro-goals instead of tiny prompt loops

The prior one-small-prompt-per-pass workflow created too much human/agent turnaround.

LanternLeaf uses **macro-goals**.

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
2. Human pulls `main` and launches Codex against the repository.
3. Codex moves `ready -> active`, creates `codex/<id>-<slug>`, and executes all authorized passes.
4. Codex writes `reports/<id>.md`, moves the goal to `done` or `blocked`, commits/pushes, and restores the checkout to `main`.
5. ChatGPT reviews the pushed branch/report directly.
6. ChatGPT may write `reviews/<id>.md`, request a bounded correction, or integrate accepted commits.
7. ChatGPT updates current status/roadmaps and queues the next goal.
8. Human pulls accepted `main` and runs runtime verification only when requested.

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

# LanternLeaf Work Queue

This directory is the live execution queue shared by ChatGPT/director, Codex, and the human maintainer.

The repository is the agent-to-agent communication channel. Normal implementation instructions and reports should not depend on chat copy/paste.

## States

- `queued/`: future director-defined macro-goals.
- `ready/`: the single goal authorized to start.
- `active/`: goal currently executing.
- `blocked/`: current execution attempt stopped at a true blocker; the same goal may later be reopened after director resolution.
- `done/`: current execution attempt claims the acceptance criteria are satisfied and is ready for director review; the same goal may be reopened if review requires correction.
- `reports/`: Codex implementation/validation reports.
- `reviews/`: ChatGPT/director review notes or correction contracts.

Keep at most one goal in `ready/`.

## Goal IDs

Use monotonically increasing four-digit IDs: `0001`, `0002`, etc.

The same ID is used for:

- goal filename;
- Codex branch lineage;
- report lineage;
- review notes.

The ID names the **repository macro-goal**, not a Codex Goal UI session. One repository goal may require multiple fresh Codex Goal sessions after director review. Do not increment the goal ID merely because a worker session terminated.

## Macro-goal design

A goal is intentionally larger than the old single-pass task packet.

Use a macro-goal when several directly-related passes can be completed without a new architecture decision.

A good macro-goal specifies:

- desired end state;
- current evidence/failure;
- authorized subtracks/passes;
- hard non-goals;
- constraints;
- acceptance gates;
- validation;
- branch/handoff protocol;
- required human verification, if any.

Codex is expected to iterate within the goal. For example, after repairing the first build blocker it should continue to the next Windows build blocker if that remains inside the stated recovery goal.

A macro-goal may also span multiple **review-separated attempts**. If Codex terminalizes and ChatGPT later issues a bounded correction, ChatGPT reopens the same goal to `ready/`; the human starts a fresh Codex Goal session; Codex continues the same goal lineage.

## Default goal template

```markdown
# 0000 — Goal title

## Outcome

## Why now

## Starting evidence

## Authorized passes / subtracks

## Non-goals

## Constraints

## Acceptance gates

## Validation

## Repository handoff

## Human verification

## Stop / escalation conditions
```

## Handoff

Unless overridden by the goal, Codex:

1. fast-forwards `main`;
2. creates/switches to `codex/<id>-<slug>`;
3. moves the goal `ready -> active`;
4. on Windows, launches `scripts/codex-goal-notify.ps1` as a detached process for the goal ID;
5. performs all authorized passes;
6. validates;
7. writes `reports/<id>.md`;
8. moves the goal to `done` or `blocked`;
9. commits and pushes the terminal branch;
10. after push succeeds, calls `scripts/signal-codex-goal-terminal.ps1 -GoalId <id> -RepoRoot <repo> -State <done|blocked>`;
11. restores the shared checkout to `main` without merging.

The signal helper writes a checkout-safe terminal sentinel under `.git/lanternleaf-goal-state/` and waits briefly for the watcher acknowledgment. The watcher emits exactly one terminal notification and exits. Missing toast/ack is non-fatal.

ChatGPT reviews and integrates directly from GitHub.
## Human invocation

The preferred human instruction to Codex is short:

> Read AGENTS.md and execute the single macro-goal in docs/work/ready/. Continue through all authorized passes until its acceptance gates pass or its escalation rules require stopping.

The goal itself carries the implementation contract.

The human does **not** separately launch the goal watcher.

## Director correction continuation

When ChatGPT reviews a terminal attempt and requests bounded corrections while preserving the same semantic goal:

1. keep the same goal ID;
2. keep the same implementation branch and report lineage unless the review explicitly says otherwise;
3. move/reopen the same goal back to `ready/`;
4. commit the correction review on `main`;
5. the human pulls `main` and starts a **fresh Codex Goal session** if the previous session is terminal;
6. Codex reads `AGENTS.md`, the reopened goal, and `docs/work/reviews/<id>.md` before editing;
7. Codex continues the existing branch and synchronizes the current director-owned review/governance state into it safely;
8. Codex launches `scripts/codex-goal-notify.ps1 -Rearm` so prior `.terminal` / `.ack` state for the same goal ID cannot retrigger;
9. Codex executes the correction pass, preserves prior report history, and uses the normal post-push terminal signal again.

The fresh Codex Goal session is a new **execution attempt**, not a new repository macro-goal.

Preferred correction invocation:

> This is a director correction continuation of repository Goal <ID>, not a new macro-goal. Read `AGENTS.md`, the reopened goal in `docs/work/ready/`, and `docs/work/reviews/<ID>.md`. The prior Codex Goal session has terminated, so start this as a fresh Codex Goal session while continuing the existing repository goal branch/report lineage. Synchronize the current director correction from `main`, re-arm the watcher for this attempt, and execute all correction items until acceptance passes or an escalation condition is reached.

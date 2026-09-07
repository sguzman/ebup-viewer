# LanternLeaf Work Queue

This directory is the live execution queue shared by ChatGPT/director, Codex, and the human maintainer.

The repository is the agent-to-agent communication channel. Normal implementation instructions and reports should not depend on chat copy/paste.

## States

- `queued/`: future director-defined macro-goals.
- `ready/`: the single goal authorized to start.
- `active/`: goal currently executing.
- `blocked/`: goal stopped at a true blocker.
- `done/`: acceptance criteria satisfied on the implementation branch.
- `reports/`: Codex implementation/validation reports.
- `reviews/`: ChatGPT/director review notes or correction contracts.

Keep at most one goal in `ready/`.

## Goal IDs

Use monotonically increasing four-digit IDs: `0001`, `0002`, etc.

The same ID is used for:

- goal filename;
- Codex branch;
- report;
- review notes.

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
4. performs all authorized passes;
5. validates;
6. writes `reports/<id>.md`;
7. moves the goal to `done` or `blocked`;
8. commits/pushes;
9. switches the shared checkout back to `main` without merging.

ChatGPT reviews and integrates directly from GitHub.

## Human invocation

The preferred human instruction to Codex is short:

> Read AGENTS.md and execute the single macro-goal in docs/work/ready/. Continue through all authorized passes until its acceptance gates pass or its escalation rules require stopping.

The goal itself carries the implementation contract.

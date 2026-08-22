# Grill-Me Decision 34: Adaptive Home Dashboard

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai uses an adaptive home page composed of an `Action Center` and a `Research Pulse`.

The home page prioritizes explicit human decisions when they exist; otherwise it summarizes autonomous research progress. It does not expose the internal mission queue as the primary product surface and does not leave the user with an empty approval-only dashboard.

## Information hierarchy

### 1. Action Center

The first section appears only when human action is required. It may contain:

- pending `PAPER_HANDOFF_APPROVAL` or `LIVE_HANDOFF_APPROVAL` decisions;
- approvals approaching expiry;
- an Idea Draft requiring completion of the one allowed clarification round;
- a low-frequency administration or incident item that cannot be resolved automatically.

Candidate approvals always rank above informational notices. Every action card states:

- the exact object requiring a decision;
- why human action is required;
- the deadline or validity window;
- the consequence of approving, rejecting or deferring;
- whether the item is part of the normal idea/approval workflow or an administration exception.

### 2. Research Pulse

When no human decision is pending, the home page emphasizes a concise system-wide research summary, including:

- active, cooling, paused and blocked Research Programs;
- Programs waiting for downstream feedback;
- current Codex Missions at a highly aggregated level;
- newly qualified Alpha Library entries;
- Portfolio Programs approaching promotion readiness;
- recent evidence-bearing progress;
- recent transitions into Cooling and their wake conditions.

The Pulse reports material progress, not raw activity counts. It should prefer statements such as:

```text
Liquidity Shock Program produced a PRIMARY_ALPHA library candidate.
Portfolio Program P-12 eliminated two redundant skeleton families.
Short-Horizon Trend Program entered Cooling while waiting for new market data.
```

It should not treat token usage, command count, file edits or trial volume as product progress.

### 3. Primary actions

The persistent primary business actions are:

```text
Propose a new idea
Review pending approvals
```

Other navigation remains available through the main product areas:

- Research Observatory;
- Alpha Library;
- Portfolio Lab;
- Handoff and Feedback views;
- Administration.

### 4. Operational notices

Operational warnings appear only when they are actionable or materially affect research or handoff, for example:

- Codex App Server unavailable;
- Sealed Evaluator blocked;
- downstream feedback contract stale;
- critical storage pressure;
- Candidate Package publication failure.

Ordinary job retries, transient tool failures and low-level internal events remain in the relevant Level 2 or Level 3 observability views rather than the home page.

## Ranking rules

The home page uses a deterministic product priority order:

1. expiring or pending capital handoff approvals;
2. Idea clarification awaiting the operator;
3. critical administration or evidence-integrity incidents;
4. material Research Pulse changes;
5. informational summaries.

The ranking must not be generated solely by an LLM. Domain states and versioned notification policies determine eligibility and severity; natural-language summaries may be generated from already selected facts.

## Empty and quiet states

When there is no pending action and no recent material progress, the home page should communicate that the autonomous system is operating normally, for example:

```text
No action is required.
Four Programs are cooling while waiting for new evidence; two Programs remain active.
```

A quiet system is not presented as failed or inactive when Programs are correctly waiting for evidence.

## Product constraints

- The home page must not become a chat transcript, mission queue, experiment table or raw event stream.
- Approval throttling and the Material Improvement Gate continue to control what enters the Action Center.
- Research Pulse remains read-only and does not introduce new routine user controls.
- Pause, resume, archive, restore and administration actions remain low-frequency management operations outside the two recurring product responsibilities.
- No model hidden reasoning, secrets, tokens or unredacted runtime data is shown.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

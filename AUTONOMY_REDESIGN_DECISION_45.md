# Grill-Me Decision 45: Immutable Portfolio Candidate Lifecycle

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and DESIGN rewrite is complete.

## Selected product behavior

Portfolio Candidates are immutable. Any material change creates a new Portfolio Candidate version and must pass the applicable evaluation and approval lifecycle again.

QuaZonai never silently upgrades an approved or handed-off Portfolio Candidate.

## Candidate mutation rules

The following changes always require a new Candidate:

- Alpha qualification version changes;
- Shadow Alpha promotion into qualified Alpha;
- Alpha removal or addition;
- target weights or weight-generation rules changes;
- Portfolio Policy changes;
- Mandate version changes;
- Risk Model changes;
- Cost Model changes;
- Capacity Model changes;
- Constraint Set changes;
- Rebalance policy changes;
- Candidate Package contract changes.

The previous Candidate remains a permanent historical object with its original evidence, approval and handoff lineage.

## Lifecycle

```text
Portfolio Research
      ↓
Portfolio Candidate v1
      ↓
Evaluation
      ↓
Approval
      ↓
Candidate Package
      ↓
Handoff
```

An improvement creates:

```text
Portfolio Candidate v2
      ↓
Independent evaluation as required
      ↓
New approval
      ↓
New handoff
```

## No in-place optimization

QuaZonai must not:

- automatically modify a live-approved candidate;
- adjust weights because a new Alpha appears;
- replace a Mandate version silently;
- let Codex optimize an already approved portfolio directly;
- treat small numerical changes as operationally meaningless without evaluation.

A "small" portfolio change can alter risk, cost, capacity, turnover and downstream behavior, so change impact is determined through versioned policy rather than informal thresholds.

## Downstream boundary

A new Candidate Package is required before any downstream system receives a changed portfolio definition.

QuaZonai does not update, patch or mutate an already claimed downstream package.

Existing downstream runtime state remains owned by the Consumer system.

## Product behavior

The user only receives a new approval when the new Candidate passes the global Approval Throttling and Material Improvement policies.

Historical candidates remain available in Research Observatory for comparison, attribution and postmortem analysis.

No candidate lifecycle transition uses application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gates.

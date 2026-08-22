# Grill-Me Decision 43: Alpha Qualification Lifecycle

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and DESIGN.md rewrite is complete.

## Selected product behavior

Alpha Library qualifications are immutable research assets. A qualification that becomes invalid, degraded or quarantined is never restored in place. A future recovery requires a new research evaluation and a new Alpha Library Qualification Version.

## Lifecycle

```text
ACTIVE
  ↓
WATCH
  ↓
QUARANTINED
  ↓
RETIRED
```

Recovery does not transition an old qualification back to ACTIVE.

Example:

```text
Alpha Model A7
Calibration v3
Qualification v5

status:
QUARANTINED

new research
↓
Alpha Model A7 revision
Calibration v4
Qualification v6

status:
ACTIVE
```

The old qualification remains historical evidence.

## Reasons

This preserves:

- immutable Alpha lineage;
- Search Ledger continuity;
- Portfolio Candidate reproducibility;
- Evidence Exposure history;
- accurate attribution of why a previous qualification failed.

A market regime change may justify a new qualification, but it does not rewrite the historical conclusion that the previous qualification was invalid under previous conditions.

## New qualification requirements

A new qualification must pass the normal lifecycle gates:

- Research evidence;
- Calibration validation;
- applicable Standalone Quality Gate or Portfolio Contribution Gate;
- Search-adjusted evidence review;
- validity conditions;
- downstream Portfolio Assembly checks when relevant.

The new qualification may supersede the old one for future Portfolio Programs only after independent evaluation and library admission.

## Portfolio impact

Existing Portfolio Candidates continue referencing the exact Alpha Qualification Version they were approved with.

A new Alpha Qualification Version does not silently replace:

- existing Portfolio Candidates;
- Candidate Packages;
- Approvals;
- Handoff Offers;
- downstream deployments.

A new Portfolio Program or Candidate evaluation is required if the new qualification is considered for future use.

## Product boundaries

- Users do not manually restore Alpha qualifications.
- Codex cannot mark a quarantined Alpha as active without producing a new qualification candidate and evidence.
- Alpha lifecycle changes are research facts, not execution commands.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

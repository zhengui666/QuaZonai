# Grill-Me Decision 32: Approval Staleness and Expiry

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai approvals are immutable decision snapshots with two independent validity controls:

1. dependency-driven invalidation when a material input changes; and
2. a policy-defined maximum validity period even when no explicit invalidating event is observed.

An approval that becomes stale or expires cannot be revived, extended or approved retroactively. QuaZonai must evaluate current facts and create a new approval snapshot when the candidate remains eligible.

## Frozen approval dependencies

Every Paper or Live approval freezes at least:

- `candidate_package_version_id`;
- approval purpose: `PAPER_HANDOFF` or `LIVE_HANDOFF`;
- evaluation evidence set and disclosure report version;
- Alpha Model, Calibration and Portfolio Policy versions;
- Risk, Cost, Capacity and Constraint Set versions;
- Candidate validity and degradation conditions;
- selected downstream system and downstream connection version;
- Candidate Package and Feedback Contract versions;
- compatibility preflight result;
- creation time, maximum validity policy and calculated expiry time.

The approval snapshot is immutable. A material dependency change requires a new snapshot rather than an in-place edit.

## Approval lifecycle

```text
PENDING
  → APPROVED
  → REJECTED
  → STALE
  → EXPIRED
```

`APPROVED`, `REJECTED`, `STALE` and `EXPIRED` are terminal states for that snapshot.

### STALE

An approval becomes `STALE` immediately when a material dependency changes or becomes invalid, including:

- the Candidate Package is superseded, withdrawn or invalidated;
- a constituent Alpha qualification becomes `QUARANTINED`;
- a required Calibration or model validity condition fails;
- new Forward Evidence materially contradicts the approval basis;
- the candidate's market or regime validity conditions no longer hold;
- the selected downstream connection or contract version changes materially;
- the compatibility preflight is no longer valid;
- a Risk, Cost, Capacity, Constraint or Portfolio Policy dependency changes materially;
- any relied-upon Evaluation Episode or evidence record is invalidated.

`STALE` means a known fact has changed. The user cannot approve a stale snapshot.

### EXPIRED

An approval becomes `EXPIRED` when its maximum validity period elapses without a specific stale event. The validity duration is determined by a versioned policy using factors such as:

- approval purpose;
- Alpha and Portfolio horizon;
- expected market regime persistence;
- data refresh cadence;
- evidence freshness requirements;
- downstream preflight validity.

The user does not choose the duration manually and cannot select a permanent approval.

## Regeneration

When a pending approval becomes stale or expires, QuaZonai must:

1. re-evaluate current candidate, evidence and downstream compatibility facts;
2. run any required fresh independent evaluation or preflight;
3. reapply the Material Improvement Gate and global approval throttling;
4. create a new immutable approval snapshot only if the candidate still qualifies.

The system must not duplicate the old snapshot and merely refresh its timestamps.

## Approved handoff claim window

Approval and handoff availability are separate facts:

```text
Approval APPROVED
→ Handoff Offer AVAILABLE
→ CLAIMED before claim_deadline
```

If the selected downstream does not claim the offer before the claim deadline:

```text
AVAILABLE → EXPIRED
```

The approval decision remains a durable historical fact, but a new publication attempt must re-check whether its approval basis remains current. If any dependency or validity policy has changed, a new approval is required.

## User experience

The Approval Inbox must show:

- `valid_until`;
- current freshness state;
- any material dependency that made the approval stale;
- whether a new evaluation or compatibility preflight is required;
- the exact consequence of approving the current snapshot.

A stale or expired approval is read-only and remains visible in history. It is never silently removed.

## Downstream isolation

- Approval expiry does not stop or revoke a Candidate Package already claimed by a downstream system.
- QuaZonai does not infer downstream runtime state from approval validity.
- A separate product rule governs revocation of an unclaimed Handoff Offer.

## Evidence and identity rules

Approval validity uses explicit business IDs, versions, states, temporal rules and field-level dependency comparisons.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.
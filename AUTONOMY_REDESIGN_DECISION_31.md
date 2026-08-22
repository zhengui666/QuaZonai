# Grill-Me Decision 31: Downstream Feedback Contract and Staleness

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai manages downstream Paper and Live feedback through a versioned `FeedbackContract`. Missing, late, partial or invalid feedback is represented as an operational and evidence-quality problem; it is never silently interpreted as candidate failure.

A Candidate cannot advance from Paper evidence to a Live handoff approval until a complete, valid and contract-conforming Paper feedback package has been imported and independently evaluated.

## Feedback contract

Every approved Handoff Offer freezes a concrete feedback contract containing at least:

- `feedback_contract_version_id`;
- handoff purpose: `PAPER` or `LIVE`;
- downstream system and downstream connection version;
- minimum observation duration;
- minimum valid sample size;
- required market, target and realized portfolio fields;
- required cost, turnover, capacity and drawdown fields;
- required runtime incident and data-quality fields;
- expected first status-report deadline;
- expected complete-feedback deadline;
- grace period;
- accepted Arrow and package contract versions;
- disclosure and retention policy.

The contract is part of the immutable Approval and Handoff snapshot. It cannot be weakened or replaced after approval. A material contract change requires a new Approval.

## Feedback lifecycle

Normal states:

```text
FEEDBACK_PENDING
→ FEEDBACK_IN_PROGRESS
→ FEEDBACK_PARTIAL
→ FEEDBACK_COMPLETE
```

Exceptional states:

```text
FEEDBACK_STALE
FEEDBACK_INCOMPLETE
FEEDBACK_INVALID
CONSUMER_UNREACHABLE
```

Semantics:

- `FEEDBACK_PENDING`: the downstream accepted the package but has not yet supplied evidence;
- `FEEDBACK_IN_PROGRESS`: the downstream is reporting progress but has not satisfied the contract;
- `FEEDBACK_PARTIAL`: some valid evidence was received, but duration, sample size or required fields remain incomplete;
- `FEEDBACK_COMPLETE`: all contract requirements have been met and the package is eligible for Forward Evidence evaluation;
- `FEEDBACK_STALE`: a contractual reporting milestone or completion deadline has passed;
- `FEEDBACK_INCOMPLETE`: the downstream declared the observation complete but omitted required evidence;
- `FEEDBACK_INVALID`: schema, provenance, time ordering or contract validation failed;
- `CONSUMER_UNREACHABLE`: QuaZonai cannot communicate with the configured downstream feedback endpoint or registry identity.

## Research and promotion consequences

- Missing or stale feedback is not a negative Alpha or Portfolio result.
- Partial feedback is not admitted as complete Forward Evidence.
- No `LIVE_HANDOFF_APPROVAL` may be generated without complete, valid Paper feedback satisfying the approved contract.
- A related Research Program may continue exploring other Branches while one Candidate waits for feedback.
- The affected Candidate family remains blocked at the relevant promotion stage.
- Late feedback may be accepted when it still satisfies the frozen contract and temporal validity rules.
- Feedback arriving after the Candidate, data contract or market assumptions become obsolete may be retained for postmortem evidence but not used for promotion.
- Infrastructure and connectivity failures are recorded as downstream operational evidence, not model-quality evidence.

## User experience

The normal user journey does not add a new approval or research-management step. Research Observatory and Handoff views present a clear status such as:

```text
Paper feedback is overdue.

The candidate has not been classified as failed.
Live promotion remains blocked because the configured downstream system has not yet met the approved feedback contract.
```

Ordinary users are not asked to repair the downstream. Administration surfaces provide diagnostics for:

- consumer reachability;
- latest accepted status report;
- missing contract fields;
- schema or contract incompatibility;
- deadline and grace-period status.

## Downstream isolation

- QuaZonai does not start, stop, restart or inspect the downstream runtime.
- A stale feedback state does not cause QuaZonai to redirect the package to another downstream system.
- Delivering the same Candidate Package to another downstream requires a separate Approval bound to that system.
- QuaZonai may revoke only an unclaimed Handoff Offer according to a separately defined product rule; it does not revoke or stop an already accepted downstream runtime.

## Evidence integrity

- Only validated, contract-complete feedback becomes a `ForwardEvidenceEpisode`.
- Partial and invalid packages remain auditable but cannot satisfy promotion gates.
- Feedback acceptance uses explicit IDs, business versions, schemas, byte counts, temporal checks and field-level validation.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

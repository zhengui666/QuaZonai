# Grill-Me Decision 47: Degradation Policy Driven Wake-up

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai uses a versioned Degradation Policy to decide when observed Alpha or Portfolio health degradation should wake autonomous research. A degradation signal does not automatically create a Mission unless it satisfies policy thresholds.

## Principle

Degradation Monitoring detects research validity concerns. It does not control downstream execution.

```text
Forward Evidence / Downstream Feedback
        ↓
Degradation Monitoring
        ↓
Degradation Policy
        ↓
(optional) Research Program Wake-up
        ↓
New Mission
        ↓
New Candidate
        ↓
Evaluation and Approval
```

## Degradation Policy inputs

A policy evaluates factors including:

- persistence duration;
- severity level;
- statistical confidence;
- impact on Portfolio Mandate objectives;
- number of affected Forward Evidence episodes;
- whether multiple independent signals indicate the same issue;
- whether a researchable hypothesis exists;
- whether the issue is operational rather than model-related.

## Behavior by severity

Examples:

```text
WATCH
→ continue observation
→ no Mission created

DEGRADING + sufficient evidence
→ wake related Research Program
→ create diagnostic Mission

INVALIDATED
→ trigger qualification/candidate reassessment workflow
```

## Restrictions

QuaZonai must not:

- start a Mission for every short-term performance fluctuation;
- treat normal market noise as automatic Alpha failure;
- let Codex independently decide whether degradation is meaningful;
- automatically replace a live Portfolio Candidate;
- alter downstream positions, orders or runtime state.

## User experience

Research Observatory may display:

```text
Portfolio Candidate P-184 shows degradation signals.
The system is monitoring until the Degradation Policy threshold is met.
No user action is required.
```

When policy conditions are satisfied:

```text
Research resumed automatically.
A diagnostic Mission was created to investigate the degradation source.
```

## Evidence handling

Degradation events, policy decisions and resulting Missions are recorded in lineage and Search Ledger records.

A degradation event is evidence requiring investigation; it is not itself proof that the original research was invalid.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

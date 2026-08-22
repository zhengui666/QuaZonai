# Grill-Me Decision 44: Shadow Alpha Lifecycle

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai introduces `SHADOW_ALPHA` as a first-class Alpha Library lifecycle state. Shadow Alpha preserves potentially valuable research assets that do not yet prove standalone predictive value but may have measurable portfolio contribution.

## Lifecycle

```text
RESEARCH_ONLY
    ↓
SHADOW_ALPHA
    ↓
QUALIFIED_ALPHA
    ↓
ACTIVE_LIBRARY
```

Shadow Alpha may also move back to research-only historical states or be retired according to normal immutable qualification rules.

## Purpose

A Shadow Alpha exists because:

- standalone performance is not the only source of portfolio value;
- diversification, hedging and regime information may be valuable despite weak standalone metrics;
- early removal would prevent Portfolio Assembly from discovering valid complementary assets.

## Allowed usage

Shadow Alpha may:

- participate in Portfolio Assembly research;
- be evaluated as `DIVERSIFIER_ALPHA`, `HEDGE_ALPHA`, `REGIME_SIGNAL` or `RISK_MODULATOR` candidates;
- undergo marginal contribution testing;
- generate new Alpha Qualification candidates.

Shadow Alpha may not:

- directly enter Paper or Live Handoff;
- be represented as a proven standalone Alpha;
- bypass Alpha Qualification Gates;
- silently replace an Active Library Alpha version.

## Qualification transition

Example:

```text
Alpha X

Standalone Quality Gate:
FAIL

Portfolio Contribution Gate:
PASS

New Qualification:
DIVERSIFIER_ALPHA
```

The transition creates a new immutable Alpha Qualification Version. It does not rewrite the previous research conclusion.

## Evidence and search discipline

Shadow Alpha participation in Portfolio research is fully recorded through:

- Portfolio Search Ledger;
- Evidence Exposure Graph;
- Qualification lineage;
- Portfolio-level evaluation episodes.

A large number of Shadow Alpha experiments cannot be used to bypass multiple-testing controls.

## Product boundaries

- Shadow Alpha is a research asset state, not a hidden production state.
- Codex may propose Shadow Alpha candidates but cannot promote them without the required evaluation.
- Users do not manually mark Alpha as Shadow or Active during normal workflows.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

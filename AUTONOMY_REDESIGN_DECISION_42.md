# Grill-Me Decision 42: Mandate-Triggered Portfolio Program Creation

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai automatically creates Portfolio Programs when an enabled Portfolio Mandate has sufficient qualified Alpha Library assets and a meaningful portfolio construction opportunity exists.

Mandate activation alone does not immediately create portfolio research. The system waits for valid research assets and avoids low-information portfolio searches.

## Trigger conditions

A Portfolio Program creation candidate requires:

- an enabled Portfolio Mandate version;
- eligible Alpha Library assets;
- required Alpha role coverage or a meaningful opportunity gap;
- potential marginal portfolio contribution;
- no existing equivalent Portfolio Program;
- sufficient research evidence for the intended assembly stage;
- no blocking lifecycle state on required dependencies.

Conceptual flow:

```text
Enabled Mandate
      +
Qualified Alpha Library assets
      +
Portfolio opportunity detected
      ↓
Create Portfolio Program
```

## Waiting behavior

A newly enabled Mandate with insufficient Alpha assets remains:

```text
WAITING_FOR_ALPHA
```

This is not a failure. It means the Portfolio layer is waiting for upstream Research outputs.

When new Alpha qualifications enter the Library, QuaZonai may reevaluate whether the Mandate now has sufficient portfolio construction value.

## Portfolio Program creation rules

QuaZonai must not:

- create meaningless Portfolio Programs immediately after every Mandate activation;
- launch unrestricted combinations of every available Alpha;
- bypass Eligibility Snapshot, Role Pooling, Redundancy Analysis or Portfolio Assembly policies;
- allow Codex to create a capital allocation objective outside Mandate rules;
- create duplicate Portfolio Programs with the same Mandate, objective and research scope.

Created Portfolio Programs still follow:

```text
Eligibility Snapshot
→ Role Pooling
→ Redundancy / Common-source Analysis
→ Portfolio Skeleton
→ Policy Research
→ Evaluation
→ Candidate
```

## User experience

The user does not manually create Portfolio Programs during normal operation.

Research Observatory may show:

```text
Core Growth Mandate enabled.
Waiting for sufficient qualified Alpha assets before starting Portfolio research.
```

or:

```text
Core Growth Portfolio Program created.
Three qualified Alpha assets are being evaluated under this mandate.
```

## Product boundaries

- Mandate defines capital objective; it does not directly create executable deployment.
- Research Program remains the source of Alpha assets.
- Portfolio Program remains responsible for combining validated assets under Mandate constraints.
- Candidate Approval remains the only human decision point after research produces a qualified recommendation.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

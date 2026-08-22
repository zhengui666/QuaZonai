# Grill-Me Decision 40: Portfolio Mandate Activation Model

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai creates one default Portfolio Mandate during initial setup and allows additional Mandate templates to be enabled later. Mandates are not automatically created by Codex or by individual Research Ideas.

## Default activation

Initial installation creates:

```text
Core Growth
```

as the default enabled Mandate.

Additional templates may include:

```text
Conservative
Market Neutral
Tail Protection
```

The user can enable, disable and review Mandates through Administration.

## Separation of responsibilities

Research Idea and Codex research do not create capital objectives.

Codex may research:

- Alpha quality;
- Portfolio construction alternatives;
- robustness;
- evidence;
- risk characteristics.

Codex may not decide:

- target risk;
- capital objective;
- drawdown tolerance;
- portfolio mandate constraints.

Those belong to versioned Portfolio Mandates.

## Mandate lifecycle

```text
AVAILABLE_TEMPLATE
→ ENABLED
→ ACTIVE
→ DISABLED
```

Disabling a Mandate does not delete:

- Portfolio Programs;
- Portfolio Candidates;
- Candidate Packages;
- Approval history;
- Handoff records;
- Forward Evidence.

It only prevents future Portfolio research from using that Mandate unless restored.

## User experience

Normal user workflow remains:

```text
Propose Idea
→ autonomous research
→ approve candidate
```

Users do not configure optimization parameters during approval.

Approval displays the selected objective:

```text
Portfolio Mandate: Core Growth v1
Candidate: P-184 v5
Purpose: LIVE_HANDOFF
```

The user approves the candidate under a defined investment objective.

## Product boundaries

- No automatic Mandate creation from user wording.
- No Codex-generated capital objectives.
- No approval-time Mandate mutation.
- No silent replacement of Candidate Mandate versions.
- Mandates define portfolio objectives only; execution systems remain responsible for order and runtime behavior.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

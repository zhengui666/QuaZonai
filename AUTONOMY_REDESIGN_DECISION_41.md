# Grill-Me Decision 41: Research-First Product Information Architecture

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai presents Research as the source of truth and Portfolio as the capital-objective projection of validated research assets.

The product hierarchy is:

```text
Research Program
    ↓
Alpha Library
    ↓
Portfolio Program + Portfolio Mandate
    ↓
Portfolio Candidate
    ↓
Approval / Handoff
```

Research and Portfolio remain separate domains but are visually connected.

## Home and navigation principles

The product should not present Portfolio as an independent replacement for Research. A Portfolio Candidate exists because Research produced validated assets and a Portfolio Mandate defined how those assets should be combined.

Primary views:

- Research Observatory: Alpha, Feature, Calibration, Branch, Mission, Evidence and lineage.
- Alpha Library: qualified research assets and their roles.
- Portfolio Lab: Mandates, Portfolio Programs, Assembly, Candidates and readiness.
- Approval Inbox: actionable Paper and Live decisions.

## User-facing flow

The user understands:

```text
The system researches ideas.
The system qualifies reusable Alpha.
The system builds portfolios under explicit Mandates.
The system asks for approval only when a real handoff decision exists.
```

The UI should show the relationship between research progress and portfolio readiness without requiring the user to manage the intermediate research objects.

## Product constraints

- Portfolio is not the only primary view; doing so would hide the evidence lineage behind recommendations.
- Research Observatory is not an engineering log; low-level Codex activity remains behind observability layers.
- A Portfolio Program cannot exist without its Mandate context.
- Candidate approval cannot bypass the Research lineage, evidence exposure and evaluation history.
- No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

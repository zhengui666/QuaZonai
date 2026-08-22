# Grill-Me Decision 48: Market Universe as a First-Class Domain Object

> Status: working decision record for Draft PR #12. This file is non-authoritative until final PRD rewrite.

## Selected product behavior

QuaZonai models Market Universe as a first-class domain object. Research, Alpha Qualification and Portfolio construction must not assume one global asset universe.

## Universe responsibilities

A Market Universe defines:

- instrument schema;
- trading calendar and session semantics;
- data requirements;
- cost model compatibility;
- capacity assumptions;
- risk model compatibility;
- allowed Alpha roles;
- downstream execution compatibility.

Examples:

```text
US Equities
Crypto Spot
Prediction Markets
US Options
```

## Binding rules

Research Programs bind:

```text
Research Charter + Market Universe
```

Alpha Qualifications bind:

```text
Alpha Version + Universe + Horizon
```

Portfolio Mandates define allowed Universes:

```text
Core Growth
  allowed: US Equities, Crypto Spot
  forbidden: Prediction Markets
```

## Isolation rules

Universe-specific assumptions must not leak across domains:

- cost models;
- capacity assumptions;
- risk models;
- execution constraints;
- data semantics.

A successful Alpha in one Universe is not automatically valid in another Universe.

## Product boundaries

Codex may propose research within the allowed Universe, but must not redefine the Universe boundary. Universe changes require explicit Research Charter or Portfolio Mandate version changes.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

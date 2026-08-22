# Grill-Me Decision 39: Portfolio Mandate Activation Model

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite is complete.

## Selected product behavior

QuaZonai provides a small set of versioned Portfolio Mandate templates. The first installation enables a default Mandate; additional Mandates are enabled explicitly when needed.

Codex does not create capital objectives. A Research Idea does not automatically define a Portfolio Mandate.

## Default activation model

The initial product state includes:

```text
Core Growth
```

Additional templates may include:

```text
Conservative
Market Neutral
Tail Protection
```

A user may enable or disable Mandate templates through Administration. Enabling a Mandate creates the capability context for Portfolio Programs; it does not automatically create approvals or force immediate research runs.

## Mandate activation lifecycle

```text
AVAILABLE_TEMPLATE
→ ENABLED
→ ACTIVE
→ DISABLED
```

Enabled Mandates remain versioned objects. Disabling a Mandate does not delete:

- existing Portfolio Programs;
- Portfolio Candidates;
- Candidate Packages;
- Approval history;
- Handoff records;
- Forward Evidence.

New Portfolio research is not created for disabled Mandates unless explicitly re-enabled.

## User experience

Normal users should not configure complex investment objective parameters during Idea submission or Approval.

The system presents:

```text
Portfolio Mandate: Core Growth v1
```

rather than exposing optimizer-level settings.

Advanced custom Mandates may be created through Administration, but they are versioned product objects and follow the same immutability and evaluation rules as built-in templates.

## Product boundaries

- Codex cannot invent a Mandate from a Research Idea.
- Approval pages cannot edit Mandate objectives or constraints.
- Mandate activation is not an execution instruction.
- Mandate changes create new versions and do not silently alter existing Portfolio Candidates.
- Disabled Mandates do not erase historical research evidence.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

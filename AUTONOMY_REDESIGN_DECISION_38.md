# Grill-Me Decision 38: Portfolio Mandate Model

> Status: working decision record for Draft PR #12. This file is non-authoritative until the final PRD and `DESIGN.md` rewrite are complete.

## Selected product behavior

QuaZonai supports multiple long-lived, versioned Portfolio Mandates. Research Programs remain independent from capital objectives; Portfolio Programs bind Alpha Library assets to a specific Mandate.

A Mandate defines the portfolio objective and constraints. It is not a user prompt, a one-off candidate parameter, or an execution instruction.

## Separation of concerns

```text
Research Idea
    ↓
Research Program
    ↓
Alpha Library
    ↓
Portfolio Program + Portfolio Mandate
    ↓
Portfolio Candidate
    ↓
Candidate Package
```

Research answers:

> Does this Alpha contain useful, repeatable information?

Portfolio answers:

> Given this investment objective, how should validated research assets be combined?

The Alpha layer must not silently encode a capital owner's utility function.

## Portfolio Mandate object

A versioned Portfolio Mandate contains at least:

- mandate identity and description;
- investment objective;
- risk preference;
- target portfolio behavior;
- concentration constraints;
- turnover preference;
- capacity requirements;
- allowed Alpha roles;
- permitted Portfolio Policy families;
- universe constraints;
- rebalance philosophy;
- downstream compatibility requirements;
- validity conditions;
- lifecycle state.

Examples:

```text
Core Growth
Conservative
Market Neutral
Tail Protection
```

The first release may have one enabled Mandate, but the domain model supports multiple Mandates without redesign.

## Portfolio Program behavior

A Portfolio Program is always bound to exactly one Mandate version.

Examples:

```text
Alpha Library
    ↓
Portfolio Program: Core Growth
    ↓
Portfolio Candidate A

Alpha Library
    ↓
Portfolio Program: Conservative
    ↓
Portfolio Candidate B
```

The same Alpha Version may be valuable under multiple Mandates with different roles, weights or eligibility outcomes.

## Mandate versioning

Mandates are immutable versions.

Changing:

- risk target;
- concentration limits;
- allowed Alpha roles;
- Policy family constraints;
- universe rules;
- rebalance philosophy;

creates a new Mandate Version.

Existing Portfolio Candidates continue referencing the old Mandate Version. They are not silently modified.

A new Mandate Version triggers new Portfolio research and evaluation when needed.

## User experience

The normal user does not select technical portfolio parameters during approval.

Approval reports clearly state:

```text
Portfolio Mandate: Core Growth v3
Purpose: Live handoff candidate
```

The user approves the candidate under that defined objective, not an editable optimization request.

## Product boundaries

QuaZonai must not:

- allow Codex to invent a capital objective for a Research Program;
- allow Approval pages to modify Mandate parameters;
- treat execution preferences as Portfolio Mandate fields;
- replace an approved Portfolio Candidate by silently changing its Mandate.

Mandates define research/portfolio objectives only. Execution systems remain responsible for translating target portfolios into orders and managing runtime behavior.

## Identity and evidence rules

Mandate changes use explicit business versions, lineage and dependency tracking.

No application-level SHA, checksum, digest, fingerprint, content-addressed identity or hash gate is introduced.

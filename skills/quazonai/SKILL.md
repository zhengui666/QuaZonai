---
name: quazonai
description: Inspect or operate a QuaZonai autonomous quantitative research and portfolio-construction workstation using its documented Web/Core API/CLI surface. Use for research status, Alpha/Portfolio inspection, Administration diagnostics, and preparing human approval handoffs. Do not bypass QuaZonai domain state, access secrets or sealed raw evidence, approve capital handoffs on the user's behalf, or control an independent downstream trading runtime.
---

# QuaZonai Operator Skill

This Skill is a thin external workflow for a human-invoked Codex/Agent. It is **not** QuaZonai's built-in Research Agent Runtime. Built-in research uses `codex app-server` plus a mission-scoped stdio MCP Tool Server as defined by `DESIGN.md` and `CLI.md`.

## Read first

Before operating the project repository:

1. Read `/AGENTS.md` completely.
2. Read the relevant sections of `/DESIGN.md`.
3. Use `/OPERATIONS.md` for the user-facing workflow.
4. Use `/CLI.md` only for concrete command/API contracts.

`DESIGN.md` is the sole complete product and architecture source of truth.

## Product boundary

QuaZonai owns:

```text
Idea
Research
Alpha
Portfolio Construction
Independent Evaluation
Candidate Approval
Candidate Package
Handoff Registry
Forward Evidence
Degradation Monitoring
```

QuaZonai does not own:

```text
broker/exchange credentials
orders/fills/positions/accounts/NAV
Paper/Live trading runtime
execution risk
runtime recovery/heartbeat
remote stop/undeploy/close-position
```

NautilusTrader, LEAN and custom trading systems are independent downstream consumers.

## Normal human workflow

The normal recurring human responsibilities are only:

1. propose a Research Idea;
2. approve or reject a system-recommended Paper/Live Candidate.

Do not turn routine research into manual model selection, Alpha picking, parameter tuning, weight editing or Mission management.

## External operation rules

When the user asks you to inspect or operate QuaZonai:

1. Read current state; never rely on an ID/state/version from conversation memory.
2. Prefer the official `quazonai` CLI or documented Core API.
3. For mutation, include the current expected revision/state/version and a fresh idempotency key when the command supports them.
4. Re-read the resource after mutation and report the observed state.
5. For long operations, follow the returned Job/Mission/Handoff resource rather than submitting duplicates.
6. Distinguish research evidence failures from infrastructure, data-quality, Codex, evaluator and downstream-operational failures.
7. Never invent field values, configuration or approval facts.

## Safe read operations

Typical reads:

```bash
quazonai status
quazonai readiness
quazonai research list
quazonai research show <program_id>
quazonai research activity <program_id>
quazonai research missions <program_id>
quazonai alpha list
quazonai alpha show <qualification_id>
quazonai mandate list
quazonai portfolio list
quazonai candidate show <candidate_id>
quazonai approval list
quazonai approval show <approval_id>
quazonai handoff list
quazonai handoff show <handoff_id>
quazonai feedback show <handoff_id>
```

Use the actual implemented `--help` as the command authority if it differs from design-stage examples.

## Idea workflow

For a user-requested new research Idea:

1. Run/inspect an Idea preview first when available.
2. Surface only material clarification requested by QuaZonai.
3. Do not add technical questions about optimizer/model/CV/feature implementation.
4. Show overlap recommendation if QZ finds an existing Program.
5. Start only after the Research Charter summary matches the user's intent.
6. Once started, treat the Charter as immutable.

Do not rewrite an existing Charter to broaden its market/data scope.

## Program lifecycle

Low-frequency operator actions may include:

```bash
quazonai research pause <program_id> --reason "..."
quazonai research resume <program_id>
quazonai research archive <program_id> --reason "..."
quazonai research restore <program_id>
```

Pause/Archive affect QuaZonai research only. Never imply they stop an independent Paper/Live runtime.

## Approval workflow

Candidate approval is human capital-allocation authority.

An external Agent may:

- read the immutable Approval Snapshot;
- summarize the system recommendation and evidence exposed to the user;
- explain Mandate, Capital Context, Candidate, downstream target, validity window and material risks;
- prepare the exact approval/rejection command for the human.

Unless the user is explicitly operating the local human CLI in the current interaction and the product surface permits it, do not autonomously approve/reject on their behalf.

Never:

- switch to a runner-up candidate;
- modify weights or Mandate during approval;
- approve a `STALE` or `EXPIRED` snapshot;
- treat Paper approval as Live authorization.

## Handoff boundary

Before claim, an unclaimed Handoff Offer may be revoked through the documented human operation.

After `CLAIMED`:

- do not attempt `stop`, `undeploy`, `cancel live`, `close position` or equivalent through QuaZonai;
- explain that the independent downstream owns runtime actions;
- QZ may only expose a Withdrawal/Degradation Advisory.

`DOWNSTREAM_ACCEPTED` means the consumer accepted the package contract, not that trading is running.

## Feedback interpretation

Do not interpret these as Candidate failure by themselves:

```text
FEEDBACK_STALE
FEEDBACK_INCOMPLETE
FEEDBACK_INVALID
CONSUMER_UNREACHABLE
```

Only complete, valid, contract-conforming Paper feedback can support Live Promotion.

## Alpha / Portfolio rules

- Alpha Qualification is scope-specific to Universe + Horizon.
- Old quarantined Qualification Versions do not become Active again; new evidence creates a new version.
- Shadow Alpha is not independently qualified for direct Handoff.
- Portfolio Candidate is immutable.
- Any material Alpha/Mandate/Capital/Risk/Cost/Capacity/Constraint/Policy change requires a new Candidate and applicable evaluation/approval.
- Multi-Universe Portfolio must respect Universe-specific cost/capacity and cross-universe risk.

Do not offer manual Alpha selection or weight editing as a normal workflow.

## Data rules

Canonical quantitative data must come through an approved QuaZonai Data Source/Connector and Dataset Revision.

Never use arbitrary web scraping, `curl`, `wget`, copied search results or model memory as canonical research data.

Never request or reveal provider credentials. A missing data entitlement is an Administration capability task, not a reason to invent data.

## Sealed evaluation

Never request, inspect, infer or reconstruct raw Sealed Promotion data or Level 0 evaluation details.

Codex/Agent-visible evaluation feedback is deliberately low-resolution. Do not attempt to reverse-engineer dates, instruments, exact metrics or threshold gaps from it.

## Secrets

Never ask the user to paste into chat or expose through Agent tools:

- API keys;
- private keys;
- passwords;
- OAuth tokens;
- broker/exchange credentials;
- wallet secrets;
- master keys.

QuaZonai does not store broker/exchange execution credentials at all in the target architecture.

## Plugin rules

Only the following plugin classes belong in QZ:

```text
DATA_CONNECTOR
DATA_TRANSFORM_ADAPTER
RESEARCH_ADAPTER
HANDOFF_CONNECTOR
```

Do not add or restore execution/broker/order plugins.

Plugin releases are side-by-side immutable versions. Do not use in-process reload/unload. Do not introduce application-level SHA/checksum/digest/fingerprint identity or integrity gates.

## Codex built-in runtime boundary

If diagnosing the built-in autonomous research runtime:

- `codex app-server` is per finite Mission;
- production transport is stdio;
- Mission workspace is an exclusive temporary worktree;
- Mission network is disabled by default;
- structured research access comes from mission-scoped stdio MCP;
- Codex Thread/Turn/Item is not the business state machine;
- hidden reasoning is not a product evidence source.

Do not fix a built-in Mission by manually changing Research/Approval database state.

## Failure handling

When an operation fails, classify it before acting:

```text
DOMAIN_PRECONDITION
DATA_QUALITY
DATA_CAPABILITY
CODEX_RUNTIME
MISSION_OUTPUT_INVALID
EVALUATOR
PLUGIN
PACKAGE
DOWNSTREAM_OPERATIONAL
NEGATIVE_RESEARCH_EVIDENCE
```

Retry only when the operation is explicitly retryable and its idempotency semantics are known. Otherwise re-read current state and form a new plan.

## No custom hash gates

Never add or recommend application-level SHA, checksum, digest, fingerprint or content-addressed identity for:

- artifacts;
- Candidate Packages;
- plugins;
- workspaces;
- approvals;
- idempotency;
- validation gates.

Use QZ UUIDs, business versions/revisions, explicit relationships, schemas, package metadata, file sizes and executable Reference Fixture conformance.

## Response format

After operating QuaZonai, report compactly:

```text
Objective
Resources read/changed
Current observed state
Automatic work still running
Human decision required
Downstream consequence, if any
Failures / unverified items
```

Do not report model hidden reasoning.
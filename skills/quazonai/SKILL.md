---
name: quazonai
description: Navigate QuaZonai development and native verification using its canonical product, command and operations contracts. Use for changes or verification in this repository.
---

# QuaZonai workflow

Read [AGENTS](../../AGENTS.md) for development authority and the relevant section of [DESIGN](../../DESIGN.md) for product requirements before editing. Local Codex may author source, tests and documentation; GitHub Codex is used only for review. This development permission does not grant the product's research Agent Operator, Reviewer or Downstream authority.

## Find the authoritative detail

| Work | Read |
|---|---|
| Entry and preview | [README](../../README.md) |
| Contributor setup, check selection and source navigation | [CONTRIBUTING](../../CONTRIBUTING.md), [architecture](../../docs/architecture.md) and [OpenSDLC context](../../.opensdlc/project.md) |
| HTTP/CLI bodies, pagination, human authorization and retry semantics | [CLI](../../CLI.md) and native `server ... --help` |
| Startup, profiles, data registration, Worker, delivery and restore operations | [OPERATIONS](../../OPERATIONS.md) |
| Remote task gateways, image assembly, lifecycle and limits | [Native Runtime](../../runtimes/native/README.md) |
| Product ownership, immutable evidence, budgets, scientific eligibility and target-only delivery | [DESIGN](../../DESIGN.md), especially A4–A8 and B3–B8 |
| Reuse/version decisions | [Reuse research](../../docs/research/reuse.md) and [compatibility matrix](../../docs/architecture/compatibility-matrix.md) |
| Existing tests and remaining full-product acceptance | [Evidence index](../../docs/architecture/issue-62-execution.md) |

Keep commands in CLI, operations in OPERATIONS and domain rules in DESIGN. Update their existing sections instead of appending another implementation diary or copying a state machine into this skill.

## Execute and report

1. Inspect branch, worktree, current diff and callers. Preserve unrelated changes and original user data; use an isolated worktree when necessary.
   Reuse the current `.opensdlc/tasks/<task-id>/task.md` for intent, plan, actual verification and handoff. Read the repository language setting before naming a new task; missing configuration means English. Keep shared review and operational rules in their existing OpenSDLC entries.
2. Follow the real flow through contracts, domain, Store transaction, Worker/native adapter and API/UI. Reuse existing Rust components and tests; generated OpenAPI/TypeScript/validators come from their actual generator.
3. Run the narrowest relevant check, then affected cross-boundary checks. `make check-docs`, `make check-unit`, `make check-store`, `make check-http` and `make check-web` have distinct scopes; database suites require an explicitly disposable PostgreSQL/PGMQ instance. Do not use production credentials or data to run repository tests.
4. Use real native processes for protocol, persistence, OCI, database concurrency and restore claims. Mock responses, FIXTURE catalogs and successful registration do not grant scientific qualification or production delivery.
5. Report exact source, command, exit/result and untested scope. Changed source invalidates prior checks; resolve findings and verify the latest PR Head before merge. Full production acceptance remains the DESIGN contract and the evidence index's explicit gaps.

## Runtime authority

A Mission uses only its registered MCP tools, current project/Attempt scope and original cumulative budget. The skill never grants direct database, arbitrary filesystem/URL, Secret or Sealed raw-data access. Never ask for or display passwords, device codes, tokens, wallet material or hidden model reasoning.

Operator authorization, independent Reviewer input and Downstream Claim/ACK are separate identities. Preserve original requests and idempotency keys after unknown outcomes; reads do not refresh evidence or grant current eligibility. Cancellation or a disconnected process does not prove remote work stopped. QZ delivers target-only packages and never acquires real order, account/NAV or broker-control authority.

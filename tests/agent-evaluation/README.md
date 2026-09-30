# Agent evaluation report foundation

This batch validates and displays externally produced runner reports. It does not
invoke a model, discover model availability, judge outputs, or certify independent
scientific qualification. The only committed report is an explicitly `PROTOCOL_ONLY`
`UNRUN` fixture under [fixtures](../fixtures/agent-evaluation/unrun-v1.json); production
code never loads it or any held-out scenario/golden answer.

## Reproduce offline validation

From the repository root, with the pinned Rust toolchain:

```sh
cargo test --locked -p domain --test agent_evaluation
cargo run --locked -p domain --example validate_agent_evaluation -- tests/fixtures/agent-evaluation/unrun-v1.json
```

The example reads at most 2 MiB plus one overflow byte, validates using the same
pure domain rules as upload, and exits nonzero for an invalid report. It performs
no network request and reads no credentials. Successful validation says only that
the report is internally consistent, not that its claimed observations happened.

## Contract and publication

[AgentEvaluationReportV1](../../crates/contracts/src/agent_evaluation.rs) owns the
wire format; [domain rules](../../crates/domain/src/agent_evaluation.rs) own its
cross-field constraints. OpenAPI and web validators are generated from Rust.

- All source, suite, dataset, scenario and assertion evidence hashes refer to
  SHA-256 of the exact externally retained bytes, encoded as lowercase hex
- The external runner must freeze required assertions and dataset membership
  before execution. Tuning and held-out dataset IDs, hashes, case IDs and scenario
  hashes must be disjoint; metadata alone cannot prove absence of prior exposure
- PASS requires observed model/reasoning/invocation identity, exact requested
  model and reasoning, and every required assertion passing with an evidence hash
- FAIL requires matching observed identity and complete assertions with at least
  one failure. A different observed model or reasoning is BLOCKED, preserving its
  original observations and measurements. BLOCKED records an impediment; UNRUN
  means no invocation or measurement
- The aggregate status precedence is FAIL, BLOCKED, UNRUN, then PASS only when all
  cases pass. No case disappears from the denominator
- Usage, tool calls, elapsed time and actual cost may be null (unknown). Known counters
  and decimals remain exact strings, including measured zero. Cost is a complete
  amount/currency tuple using the pinned native ISO currency capability. No pricing
  estimate
  is presented as an observed charge
- `LIVE` is the uploader's declaration, not an attestation by QuaZonai. Execution
  provenance and external evidence still require review

Upload through the project's **Agent 评估** tab, or the existing authenticated
`POST /api/v2/artifacts` with kind `REPORT`, original JSON string `content`, and
an idempotency key. Immutable storage, quota, original-byte replay, project scope
and revocation rules remain the existing artifact implementation. Generic REPORT
uploads remain supported. The read-only `GET /api/v2/artifacts/{id}/agent-evaluation`
projects only a strictly validated Agent report and retains the artifact buffer
permit until its response body is consumed or dropped.

Existing report artifacts have `RESEARCH` visibility within their project. Store
evaluation results in a dedicated project that the evaluated subject cannot read;
this upload route does not establish evaluator/subject isolation by itself. Do
not include hidden scenarios, golden answers, credentials or raw private traces
in report text. Once held-out results are disclosed to a tuning subject, treat
that exposure as consumed and use a fresh held-out set for subsequent independent
assessment. Hash disjointness alone does not prove confidentiality or no prior
exposure.

## Live-run follow-up

The requested evaluation policy is exactly `gpt-6-luna` with reasoning effort
`max`. This selection belongs in evaluation policy and reports, not hardcoded
production task logic. A future live runner must discover that exact available
pair through the authorized native subject interface, record the actual observed
identity, and report BLOCKED if unavailable; it must not silently substitute a
model or reasoning level. Codex is the evaluation subject, never the development
or review engine. This validator does not perform discovery or a live invocation.

Freeze independent held-out scenarios and scoring before live execution. Keep
scenario contents and golden answers outside runtime product logic; retain
explicit budget/authorization, public invocation IDs and auditable observation
artifacts. No hidden reasoning or credentials belong in a report.

The typed evaluation endpoint serializes the parsed native report, including UTC
RFC3339 timestamp normalization. Original uploaded bytes remain unchanged and are available from the generic
artifact `/content` endpoint for external hash verification; that endpoint does
not supply a server-computed digest.
Extended/signed years and leap seconds outside the UTC end-of-day position are
rejected at report admission so typed reads stay within the HTTP date-time schema.

# Agent evaluation report foundation

The product validates and displays externally produced runner reports. The offline
example below freezes evaluator-owned inputs and scores public JSON declarations
against frozen equality/existence assertions. Neither path invokes a model,
discovers model availability, attests execution, or certifies scientific or model
quality. The committed [report fixture](../fixtures/agent-evaluation/unrun-v1.json)
and [controller self-test pack](../fixtures/agent-evaluation/controller-self-test/suite.json)
are public protocol self-tests only. Production code never loads these fixtures
or evaluator-owned scenarios and expected answers.

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


## Offline freeze and replay utility

The [example controller](../../apps/server/examples/agent_evaluation.rs) is an
evaluator-only utility, outside the production API and subject runtime. It always
emits `PROTOCOL_ONLY`, including when all replayed assertions pass. A replayed
PASS means only that supplied declarations match frozen predicates; it does not
establish that an invocation happened or that a model is good. There is no model
catalog query, subject launch, network call, environment/credential read, event
stream adapter, script evaluator, or model grader in this utility.

Run the public self-test with a fresh output directory:

```sh
cargo test --locked -p server --test agent_evaluation_controller --example agent_evaluation
tmp=$(mktemp -d)
cargo run --locked -p server --example agent_evaluation -- freeze \
  --input tests/fixtures/agent-evaluation/controller-self-test --output "$tmp/frozen"
LOCK_SHA256=$(cat "$tmp/frozen/lock.sha256")
mkdir "$tmp/observations"
printf '{"schema_version":1,"lock_sha256":"%s","observations":[]}\n' "$LOCK_SHA256" \
  > "$tmp/observations/observations.json"
cargo run --locked -p server --example agent_evaluation -- replay \
  --input tests/fixtures/agent-evaluation/controller-self-test \
  --lock "$tmp/frozen/lock.json" --lock-sha256 "$LOCK_SHA256" \
  --observations "$tmp/observations" --output "$tmp/replayed"
cargo run --locked -p domain --example validate_agent_evaluation -- "$tmp/replayed/report.json"
```

This example deliberately leaves all cases UNRUN. The `held_out` key exercises the
native two-split shape; all committed cases are public and are not an independent
held-out benchmark. Actual held-out inputs and expected answers must be supplied
from an evaluator-owned directory outside the repository and every subject mount,
project, API, and context. Do not place the output directory inside a subject mount.

### Frozen inputs and retained bytes

`input/suite.json` follows the public pack's strict shape. The offline utility
validates and binds the suite's requested settings through the native report
contract; it is not coupled to any particular model/effort identifier. The public
sample and this task's requested live policy remain exactly `gpt-6-luna` / `max`.
Live policy enforcement, availability discovery and verification of effective
serving identity belong to the separate, deferred live runner; accepting a suite
here does not authorize a live invocation or assert availability.

The required timestamp
is an explicit fixed `recorded_at`; the utility never supplies the current time.
Each split has a distinct dataset ID and at least one case. Case IDs are globally
unique. Scenario bytes must be disjoint across splits. Each case has 1–100 uniquely
named required assertions. Only `EQUALS` with an exact JSON value and `EXISTS` at an
RFC6901 pointer are supported. The empty pointer selects the whole data value;
`~0` and `~1` escape tilde and slash. Missing pointers fail; an existing null value
satisfies `EXISTS`. Values use native `serde_json::Value` equality, without numeric
coercion or approximate comparisons. Fractional/exponent-form JSON numbers and
integers outside the native signed/unsigned 64-bit range are rejected recursively,
including inside predicates and observation data, before they can round into a
false match. Use strings for exact decimal quantities and larger integers. Native
measurement counters/decimals retain their existing string-only contracts.

Freeze writes a create-new `lock.json`, its `lock.sha256`, an UNRUN `report.json`,
and `tuning.manifest.json` / `held-out.manifest.json`. These compact deterministic
manifests retain dataset membership, scenario hashes, and complete predicates.
Their exact bytes are the dataset hashes in the native report. The lock binds the
exact suite bytes, source archive bytes, both manifests, and all scenario bytes.
Retain the input pack as well as every generated file. Both splits bind the same
candidate source and suite; a later candidate cannot reuse an earlier tuning run.
The source revision is an operator-supplied label, not proof of source authenticity.

Before accepting any observations, retain the lock digest independently of the
pack. Replay requires that external value via `--lock-sha256`, rereads the original
input pack and retained manifests, and refuses any byte drift, including whitespace.
Reading the current adjacent `lock.sha256` at replay time is only a convenience in
the public example above; it is not protection against someone replacing the lock
and its digest together. Neither hashing nor separate directories prove that an
actual held-out case was never previously exposed.

### Observation declarations

`observations/observations.json` has exactly this index shape:

```json
{"schema_version":1,"lock_sha256":"<externally retained lowercase SHA-256>","observations":[{"case_id":"<frozen case ID>","file":"case.json","sha256":"<SHA-256 of exact case.json bytes>"}]}
```

Each indexed file has this shape, using bindings from the frozen report:

```json
{
  "schema_version": 1,
  "case_id": "<frozen case ID>",
  "source_sha256": "<frozen source SHA-256>",
  "suite_sha256": "<frozen suite SHA-256>",
  "scenario_sha256": "<this case's frozen scenario SHA-256>",
  "requested": {"model": "gpt-6-luna", "reasoning_effort": "max"},
  "observed": {
    "settings": {"model": "gpt-6-luna", "reasoning_effort": "max"},
    "invocation_id": "<public declared native invocation identity>"
  },
  "complete": true,
  "measurements": {
    "input_tokens": null,
    "output_tokens": null,
    "elapsed_ms": null,
    "tool_calls": null,
    "cost": null
  },
  "data": {"answer": "<public observation data>"}
}
```

An absent index entry is UNRUN and stays in the denominator. An indexed missing
file is an invalid pack, not an omitted case. `observed: null`, different actual
model/reasoning, or `complete: false` yields BLOCKED without scoring predicates.
Malformed identity fields are invalid input. Actual differing settings and supplied
valid measurements are retained. Complete, matching-identity declarations score
every frozen assertion; no supplied status/assertion verdict can override scoring.
Duplicate case entries, repeated file paths, repeated invocation IDs, unknown cases,
and cross-case/source/suite/scenario/requested-policy substitutions are rejected.
There is no attempt selection or best-of aggregation.

`complete` is the operator's declaration of a terminal, complete public observation,
not independently verified execution. Metrics are optional native exact declarations:
unknowns stay null, counters are decimal strings, and reported cost must include
both a nonnegative decimal-string amount and native currency code. Never substitute
estimated pricing. A tool count must be an explicitly reported complete total, not
a count of retained events; incomplete observations with a supplied tool count are
rejected. This utility has no event stream interpretation or completeness attestation.

Replay retains raw observation bytes under `evidence/<sha256>.json`, the original
index as `observations.index.json`, the lock/digest, manifests, and report. Assertion
evidence hashes point to those exact bytes. The original index paths describe the
original input directory; retained evidence filenames are content hashes. Keep the
original observation directory if it must itself be replayed again. Public observation
files must contain only the bounded public data needed by assertions, never credentials,
environment dumps, full sensitive tool requests, or private model reasoning. The
utility cannot establish that operator-supplied data is true or automatically redact it.

### Failure and resource boundaries

Successful freeze/replay exits zero, including a valid report whose subject outcome
is FAIL, BLOCKED, or UNRUN. Invalid packs, binding violations and I/O failures exit 2
with a fixed error category, without echoing raw source or observation content.
Reports pass the existing native `domain::agent_evaluation::validate` rules; there
is no second report-schema implementation.

Limits are 500 total cases, 10,000 total assertions (at most 100 per case), 512-byte
relative paths and JSON pointers, 2 MiB per JSON input/output, 256 KiB per scenario,
64 MiB source archive, 96 MiB aggregate bytes read, and 1,100 total file reads.
Duplicate JSON keys are rejected before conversion, including keys inside public
observation data. Unknown fields in typed input structures are rejected. Scenario
and source bytes are opaque and are never executed or extracted.

Paths referenced by the suite/index must be relative normal components; absolute
paths, parent/current-directory components, backslashes and drive prefixes are
rejected. Existing symlinks in inputs, root/ancestor paths, and output parents are
rejected. Output parents must already exist. Outputs use new directories/files and
never overwrite existing content; on Unix they are created with owner-only modes.
The standard-library checks protect operator-owned stable paths, not a concurrently
hostile filesystem sandbox. Keep all input/output parents evaluator-controlled.
A failed filesystem write may leave a partial new directory; preserve/inspect it and
retry with a new output directory. No partial output is presented as a completed run.

# Offline Agent evaluation controller

## Scope and dependency

Add an evaluator-owned, bounded offline freeze/replay utility on top of the
[Agent report foundation](../agent-evaluation-foundation/task.md) and
[PR 145](https://github.com/zhengui666/QuaZonai/pull/145). Publication is pending;
there is no separate Issue or published controller PR yet. The maintained input
format, commands and limitations are in the
[evaluation guide](../../../tests/agent-evaluation/README.md).

The utility is an example entrypoint, not a second product evaluation framework.
It reuses native contract scalars and domain report validation. No production
wire types, generated code, API, subject runtime, or model configuration changes.
Codex remains the evaluation subject only; it is not used to develop or review
this work. The existing read-only subject-startup blocker remains unchanged and
no launch retries or live inference are part of this batch.

## Design

- Freeze exact source/suite/scenario bytes with SHA-256, fixed input timestamp,
  one candidate source across both splits, and retained deterministic dataset
  manifests. Source revision labels and hashes do not prove authenticity
- Require an independently retained lock digest before replay. Reject changes to
  all bound input/evidence bytes, typed unknown fields, duplicate JSON keys,
  cross-case substitution, unsafe paths, symlinks and bounded-work violations
- Only frozen RFC6901 equality/existence predicates score public JSON data.
  Complete matching-identity declarations yield PASS or FAIL; incomplete, absent
  or mismatched declared identities yield BLOCKED. Missing cases are UNRUN and
  never removed from either split or the aggregate denominator
- Every report is unconditionally PROTOCOL_ONLY. Raw evidence is retained under
  content hashes, supplied native exact measurements stay exact, and unknowns
  remain null. Replayed declarations are not attested invocations or model-quality
  evidence. A valid FAIL report exits zero; an invalid pack exits 2
- Create-new private outputs preserve existing files. Standard-library path
  guards assume evaluator-owned stable input/output parents; they do not claim
  to sandbox a concurrently hostile filesystem
- Public committed fixtures test only this protocol. Actual held-out scenarios
  and expected answers must remain outside the repository and all subject mounts,
  projects, APIs, and contexts. No held-out quality claim is made

## Verification and remaining gates

The focused integration tests exercise deterministic freeze/replay, exact-byte
hashes and retained manifests, all-case accounting, identity/policy/source/case
binding, malformed keys/fields, required assertions, missing pointers, unknown
metrics, private output modes, aggregate/per-file limits and collision/symlink
rejection. CLI smoke checks distinguish invalid pack errors from valid subject
FAIL and verify PROTOCOL_ONLY output through the native report parser.

Record final local results below after execution. Applicable exact-final-Head
hosted CI and independent native review are required before any merge into dev.
Full CI, actual service/browser acceptance and live subject evaluations are not
implied by focused offline tests. No publication, merge or deployment is performed
by this implementation task.

## Local verification on 2026-09-30

Passed against the final local source tree, using the pinned Rust toolchain:

- `cargo fmt --all -- --check`
- `cargo test --locked --offline -p server --test agent_evaluation_controller --example agent_evaluation`: all 23 controller tests pass; the example test target builds
- `cargo clippy --locked --offline -p server --test agent_evaluation_controller --example agent_evaluation -- -D warnings`
- `cargo build --locked --offline -p server --example agent_evaluation`
- Actual CLI freeze/replay smoke checks produce deterministic PROTOCOL_ONLY
  UNRUN, PASS and FAIL declarations with exit 0. All three generated report shapes
  pass the native `validate_agent_evaluation` example. Evidence hash drift and
  fractional numeric JSON produce exit 2 before creating an output directory
- CLI help works; invalid arguments return exit 2 without echoing supplied values
- Changed Markdown relative link targets resolve and `git diff --check` passes

The equality guard rejects fractional/exponent-form numbers and overflowing
integer literals recursively rather than allowing IEEE-754 rounding to create
false matches. Regressions cover adjacent values beyond exact binary-float
precision, signed/unsigned integer limits, and large/fractional literal rejection
inside both predicates and observations. Exact counters and decimals remain native
strings. Replay writes its report only after retaining all indexed evidence.

The full repository checks (including pinned Lychee documentation checks), hosted
final-Head CI, and independent native review remain pending. No live model calls,
held-out quality measurements, account changes, publication or merge occurred.


## Offline policy binding correction

The example binds the suite's requested settings through the native report
contract, frozen lock and observation provenance; no model/effort identifier is
hardcoded in the controller. The public sample and actual CLI smoke declarations
remain exactly `gpt-6-luna` / `max`. A synthetic-identifier regression establishes
that the offline mechanism is reusable, preserves mismatched observed settings as
BLOCKED, and rejects substituted requested settings or a changed frozen suite.
It does not invoke or discover any model. Enforcement of the owner's actual live
policy and verification of serving identity remain separate, deferred live-runner
responsibilities.

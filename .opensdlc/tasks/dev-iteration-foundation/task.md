# Dev iteration foundation

## Scope and baseline

Owner request: continuous autonomous improvement of `dev`, with design, tests,
Agent evaluation, defect discovery, draft PRs and merge after final checks.
Codex is only the evaluation subject, using verified `gpt-6-luna` / `max`.
Development and independent review use native workers. No changes target `main`.

Baseline inspected on 2026-09-30: `5e7f2959e38ac3b9c47676c517a39040343e23a2`.
No checkout-local `.agents/skills` exists. Read [project](../../project.md),
[architecture](../../architecture.md), [review](../../review.md), the existing
[evaluation suite](../../evals/suite.md) and source acquisition/native preparation
boundaries. Existing [PR 135](https://github.com/zhengui666/QuaZonai/pull/135)
and [PR 136](https://github.com/zhengui666/QuaZonai/pull/136) remain independent:
this batch does not modify artifact export, native partial-fill/settlement or
Codex probe guards. Their reported checks do not validate this branch.

Observed gaps and ordered work:

| Priority | Evidence | Bounded next delivery | Completion evidence |
| --- | --- | --- | --- |
| 1 | `App.tsx` eagerly imports all six sections; `ErrorNotice` starts a permanent one-second clock even with no error | Defer inactive sections, keep navigation available during failed/interrupted loads, stop idle error clocks | Unit/type/generation/build checks, synthetic browser regressions, actual native browser CI, bundle comparison |
| 2 | `runtimes/data` has immutable Hugging Face/EVM acquisition and native Polymarket conversion but no shared provider registry or crypto adapter | Generic bounded free source acquisition with Polymarket and crypto capabilities, raw evidence and deterministic offline verification | Provider fixtures, malformed/duplicate/gap/precision/budget/interruption cases; bounded live read recorded separately |
| 3 | Existing model evaluation is four manual paired cases with undisclosed exact serving identity; no reusable runner or in-app results view | Typed versioned evaluation records, isolated runner, model capability/identity attestation, persisted results and in-app dashboard | Reproducible dataset/suite hashes, explicit blocked/unrun states, real observed tool traces, operational and quality metrics |
| 4 | Acquisition is not native catalog preparation or service admission | Complete both free plugin domains through supported native normalization, existing permission/registration/quality flow and UI | Original definition and timestamp preservation, real disposable end-to-end registration/validation, recovery and failure evidence |
| 5 | CI repeats cold Rust compilation in multiple jobs; actual timing baseline not yet captured | Measure per-job/step costs, cache only reproducible locked dependencies/builds with toolchain/platform/features bound | Equal checks and outputs on exact Head, cold/warm measurements; no skipped correctness gates |
| 6 | Shell navigation is local React state and ignores browser Back/Forward | Explicit navigation/history contract with unsaved/pending operation preservation | Deep-link/reload/back/forward, cancellation, rapid navigation and narrow-view regressions |
| 7 | Broad duplicate/dead feature concerns are unmeasured | Remove code only after tracing callers/contracts and recording the redundant behavior | Static usage evidence plus regression tests; retain scientific failures and immutable data |

## Design boundaries

Source plugins live at the existing operator acquisition boundary. They advertise
record capabilities instead of pretending price marks, OHLCV, trades and depth
are interchangeable. Selection is explicit and bounded; immutable raw responses,
provider/terms references, retrieval clocks, exact decimals and hashes precede
normalization. Missing candles stay absent. Present-day metadata and retrieval
time do not become historical availability or permission evidence. Native catalog
preparation, partition isolation, data grants and fresh `DATA_VALIDATE` remain
the research admission boundary. Acquisition alone is not a complete plugin.

The evaluation framework belongs to the existing application and its persisted
evidence/API boundaries, not a separate dashboard service. Freeze suite version,
dataset and client/tool revision, case split, budget, seed where supported,
requested/observed model, reasoning, outcome and nonsecret observable events.
Include functional correctness, boundary violations, task completion, tool count,
latency and token/cost observations when actually available. Missing usage is
unknown, never zero. Separate infrastructure failure from task failure and
blocked/unrun from passed. Tune on development cases only; use disjoint held-out
cases and new scenario families for generalization. Production code never reads
golden answers or case-specific success rules. Authentication and unavailable
model capabilities are blockers, never permission to change accounts or model.

## First batch

- Lazy section imports use native React Suspense; navigation and theme controls
  remain mounted. A section-scoped instance of the existing error boundary keeps
  other sections reachable after a chunk failure. A late load cannot select itself.
- Error display mounts a bounded timeout only for an actual future Retry-After
  deadline. Empty notices and errors without deadlines have no polling timer.
  Expiration enables the explicit retry control without submitting a request.
- Preserve editor identity on theme changes and native cancellation behavior.
- Run unchanged frontend contract/type/unit/build/synthetic-browser checks before
  compiling the Rust API. Still build and compare the actual native schema before
  real browser acceptance; no check or prerequisite is skipped. A previous
  [successful Web run](https://github.com/zhengui666/QuaZonai/actions/runs/36732632168)
  spent 115 seconds on this native build before frontend feedback. This ordering
  reduces avoidable feedback delay, not the claimed duration of successful CI.
- Update review guidance to the owner's evaluation-only Codex boundary while
  retaining exact-Head CI and independent review gates.

## Verification

Baseline actual local results: 542 Web unit tests, typecheck, native-generated
client no-drift, production build/PWA precache, and 23 Python acquisition tests
passed. Baseline minified main application chunk: 262.12 kB (68.37 kB gzip);
vendor/contracts/chart chunks are separate. This is a byte measurement, not a
claim about runtime latency or overall PWA download reduction.

Candidate local results: the same 542 unit tests, typecheck and production/PWA
build pass. The main application chunk is 22.95 kB (9.33 kB gzip); initial research
also loads its 29.22 kB page, shared Runs/budget/resource modules and unchanged
vendor/contracts. All sections remain precached, so total PWA bytes are slightly
higher (3,938.57 KiB versus 3,933.41 KiB), not a total download reduction.
The four new synthetic browser cases could not launch local Chromium: its
process-singleton socket operation is prohibited in this executor, including a
permitted sandbox escalation. They must pass on the hosted browser runner before
merge; no local browser pass is claimed. The first hosted run executed all eight
synthetic cases: seven passed; the idle-editor case timed out on an exact
textbox-name locator. The amended test uses the same labelled-control lookup as
the existing native acceptance suite, scoped to the editor, without removing its
timer/edit/cancel assertions. Full final-head review/CI remains pending.
Rust/PostgreSQL/native runtime acceptance and real Agent evaluation are not
established by these baseline frontend and acquisition checks.

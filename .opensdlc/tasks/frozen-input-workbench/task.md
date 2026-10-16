# Frozen input workbench

## Goal and ownership

Expose the existing atomic InputSet creation/freeze command and separate bounded
`DATA_VALIDATE` command in Web. Delivery base: `dev` at
`6024c557484da377d98e6b5f19020fc836ed4ca3`. Issue/PR links are recorded when the
coordinator publishes this batch; this local implementation does not publish or
merge. Design, implementation and review use native development tools, not Codex
inference. Codex remains an evaluation subject only.

## Implemented boundary

- Settings → Data has separate registry and frozen-input surfaces. Existing
  `data.tsx` and `projects.tsx` are unchanged; registry dialog recovery and Agent
  evaluation work remain separately owned.
- Explicit project selection lists every frozen purpose. Creation is dataset-only
  DISCOVERY or VALIDATION, with an explicit Runtime, registered revision IDs and
  an exact UTC cutoff. The UI accepts at most six fractional digits without
  rounding, validates microsecond availability comparisons using integers, and
  lets the server enforce authoritative PIT/admission rules.
- The workbench allows 1–255 datasets, reserving the native parameter artifact
  position. The general InputSet API limit remains 256. Source/Runtime binding,
  partition, license observation, original snapshot/storage versions and
  unqualified FIXTURE/UNVERIFIED state stay visible. Global registration quality
  and metadata IDs are displayed without unsupported generic download links.
- Creation returns the original immutable freeze receipt and does not start a
  Run. A second, explicit action chooses the actual bound Runtime/revision and
  CPU, wall-time, memory and output limits for `DATA_VALIDATE`. Expired probes,
  changed revisions and confirmed failures require reload/reconfirmation.
- In-flight requests are single-flight. Unknown responses lock the original
  body and idempotency key for exact retry; changed-body retry is unavailable.
  Dismissal is blocked during submission, and dirty/failed dismissal requires
  explicit abandonment. Project/purpose changes clear dependent selections;
  offline writes are disabled.
- Original queued receipts remain separate from live Run snapshots. Existing
  Run details supply polling, SSE and cancellation. Project Run and artifact
  pages are explicit and paginated, including empty filtered pages.
- Downloadable artifacts must match the selected project, producer Run and
  that Run snapshot's exact active attempt. Metadata and byte counts are
  verified through existing artifact APIs. Historical attempts do not become
  current results; failed/nonterminal results are not represented as a passed
  scientific conclusion. No qualification or licensed-use claim is inferred.

## Files

UI/selection implementation:

- [data workbench](../../../apps/web/src/data-workbench.tsx)
- [frozen input flow](../../../apps/web/src/data-inputs.tsx)
- [pure selection/request constraints](../../../apps/web/src/data-input-options.ts)
- [Settings mount](../../../apps/web/src/settings.tsx)
- [focused tests](../../../apps/web/src/data-input-options.test.ts)
- [synthetic interaction cases](../../../apps/web/tests/data-inputs.spec.ts)
- [interaction suite configuration](../../../apps/web/playwright.auth.config.ts)

The coordinating native fixture extension owns the separate
[real API data browser scenario](../../../apps/web/tests/native-data-inputs.spec.ts),
[scenario configuration](../../../apps/web/playwright.data.config.ts),
[private phase validation](../../../apps/web/tests/native-auth-support.ts),
[native browser harness](../../../apps/web/scripts/native-browser.mjs),
[test-only TLS metadata peer](../../../apps/server/examples/browser_data_fixture.rs)
and [Web CI build](../../../.github/workflows/web.yml). It retains the original
idle Worker/restart assertions and adds a distinct admission-only phase with
initial resources registered through the real API and the Worker stopped.

## Verification and remaining gates

Local checks executed during implementation:

- TypeScript checking passed, including both new browser specifications
- All Web unit/contract tests passed: 20 files, 555 tests, including 10 focused
  selection/request/attempt-binding tests
- Generated TypeScript/client validators have no drift; production build passed
- Six synthetic browser interaction cases collect successfully. They exercise
  unknown outcomes, immutable retries, dismissal, double submission, stale
  Runtime revisions/probes, project/purpose changes, Sealed exclusion, offline
  behavior and narrow viewports. Collection is not execution
- The typed native fixture helper compiled; scoped helper Clippy and formatting
  passed; actual server OpenAPI output exactly matches the checked-in schema.
  The Web workflow builds the helper with `cargo build --locked -p server --bin
  server --example browser_data_fixture`; the helper does not execute OCI jobs.

Browser execution is not available in this execution environment: loopback
sockets are blocked. No repeated socket attempt was made. Hosted execution of
both the synthetic interaction suite and the separate real API scenario is a
mandatory final-Head gate, along with applicable CI and independent review.
No synthetic terminal Run or scientific quality artifact substitutes for it.

The broader plugin → catalog preparation → Runtime → registration → frozen
InputSet → Worker/OCI scientific execution chain remains a separate acceptance
gate. This batch's real API scenario establishes registration, atomic freeze,
admission, immutable identity and original-command replay only. It does not
establish a live account, grant, qualification, paid-data or trading outcome.

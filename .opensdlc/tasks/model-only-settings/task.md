# Model-only settings and unrestricted application execution

<a id="task"></a>
## Task

**Task ID:** model-only-settings

The owner's 2026-10-13 request is to rebuild Settings around model configuration, using sibling inkos as a reference, and remove timeouts and other execution restrictions. The owner explicitly chose full-chain technical-limit removal while retaining research budgets and scientific/identity invariants, then approved the implementation and delivery plan.

Endpoint: implemented and locally verified changes, independent final-Head review and applicable hosted CI, followed by merge into remote `dev`. The owner subsequently authorized updating the existing local QuaZonai deployment **only after successful CI and publication of that revision**. This does not authorize real account/model inference or another deployment.

<a id="intent"></a>
## Intent

Settings currently mixes model selection with application identity, global trial defaults and task/native deadlines. Replace it with actual provider connections, model selection and supported reasoning. Remove application-imposed execution deadlines and process/resource caps rather than merely hiding controls. Retain frozen research inputs, explicit per-study budgets, PIT/Sealed isolation, trial accounting, original receipts and cancellation/recovery.

The launch checkout was old Rust control-plane commit a28a8469 with unrelated uncommitted changes. This isolated branch starts from current remote dev bcbef01a, the TypeScript/Pi Durable workbench. The original checkout and unrelated worktrees are unchanged.

<a id="spec"></a>
## Requirements and design

- Model-only Settings, official provider catalog/auth, API-key/OAuth login and cancellation/logout, model and reasoning selection, responsive Ant Design UI consistent with the existing theme.
- Official pi-ai owns provider capabilities, login and token refresh; a private QZ credential adapter implements its public storage interface. No copied third-party implementation, personal credential discovery or secrets in public data/research history.
- New research freezes its actual model/reasoning selection. Existing requests/Tasks retain original identities and scientific inputs; no silent fallback or repeated external POST.
- Remove QZ stream/catalog/task/native/downstream/CLI deadlines, Wasm process memory ceiling, adapter output cap and injected default fuel. Explicit scientific budgets, auth/target validity and native owner cancellation remain. Do not fake infinity with large limits. Unsupported upstream/provider disable semantics remain disclosed.
- Historical settings/Task inputs remain readable but cannot reactivate retired application timers. Settings PATCH only mutates model preferences.

<a id="plan"></a>
## Implementation plan

1. Update source DTOs, configuration and historical projections; add private official credential/auth adapters, typed provider endpoints and frozen reasoning choice.
2. Replace Settings with model selection plus searchable provider connections and transient native login prompts; decouple research budgets/application identity/observation connections from model settings.
3. Remove active TypeScript execution restrictions, reusing original Context/runtime signals and official NodeExecutionEnv cleanup; update CLI and current operational docs/examples.
4. Add focused regressions for model-only settings, credential lifecycle/redaction, old-setting compatibility, no imposed timeout/default fuel, explicit cancellation, scientific budgets and unsafe-write reconciliation.
5. Run routine checks, browser/axe and real native core/science checks; inspect desktop/mobile screenshots against the approved layout.
6. Obtain independent review, resolve findings, commit/push a scoped PR to dev and use native PR monitoring for final-Head CI before merge.

Reuse: official Models public APIs, Harness/configure/native Task persistence, NodeExecutionEnv, existing Ant Design/React Query UI helpers, retained optional-fuel Rust kernel and original target/account adapters. inkos is a behavioral reference only.

<a id="verification"></a>
## Verification

- Before implementation, `node --experimental-strip-types --test apps/research/test/execution.test.ts` failed both regressions: the injected stream deadline was 120000 ms and omitted fuel became `10000000`. After removal both pass.
- Latest `make check` passed: service 8/8, research 23/23, CLI 3/3, Web typecheck/unit/build; deployment helper tests 11 passed/1 existing release-distribution skip, plus 4 Python packaging/release tests. The distribution skip requires a prebuilt release bundle; it is not reported as executed installation acceptance.
- `npm --prefix apps/web run test:e2e` passed 5/5, including optional blank auth prompts and cancellation while response acknowledgement is pending, model-only save/capabilities, preserved dirty edits on refresh, transient secret/unknown-response retry and cancellation, adjacent research/portfolio/observation/extension flows, 390px keyboard/reduced-motion/layout and axe. Two earlier runs failed on test selectors for Ant Design's selected display (not its empty search input); corrected selectors, retaining the assertions.
- A disposable **real** service with official built-in providers and no inherited credentials served 42 unconfigured providers. At 1440px/390px the page exposed only provider/model/reasoning fields, passed axe and horizontal-overflow checks, raised no page exceptions, and opened/cancelled the official Anthropic API-key prompt without submitting any credential. Screenshots: [desktop](assets/settings-1440.png), [mobile](assets/settings-390.png). Existing Ant Design Drawer-width deprecation is not a page exception or a new regression.
- Controlled official provider tests cover API Key/OAuth/select/manual-code prompts, correlated replay/conflict, cancel-before-start, active-login resume metadata, logout/environment truthfulness, stable installation identity, safe failures, shutdown and secret non-reflection. SQLite tests cover private permissions, serialization, rollback, deletion and restart. These synthetic credentials are test fixtures, not real-account authorization.
- Real child execution/cancellation test passed, including provider-env isolation and joined child exit. Historical tiny task/native settings remain stored but do not prevent a portfolio completing; saved reasoning freezes into admitted research and survives changed preferences/restart/replay. Existing one-original-unsafe-POST restart reconciliation tests pass.
- QZ's hardcoded two-no-progress/maximum-yield stops were also removed as technical execution restrictions. Original explicit trial budget, final/owner stop and error retry policy remain. Regression continues through six empty yields without resetting input/budget; a study can continue consuming model usage until scientific budget/finalization/explicit stop. Native fuel regressions now expect omitted/null fuel to remain unmetered; the infinite-loop failure test explicitly supplies fuel, while malformed memory replaces the obsolete artificial memory-ceiling expectation. No runtime assertions are silently skipped.
- `make check-science` could not complete locally: rustc failed with `No space left on device` during retained contracts/domain compilation. Only this task's failed `target/` artifacts were removed. Local native/core verification remains **unrun**, not passed; applicable hosted final-Head science/core CI must pass before merge.
- Exact `gpt-6-luna` with `max` was unavailable in the credential-free native catalog (zero configured models). Real-model behavior evaluation is **blocked/unrun**; deterministic protocol checks do not establish research judgment, paid-provider/account results or model quality. No fallback model or real credential authorization was inferred.
- `lychee` is not installed; the full Markdown link checker is unrun. New screenshot links point to committed task assets.

<a id="review"></a>
## Review

Independent read-only non-Codex review of initial Head `2d0a3da2` found four actionable defects. Author remediation and regression evidence:

1. Queued credential mutations ignored the official cancellation signal: the official Models.login reproduction rejected cancellation but subsequently saved its key. Store operations now check cancellation immediately before starting; already-started refresh still commits. New regression failed before the fix and passes after it.
2. Retained Rust serialization omits null/absent fuel; preserving null in the execution request falsely rejected the native echo. Execution-only normalization now removes null, retains original admitted input, checks the actual omitted echo, and keeps consumed fuel null. Protocol-child regression reproduced `native_result_request_mismatch` before the fix and passes after it; matching native CI remains required.
3. Official optional/confirmation prompts require blank text responses. API accepts string responses, retaining nonblank secret, prompt/selection/type validation. UI permits blank text confirmation. Controlled OAuth test reproduced 400 before the fix and passes after it.
4. Pending response POST disabled Cancel/close/Escape. A separate cancellation state and response AbortController now detach that local request and explicitly DELETE the original login ID. The held-response browser regression left the modal open before the fix; it now confirms correlated cancellation and closure without a timer or automatic resubmission.

Independent recheck at `a1701466` verified the four original remedies and found one introduced regression: blank secret submission could overwrite an existing API key because official generic/Anthropic login returns that value without validation. Backend/UI now reject blank secrets while preserving optional blank text. The new preservation regression failed before correction (200 instead of 400); it now confirms the original synthetic credential remains unchanged and the login is not falsely completed. Full `make check` and 5/5 e2e pass after this correction.

Independent recheck of the corrected final committed Head is pending; author testing is not independent acceptance.

<a id="delivery"></a>
## Delivery

[PR 194](https://github.com/zhengui666/QuaZonai/pull/194) targets `dev`; GitHub owns exact revision/CI/merge evidence. Applicable final-Head hosted CI and independent recheck remain pending.

The owner-authorized local update targets the existing **schema-4** installation at `$HOME/.local/share/quazonai-ts`, port 18080, retaining its original `data/research/research.sqlite`, configuration/auth, PostgreSQL, releases and backups. The separate legacy `$HOME/.local/share/quazonai` root is not the running Node deployment and must remain untouched. Preflight confirmed original revision `bcbef01a`, both application/database healthy. Use the installed schema-4 manager for a stopped-writer backup and immutable published-version update, refuse unresolved work, verify image/source/health/Web and original state after activation. Disk availability is approximately 1 GiB; do not prune unrelated Docker images, volumes, backups or caches without separate authority. No local update has occurred yet; publication and final acceptance must precede it.

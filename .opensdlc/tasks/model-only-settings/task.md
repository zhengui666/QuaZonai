# Model-only settings and unrestricted application execution

<a id="task"></a>
## Task

**Task ID:** model-only-settings

The owner's 2026-10-13 request is to rebuild Settings around model configuration, using sibling inkos as a reference, and remove timeouts and other execution restrictions. The owner explicitly chose full-chain technical-limit removal while retaining research budgets and scientific/identity invariants, then approved the implementation and delivery plan.

Endpoint: implemented and locally verified changes, independent final-Head review and applicable hosted CI, followed by merge into remote `dev`. No production deployment or real account/model authorization is included.

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
- Latest `make check` passed: service 8/8, research 21/21, CLI 3/3, Web typecheck/unit/build; deployment helper tests 11 passed/1 existing release-distribution skip, plus 4 Python packaging/release tests. The distribution skip requires a prebuilt release bundle; it is not reported as executed installation acceptance.
- `npm --prefix apps/web run test:e2e` passed 4/4, including model-only save/capabilities, preserved dirty edits on refresh, transient secret/unknown-response retry and cancellation, adjacent research/portfolio/observation/extension flows, 390px keyboard/reduced-motion/layout and axe. Two earlier runs failed on test selectors for Ant Design's selected display (not its empty search input); corrected selectors, retaining the assertions.
- A disposable **real** service with official built-in providers and no inherited credentials served 42 unconfigured providers. At 1440px/390px the page exposed only provider/model/reasoning fields, passed axe and horizontal-overflow checks, raised no page exceptions, and opened/cancelled the official Anthropic API-key prompt without submitting any credential. Screenshots: [desktop](assets/settings-1440.png), [mobile](assets/settings-390.png). Existing Ant Design Drawer-width deprecation is not a page exception or a new regression.
- Controlled official provider tests cover API Key/OAuth/select/manual-code prompts, correlated replay/conflict, cancel-before-start, active-login resume metadata, logout/environment truthfulness, stable installation identity, safe failures, shutdown and secret non-reflection. SQLite tests cover private permissions, serialization, rollback, deletion and restart. These synthetic credentials are test fixtures, not real-account authorization.
- Real child execution/cancellation test passed, including provider-env isolation and joined child exit. Historical tiny task/native settings remain stored but do not prevent a portfolio completing; saved reasoning freezes into admitted research and survives changed preferences/restart/replay. Existing one-original-unsafe-POST restart reconciliation tests pass.
- QZ's hardcoded two-no-progress/maximum-yield stops were also removed as technical execution restrictions. Original explicit trial budget, final/owner stop and error retry policy remain. Regression continues through six empty yields without resetting input/budget; a study can continue consuming model usage until scientific budget/finalization/explicit stop. Native fuel regressions now expect omitted/null fuel to remain unmetered; the infinite-loop failure test explicitly supplies fuel, while malformed memory replaces the obsolete artificial memory-ceiling expectation. No runtime assertions are silently skipped.
- `make check-science` could not complete locally: rustc failed with `No space left on device` during retained contracts/domain compilation. Only this task's failed `target/` artifacts were removed. Local native/core verification remains **unrun**, not passed; applicable hosted final-Head science/core CI must pass before merge.
- Exact `gpt-6-luna` with `max` was unavailable in the credential-free native catalog (zero configured models). Real-model behavior evaluation is **blocked/unrun**; deterministic protocol checks do not establish research judgment, paid-provider/account results or model quality. No fallback model or real credential authorization was inferred.
- `lychee` is not installed; the full Markdown link checker is unrun. New screenshot links point to committed task assets.

<a id="review"></a>
## Review

Independent read-only native review is running on the implementation. Findings, remediation and the exact final committed Head must be recorded before merge; author testing is not independent acceptance.

<a id="delivery"></a>
## Delivery

Delivery proceeds through the scoped task branch and a PR targeting `dev`; GitHub owns exact revision/CI/merge evidence. Applicable final-Head hosted CI and independent review remain pending. No production deployment occurred.

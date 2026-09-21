# Local personal console

## Intent
Implement the owner's four requirements on main `92f501ee7b3d1999e4ed397bd4d5825310a6f330`:
essential UI copy, no TOTP challenge/code, automatically discovered native Codex
with model/effort preferences, and persistent light/dark themes.

## Contract and plan
[DESIGN 0.5](../../../DESIGN.md#local-console) is authoritative. Preserve user data,
immutable historical migrations and scoped machine/research authority. Do not
modify the older dirty CodexPro workspace. The web assistant authors changes;
Actions performs native generation and verification; Codex reviews only.

1. Replace interactive enrollment/login/reverification with automatic local opaque
   sessions; require loopback deployment and preserve Host/Origin/Bearer isolation.
2. Remove Provider/config-file/home registration and credential injection. Discover
   the installed native executable/home; seed separate Researcher/Reviewer roles.
   Model-only preferences retain original revision and uncertain-result replay.
3. Remove explanatory UI copy, preserve actionable failures/labels and add native
   Ant Design light/dark themes without losing form state.
4. Replace obsolete tests with local-entry, boundary, native discovery/model and
   both-theme regressions. Generate contracts and dependency locks natively.
5. Publish a PR, inspect exact-Head CI and independent read-only review; merge only
   when all applicable checks pass and findings are resolved.

## Verification
Checkpoint 1: Actions run `35503066114` applied/ formatted the authored source,
but native Rust and TypeScript checks failed. No pass is claimed.
Checkpoint 2: Actions run `35503533622`, published source
`61feeb87e403d2465957cc61ba0efb5dc206e139`, passed Rust library/binary check and
native contract generation. TypeScript failed on removed-login tests, obsolete
provider typing and unused imports. The next authored test/cleanup batch replaces
those contracts; results are not yet available. Baseline passes are not new evidence.
Checkpoint 3: Actions run `35504620791`, published source
`c848a41e81d8cd7a2b757cb4ddf09026100f8f46`, passed the Rust library/binary check,
native contract generation and the production frontend build. All-targets check
failed on obsolete cookie fallback variables; 540 unit tests passed and three
failed on removed demo/provider contracts. Browser execution was blocked by an
obsolete demo response. These failures remain failures; the next patch updates
the fixtures, adds explicit bad-Bearer regressions and shortens remaining empty
states. No full database/browser or final review pass is claimed.
Dedicated-account acceptance remains COMPLETED_BY_OWNER_WAIVER / NOT_RUN.

## Repair and validation checkpoint

Tracked by [Issue #98](https://github.com/zhengui666/QuaZonai/issues/98).
Baseline: `d7062806d9308939a035dc9a67d2e2c118f0889a`. Downloaded run
`35505410187`, artifact `10603638919`; inspect original outcomes, not green
step conclusions produced by continue-on-error.

| Check at that checkpoint | Actual result |
| --- | --- |
| Rust format, library/binary check and native contract generation | Passed |
| Frontend production build | Passed |
| Frontend unit suite | 544 passed; separate 5 PWA file tests passed |
| All-targets Clippy | Failed: removed login imports/fallback variables, shared-fixture unused items and a single-element loop |
| Browser suite | 103 passed, 12 failed, 1 interrupted, 304 not run |
| Focused Store and HTTP suites | Not executed: unsupported cargo test --keep-going argument |
| Full final-Head CI and independent review | Not obtained |

The authored repair removes stale imports/fallback names, replaces obsolete
authenticator instructions, preserves failure/replay assertions, restores the
actual cost-assumption field without its explanatory paragraph, corrects Menu
and primary-button theme tokens, and makes render recovery inherit the theme.
The removed nested-login crash test becomes a real nested-project crash in
both themes. PWA and hosted-service scope no longer advertise removed login.

All temporary local-console transfer files and their materialization workflow
are removed. The existing CI, Web, Native Runtime, Personal hosting and CodeQL
PR workflows own verification. Their original supported database commands,
test thresholds and permissions are unchanged. Text transfer and native
formatting are not application validation.

The repair was first prepared while connector calls failed. Access was later
restored and Issue #98 created. Transfer35518044736 failed decoding the authored
JSON before touching source; that transfer syntax is corrected, not a product
test pass. No new application-test, independent-review, merge or owner-deployment
result is claimed by this source update. Subsequent PR records must identify
the actual tested Head and original artifacts.

## Delivery
Delivery status and exact tested Head are recorded in [PR #99](https://github.com/zhengui666/QuaZonai/pull/99). All applicable same-Head CI, explicit clean independent review and actual merge are required. This task does not certify unrelated Issue #62 acceptance.

## Integration with the current main
Native Git inspection35523462906/artifact10608198441 compared branchba393b87
with main2135def63. Two conflicts were observed: the historical evaluation UI and
generated response validators. The author preserves the new portfolio equity
curve, removes its neighboring explanatory copy, and regenerates all contracts
using the original Rust/npm generators. The new equity HTTP check covers direct
local reads and explicit bad-Bearer denial; the actual chart browser test runs
in both themes at all existing viewports. No scientific output is rewritten.
The temporary inspection/preparation files must be absent from the delivered
tree. Inspection success is not an application, CI or independent-review pass.

## PR99 review corrections in progress

Issue98 comment5750675699 records the field-level correction plan for the nine
independent findings on Head2da592747987393dd3f9676ab6b491e938a5b1c8. No thread is
resolved on the strength of authored text alone.

That exact Head's Native Runtime35518292496 passed: original artifacts contain
17 OCI,2 cold/ownership,1 joint restore,6 research-loop and5 science-thread passes.
Personal hosting35518292499 and CodeQL35518292526 passed. CI35518292491 failed
documentation link checking and two old-schema fixtures in the first HTTP test
binary; later Store/HTTP targets were not executed by the original fail-fast
Cargo command. Web35518292523 passed build/type/unit(544+5) and separate Demo,
then failed17 of426 browser cases(409passed). Actual packaged-service browser
acceptance was skipped and is not a pass.

The next authored source removes inherited Provider credentials, creates
collision-free database-native local roles, serializes their shared account
lifecycle and invalidates both observations, derives Mission Origin from the
actual local API, guards new model overrides without breaking unknown replay,
and fixes direct-session SSE revocation tests. Regression tests use actual
PostgreSQL migration/admission plus declared frontend fixtures and both themes.
Current docs no longer advertise removed TOTP/custom Provider/remote login.
The existing CI uses native --no-fail-fast to expose all test-binary failures;
test selection, concurrency limits, scientific rules and timeouts are unchanged.

These are unverified corrections until the new Head's original workflows and
independent read-only review complete. No owner account or machine was accessed.

## Final integration regression corrections

At e305899, Web35523836975 built the original Rust API and frontend, passed548
unit tests,5PWA file tests,8service-helper tests and the separate Demo scenario.
The full three-viewport suite passed468/471: all3 failures were the preview's
incorrect Researcher-first assertion. The preview's model observation route
also lacked a response; the correction returns only NEVER_PROBED/null for its
two synthetic roles, never a fabricated native catalog, and explicitly exercises
both role choices without assuming list order. Hosted-service acceptance was
not reached and is not counted as passed.

CI35523836841/artifact10609475872 ran all Store/Server binaries:681passed,6failed,
0ignored/filtered. Correct the deleted verify call, local missing-record/security
schema expectations and old-schema/epoch fixtures; retain original migration,
expiry, cryptographic-slot, receipt and bad-Bearer checks. Native Rust contracts,
scientific/protocol and NativeRuntime35523836792 passed at e305; source changes
require fresh results, not reuse of those passes.

Independent review5261136146 found three remaining defects: active model
settings lacked Store-side catalog validation, and operations/reuse/dependency
records still described removed capabilities. New active commands now reuse the
existing native override/tier rules with the latest, unexpired, matching profile
observation and shared-account invalidation; an older success never masks a new
failure. Check after native-binding work and before mutation; native-default
recovery and original receipt replay remain possible without a fresh catalog.
Real PostgreSQL regressions cover absent/stale/expired/failed/unsupported data,
account mutation, supported fast-only settings, and exact replay. Raw HTTP must
not activate unobserved values or start a probe implicitly. No dependency, API
schema, historical result or user database is changed by this correction.

The first focused preparation35525522863/artifact10610195945 ran32Store
cases successfully but rejected the new already-expired fixture at the original
database insert guard. No application/HTTP/Web pass is inferred from skipped
later steps. The corrected expiry case inserts a valid, short-lived relational
fixture and observes its natural expiration on the database clock; it changes
neither the production60s cache nor an immutable observation. The remaining
Custom Provider sentence in DESIGN and obsolete CLI request guidance are also
removed; historical backup provenance remains accurate.

## Native-default model provenance correction

Review5261270900 on26a46a2 found that clearing an active model reused the
post-override effective model to validate effort/Fast, and two CLI examples
still used a public origin. Issue98 comment5753259522 owns the field-level plan.

The adapter already observes an override-free Thread; retain its model as
`native_default_model` separately from `effective.model`. New publications
require this provenance. The optional wire shape only permits historical
observations to remain readable without rewriting them; absent provenance
does not authorize inherited effort/Fast. Store/UI use the native default when
the model is cleared and retain original receipt replay and native recovery.
No configuration parser, probe on save, dependency or database migration is
introduced. Tests cover divergent model capabilities, misleading catalog
defaults, historical records, both browser themes and actual native B→A→B
Thread observations with zero Responses calls. CLI examples use the actual
loopback origin and matching HTTP flag.

All five original26a46a2 workflows completed successfully (CI35526549909,
Web35526549879,Runtime35526549926,PersonalHosting35526549873,CodeQL35526549901).
They do not approve this later source change. Native formatting/generation and
focused regression preparation, fresh full PR CI and independent read-only
review are required. No merge, private account, user-host operation or #62
completion is claimed by this source record.


## Local origin end-to-end and current T05 contract

Independent e5cb33f review4058433404/406/409 found residual literal-only
Mission/MCP validation and obsolete CLI/T05 documentation. Issue98
comment5753524650 records the correction. Reuse WebPolicy through launcher,
native Thread start/resume and MCP; preserve the exact configured localhost
origin and explicit HTTP flag. Runtime/Downstream endpoint rules remain
unchanged. Original native Mission/HTTP/PostgreSQL and scientific-feedback
fixtures now exercise localhost for the control plane, not only an outer
configuration check. No scientific, scope, resource or timing rule is relaxed.

The old custom-Provider T05 is superseded by mandatory credential-free native
configuration-ownership and rejected-registration coverage, not marked waived
or passed. The account-only waiver excludes this replacement; all other
non-account obligations remain.

Fresh focused native tests, all original final-Head workflows and independent
read-only review are required. The previous e5 Web pass is not approval of this
later source. No user-host installation, real account or #62 completion claimed.

## Explicit transport and recovery-state follow-up

Review5262205269 on027e156 identified inferred Worker HTTP opt-in and an
independent recovery theme that lost session-only selection with blocked
storage. Issue98/comment5754123055 specifies the correction. Worker now
validates and forwards the parsed flag, not a scheme-derived value. A single
native React context owns the existing theme state above the error boundary;
the application and recovery share it. Actual browser regressions toggle
with blocked storage before a nested render error in both system themes.

CI35545652416 returned694Store/Server passes and one failed daily-quota
assertion. Its old assertion did not record the actual result, so a root cause
is not claimed from that log. The regression now distinguishes deliberate
SKIP LOCKED contention from an uncontended quota check with a native committed
row-lock barrier and diagnostic output. Both exact quota assertions and
unchanged run/attempt counts remain. Production scheduler, limits and timing
are unchanged. No sleep or retry converts a failure into success.

All source in this follow-up requires actual native verification, all original
same-final-Head workflows and explicit clean independent read-only review
before merge. Account-only waivers, owner-host nondeployment and remaining
Issue62 boundaries are unchanged.

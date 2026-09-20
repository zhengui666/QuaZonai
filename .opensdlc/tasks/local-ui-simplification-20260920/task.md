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
Not merged. This task does not certify unrelated Issue #62 acceptance.

# Shared ChatGPT account with independent role settings

<a id="intent"></a>
## Intent

The owner requested development from latest main, one ChatGPT Codex account for
researcher and independent reviewer, independently configurable model, reasoning
effort and speed, then a PR reviewed and merged into main. Production deployment
and changes to the owner's native account/configuration are outside this task.

<a id="spec"></a>
## Requirements and design

Reuse the existing local role profiles, shared native account admission and
separate Mission workspaces/Threads. Show the shared login above role settings.
Each profile retains its own model, effort, speed and revision. Native defaults
omit overrides; custom standard speed explicitly overrides native fast defaults.
Fast speed remains conditional on the selected model's fresh native catalog.
Do not change frozen Cycle selections or introduce a second credential store.

<a id="plan"></a>
## Implementation plan

1. Trace profile update/probe, native Thread options, Mission bootstrap, shared
   account operations and the Codex settings UI from main `323054f2`.
2. Correct speed resolution in `crates/domain/src/codex.rs`, native deployment
   inspection, observation validation and `store/lifecycle/mission.rs` session
   receipt invariants; retain existing wire/storage fields.
3. Clarify shared login and role-specific model/effort/speed in the existing web
   components. Update DESIGN and OPERATIONS with the actual behavior.
4. Extend native process, domain, PostgreSQL and browser regression coverage;
   run focused checks, submit the PR, resolve current-head review/CI, then merge.

<a id="verification"></a>
## Verification

Pre-fix reproduction with the pinned official Codex 0.156.1 App Server, an empty
temporary HOME/CODEX_HOME and `service_tier = "fast"`: an omitted `serviceTier`
returned `priority`, while explicit `serviceTier: "default"` returned `default`.
No account login or model turn was performed. The native catalog advertises
optional fast tiers but does not list the built-in standard tier.

The extended `server --test codex_probe` regression failed before implementation:
custom standard speed returned `Some("priority")` instead of `Some("default")`.

Local verification:

- `cargo test --locked -p domain --test codex_profiles --test rules`: 47 passed.
- `cargo test --locked -p server --features native-codex --test codex_probe`:
  5 passed, including real native default, custom standard, custom fast and
  restored default probes without starting inference or modifying native config.
- `make check-web`: generated clients unchanged, typecheck passed, 542 tests
  passed, production/PWA build succeeded.
- `npm --prefix apps/web run test:auth-ui`: 4 passed, including independent role
  settings, one shared login, lost acknowledgements and reload. Inspected the
  actual 390px/1280px browser screenshots; the account/model fixtures are synthetic.
- Store/PostgreSQL execution and the complete CI matrix run in GitHub Actions;
  the local Docker socket is unavailable. No production database is used.
- `cargo test --locked -p store --lib native_speed_receipt_matches_the_frozen_role_settings`:
  reproduced the old receipt invariant failure, then passed all 20 default/custom
  and speed combinations in one regression check after correction.
- `cargo clippy --locked -p domain -p store -p server --lib --tests --features server/native-codex -- -D warnings`:
  passed, including compilation of the new standard/fast Mission start/resume tests.

Live ChatGPT authorization and paid inference are not claimed by the isolated
protocol/UI tests. Hosted CI owns the complete current-head verification results.

<a id="review"></a>
## Review

[PR #117](https://github.com/zhengui666/QuaZonai/pull/117) owns GitHub Codex findings, remediation, current-head CI
and the merge decision. Only an explicit clean review and passing applicable CI
permit the requested merge; earlier revisions are not approval of later changes.

The first review identified that the Mission receipt invariant still rejected a
custom role's explicit standard tier after Thread creation. The invariant now
requires an omitted tier for native defaults, `default` for custom standard, and
`priority`/`fast` for custom acceleration. Added real App Server + PostgreSQL
bootstrap, turn and same-Thread recovery cases to the existing Mission CI suite.

<a id="delivery"></a>
## Delivery

Worktree: `/home/zzy/.codex/worktrees/codex-shared-role-settings/QuaZonai`.
Branch: `codex/shared-account-role-settings`. The dirty primary checkout is
preserved. The owner authorized implementation, PR and merge in this session;
the older web-only execution wording does not prevent this requested work.

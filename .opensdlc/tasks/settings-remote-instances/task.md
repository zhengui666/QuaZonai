# Unified settings and remote instances

Owner request: add a Settings page containing Model settings, Extensions and Instance connections, so remote Nautilus backtest, paper and live instances can be configured in the interface.

Base: remote `dev` at `30e51168`; branch: `codex/settings-remote-instances-20261026`.

## Acceptance

- One Settings entry exposes model settings, extensions and remote instance connections on desktop and mobile; existing model settings and extension URLs remain usable.
- The owner can explicitly save a backtest/research worker and independent paper/live connections using the existing endpoint, account, client and credential-reference fields.
- Successful saves survive reload and service restart. Invalid input, persistence failure, stale updates and uncertain responses remain distinguishable from success.
- New work captures the configured instance. Existing research, PM and observation facts retain their original bindings; clearing a configuration cannot silently fall back to an obsolete process-wide instance.
- Actual HTTP and browser checks verify configuration and recovery. These checks do not establish remote-worker health, scientific quality, real-account access, trading or a user deployment.

## Implementation and verification

The Web groups the original model and extension controls under Settings. A separate revisioned `/api/v3/settings/instances` resource owns instance configuration, using an atomic private `instances.json` in the existing data directory. No applied database migration or original frozen business fact is changed.

Final applicable CI, independent review and GitHub Codex review must cover the delivered Head before merging to `dev`, as required by repository instructions.

- Service: typecheck, 304 passing tests and 21 native-fixture skips. Research: typecheck and 186 passing tests. CLI: typecheck and 9 passing tests.
- The first aggregate run hit an unchanged Worker disconnect receipt race (`UNKNOWN` versus `CANCELLED`). An independent review located a pre-existing old-file/active-map window; the unchanged Node 22 Worker suite passed 5/5, and 12 isolated Node 24 repetitions passed. No runtime-version cause is established and no Worker code was changed here. The issue is tracked separately.
- Actual production assets on disposable HTTP/Durable storage verified backtest + paper + live saves, reload, restart, mobile layout and accessibility. Four targeted browser tests passed, including uncertain-save reconciliation, revision conflicts and an unmounted-response regression. The first complete local browser run passed 63/64, with the remaining click timing out in Playwright's post-action navigation wait after the page already showed the expected pending-save state; the isolated case passed. Its SPA action now skips that irrelevant wait and explicitly asserts the original URL and pending-save state.
- Hosted browser checks on the first Head passed 64/65; the new real-HTTP test ambiguously selected an exiting dropdown alongside the current dropdown. The test now selects options using the active connection combobox's `aria-controls`, and its real-HTTP rerun passed.
- Web typecheck and production build passed. Deployment helpers passed except an initial packaging check could not find LICENSE in the managed Node installation; the packaging and remaining release checks passed with the official checksum-verified Node 22.19.0 distribution.

The implementation received independent review and an explicit clean GitHub Codex review on the first Head. Updated exact-Head CI and review remain required after the browser-test corrections. No local native science check, merge, release or production deployment is claimed by this pre-merge task record.

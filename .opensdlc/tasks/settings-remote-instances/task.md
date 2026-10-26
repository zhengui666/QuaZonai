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

Final applicable CI, independent review and GitHub Codex review must cover the delivered Head before merging to `dev`, as required by repository instructions. Service typechecking and 6 targeted HTTP/configuration/remote-routing checks passed. Actual production assets on disposable HTTP/Durable storage verified backtest + paper + live saves, reload, restart, mobile layout and accessibility. Controlled browser checks cover uncertain saves and revision conflicts; independent review identified and the author fixed a late unmounted-response cache rollback, with a held-response navigation regression. The full local TypeScript and browser suites are still running. Deployment helpers passed except an initial packaging check could not find the managed Node runtime LICENSE; the packaging and remaining release checks passed when rerun with the official checksum-verified Node 22.19.0 distribution. No local native science check, merge, release or production deployment is claimed yet.

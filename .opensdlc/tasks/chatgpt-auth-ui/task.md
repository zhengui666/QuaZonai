# ChatGPT authentication in the web console

<a id="intent"></a>
## Intent

The owner requested current main, design and implementation of ChatGPT Auth in
QuaZonai's frontend, followed by a PR and merge. Preserve the existing dirty
checkout; work from `d01f5cc9` in the isolated `feat/chatgpt-auth-ui` branch.
The endpoint is a reviewed merge, without a production deployment or private
account authorization.

<a id="spec"></a>
## Requirements and design

Settings → Codex exposes ChatGPT login, native operation progress, a copyable
one-time code and the native HTTPS authorization link, cancellation, and confirmed
logout. The owner completes authorization on OpenAI's page; QuaZonai polls its
existing operation endpoint and refreshes both roles' account/model observations.

Reuse the native account-operation API and its immutable command receipts.
Network ambiguity retains the exact request/revision/idempotency key. A 202 or
cancel acceptance is not successful authorization/cancellation. A local deadline
hides the code without inventing a native terminal outcome. Reloading reads the
shared operation without recovering secrets; the owner can cancel and restart.
The code stays in initiating component memory, outside query/mutation caches and
browser persistence. OAuth tokens remain owned by native Codex in its persistent
home. No new OAuth implementation, backend route, dependency or migration.

<a id="plan"></a>
## Implementation plan

Inspected `codex.tsx`, generated contracts, `codex_profiles/account.rs`, the Store
account state machine and shared-role rules, native login validation and container
deployment. Add the account controls beside existing model settings; suspend
automatic probes/model edits during account operations and retain navigation
guards. Update DESIGN and deployment instructions. Exercise disclosure/terminal
states with unit samples and the real Rust browser harness for unavailable
deployment and lost-acknowledgement retry. Keep real account authorization under
the existing [account acceptance scope](../../../DESIGN.md#acceptance-scope).

<a id="verification"></a>
## Verification

Local type checking, all 542 Vitest cases, the production build and unchanged
generated API/validators passed. The 8 native user-service harness checks and
script syntax checks passed. Three Playwright interaction tests passed for lost
login/cancel acknowledgements, preserved request identities, confirmed completion,
model refresh, reload recovery and timeout presentation. Waiting-state screenshots
were inspected at 1280px and 390px. These tests use explicit synthetic HTTP/code
samples, not an OAuth authorization or account persistence result.

The added real Rust/PostgreSQL browser case exercises actual failed account
receipts, same-key replay and shared-role/reload reads with an explicitly unavailable
Codex deployment; it cannot discover the host owner's account. Real-service
acceptance runs in the existing Web CI. Real account authorization remains NOT_RUN
under the existing owner waiver. CI and review results belong to the containing PR.

<a id="review"></a>
## Review

Require applicable CI and explicit clean GitHub Codex review on the final PR head.
The PR is the authority for findings and current results.

<a id="delivery"></a>
## Delivery

Pending PR, review and merge. No production service or credential has been changed.

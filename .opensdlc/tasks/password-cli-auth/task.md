# Instance password and persistent CLI devices

<a id="task"></a>
## Task

Implement the owner's 2026-09-26 request: password setup/login, optional 30-day browser persistence, interactive CLI login using the frontend address and the same password, and settings for password changes and revoking individual CLI machines. Deliver implemented, verified code and a reviewable PR; production deployment is not requested.

<a id="intent"></a>
## Intent

Replace automatic local browser admission and externally provisioned owner CLI credentials. Keep the single-user product, existing scientific/Reviewer/Downstream boundaries, and persistent data. The owner explicitly authorized discarding all old local QuaZonai worktrees/branches and starting from refreshed main. Main 0c37ebfd removes the previous local-authoring restriction.

<a id="spec"></a>
## Requirements and design

- The first browser visit sets an instance password; subsequent visits require login. Unchecked persistence uses a session cookie with a fixed 12-hour server deadline; checked persistence uses a fixed 30-day cookie/server deadline. Expired/revoked cookies never silently regain authority.
- Reuse Argon2id, opaque capabilities, tower-sessions, PostgreSQL row locks, the native CLI transport and Ant Design. Passwords stay out of logs, argv, browser storage and CLI profiles. Password setup is atomic; password verification is rechecked against the locked password version before admission.
- `server client login` prompts for the frontend origin and a hidden password, then stores a private owner-device token locally. Subsequent commands load this connection automatically. Devices have no automatic expiry and require no extra one-time operator grant. Existing scoped Mission, Reviewer, Automation and Downstream credentials retain their rules.
- Settings has a dedicated authentication category for changing the password and listing/revoking CLI devices. Changing a password invalidates browser sessions; connected CLI devices remain until explicitly revoked. Logout invalidates its server session. Restore cutover revokes CLI devices too.
- API: anonymous `GET /auth/status`; browser `POST /auth/setup`, `/auth/login`, `/auth/logout`, `/auth/password`; `GET /auth/session`; CLI `POST /auth/cli/login`, `GET /auth/cli/session`; browser management `GET /auth/cli/devices`, `DELETE /auth/cli/devices/{id}` (all under `/api/v2`). Password operations never use command receipts containing secrets.
- HTTPS origins may use the public frontend hostname while API listening stays loopback; explicit development HTTP remains loopback-only. Keep Host/Origin checks, secure cookies, no redirects and no mixed cookie/Bearer authority.

<a id="plan"></a>
## Plan

1. Extend `contracts::auth`, append a migration, implement password/session/device transactions in Store, and reuse integration crypto. Route device actors through owner business authority with locked revocation checks; never expose this identity to bound Missions.
2. Implement HTTP admission and auth routes, preserving CSRF and capacity limits. Update current tests/fixtures and generate OpenAPI/client schemas from Rust.
3. Independent CLI work: native interactive login/private profile, default connection reuse, device identity, and portable Skill instructions. Independent Web work: login gate, authentication settings, session expiry handling and native browser checks. Isolate these work packages from backend edits and integrate before verification.
4. Run focused real PostgreSQL/HTTP/CLI tests, Rust formatting/lint, contract generation and Web type/tests/build; use actual browser acceptance. A fresh read-only verifier reviews integrated auth and Skill behavior. Record actual results and remaining external checks here.

<a id="verification"></a>
## Verification

The original checkout and 14 linked worktrees/32 local branches were cleared only after explicit owner authorization. A fresh fetch still matched main 0c37ebfd before delivery. Temporary implementation worktrees were removed after integration. Production containers and data were not modified.

Executed locally against an isolated PostgreSQL 18/PGMQ instance on port 55439:

- `cargo test -p server --test auth_http --test control_http`: 13 passed, covering password admission, session deadlines, logout/change, device revocation, CSRF and existing scoped authority.
- `cargo test -p store --test auth --test browser_epoch --test control --test upgrade_cutover`: auth/epoch/control passed; upgrade tests initially exposed stale epoch expectations. After accounting for the appended password migration, all eight upgrade tests passed. Rollback, history and lock assertions remain intact.
- Native CLI help, Skill package, real terminal login and transport checks passed (24 tests), including a password equal to `schema_version`, hidden input, private profile reuse, writes without grants, device revocation and rejection of explicit owner-token credential files. Five experiment HTTP tests also passed.
- Web types, 542 unit tests and production build passed. Four separate ChatGPT authentication UI tests passed. Native browser acceptance found and drove fixes for empty-table contrast, modal-exit timing and keyboard access to horizontally scrolling empty tables; the final run is recorded below when complete.
- Pinned lychee 0.24.2: zero broken local links; deployment guide/help tests: four passed; smoke script parses successfully.

The complete migration/recovery rerun, workspace Clippy, final native browser acceptance and hosted CI/review remain pending at this checkpoint. These pending results are not passes. Real model-driven service Skill rollouts are unrun: no disposable model runner/comparative baseline is configured; the following independent review is source/document verification only.

<a id="review"></a>
## Independent review

A fresh read-only verifier inspected backend admission/revocation/restore, Web flows and portable Skill instructions. Fixed findings: native browser CLI registration needed Origin and HTTP 201; container smoke needed explicit password admission; successful CLI responses wrongly treated protocol keys matching the password as a leak; explicit owner credential files differed between preview and execution. The verifier reread the CLI fixes at e5d0b5ef and found both resolved without new findings. Bound Mission failure still returns to its launcher, revoked machines require private terminal login, and report text cannot authorize credential access. This is separate from GitHub final-Head review.

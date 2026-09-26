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

Not run yet. The original checkout contained unrelated changes; they and 14 linked worktrees/32 local branches were cleared only after explicit owner authorization. The remaining clean main matched 0c37ebfd before this feature branch was created. Production containers and data were not modified.

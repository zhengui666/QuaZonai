# Settings autosave

<a id="task"></a>
## Task

Implement the owner's 2026-09-28 request: editable settings save automatically, without unsaved-change navigation blocks or prompts. This task stops at a reviewable code change; no production deployment was requested.

<a id="intent"></a>
## Intent

Persist valid edits to existing Codex role, Runtime, Downstream and data-source configuration without a Save step. Navigation through Settings and away from Settings stays available. A failed or unknown write must remain visible and recoverable; it cannot be presented as saved.

Password changes, initial registrations, credential registration, data grants/revocations and imports are explicit operations with separate consequences and retain their submit actions.

<a id="spec"></a>
## Requirements and design

- Send valid configuration edits through the existing revision-checked, idempotent APIs. Keep writes ordered per resource across editor closure/reopening and retain an uncertain request identity until retry resolves it.
- Let users leave a settings category or the page without an unsaved-change warning. Remove settings-only guards and discard prompts; keep genuine operation/error feedback.
- Never send incomplete credentials, password fields or partially valid new records as background writes. Preserve form validation and server error visibility.
- Retain active ChatGPT device challenges and explicit import identities in page memory across Settings navigation. A page reload intentionally does not persist device codes. Defer PWA updates while a settings write or recoverable operation is active.

<a id="verification"></a>
## Verification

`make check-web` passed: generated contracts unchanged, TypeScript passed, 542 unit tests passed, and the production build succeeded. `npm --prefix apps/web run test:auth-ui` passed 19 Chromium tests against contract-validated API fixtures. They cover independent Codex roles, navigation during in-flight and uncertain saves, Runtime/Downstream/data-source autosave, ordered edits across editor reopenings, same-key retry after a lost response, corrected input after a definitive rejection, validation on close, one-time credential and CA reference binding, navigation with a device challenge, terminal native account operation reconciliation, password-change errors after navigation, and explicit import retry and rejection after navigation. The 1280px and 390px role-settings screenshots were inspected for layout and horizontal overflow.

`make check-links` could not start because the pinned `lychee` executable is absent locally; the PR link below was verified by creating and reading the PR. The native Rust/PostgreSQL/Caddy browser harness was not run locally; its disposable database administrator and packaged server prerequisites were not supplied in this worktree.

<a id="delivery"></a>
## Delivery

[PR #131](https://github.com/zhengui666/QuaZonai/pull/131) carries the web change. No contract, migration or production deployment is included. Final-Head CI and GitHub Codex review remain owned by the PR.

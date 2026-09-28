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

- Send valid configuration edits through the existing revision-checked, idempotent APIs. Keep writes ordered per resource and retain an uncertain request identity until readback or retry resolves it.
- Let users leave a settings category or the page without an unsaved-change warning. Remove settings-only guards and discard prompts; keep genuine operation/error feedback.
- Never send incomplete credentials, password fields or partially valid new records as background writes. Preserve form validation and server error visibility.

<a id="verification"></a>
## Verification

`make check-web` passed: generated contracts unchanged, TypeScript passed, 542 unit tests passed, and the production build succeeded. `npm --prefix apps/web run test:auth-ui` passed seven Chromium tests against contract-validated API fixtures. They cover independent Codex role settings, navigation during an in-flight save, Runtime/Downstream/data-source autosave, ordered edits, uncertain-response retry with the same idempotency key, one-time credential reference binding, and closing before the debounce expires. The 1280px and 390px role-settings screenshots were inspected for layout and horizontal overflow.

`make check-links` could not start because the pinned `lychee` executable is absent locally; this new task record has no links. The native Rust/PostgreSQL/Caddy browser harness was not run locally; its disposable database administrator and packaged server prerequisites were not supplied in this worktree.

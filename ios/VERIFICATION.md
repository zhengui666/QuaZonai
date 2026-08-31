# Native Client Verification Map

The latest implementation HEAD is verified through independent GitHub workflows rather than one aggregate shell command:

| Contract | Evidence |
| --- | --- |
| FastAPI wire contract | Canonical OpenAPI export and drift check |
| TOTP-only native authentication | Backend unit/integration tests and OpenAPI negative-field gate |
| Trusted-device lifecycle | Refresh rotation, concurrent refresh, revoke/logout, and session tests |
| Web/iPhone/iPad capability parity | `contracts/client-capabilities.yaml` and `tools/client_parity.py` |
| Native build | Swift package resolution and iPhone/iPad simulator builds |
| Native interaction | iPhone and iPad UI tests against the shared fixture |
| Event synchronization | SSE parsing, cursor recovery, reconnect, duplicate protection, and authorization termination tests |
| Offline behavior | Read-cache and Idea-draft tests; offline mutation denial |
| Localization/accessibility | Seven-language catalog, Arabic RTL, VoiceOver, Dynamic Type, focus, and chart-summary checks |
| Security/privacy | Keychain, non-persistence/redaction, HTTPS policy, privacy cover, Privacy Manifest, and forbidden-surface scans |
| Distribution readiness | Unsigned generic iOS archive, AppIcon, Launch Screen, metadata, privacy policy, and screenshot plan |
| Existing product | Backend, frontend, PostgreSQL, compose, Operator auth, and remote Nautilus workflows |
| Independent review | GitHub `@codex review` on the exact all-green HEAD |
| Completion | Squash merge followed by Main Post-Merge Verification on the merge SHA |

A generated artifact workflow may commit only the deterministic OpenAPI snapshot or AppIcon. A connector-authored follow-up commit then forces every validation workflow to execute on the resulting exact HEAD before review.

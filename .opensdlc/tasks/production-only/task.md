# Production-only repository

## Intent and scope

Owner request: maintain QuaZonai as a production system repository; delete docs/, remove executable Demo/Mock paths and development-status disclaimers. Preserve real research behavior, historical data, immutable migrations and licenses.

## Implementation

Delete the standalone in-memory UI backend and its dedicated fixtures/configuration/tests. Use a single real Rust/PostgreSQL/Worker/Caddy browser entry; cover viewport, accessibility and PWA behavior there. Remove the client’s injectable transport; retain strict response parsing as production functions with pure input tests. Keep historical non-real provenance rejection rather than relabeling old data. Consolidate necessary installation, recovery and data-preparation instructions into the existing root entrypoints and native Runtime README; remove obsolete historical ledgers.

## Verification and delivery

Executed in GitHub Actions: generated frontend consistency, TypeScript typecheck, all 528 unit cases, production static/PWA build, 14 installation shell cases and 8 user-service helper cases passed. Current-Head hosted browser/PWA, Rust/database/native Runtime regressions, Markdown links and independent review are tracked in PR #106. Transfer workflows are temporary and excluded from delivery. No user database or running service is modified.

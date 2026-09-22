# Production-only repository

## Intent and scope

Owner request: maintain QuaZonai as a production system repository; delete docs/, remove executable Demo/Mock paths and development-status disclaimers. Preserve real research behavior, historical data, immutable migrations and licenses.

## Implementation

Delete the standalone in-memory UI backend and its dedicated fixtures/configuration/tests. Use a single real Rust/PostgreSQL/Worker/Caddy browser entry; cover viewport, accessibility and PWA behavior there. Remove the client’s injectable transport; retain strict response parsing as production functions with pure input tests. Keep historical non-real provenance rejection rather than relabeling old data. Consolidate necessary installation, recovery and data-preparation instructions into the existing root entrypoints and native Runtime README; remove obsolete historical ledgers.

## Verification and delivery

Pending execution against this branch: generated-contract consistency, typecheck/unit/build, actual hosted browser/PWA tests, Rust/database/native Runtime regressions, Markdown links, and current-Head independent review. The source-bundling workflow is temporary and is not part of the delivered tree. No user database or running service is modified.

# Issue62 implementation evidence

DESIGN is the normative contract; this file records evidence, not a second design.

## Current changes

- Rename apps/job, crates/contracts, crates/domain and related package/build paths without qz- directories.
- Delete legacy backend/frontend/plugin/deployment implementations, obsolete tests and archived design; retain Git history, LICENSE/NOTICE and user-data boundaries. There is no legacy compatibility service.
- Replace PyO3 scientific probes with Rust-native Nautilus0.63.0, Clarabel0.11.1 and Arrow56.2.0. Upgrade Rust1.98.0 rather than using an old compiler as a Python exception.
- Fix mandatory nullable metric keys, INVALID_INPUT evidence classification, UUIDv7 schema, Fast=false omission, cancellation-after-real-failure and ISO currency membership. Preserve version/lease/terminal and precision tests.
- Complete previously missing design contracts for SelectionRule, immutable data authorization, native snapshot identity, logical feedback deduplication, operator rejection, candidate uniqueness, integration setup and machine credentials. These design additions do not claim their SQL/API implementation exists.

## Actual local evidence

35 Rust tests pass (7 contract,20 domain,8 native/report), strict Clippy and rustfmt check pass. A real native run produces weights0.7999999999997491/0.20000000000025078,745 iterations,12 native orders,24 native events and an Arrow round-trip; FIXTURE, deliverable=false, python_runtime=false. Counts are observed, not assertions of full business acceptance.

The upstream feasibility run33952841460 independently compiled Rust Arrow/Clarabel/ISO and ran the official Rust Nautilus example without PyO3. It is a development study, not a final product lock/check. New source needs its own latest-Head CI and review.

## Explicit gaps

Full Store/migrations/API/Worker/CLI/MCP, autonomous same-Thread model/tool/job/evidence cycle, independent Reviewer, PIT/sealed isolation, multi-Alpha shared-capital portfolio, target-only approvals/Paper/Live/Forward/Wake, Ant Design product UI/PWA, migration/backup/restore and protected real-account acceptance remain incomplete. Removing legacy tests does not satisfy new acceptance. Do not merge until all W0–W8/T01–T42 and CI/review boundaries are met.

Earlier references to nonexistent commits0f1b84a and2e98539 were incorrect and must not be used as evidence. The actual starting tree was45153c6956f556449d5a5acb4b3abfef0a68df9e at eea0f2e; only successfully published and reread GitHub Heads count.

# Issue62 implementation evidence

DESIGN is the normative contract; this file records evidence, not a second design.

## Current changes

- Rename apps/job, crates/contracts, crates/domain and related package/build paths without qz- directories.
- Delete legacy backend/frontend/plugin/deployment implementations, obsolete tests and archived design; retain Git history, LICENSE/NOTICE and user-data boundaries. There is no legacy compatibility service.
- Replace PyO3 scientific probes with Rust-native Nautilus0.63.0, Clarabel0.11.1 and Arrow56.2.0. Upgrade Rust1.98.0 rather than using an old compiler as a Python exception.
- Fix mandatory nullable metric keys, INVALID_INPUT evidence classification, UUIDv7 schema, Fast=false omission, cancellation-after-real-failure and ISO currency membership. Preserve version/lease/terminal and precision tests.
- Complete previously missing design contracts for SelectionRule, immutable data authorization, native snapshot identity, logical feedback deduplication, operator rejection, candidate uniqueness, integration setup and machine credentials. These design additions do not claim their SQL/API implementation exists.

## Verification contexts (do not inherit results across Heads)

At `e8668ca850def834735414ed9ba94fed38d4aa7e`, the committed suites contain
8 contract, 28 domain and 8 native/report tests: **44 total**, not the earlier 35.
[CI 33962063262](https://github.com/zhengui666/QuaZonai/actions/runs/33962063262)
validated that exact dependency lock. The historical local 36 contract/domain
checks used a development lock and did not establish product-lock acceptance.

The subsequent mission-turn/bigint review-fix source has been tested locally on
Rust 1.98.0 **with the unchanged committed Cargo.lock**: 9 contract, 35 domain,
8 native/report tests, **52 total**, zero ignored/failed. Strict workspace Clippy
and `cargo fmt --all -- --check` pass. Both Rust parsing and generated schema
checks consume the shared corpora: 242 bigint and 204 decimal cases.

Mission turn reservations count used plus outstanding total and repair turns,
are bound to the exact Mission identity, and share the cycle token/cost guard.
A follow-up model turn does not reserve another experiment or job slot. Tests
cover disabled repairs, exact caps, overflow, missing/mismatched Mission usage,
used and in-flight turns, failed-reservation nonmutation and property cases.
These are pure rules, not yet a claim of persistent worker/lease admission.

The earlier native fixture run produced weights
0.7999999999997491 / 0.20000000000025078, 745 iterations, 12 native orders and
24 native events with Arrow round-trip. This is explicitly FIXTURE,
`deliverable=false`, `python_runtime=false`, not full business acceptance.
The upstream feasibility run 33952841460 is a development study, not a final
product check. Every new Head still needs independent read-only CI and review.

## Explicit gaps

Full Store/migrations/API/Worker/CLI/MCP, autonomous same-Thread model/tool/job/evidence cycle, independent Reviewer, PIT/sealed isolation, multi-Alpha shared-capital portfolio, target-only approvals/Paper/Live/Forward/Wake, Ant Design product UI/PWA, migration/backup/restore and protected real-account acceptance remain incomplete. Removing legacy tests does not satisfy new acceptance. Do not merge until all W0–W8/T01–T42 and CI/review boundaries are met.

Earlier references to nonexistent commits0f1b84a and2e98539 were incorrect and must not be used as evidence. The actual starting tree was45153c6956f556449d5a5acb4b3abfef0a68df9e at eea0f2e; only successfully published and reread GitHub Heads count.

# Native compatibility matrix

| Component | Pin | Verified boundary |
|---|---|---|
| Rust | 1.98.1 | Pinned compiler patch for the 1.98.0 vtable miscompilation; current-pin revalidation is required, historical 1.98.0 runs are not new evidence |
| Nautilus backtest/model/trading | 0.63.0, release family v2.0.0rc4 | Rust-only synthetic EMA fixture,745 iterations/12 orders/24 events; target-weight/market/isolation acceptance remains separate |
| Clarabel | 0.11.1 | Native QP golden and infeasible certificate; no Python binding |
| Arrow | 56.2.0 | Rust RecordBatch IPC schema/provenance/value round-trip |
| Codex | 0.144.4 | Official binary: account-policy/absence checks; native tool discovery -> stdio MCP -> HTTP -> PG; workspace write/outside credential sentinel; cumulative tool-loop usage and same-Thread process restart, with synthetic model responses, not real-account inference |
| PGMQ | 1.10.0 on PG18 | Native transaction rollback/redelivery/archive fixture, not full production domain recovery |
| SQLx | =0.9.0, one workspace dependency | Original PostgreSQL/SQLite drivers, transactional migration and test database lifecycle; final-Head Store/session/recovery checks are required after the upgrade |
| Runtime SQLite | libsqlite3-sys =0.37.0, bundled SQLite 3.51.3 | [Journal regression](../../apps/runtime/tests/journal.rs) checks the linked engine, WAL, identity and integrity; existing OCI/cold-restore tests remain separate from full coordinated recovery |
| Browser session store | tower-sessions =0.15.0; official SQLx adapter Git revision d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d; time =0.3.47 | Native Postgres session schema/serialization and existing authentication/recovery tests; the adapter is a fixed upstream revision, not a newly published compatible crate |

Platform: Linux x86_64. Cargo.lock and the Codex npm lock are committed native resolver outputs; CI must use locked installs and must not rewrite tracked files. No Python scientific bridge remains. A future Python exception requires concrete evidence in ../research/reuse.md; none is approved by this matrix.

The SQLx/SQLite/session dependency rationale is in [reuse research](../research/reuse.md#bundled-sqlite-wal-reliability-and-sqlx-compatibility). [PR #87](https://github.com/zhengui666/QuaZonai/pull/87) owns actual candidate checks, review and merge state; a pin or listed test is not a pass. Older successful runs do not approve the new graph.

Pre-release Nautilus status, unsupported market paths and incomplete product scope must stay visible. A compatibility probe is not a release, account-login, multi-Alpha, security-isolation or full T01–T42 certificate.

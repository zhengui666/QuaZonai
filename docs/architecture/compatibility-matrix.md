# Native compatibility matrix

| Component | Pin | Verified boundary |
|---|---|---|
| Rust | 1.98.0 | Toolchain required by Nautilus 0.63.0 |
| Nautilus backtest/model/trading | 0.63.0, release family v2.0.0rc4 | Rust-only synthetic EMA fixture,745 iterations/12 orders/24 events; target-weight/market/isolation acceptance remains separate |
| Clarabel | 0.11.1 | Native QP golden and infeasible certificate; no Python binding |
| Arrow | 56.2.0 | Rust RecordBatch IPC schema/provenance/value round-trip |
| Codex | 0.144.4 | Pinned official binary; unauthenticated stdio/model pagination/Thread startup fixture |
| PGMQ | 1.10.0 on PG18 | Native transaction rollback/redelivery/archive fixture, not full production domain recovery |

Platform: Linux x86_64. Cargo.lock and the Codex npm lock are committed native resolver outputs; CI must use locked installs and must not rewrite tracked files. No Python scientific bridge remains. A future Python exception requires concrete evidence in ../research/reuse.md; none is approved by this matrix.

Pre-release Nautilus status, unsupported market paths and incomplete product scope must stay visible. A compatibility probe is not a release, account-login, multi-Alpha, security-isolation or full T01–T42 certificate.

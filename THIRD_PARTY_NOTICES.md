# Third-party notices

Original QuaZonai code remains AGPL-3.0-only under LICENSE/NOTICE. Third-party software retains its upstream license. The authoritative dependency inputs are Cargo.toml/Cargo.lock and runtimes/codex/package.json/package-lock.json. Obsolete Python/frontend manifests are not release inputs.

| Component | Upstream license | Use |
|---|---|---|
| Nautilus Rust 0.63.0 | LGPL-3.0-only | Native BacktestEngine/model/trading; apps/job/src/backtest.rs adapts the official v2.0.0rc4 engine_ema_cross example and retains its copyright/license header |
| Clarabel.rs 0.11.1 | Apache-2.0 | Native convex solver, not a QZ-owned numerical implementation |
| Apache Arrow Rust 56.2.0 | Apache-2.0 | Native arrays/schema/IPC |
| Serde, UUID, Chrono, BigDecimal, utoipa, thiserror, proptest | Resolved upstream license texts | Wire types, standard scalar implementations, schema and testing |
| iso_currency 0.7.0 | Upstream license and versioned data attribution | ISO-code membership; not an online authoritative ISO service |
| OpenAI Codex 0.144.4 | Apache-2.0 | Native App Server binary/protocol; no copied Agent loop |
| PostgreSQL / PGMQ | PostgreSQL License / PGMQ upstream license | Isolated native transaction fixture and future persistence |

A dependency inventory is not a completed license audit. Before distribution, generate a complete license report/SBOM for the exact resolved graph, inspect upstream license texts and NOTICE requirements, and comply with LGPL requirements for redistribution/linking (including relinking/source obligations as applicable). No license is changed by a directory rename or rewrite. The repository does not vendor Cargo dependencies or toolchains.

# Third-party notices

Original QuaZonai code remains AGPL-3.0-only under LICENSE/NOTICE. Third-party software retains its upstream license. The authoritative application dependency inputs are Cargo.toml/Cargo.lock, apps/web/package.json/package-lock.json and runtimes/codex/package.json/package-lock.json. The current Ant Design frontend is a release input; deleted legacy Python/frontend manifests are not.

| Component | Upstream license | Use |
|---|---|---|
| Nautilus Rust 0.63.0 | LGPL-3.0-only | Native BacktestEngine/model/trading; apps/job/src/backtest.rs adapts the official v2.0.0rc4 engine_ema_cross example and retains its copyright/license header |
| Clarabel.rs 0.11.1 | Apache-2.0 | Native convex solver, not a QZ-owned numerical implementation |
| Apache Arrow Rust 56.2.0 | Apache-2.0 | Native arrays/schema/IPC |
| Serde, UUID, Chrono, BigDecimal, utoipa, thiserror, proptest | Resolved upstream license texts | Wire types, standard scalar implementations, schema and testing |
| iso_currency 0.7.0 | Upstream license and versioned data attribution | ISO-code membership; not an online authoritative ISO service |
| OpenAI Codex 0.144.4 | Apache-2.0 | Native App Server binary/protocol; no copied Agent loop |
| PostgreSQL / PGMQ | PostgreSQL License / PGMQ upstream license | Native application persistence, transactions and durable work delivery |
| axum-0.8.9 | MIT | Rust authentication / HTTP / persistence / CLI integration |
| tower-sessions-0.15.0 | MIT | Rust authentication / HTTP / persistence / CLI integration |
| tower-sessions-sqlx-store at d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d | MIT | Official SQLx0.9 Postgres adapter; fixed upstream Git revision, not a new published crate release |
| totp-rs-5.7.0 | MIT | Rust authentication / HTTP / persistence / CLI integration |
| argon2-0.5.3 | MIT OR Apache-2.0 | Rust authentication / HTTP / persistence / CLI integration |
| chacha20poly1305-0.10.1 | Apache-2.0 OR MIT | Rust authentication / HTTP / persistence / CLI integration |
| cap-std-3.4.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Rust authentication / HTTP / persistence / CLI integration |
| clap-4.5.46 | MIT OR Apache-2.0 | Rust authentication / HTTP / persistence / CLI integration |
| sqlx-0.9.0 | MIT OR Apache-2.0 | Rust authentication / HTTP / persistence / CLI integration |
| libsqlite3-sys-0.37.0 | MIT | Native SQLite binding, shared with SQLx; no separate persistence framework |
| Bundled SQLite 3.51.3 | [Public domain](https://www.sqlite.org/copyright.html) | Runtime WAL journal; upstream WAL-reset fix |
| time-0.3.47 | MIT OR Apache-2.0 | Native session expiry serialization |
| rmcp / rmcp-macros 3.2.0 | Apache-2.0 | Official native MCP stdio transport, protocol lifecycle, tool routing and strict argument schemas; no copied JSON-RPC implementation |
| reqwest 0.12.23 | MIT OR Apache-2.0 | Fixed-route Mission control API client; no redirects, automatic retries, ambient proxy or browser authority |

The MCP adapter is `apps/server/src/mcp/`; its transport regression is
`apps/server/tests/mcp_transport.rs`. The official SDK pin is
https://github.com/modelcontextprotocol/rust-sdk/tree/rmcp-v3.2.0 .
Only `server`, `macros`, `transport-io` and the test-only `client` features are
requested. Resource bounds and Mission authorization adapt existing domain/API
contracts; the SDK owns protocol parsing and service lifecycle. The native
`Cargo.lock` records the exact transitive graph. Protocol fixture tests do not
establish a complete Codex research loop or a finished license audit.

A dependency inventory is not a completed license audit. Before distribution, inspect the complete resolved graph's upstream license texts and NOTICE requirements and comply with LGPL redistribution/linking obligations, including applicable relinking/source requirements. The generated source inventory below supports that work but does not replace it. No license is changed by a directory rename or rewrite. The repository does not vendor Cargo dependencies or toolchains.

## Generated source inventory

The existing [CI Rust job](.github/workflows/ci.yml) generates
`source-dependencies.syft.json` and `source-dependencies.spdx.json` from only the
six committed application manifests/lockfiles named above. Retrieve the
`native-evidence-<commit>` artifact from the successful run for the reviewed commit.
Check `tested-commit.txt`, `syft-version.txt`, `sbom-scope.txt` and the original
`dependency-inputs/` alongside both generated files. After extracting that artifact,
this standard query displays the native findings without rewriting them:

```sh
jq '.artifacts[] | {name, version, type, metadata, licenses}' source-dependencies.syft.json
```

The inventory includes optional, development and other-platform lock entries; it
is not a list of packages proven to be linked into the released executable or
installed in a Docker image. Cargo.lock does not contain all license texts.
Missing licenses remain unknown, and a successful scan is not automatic clearance.
Preserve native metadata and the original locks when distinguishing Git sources
from package coordinates converted to SPDX. CI tools, container base packages and
host services are not covered by these six source files.

The [reuse note](docs/research/source-dependencies.md) records the exact upstream
tool/action pins and limits. No release assets, GitHub dependency snapshots,
vulnerability thresholds or new licensing policy are created by this scan.
A configured step alone is not an executed SBOM; actual run results are required.

## Native PostgreSQL session-schema adaptation

`migrations/202609060009_native_sessions.sql` reuses the default DDL from
`tower-sessions-sqlx-store 0.15.0`, upstream commit
`b34a2f363217c0c557ee332c8847f4e2d1b5e6b4`, `sqlx-store/src/postgres_store.rs`.
The SQL is executed by SQLx in the deployment transaction; the upstream Rust
SessionStore remains unmodified. Upstream MIT notice follows:

MIT License

Copyright (c) 2024 Max Countryman

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

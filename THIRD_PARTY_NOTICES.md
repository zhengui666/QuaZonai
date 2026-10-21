# Third-party notices

Original QuaZonai code remains AGPL-3.0-only under LICENSE/NOTICE. Third-party software keeps upstream licenses. Current dependency inputs are Cargo.toml/Cargo.lock and apps/{service,research,cli,web}/package.json/package-lock.json. Historical notices remain even when an execution component is retired.

The selected components below explain integration and attribution. The generated inventory and original lockfiles, not this abbreviated table, identify the complete source dependency set.

| Component | Upstream license | Use |
|---|---|---|
| Nautilus Rust 0.63.0 | [LGPL-3.0-only](licenses/nautilus-backtest-LGPL-3.0.txt) | Native BacktestEngine, model, trading, analysis, persistence and Polymarket adapters; original license text retained without implementation source |
| Clarabel.rs 0.11.1 | Apache-2.0 | Native convex solver, not a QZ-owned numerical implementation |
| GNU tar | GPL-3.0-or-later | Stream original recorded-cost archives through the installed upstream executable |
| Zstandard CLI | BSD-3-Clause OR GPL-2.0-only | Decompress original `.tar.zst` inputs; native images retain installed tool/library copyright and common license files |
| Apache Arrow Rust 56.2.0 / 59.3.0 | Apache-2.0 | Native arrays/schema/IPC; both versions occur in the resolved source graph |
| Serde, UUID, Chrono, BigDecimal, utoipa, thiserror, proptest | Resolved upstream license texts | Wire types, standard scalar implementations, schema and testing |
| iso_currency 0.7.0 | Upstream license and versioned data attribution | ISO-code membership; not an online authoritative ISO service |
| OpenAI Codex (historical component, retired from production) | Apache-2.0 | Original attribution is retained; no current App Server/image/login dependency |
| @earendil-works/pi-durable / pi-ai / chord 1.1.0 | MIT | Official persistent Harness, model/providers, conversations, submissions, tasks, extensions, recovery, cancellation, views and usage; normal npm packages, no copied Agent implementation |
| WABT 1.0.39 | Apache-2.0 | Official published JavaScript bindings for WAT-to-Wasm compilation |
| PostgreSQL / PGMQ | PostgreSQL License / PGMQ upstream license | Preserved historical database, migrations and previous work ledgers; current execution belongs to official Pi Durable |
| Fastify 5.6.2, @fastify/cookie 11.0.2, @fastify/static 8.3.0 | MIT | Official HTTP, cookie and static-file plugins |
| argon2 0.44.0 (npm), pg 8.16.3 | MIT | Owner password verification and read-only PostgreSQL archive access |
| Undici 7.16.0 | MIT | Official Node HTTP client and custom CA support; no automatic write retries |
| React 19.2.8, Ant Design 6.5.2, TanStack React Query 5.102.8 | MIT | Five-page browser workbench and server-state queries |
| Apache ECharts 6.1.0 | Apache-2.0 | Display of actual native shared-account curves; no generated returns |
| axum-0.8.9 | MIT | Rust authentication / HTTP / persistence / CLI integration |
| tower-sessions-0.15.0 | MIT | Rust authentication / HTTP / persistence / CLI integration |
| tower-sessions-sqlx-store at d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d | MIT | Official SQLx0.9 Postgres adapter; fixed upstream Git revision, not a new published crate release |
| argon2-0.5.3 | MIT OR Apache-2.0 | Rust authentication / HTTP / persistence / CLI integration |
| rpassword-7.4.0 | Apache-2.0 | Native hidden terminal password input and terminal restoration; no custom terminal-control implementation |
| chacha20poly1305-0.10.1 | Apache-2.0 OR MIT | Rust authentication / HTTP / persistence / CLI integration |
| cap-std-3.4.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Rust authentication / HTTP / persistence / CLI integration |
| clap-4.5.46 | MIT OR Apache-2.0 | Rust authentication / HTTP / persistence / CLI integration |
| sqlx-0.9.0 | MIT OR Apache-2.0 | Rust authentication / HTTP / persistence / CLI integration |
| libsqlite3-sys-0.37.0 | MIT | Native SQLite binding, shared with SQLx; no separate persistence framework |
| Bundled SQLite 3.51.3 | [Public domain](https://www.sqlite.org/copyright.html) | Runtime WAL journal; upstream WAL-reset fix |
| time-0.3.47 | MIT OR Apache-2.0 | Native session expiry serialization |
| rmcp / rmcp-macros 3.2.0 (historical component, retired) | Apache-2.0 | Original attribution retained; old Mission MCP adapter and dependencies are removed |
| reqwest 0.12.23 | MIT OR Apache-2.0 | Thin fixed-route service/CLI/durable transport; no redirects or automatic writes |

The Rust HTTP/authentication/storage/CLI notices above describe historical
components; their implementations are retired from the current control plane.
Node CLI distributions carry the official platform runtime with its upstream
LICENSE. All installed npm packages retain their original license/NOTICE files.

Research dependencies are exact published official packages in npm lockfiles.
Their upstream license files remain in the installed packages. Node's built-in
SQLite storage is used through the official durable adapter, not a QZ session store.
Deterministic execution tests use the official Faux Provider only; they do not
establish paid inference, model quality or a completed license audit.

A dependency inventory is not a completed license audit. Before distribution, inspect the complete resolved graph's upstream license texts and NOTICE requirements and comply with LGPL redistribution/linking obligations, including applicable relinking/source requirements. The committed lockfiles support that work but do not replace it. No license is changed by a directory rename or rewrite. Component implementations are unmodified official packages resolved outside the source tree; QuaZonai integration code uses their public APIs. The repository does not vendor or patch dependency implementations or toolchains. Existing license notices and applied database compatibility migrations remain preserved.

## Dependency inventory

The optional offline archive preparation path retains the original license texts
for its 16 added registry package versions in
[archive preparation notices](licenses/archive-source/README.md). Exact duplicate
texts are stored once, with every package mapped to its applicable texts. The
application image includes these retained license files.
This scoped addition does not claim a complete repository license audit.

The committed Cargo/npm lockfiles identify resolved dependency versions. Use
`cargo tree --locked` and the relevant npm lockfile to inspect a build's graph;
verify upstream license texts and redistribution obligations before distribution.
CI no longer generates duplicate Syft/SPDX inventories on every change. Existing
license notices remain applicable; optional/development dependencies in a lockfile
are not proof that they are linked into a particular executable.

## Native PostgreSQL session-schema adaptation

`migrations/202609060009_native_sessions.sql` reuses the default DDL from
`tower-sessions-sqlx-store 0.15.0`, upstream commit
`b34a2f363217c0c557ee332c8847f4e2d1b5e6b4`, `sqlx-store/src/postgres_store.rs`.
The SQL remains in the applied historical PostgreSQL migration ledger. The
upstream Rust SessionStore was unmodified; it is not a current dependency.
Upstream MIT notice follows:

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

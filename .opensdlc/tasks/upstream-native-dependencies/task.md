# Use unmodified upstream components

<a id="task"></a>
## Task

**Source:** The owner requests that QuaZonai contain no third-party component source and adapt its own code instead of modifying dependencies.

**Endpoint:** Design, implementation and verified reviewable changes. Production installation and merging are not requested.

<a id="intent"></a>
## Intent

Remove the local `nautilus-backtest` fork and prevent it from returning. Preserve original market data, definition histories, PIT checks, settlement evidence, native execution and license notices. Third-party packages remain package-manager inputs outside the source tree.

<a id="spec"></a>
## Requirements and design

The official locked `nautilus-backtest 0.63.0` package exposes no in-place instrument update on `BacktestEngine` or `SimulatedExchange`. Repeated `add_instrument` creates a new matching engine. The native `OrderMatchingEngine::update_instrument` is inaccessible through those public mutable interfaces; cache-only updates would leave matching on the old specification. The inspected upstream `develop` sources (workspace version 0.65.0) also expose no forwarding method. Copying the fork into a first-party directory or manipulating private state would violate this task.

The owner explicitly accepted: prioritize removing all third-party source and patches; retain definition acquisition, catalog preparation and point-in-time planning, but reject simulation selections containing definition transitions. Unchanged-definition windows continue using the official engine. This includes transitions at selection-start and after event-end through the receive-time cutoff. QuaZonai never alters the original catalog to manufacture a supported selection. Continuous replay across transitions requires a verified public upstream capability or a separately designed QuaZonai integration.

The shared `simulation::run` checks this capability before creating a native engine. The local native CLI exposes only a fixed diagnostic code, with no catalog paths or original values; managed failure/output privacy remains unchanged. The Runtime/image native-stack marker includes `static-instruments/1` so an old patched job image cannot silently satisfy the new Runtime contract. The application and scientific images copy the unchanged original LGPL text from `licenses/nautilus-backtest-LGPL-3.0.txt`; license notices remain in the repository without any component implementation source.

<a id="plan"></a>
## Implementation plan

1. Extend the existing architecture check to inspect resolved dependency sources and repository component manifests. Reproduce its failure on the local patch before removing it.
2. Remove the third-party source tree and Cargo override; restore the registry package checksum without changing unrelated locked versions. Remove build-context and image-notice references to the deleted source.
3. Resolve the simulation capability boundary in QuaZonai; preserve native account, order, price-bound and settlement checks and original catalog/history tests.
4. Update the architecture rule, data guide and attribution without altering applied migrations. Run native computation/history tests, architecture, formatting, Clippy and affected build/documentation checks.
5. A fresh read-only reviewer checks the implementation and the contributor cases in the existing instruction evaluation suite while the primary author runs native integration checks. This is a scoped instruction review, not an automated model benchmark.

<a id="verification"></a>
## Verification

The initial inventory found one copied component implementation: `vendor/nautilus-backtest`. Its entire implementation tree and Cargo override have been removed. The remaining external Git dependency is the official fixed `tower-sessions-stores` revision. The existing session-schema migration is an applied QuaZonai database compatibility contract with retained MIT attribution; do not edit its checksum or copy the upstream SessionStore implementation. Dependency caches, native binaries and license text are not authored component source.

The architecture regression was executed before cleanup: it failed with `Third-party source must not be vendored or path-patched: nautilus-backtest`. After restoring the registry dependency and removing the component tree, the same check passed. The official archive checksum matches `Cargo.lock`; all 49 archive files match the cached official package byte for byte. The lockfile changes only the `nautilus-backtest` source and checksum, with no unrelated version or dependency-edge changes. The npm locks resolve to official registry packages and contain no local links.

Executed locally on the implementation ending at `9ad12ec4`:

| Check | Result |
| --- | --- |
| `make check-unit` | Formatting and workspace Clippy passed; 367 tests passed, zero failures or ignored tests. This excludes Store/Server database suites. |
| Job Clippy and history feature tests (`polymarket-history,catalog-prepare`) | 72 tests passed across history acquisition, catalog/preparation, managed execution, PIT planning and native instrument cases. |
| `python3 -B -m unittest discover -s runtimes/data -v` | 23 tests passed. |
| `node --test runtimes/native/native-files.test.mjs` and assembler syntax check | Five tests passed; syntax valid. |
| Default locked `job` and `runtime` binaries | Build passed. |
| `make check-docs check-architecture` | Markdown links passed; 14 CLI/Skill/schema tests and the resolved-source architecture check passed. |
| `cargo test --locked -p runtime --features native-oci --test native_oci --test native_restore -- --test-threads=1 --nocapture` with the built image and explicit Docker socket | All 18 actual OCI tests and both cold-restore/ownership tests passed; zero ignored tests. |

The full unit/history runs preceded the final cache fix; that fix changes image license copying, CI invocation and the architecture check's metadata retrieval, not simulation behavior. Final documentation and architecture checks passed after that fix. The native image was assembled with an empty `CARGO_HOME`, which remained empty; its job and compiler both executed inside the image. The image license matches the retained official text byte for byte. The actual OCI checks passed native computation, candidate/sequence/rolling/Polymarket paths, one-account state, cancellation, identity reconciliation, kernel limits and namespace isolation. Cold restoration preserved original identities, output bytes, cancellation tombstones and native file ownership. These controlled fixtures verify software behavior, not historical-market admission or production recovery objectives.

Image evidence: source revision `9ad12ec4144f66b83fe6f216de8371c4151bcc6f`, immutable image `sha256:88aae5c80b8a7a7b0ee4c817e7728238cd14c18dc1344e93530ea2e9bf744ccc`, stage `BUILT_AND_EXECUTED`. The `static-instruments/1` label matches Runtime. The final task-evidence commit changes documentation only.

<a id="review"></a>
## Review

An independent read-only reviewer inspected `3245bc64..9ad12ec4`. Two cold-cache findings were corrected: image assembly must not resolve the workspace dependency graph just to locate a license, and the architecture check must be able to fetch missing locked packages. The assembler now copies the unchanged license-only repository file; CI runs assembly with a fresh empty `CARGO_HOME`. The reviewer reproduced both conditions and verified the fixes, including a real download and resolution of a missing pinned crate. No findings remain at the reviewed revision.

All five Contributor navigation scenarios in the [instruction evaluation suite](../../evals/suite.md) passed a fresh instruction/source check: installation wording, dependency direction, timeout/cancellation reconciliation, stale-head review and a missing upstream API. This is not a statistical model evaluation.

<a id="delivery"></a>
## Delivery boundary

Changes are committed on `codex/upstream-native-dependencies` in an isolated checkout. No database migration or generated wire contract changed. Existing user changes and production services are preserved. Remote CI, GitHub Codex review, merge and deployment are outside this local development endpoint; local tests and the independent review do not stand in for those release gates.

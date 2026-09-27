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

Investigation: only `vendor/nautilus-backtest` contains a copied component implementation. The remaining external Git dependency is the official fixed `tower-sessions-stores` revision. The existing session-schema migration is an applied QuaZonai database compatibility contract with retained MIT attribution; do not edit its checksum or copy the upstream SessionStore implementation. Dependency caches, native binaries and license text are not authored component source.

The architecture regression was executed before cleanup: it failed with `Third-party source must not be vendored or path-patched: nautilus-backtest`. After restoring the registry dependency and removing the component tree, the same check passed. Further checks are pending.

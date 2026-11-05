# Core engine performance investigation

## Goal

Locate avoidable response-time and resource costs in scientific computation, portfolio studies, backtesting, and the surrounding runtime. Deliver an evidence-backed optimization design with upstream research, reproducible probes, correctness constraints, and an implementation sequence.

The revised design must make the latest official Nautilus Rust release a mandatory
upgrade. The user explicitly requires a clean cutover: no old-version compatibility,
dual-version execution, or rollback path.

## Scope and baseline

- Inspected source: `d7da6bff197bcbf272ba998ae0a15b2c1b25071c`.
- Investigation date: 2026-10-29.
- This task delivers a design and measurement artifacts; it does not change production behavior, dependencies, schemas, or deployment.
- No production traces or representative production workload were supplied. Structural findings and synthetic component measurements are explicitly distinguished from end-to-end results.
- No Issue or PR was created for this local investigation.

## Delivery

- [Detailed optimization design, in the user's language](report.md).
- [Reproduction notes and evidence](evidence/README.md).

## Acceptance

1. Every prioritized finding identifies an actual source path and an observable mechanism.
2. Library recommendations distinguish current capabilities from the verified upgrade target: Nautilus Rust 0.67.0 and Rust 1.99.1. Python 2.0.0rc8 is recorded separately and is not introduced into the Rust execution path.
3. Measurements record their workload and limitations; no synthetic probe is represented as production speedup.
4. The plan retains PIT, independent evaluation, deterministic fuel, shared-account simulation, numerical contracts, and durable execution identities.
5. Independent review checks the report and evidence before delivery.
6. The upgrade plan covers dependencies, optional/dev features, toolchains, CI, image identities, catalog APIs, fee provenance, numeric semantics, and result producers/validators, without old-version adapters or rollback.
7. Historical measurements retain their actual versions; the upgraded engine receives its own correctness acceptance and performance baseline.

## Status

Completed: source investigation, upstream research, four classes of local probes,
the detailed design, and independent report review. Review corrections include
bounded compiled-module caching, registered estimator identity for incremental
covariance, exact hotspot line references, and map-specific complexity.

Revision for the latest-release requirement: official registry metadata and exact
0.67.0 source packages were inspected. The plan now includes the complete upgrade
closure, lazy typed catalog sessions, typed replay, revised fee schedules and Money
semantics, and a single-version deployment sequence. Archived inputs are preserved
without relabeling them as new-version artifacts. Same-version crash recovery
remains required; it is not a rollback mechanism.

Two independent final reviews passed: one checked the exact 0.67.0 public APIs and
performance claims; the other checked the repository upgrade closure, fee provenance,
single-version cutover, and measurement boundaries. The report also incorporates
their optional clarifications on contiguous replay timers and exact Money construction.

Verification completed:

- `make check-docs` passed using the CI-pinned lychee 0.24.2 and Rust 1.98.1.
- Local Markdown link validation: zero errors; remote links were investigated
  separately and are excluded by the repository's offline link configuration.
- Packaged standalone Wasmi harness built with `--release --offline --locked`
  and reran all 60 measured pairs with checksum/fuel assertions. This rerun
  checks packaging only; the report retains the coordinated original timings.
- Evidence JSON parsed, production source SHA-256 matched, standalone dependency
  versions matched entries in the project lockfile, and `git diff --check` passed.
- After the latest-release revision, `make check-docs` passed again: 289 link
  occurrences, zero link errors, and all 14 documentation/CLI/schema tests passed.
  The 12-crate registry receipt also passed its version/MSRV/checksum-shape checks.

The full production job build was attempted for the existing example, then stopped
when it required rebuilding the broad dependency tree. The actual production
`signals.rs` was instead compiled directly by the isolated release harness. No
full-engine performance or OCI behavior acceptance is claimed.

The 0.67.0 revision is a researched design, not an implemented dependency change.
No full QuaZonai build or OCI run on Rust 1.99.1 / Nautilus 0.67.0 is claimed.

Live 星序 board synchronization remains pending: its supported board tools are not
connected in this session, and the execution network policy does not allow the
owner-private Site origin. The Site's current owner-only access was verified; no
unsupported authentication or network route was used and no live update is claimed.

Implementation, merge, deployment, and production performance acceptance are
separate future tasks, not completed by this design delivery.

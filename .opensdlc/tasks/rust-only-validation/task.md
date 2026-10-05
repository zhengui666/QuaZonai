# Rust-only routine validation

## Request

The owner requested routine validation to cover only Rust compilation/build and tests, without frontend or container checks. Keep the real failing portfolio-release regression and necessary PostgreSQL/PGMQ test dependencies; preserve release image and portable CLI production.

Integration: [PR #166](https://github.com/zhengui666/QuaZonai/pull/166), targeting `dev`. The portfolio-release stack correction is a separate change.

## Implementation

- Run only [Rust CI](../../../.github/workflows/ci.yml) for ordinary PR and main/dev push source validation
- Keep checkout pinned to the event revision and record the Git version, without separate event-head/SHA comparison gates
- Compile all optional Rust paths, build workspace targets, and retain non-database Rust tests including Job operator/Paper/Sandbox features
- Retain the original Store/Server Rust suite against disposable PostgreSQL/PGMQ, including native subprocess prerequisites
- Keep frontend, OCI/browser, container and packaging-cost acceptance as manually dispatched focused workflows
- Keep four-platform CLI and GHCR image production in the reusable version publisher; wait only for the Rust source workflow before publication
- Align local Make targets, release selection/waiting regressions and developer instructions with the reduced routine scope

## Evidence

Before this change, run [37276841133](https://github.com/zhengui666/QuaZonai/actions/runs/37276841133) spent 14m57s in Rust source/dependency verification and 30m36s in the Store/Server build/test step. The associated Container run took about 27 minutes. These are observations of different workloads, not a paired before/after speed comparison.

Local feasible verification:

- Nine release-selection/dev-release waiting regressions passed
- All workflow YAML parsed; every Bash workflow step passed shell syntax checking
- Changed Python modules passed syntax compilation
- Local Make check/check-unit commands expanded as intended

This cloud workspace has no Rust compiler/Cargo. Rust execution must be verified by the final-source GitHub CI after publication. No runtime-performance or compilation-speed percentage is claimed.

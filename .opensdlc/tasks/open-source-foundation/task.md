# Open-source foundation

<a id="task"></a>
## Task

**Task ID:** `open-source-foundation`. **Source:** repository owner's request on 2026-09-17: adopt OpenSDLC, standardize documentation/code/architecture, refactor without legacy compatibility where useful, and research excellent open-source projects to improve the README.

**Endpoint:** the owner explicitly accepted completion of this scope through successful GitHub CI and an explicit clean GitHub Codex review on the latest Head, followed by merge and main verification. This task does not deploy the product or close the separate whole-product Issue #62.

<a id="intent"></a>
## Intent

New readers need to understand the project, try it, find authoritative documentation, and contribute a verifiable change without reconstructing its development history. Maintainers need one project context, review policy, operational workflow and agent evaluation entry. Preserve existing work, product contracts, user data and licenses.

<a id="spec"></a>
## Requirements and design

See the [research and specification](spec.md). The owner authorized this implementation and its CI/review acceptance endpoint in this task. Routine documentation, navigation and regression checks stay within existing [AGENTS](../../../AGENTS.md) authority; no new production or external-account authority is introduced.

<a id="plan"></a>
## Implementation plan

1. Start from `c21f75a61f270bccd9f872c5c645e4139db08e81` in isolated branch `codex/open-source-foundation-20260917`. Preserve the dirty original checkout and the other worktrees. Read DESIGN, actual Cargo dependencies, Make/CI, CLI, operations and browser preview tests.
2. Research upstream README/contribution patterns; preserve Chinese product documentation and use English for new OpenSDLC documents because `.opensdlc/config.json` is absent. Write the researched specification here, not another project-wide design ledger.
3. Correct DESIGN's stale directory map; add a linked architecture guide, CONTRIBUTING, shared OpenSDLC context/review/operations/evaluation entries and native entry links. Rework README around purpose, screenshot, runnable preview, source-backed architecture and contribution.
4. Reuse Cargo metadata, Rust tests, Make and existing CI for workspace dependency boundaries; align EditorConfig with Rust formatting. Do not add libraries, microservices, a documentation site, or a new approval platform.
5. Verify links/anchors, CLI help, architecture tests including a deliberate invalid dependency, formatting, generated client, web tests/build and the documented browser preview. Run independent fresh-context navigation checks; GitHub owns independent review and full CI.
6. Publish a scoped PR, fix findings, rerun affected local checks and all applicable remote checks after changes, request review for the new Head, resolve threads, merge with the expected Head and verify main.

Highest risk: turning target architecture or fixtures into delivered claims. Keep current evidence in its existing index, trace diagrams to actual source, and preserve the distinction between this task's acceptance and product release. Existing runtime boundaries already fit the product; splitting working services only for appearance would add risk without meeting a missing requirement.

<a id="verification"></a>
## Verification

Baseline inspection: PRs #63 and #78 are merged; main is the commit above. No existing OpenSDLC language setting or shared project/review/operations/evaluation entry exists. Main branch protection returned `404 Branch not protected`; CODEOWNERS is present but is not enforced approval. Changed-source local verification passed on Linux x86_64 with Rust 1.98.1 and locked npm dependencies:

- `make check-docs`: native server help traversal and lychee 0.24.2 offline file/anchor checks passed. Remote URLs are deliberately outside this deterministic check.
- `make check-architecture`: actual workspace metadata passed. Temporary forbidden `store -> integrations` normal and renamed `cfg(windows)` build dependencies each failed with the exact expected diagnostic (exit 101); manifests were restored and the actual graph passed again.
- Rust formatting, `cargo clippy --locked -p contracts --all-targets -- -D warnings`, and `git diff --check` passed. The architecture integration test is automatically included in the existing CI workspace tests.
- `make check-web`: unchanged regenerated client, typecheck, 517 Vitest cases, 5 PWA file cases and production/PWA build passed. Existing chunk-size warnings remain.
- `npm --prefix apps/web run test:e2e -- demo-preview.spec.ts`: three actual Chromium viewport cases passed, covering the credential-free preview and rejection of writable production delivery.

Full database/OCI/browser/CodeQL coverage is delegated to the existing GitHub workflows for the final Head. No production deployment, account/data acceptance or user-backup operation is part of this task. Full native command logs remain in the local execution session and CI, not duplicated here.

<a id="review"></a>
## Review

[PR #84](https://github.com/zhengui666/QuaZonai/pull/84) owns the independent GitHub Codex review, threads and latest-Head CI. Read its actual results before accepting delivery; this document does not freeze a future result as green. The separate [fresh-context evaluation](../../evals/runs/2026-09-17-onboarding.md) found and verified fixes for Attempt timing and the lifecycle source link; all three reading cases met their expected behavior.

<a id="delivery"></a>
## Delivery

Implementation and local verification are complete; delivery is [PR #84](https://github.com/zhengui666/QuaZonai/pull/84). Its native merge state, exact-Head CI and review are the canonical acceptance record under the owner's endpoint. Follow the [review procedure](../../review.md#approval), then verify the resulting main commit and its checks. No formal product release or incident occurred, so neither record is fabricated.

<a id="handoff"></a>
## Handoff

Branch: `codex/open-source-foundation-20260917`; isolated worktree: `.ai-bridge/open-source-foundation-worktree` under the original checkout. Resume by reading this entry, `git status`, the branch PR and its latest Head/checks/review threads. Reconcile an uncertain GitHub write by reading its state before retrying. Do not reset or bulk-stage the original `bug/clean` checkout.

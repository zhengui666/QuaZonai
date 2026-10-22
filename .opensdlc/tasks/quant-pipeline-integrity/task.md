# Current scope: design only
The owner has explicitly paused implementation and merging. The current deliverable is the complete [business and technical design](design/README.md), covering platform generalization, strategy modeling, a professional small proprietary-fund pipeline, rigorous and intelligent workflows, and automatically derived, continuously updated metrics, factor composition, weights and capacity. Earlier engineering work and PR #230 remain draft evidence at their original scope; they do not fulfill this business goal. No new production implementation is authorized in this design stage.

The original task record below is retained as historical work evidence.

---

# Quantitative pipeline integrity

## Goal and delivery

Improve the current unattended research pipeline through primary-source research, reproducible failures, bounded fixes and independent verification. Preserve original research requests, counted trials, point-in-time inputs, independent final evaluation and explicit strategy assumptions. Factor research remains useful without a portfolio consumer; QuaZonai remains target-only.

Base: remote `dev` at `a233b966ca0f6210f3c9bf5db1872eb5c982bfdf`. Branch: `codex/quant-pipeline-integrity`. Issue: not created. Pull request: not created. Current status: implementation and final acceptance pending.

The authorized endpoint is a PR merged into remote `dev` after the exact final Head's applicable CI passes, independent review findings are resolved and GitHub Codex explicitly reports no findings. Follow the [review policy](../../review.md#approval); local checks do not satisfy that endpoint. Merge does not establish deployment, actual-account results or profitable out-of-sample performance. The sources below inform engineering practice, not fund certification or universal regulatory compliance.

## Current scope

1. **Submission versus owner stop.** A controlled baseline reproduction using official Faux Provider and real Durable SQLite observed no submission when stop returned, followed by one late admission. Serialize the remaining submit/reconciliation phase with the existing per-conversation owner-operation boundary. Preserve original `requestId` replay, native cancellation, restart recovery and explicit continuation semantics; do not create another scheduler.
2. **Criterion evidence validity.** The current deterministic criterion can classify a finite `0.9` as meeting a `0.1` target even when the named metric reports `FAILED` or `INSUFFICIENT_SAMPLES`. Require available evidence, reject ambiguous duplicate or conflicting partition identities, and preserve supported legacy metrics whose status is absent. A finite number and a successful process are insufficient evidence of a valid metric. Keep component diagnostics distinct from strategy/account evidence.
3. **Ordinary variance QP.** A valid singular positive-semidefinite covariance must not be rejected solely because strict Cholesky factorization fails. Use the pinned native solver's ordinary QP path while retaining the checks needed by factorization-dependent risk-cap and risk-budget formulations. Preserve the original estimator, constraints and native result evidence; do not add diagonal jitter or fallback allocations.
4. **Native Sharpe annualization.** The structured mandate currently permits only the 252-day key, although the retained native pipeline produces a 365-day key for supported calendar-day accounts. Add explicit 365-day selection in the shared contract, validator, criterion and Web form. Match the selected native key exactly; never substitute, relabel or rescale 252-day evidence as 365-day evidence or vice versa.

The current TypeScript Service and Pi Durable Host own orchestration and business facts. The retained Rust kernel owns native science. Changes stay in those active paths and preserve historical data, migrations and dependency provenance.

## Research evidence and implementation mapping

The following primary sources were read on 2026-10-22. Upstream development documentation is guidance; capability claims must also match the repository's locked package and actual execution.

| Primary source | Applicable rule | Current QZ boundary |
| --- | --- | --- |
| [Official Pi Durable README](https://github.com/earendil-works/pi/blob/main/packages/durable/README.md), cross-checked against the installed `1.1.0` README | Submission durably admits work; the same request identity replays it; cancellation of a wait is not cancellation of work. Atomic commits and task recovery belong to the Harness. | [Host admission and owner operations](../../../apps/research/src/host.ts), [recovery](../../../apps/research/src/recovery.ts), [research facts](../../../apps/research/src/documents.ts). |
| [QuantConnect research guide](https://www.quantconnect.com/docs/v2/writing-algorithms/key-concepts/research-guide) | Freeze a falsifiable hypothesis, distinguish development from unseen evaluation, and use historical availability and universe membership. | [Research specification](../../../packages/contracts/src/research.ts), [trial ledger](../../../apps/research/src/trials.ts), [exposure accounting](../../../apps/research/plugins/native-exposure.ts). |
| [LEAN Algorithm Framework](https://www.quantconnect.com/docs/v2/writing-algorithms/algorithm-framework/overview) | Separate signals, target construction, risk and execution. The framework itself identifies limits for tightly coupled entry/exit strategies; modularity does not imply support for every strategy. | [Explicit strategy adapters](../../../apps/research/src/strategy.ts), [strategy evaluation](../../../apps/research/plugins/strategy-portfolio.ts), [factor projection](../../../apps/service/src/factors.ts). |
| [scikit-learn data-leakage guidance](https://scikit-learn.org/stable/common_pitfalls.html) | Split before fitting a transformation; test observations must not train the transformation used to evaluate them. | [Native factor evaluation](../../../apps/job/src/factors.rs), [fixed final selection](../../../apps/research/plugins/heldout-wasm.ts), [model-visible feedback](../../../apps/research/src/feedback.ts). Immutable Wasm models receive causal features, not future labels. |
| [Bailey and Lopez de Prado: Deflated Sharpe Ratio](https://www.davidhbailey.com/dhbpapers/deflated-sharpe.pdf) | Selection-bias adjustment needs a defined trial population and statistical inputs; a best observed Sharpe is insufficient. | [Counted original trials](../../../apps/research/src/trials.ts) and [final selection](../../../apps/research/plugins/strategy-portfolio.ts). The ledger is necessary provenance, not a DSR implementation. |
| [Bailey et al.: Probability of Backtest Overfitting](https://www.davidhbailey.com/dhbpapers/backtest-prob.pdf) | The paper's CSCV procedure evaluates a collection of strategy trials. A held-out result or a cross-validation split alone is not a PBO estimate. | [Research lifecycle](../../../apps/research/plugins/research.ts) and [current evaluation contract](../../../packages/contracts/src/index.ts). This task does not manufacture a PBO value. |
| [NautilusTrader `SharpeRatio`, version 0.63.0](https://docs.rs/nautilus-analysis/0.63.0/nautilus_analysis/statistics/sharpe_ratio/struct.SharpeRatio.html) | Native Sharpe annualizes using the specified period, defaulting to 252; distinct periods are distinct metric identities. Reuse the original native result rather than recalculating it in TypeScript. | [Annualization policy](../../../crates/domain/src/prediction.rs), [shared-account statistics](../../../apps/job/src/simulation.rs), [spot-cash statistics](../../../apps/job/src/spot_cash_runtime.rs), [criterion](../../../packages/contracts/src/research.ts). |
| [Clarabel problem formulation](https://clarabel.org/stable/) | The quadratic objective permits a symmetric positive-semidefinite matrix; strict positive definiteness is not a general ordinary-QP requirement. | [Allocation](../../../apps/job/src/optimization.rs), [sample covariance](../../../crates/domain/src/portfolio/covariance.rs), [native analytical regressions](../../../apps/job/tests/allocation.rs). |

## Capability limits

The structured mandate supports single-factor Pearson IC or an ensemble's named native account Sharpe, registered partitions, bounded member count and budget-or-selection stopping. This task extends the Sharpe selection from 252 days to an explicit choice of 252 or 365 days. It does not provide arbitrary objective plugins, automatic statistical significance, cross-market independence, DSR/PBO or a profitability guarantee. Criterion validity and the original metric identity remain required.

The final evaluator records selection before reserving unseen exposure and returns no final tuning metrics to the research model. Historical reassessment is explicitly non-independent. Failed and out-of-scope admitted trials retain their budget cost. Composite members' diagnostics do not become standalone independent factor evidence. These boundaries remain in force.

Further generalization needs its own reproducible scope: additional strategy-specific objectives, explicit evidence sufficiency, and cross-fold or cross-asset robustness. The present changes neither certify those capabilities nor replace missing observations with inferred values.

## Acceptance matrix

| Change | Required evidence on the integrated implementation | Current result |
| --- | --- | --- |
| Submission and stop ordering | Deterministically pause admission between captured facts and native submission; race owner stop; verify no late unobserved submission survives a successful stop. Verify same-request replay, explicit continuation and reopen/recovery using official Harness and disposable SQLite. Cover [Host tests](../../../apps/research/test/host.test.ts) and [recovery races](../../../apps/research/test/recovery-races.test.ts). | Baseline race reproduced; fix verification pending. |
| Criterion validity | A finite failed/unavailable metric remains insufficient; conflicting partition or duplicate identities cannot pass; a single available metric follows the original threshold. Preserve absent-status legacy behavior and reject component-only substitutions through [structured service tests](../../../apps/service/test/structured.test.ts), [factor views](../../../apps/service/test/factors.test.ts) and their callers. | Baseline misclassification reproduced; fix verification pending. |
| PSD variance QP | Solve correlated/rank-deficient ordinary QPs with the pinned Clarabel solver; independently check objective, feasibility and frozen constraints. Retain factorization-dependent risk behavior and explicit invalid/infeasible outcomes in [allocation regressions](../../../apps/job/tests/allocation.rs). | Implementation and verification pending. |
| Native Sharpe annualization | Accept explicit 365-day ensemble mandates; select only the exact native key; a 252-only result cannot satisfy a 365-day criterion or vice versa. Preserve existing 252-day requests and shared-account/component distinctions. Verify the explicit [Web form](../../../apps/web/src/research.tsx) choice and [structured service checks](../../../apps/service/test/structured.test.ts). | Source mismatch identified; implementation and verification pending. |

Run the applicable [project checks](../../project.md#commands), including TypeScript/core-flow checks, affected native science and current Markdown links. Record exact commands, versions and outcomes after integration; a missing prerequisite, skipped test, stale binary or old-Head result is not a pass. Use an independent reviewer for the delivered diff and complete the final-Head CI/GitHub Codex/merge requirements above.

Documentation preparation used official lychee `0.24.2`: this task's local paths and anchors passed (40 links, 32 OK, 8 offline exclusions). Repository-wide `make check-links` reported 59 errors across 18 unchanged inputs, chiefly historical links to removed modules and the absent `installed-free-source-tools` anchor. The full check is failing, not waived; keep that baseline evidence separate from this task's passing link check.

Controlled providers and synthetic numerical fixtures establish orchestration and mathematical behavior, not real model quality, market coverage, account execution or a literal unattended observation period. Final reporting must keep those claims separate.

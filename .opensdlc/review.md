# Review

<a id="scope"></a>
## Scope

Review the changed behavior and its callers against [architecture](architecture.md) and the linked task. Prioritize data loss, incorrect scientific results, duplicated external effects and broken user workflows. Do not turn cosmetic preferences into new product requirements.

<a id="focus"></a>
## Findings

A finding identifies the changed path, triggering conditions, observable failure and a focused correction. Check generated contracts, source/permission lineage, transaction boundaries, cancellation/recovery and native component reuse where affected. Documentation changes must retain operational prerequisites and working references. Deletions must remove stale callers without deleting useful regression coverage or user data.

<a id="approval"></a>
## Merge

For the owner's dev iteration authorized on 2026-09-30, Codex is an evaluation subject only: do not delegate design, implementation or code review to it. A separate native reviewer inspects the exact final Head; the implementation author fixes findings. Merge only into `dev`, after final-Head Rust compilation/build and tests pass and actionable independent-review findings are resolved. Frontend/container acceptance, documentation, formatting, Clippy and benchmarks are not routine merge prerequisites; publication still builds its deliverable assets. An old-Head review, emoji-only acknowledgement, skipped check or written handoff is not that evidence. Recheck the final Head and merge result in GitHub. Preserve existing PR scope and evidence; this workflow change does not retroactively review or approve those PRs.

Executed Agent evaluations must first verify that the native model catalog actually offers exact `gpt-6-luna` with `max` reasoning, and record requested and observed model identity. No fallback model, undisclosed identity or synthetic transcript counts as a successful evaluation. Unavailable model, authorization or runtime is an explicit blocked evaluation, independent of deterministic test results. Keep tuning and held-out validation separate; do not add task answers to production logic. Applicable evaluation evidence accompanies changes to Agent behavior; UI and infrastructure checks do not claim model quality.

Account authorization and deployment require their own scoped execution. Fixture tests, scoped maintenance and a merged PR do not establish unperformed end-to-end account results.


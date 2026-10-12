# Review

<a id="scope"></a>
## Scope

Review the changed behavior and its callers against [architecture](architecture.md) and the linked task. Prioritize data loss, incorrect scientific results, duplicated external effects and broken user workflows. Do not turn cosmetic preferences into new product requirements.

<a id="focus"></a>
## Findings

A finding identifies the changed path, triggering conditions, observable failure and a focused correction. Check generated contracts, source/permission lineage, transaction boundaries, cancellation/recovery and native component reuse where affected. Documentation changes must retain operational prerequisites and working references. Deletions must remove stale callers without deleting useful regression coverage or user data.

<a id="approval"></a>
## Merge

For the owner's dev iteration, Codex remains an evaluation subject, not a design/implementation/review delegate. A separate native reviewer inspects the exact final Head; the implementation author fixes concrete findings and reruns affected checks. Merge only into `dev` after the final Head's applicable TypeScript, core-flow, Web and retained scientific CI pass and actionable findings are resolved. The TypeScript workbench intentionally retires Rust server/store/runtime/CLI and deleted-page suites; its short replacement chain must execute real durable storage, native science and target/account observation rather than rename a mocked pass. Image publication consumes the same checked revision and tested assets without recompiling retired services or repeating unrelated legacy regression. An old-Head review, skipped applicable check, emoji acknowledgement or written handoff is not acceptance. Recheck the final Head and merge state in GitHub; preserve earlier PR evidence at its original scope.

For the separately approved `durable-factor-platform` goal, preserve a separate non-Codex independent review and additionally obtain the explicitly required existing GitHub Codex read-only final-Head review as development validation. This task-specific acceptance does not restore production Codex execution or delegate its design/implementation to Codex. Its complete Rust/Web/durable/source-generation and applicable native image/browser/install checks must actually run before claiming delivery. Retain the prior automated-iteration/PR evidence scope rather than relabeling old checks.

Executed Agent evaluations must first verify that the native model catalog actually offers exact `gpt-6-luna` with `max` reasoning, and record requested and observed model identity. No fallback model, undisclosed identity or synthetic transcript counts as a successful evaluation. Unavailable model, authorization or runtime is an explicit blocked evaluation, independent of deterministic test results. Keep tuning and held-out validation separate; do not add task answers to production logic. Applicable evaluation evidence accompanies changes to Agent behavior; UI and infrastructure checks do not claim model quality.

Account authorization and deployment require their own scoped execution. Fixture tests, scoped maintenance and a merged PR do not establish unperformed end-to-end account results.


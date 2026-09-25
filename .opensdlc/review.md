# Review

<a id="scope"></a>
## Scope

Review the changed behavior and its callers against [architecture](architecture.md) and the linked task. Prioritize data loss, incorrect scientific results, duplicated external effects and broken user workflows. Do not turn cosmetic preferences into new product requirements.

<a id="focus"></a>
## Findings

A finding identifies the changed path, triggering conditions, observable failure and a focused correction. Check generated contracts, source/permission lineage, transaction boundaries, cancellation/recovery and native component reuse where affected. Documentation changes must retain operational prerequisites and working references. Deletions must remove stale callers without deleting useful regression coverage or user data.

<a id="approval"></a>
## Merge

GitHub Codex performs read-only review; the web author fixes findings. Merge only when the final PR Head has all applicable CI passing, actionable review findings resolved and explicit clean Codex review. An old-Head review, emoji-only acknowledgement, skipped check or written handoff is not that evidence. Recheck the final Head and merge result in GitHub.

Account authorization and deployment require their own scoped execution. Fixture tests, scoped maintenance and a merged PR do not establish unperformed end-to-end account results.

# Project review policy

<a id="scope"></a>
## Scope
Follow [AGENTS](../AGENTS.md) and the current [owner amendment](../DESIGN.md#personal-lean). One task record and one PR are sufficient; no separate approval ledger or review calendar.

<a id="focus"></a>
## Focus
Check changed behavior, data flow, native reuse, numerical validity, recovery and unintended scope. Deletions must remove their obsolete callers and docs without deleting useful regression coverage.

<a id="severity"></a>
## Severity
Prioritize data loss, incorrect results and unauthorized external effects; then broken workflows and performance. Cosmetic suggestions are not new product requirements.

<a id="approval"></a>
## Approval
GitHub Codex performs read-only review. Web ChatGPT addresses findings. Require current-Head applicable CI and explicit clean review before merging; missing evidence is not success. This is the owner's delivery procedure, not a claim about native branch protection.

<a id="quality"></a>
## Finding quality
Each finding identifies a changed path, reproducible failure and minimal correction. Distinguish actual execution, fixtures and untested assumptions. Do not create new gates merely to satisfy this document.

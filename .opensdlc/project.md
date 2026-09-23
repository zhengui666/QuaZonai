# QuaZonai project context

<a id="purpose"></a>
## Purpose and architecture
Single-user local research and target-only delivery. Read [DESIGN](../DESIGN.md) for module boundaries and contracts.

<a id="commands"></a>
## Working commands
Use [CONTRIBUTING](../CONTRIBUTING.md#verify-the-change), the root Makefile and actual native CLI help. Do not maintain another command table here.

<a id="conventions"></a>
## Conventions
Reuse native components; keep one contract source. Preserve data, licenses and immutable migration history. Remove obsolete source and its dedicated tests rather than archiving them. See [AGENTS](../AGENTS.md).

<a id="owners"></a>
## Responsibilities
The repository owner is the sole user and requirements authority. Web ChatGPT authors; GitHub Actions executes; GitHub Codex reviews read-only. Product Mission/evaluator/downstream boundaries are not extra human users.

<a id="sources"></a>
## Sources
Product: DESIGN. Operation: OPERATIONS / CLI. Source navigation: DESIGN and the native workspace packages. Task intent and actual results: `.opensdlc/tasks/<task-id>/task.md`. Current delivery policy: [review](review.md); maintenance response: [operations](operations.md).

<a id="native"></a>
## Native integration points
Existing GitHub Issues, PRs, Actions and review comments hold delivery evidence. The operational Skill is [skills/quazonai](../skills/quazonai/SKILL.md), not a development harness.

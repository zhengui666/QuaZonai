# QuaZonai — project context

<a id="purpose"></a>
## Purpose and architecture

A self-hosted, single-user research workbench that produces traceable evidence and target-only portfolio packages. QZ does not own broker credentials or trading execution. [Architecture navigation](../docs/architecture.md) follows actual source; [DESIGN](../DESIGN.md) is authoritative. React → server/domain/Store → PostgreSQL/PGMQ and immutable artifacts; trusted Worker → native Codex/Runtime → bounded Rust scientific jobs.

<a id="commands"></a>
## Working commands

From the repository root after [contributor setup](../CONTRIBUTING.md#set-up-a-checkout):

| Command | Healthy result / boundary |
| --- | --- |
| `make demo-preview` | Synthetic UI at loopback port 4179; Ctrl+C stops it; no production evidence |
| `make check-docs` | Local Markdown links/anchors and server CLI help succeed |
| `make check-architecture` | Actual direct workspace dependency graph follows DESIGN |
| `make check-unit` | Format/Clippy and non-Store/non-Server workspace tests pass |
| `make check` | Full defined Make checks pass with disposable PostgreSQL/PGMQ and native prerequisites |
| `make check-web` | Generated client unchanged, types/tests/build succeed |

Detailed prerequisites, narrower targets and browser/native checks live in [CONTRIBUTING](../CONTRIBUTING.md#verify-the-change) and [CLI](../CLI.md#开发测试), not a second command catalog here.

<a id="conventions"></a>
## Conventions and recurring mistakes

Read [AGENTS](../AGENTS.md). Preserve dirty worktrees and verify ownership. Update DESIGN before contract changes; follow callers through the real flow. Generate contracts rather than editing output. Reuse Rust/native components; no legacy wrappers or speculative service layers. Missing prerequisites, synthetic fixtures and old-Head reviews are not current passes. Never put account secrets or hidden reasoning in artifacts. This task's CI/review acceptance does not certify an unrelated production release.

<a id="owners"></a>
## Responsibilities

[@zhengui666](https://github.com/zhengui666) is the repository/code owner in [CODEOWNERS](../.github/CODEOWNERS) and the decision route for requirements, engineering, releases and service operation. Local authors implement and verify; GitHub Codex supplies independent review. Product Operator/Reviewer/Downstream identities are distinct from development permissions.

<a id="sources"></a>
## Authoritative sources

| Subject | Source |
| --- | --- |
| Product, architecture, interfaces, UX, acceptance | [DESIGN](../DESIGN.md) |
| Actual operations and commands | [OPERATIONS](../OPERATIONS.md), [CLI](../CLI.md) |
| Coverage and unsupported acceptance | [Evidence index](../docs/architecture/issue-62-execution.md), [compatibility](../docs/architecture/compatibility-matrix.md) |
| Contributor onboarding and development governance | [CONTRIBUTING](../CONTRIBUTING.md), [AGENTS](../AGENTS.md) |
| Review, maintenance and agent behavior checks | [review](review.md), [operations](operations.md), [evaluations](evals/suite.md) |
| Current open-source improvement | [Task and research](tasks/open-source-foundation/task.md) |

<a id="native"></a>
## Native integration points

| Capability | Actual entry / observed discovery |
| --- | --- |
| Agent instructions | Root [AGENTS.md](../AGENTS.md), read in this task |
| Project workflow skill | [skills/quazonai/SKILL.md](../skills/quazonai/SKILL.md), explicitly linked/read; this path is not claimed to auto-install into every host |
| Build and test | [Makefile](../Makefile), Cargo workspace, [web package](../apps/web/package.json) |
| CI | [.github/workflows](../.github/workflows); GitHub run results are canonical |
| Contribution and ownership | [PR template](../.github/PULL_REQUEST_TEMPLATE.md), [Issue forms](../.github/ISSUE_TEMPLATE), [CODEOWNERS](../.github/CODEOWNERS) |

No repository-managed host hooks, autonomous deployment trigger or scheduled model-evaluation runner is installed by this change. The [evaluation suite](evals/suite.md) defines the bounded manual adoption and follow-up cadence.

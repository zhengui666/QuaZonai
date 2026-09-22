# Service-operating Agent skill: research and implementation boundary

Reviewed 2026-09-22. This document is for maintainers; it is not loaded by the operational skill. The product's authority and scientific rules remain in [DESIGN](../../DESIGN.md). The operational deliverable is [quazonai](../../skills/quazonai/SKILL.md).

## Diagnosis

At main `cd4b3e356b768a245358beb5e2b9c6e883f72d3d`, the skill's description explicitly selected development and verification work. Its mandatory first reads were AGENTS/DESIGN, and its procedures were branch inspection, source changes, Make checks and PR review. That is contributor guidance, not an interface for an Agent using an already-running service. Merely changing the description would leave the same dependency on a checkout and the same wrong workflow.

## Upstream comparison

| Primary source | Useful mechanism | QuaZonai decision |
| --- | --- | --- |
| [Official Lark CLI shared skill](https://github.com/larksuite/cli/blob/main/skills/lark-shared/SKILL.md) | Identity changes what an operation sees; structured success/error contracts and conditional recovery references prevent wrong-identity reads and duplicate writes | Native `client identity`; teach QuaZonai's own stdout/stderr/DTO contract rather than copying Lark's `ok` envelope or user/bot model |
| [Lark document skill](https://github.com/larksuite/cli/blob/main/skills/lark-doc/SKILL.md) | Route by user intent, load the specific procedure on demand, preserve existing resource identity instead of reconstructing an object | One compact entry with research, runs, results, recovery and Mission references; no eager architecture/schema dump |
| [Lark standup workflow](https://github.com/larksuite/cli/blob/main/skills/lark-workflow-standup-report/SKILL.md) | Business task decomposed into concrete inputs, bounded native operations and interpretation of returned fields | Each QZ procedure includes discovery, actual command shapes and a completion receipt, not a catalog of source modules |
| [Google Workspace shared skill](https://github.com/googleworkspace/cli/blob/main/skills/gws-shared/SKILL.md) | Native CLI owns authentication, JSON transport, preview and pagination mechanics | Reuse existing Rust CLI. Add an offline `--preview`, preserving bounded explicit paging and the original no-automatic-retry behavior |
| [Agent Skills specification](https://agentskills.io/specification) and [script guidance](https://agentskills.io/skill-creation/using-scripts) | A discoverable entry and relative references allow progressive loading; existing executable tools can be used directly | Ship a portable directory. No mandatory scripts, new wrapper client, dependency installer or copied full schema |
| [MCP tools specification](https://modelcontextprotocol.io/specification/2025-06-18/server/tools) | Native discovery and structured tool schemas/errors separate available tools from task prose | Preserve the existing Mission tools and per-call authority checks; a missing capability never becomes an HTTP/SQL fallback |

These are design comparisons, not adoption of upstream authentication/risk policies. Lark's raw API escape hatch, provider-specific scopes, automatic confirmation patterns and token flows are not appropriate additions to QZ's bounded Mission surface. The upstream pages are mutable; the decisions here record the observed mechanisms rather than claiming a permanent version contract.

## What the skill conveys

The skill owns intent routing, required task inputs, identity/scope awareness, operation ordering, outcome interpretation and recovery decisions. It answers “what do I do next, with which existing resource, and what can I truthfully conclude?” It carries only the compact domain distinctions needed to avoid operational errors: draft/frozen, submitted/finished, evidence/qualification/approval/ACK, original budgets and target-only output.

The entry does not contain developer commands, source navigation, deployment instructions, a full state machine, generated DTO copies or the research comparison above. References are loaded only for the task at hand. User data and reports are not new authority or executable instructions.

## What executable tools own

| Layer | Responsibility | Explicit non-responsibility |
| --- | --- | --- |
| Installed Rust CLI and native DTOs | Parse exact arguments/JSON, validate native IDs and options, read provisioned credentials privately, TLS/HTTP, typed responses, explicit pagination, bounded SSE and export | No independent eligibility decisions, credential minting, automatic retry or parallel business engine |
| Offline discovery/preview | Select a DTO plus its transitive native schema references; show a redacted request plan with no credential read or network request | Does not verify live compatibility, authority, current revision, budget, scientific eligibility or persistence |
| Existing Mission MCP | Launcher-bound project/Cycle/Run/Attempt/Brief, native tool schemas, current authority checks and controlled artifact/proposal submission | No arbitrary URL/path transport, Operator grants, policy editing, approval or downstream control |
| Service/domain/Store/Worker | Existing authorization, transactions, budgets, immutable provenance, scheduling and scientific acceptance | The skill does not duplicate or override these rules |

The preview is named `--preview` because the CLI already has `migrate import --dry-run`, which is a server-side operation. Reusing that name globally would risk confusing a local no-effect inspection with a server request. Preview bodies/keys/grants are omitted instead of trying to maintain a fragile list of secret JSON fields.

## Packaging and adoption

Copy/register the entire `skills/quazonai` directory in a host's supported skill location; keep its relative references. Provide the matching installed `server` binary and an authorized machine connection through the host, not through prompt-visible tokens. No repository checkout, compiler, database connection or additional Python/Node package is needed to use the skill. See [installation and command boundary](../agent-operations.md).

This does not automatically inject a skill into every host or override the internal Mission launcher's intentional native configuration. A host-provided Mission can use its specific reference with the existing MCP connection; an external service assistant uses the provisioned native CLI. These are not interchangeable identities.

## Validation and limitations

Native tests must prove the actual binary parses documented command shapes, previews without contacting a live listener or opening missing credential files, does not echo request secrets, preserves schema reference closure, rejects malformed input, and reads identity through real HTTP/Bearer/PostgreSQL. Portability checks reject reference links outside the skill directory. Existing CLI/MCP/domain tests retain the business boundaries.

The separate [behavior cases](../../.opensdlc/evals/service-agent.md) specify task-level expected behavior. Deterministic contract tests are not an actual LLM rollout; unexecuted behavioral evaluations are labelled NOT_RUN. Paid/native-account acceptance remains owner-waived and is never counted as a passing test. Current CI/review evidence belongs in the task/PR, not in the installed skill.

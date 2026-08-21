# QuaZonai MCP Tool Map

This reference summarizes the expected QuaZonai MCP Tools and Resources. The connected server's current `tools/list`, Resource templates and `quazonai://manifest` are authoritative for availability, OAuth scopes and schemas.

## Session bootstrap

```text
inspect tools/list and resources/list
→ read quazonai://manifest
→ call quazonai.system.status
```

Always complete this sequence before mutations.

## System and observation

| Tool / Resource | Purpose | Mutation |
|---|---|---:|
| `quazonai.system.status` | Component readiness and blocking causes | No |
| `quazonai://system/status` | Current status Resource | No |
| `quazonai.event.list` | Durable control-plane events | No |
| `quazonai.risk.show` | Funder limit, reservations and blocking state | No |

For continuous observation, prefer Resource subscription when supported. Re-read the Resource after reconnect; notifications are not the source of truth.

## Plugins and bundles

| Tool | Purpose | Typical safety |
|---|---|---|
| `quazonai.plugin.list` | List active/staged/draining/failed releases | READ_ONLY |
| `quazonai.plugin.show` | Release descriptor, state and references | READ_ONLY |
| `quazonai.plugin.impact` | Show resources affected by activate/deactivate/remove | READ_ONLY |
| `quazonai.plugin.stage` | Consume wheel Artifacts and create install Job | PLATFORM_MUTATION |
| `quazonai.plugin.prewarm` | Build or reuse immutable Runtime Bundle | PLATFORM_MUTATION |
| `quazonai.plugin.activate` | Make release default; old default drains | PLATFORM_MUTATION |
| `quazonai.plugin.deactivate` | Prevent new bindings and begin drain | PLATFORM_MUTATION |

The MCP server never exposes forced plugin removal.

### Stage flow

```text
quazonai.artifact.begin_upload(kind=PLUGIN_WHEEL)
→ approved HTTPS companion upload
→ quazonai.artifact.finalize_upload
→ quazonai.plugin.stage
→ quazonai.plugin.show / Task or Job Resource
→ quazonai.plugin.impact
→ optional quazonai.plugin.activate when scope permits
```

## Credentials and connections

| Tool | Purpose |
|---|---|
| `quazonai.credential.list` | List Credential Sets and configured-field presence |
| `quazonai.credential.show` | Show non-secret metadata only |
| `quazonai.data_source.list/show` | Inspect Data Sources |
| `quazonai.data_source.create/update` | Bind public config and existing Credential Set to exact release |
| `quazonai.data_source.preflight` | Construct and optionally test data config |
| `quazonai.execution_connection.list/show` | Inspect Execution Connections |
| `quazonai.execution_connection.create/update` | Bind public config and existing Credential Set to exact release |
| `quazonai.execution_connection.preflight` | Production read-only construction/connectivity preflight |

Secret create/update/read operations are local-human-only and absent from `tools/list`.

## Artifact uploads

Expected kinds:

```text
STRATEGY_SOURCE
PLUGIN_WHEEL
PARQUET_L2
```

Flow:

```text
quazonai.artifact.begin_upload
→ HTTPS resumable upload outside MCP JSON
→ quazonai.artifact.finalize_upload
→ quazonai.artifact.show confirms READY
→ one intended consuming Tool
→ quazonai.artifact.show confirms CONSUMED when applicable
```

Never put file bytes or Base64 in Tool arguments. Never ask the server to fetch an arbitrary URL.

## Data Catalog

| Tool | Purpose |
|---|---|
| `quazonai.dataset.list/show` | Inspect imported Datasets |
| `quazonai.dataset.import_parquet_l2` | Create Import Run from `PARQUET_L2` Artifact |

Import flow:

```text
begin/upload/finalize PARQUET_L2 Artifact
→ quazonai.dataset.import_parquet_l2
→ observe MCP Task or quazonai://runs/{id}
→ quazonai.dataset.show
```

## Strategies

| Tool | Purpose |
|---|---|
| `quazonai.strategy.list/show` | Inspect logical Strategies and versions |
| `quazonai.strategy.create` | Create Strategy container |
| `quazonai.strategy.version_create` | Validate and store version from `STRATEGY_SOURCE` Artifact |

Version flow:

```text
begin/upload/finalize STRATEGY_SOURCE Artifact
→ quazonai.strategy.version_create
→ quazonai.strategy.show
```

## Research

| Tool | Purpose |
|---|---|
| `quazonai.research.list/show` | Read state, sections and revisions |
| `quazonai.research.create` | Create DRAFT Research |
| `quazonai.research.section_set` | Add section revision |
| `quazonai.research.activate` | DRAFT → ACTIVE after prerequisites |

Canonical sections:

```text
HYPOTHESIS
MARKET_CONTEXT
DATA
METHOD
RESULTS
RISKS
CONCLUSION
```

## Experiments and Runs

| Tool | Purpose |
|---|---|
| `quazonai.experiment.create` | Fix Strategy, Dataset, ranges, seed and Runtime Bundle |
| `quazonai.experiment.start` | Queue Optimization/Research Run |
| `quazonai.experiment.show` | Inspect plan and selected result |
| `quazonai.run.list/show` | Inspect Run state and summaries |
| `quazonai.run.report` | List or retrieve report reference/content |

Long operations may return an MCP Task, QZ `run_id`, or both. If Tasks are unsupported, poll/read `quazonai://runs/{id}`.

Do not manually choose the Pareto candidate or run another candidate on the same Holdout.

## Approvals

| Tool | Purpose |
|---|---|
| `quazonai.approval.list/show` | Read immutable Approval snapshot and state |
| `quazonai.approval.prepare_decision` | Generate human decision summary and local CLI handoff |

Approval approve/reject are not MCP Tools.

## Deployments

| Tool | Purpose | Notes |
|---|---|---|
| `quazonai.deployment.list/show` | Read desired/observed state, generation and bundle | Read first |
| `quazonai.deployment.create` | Create Deployment and pending start Approval | Does not self-approve |
| `quazonai.deployment.stop` | Request risk-reducing Stop | Does not liquidate positions |
| `quazonai.deployment.restart_request` | Create new start Approval | No direct start |

Before mutation read:

```text
desired_state
observed_state
generation
approval
runtime_bundle_id
plugin release IDs
heartbeat
reconciliation
risk status
position consequence
```

Use impact/preflight first when exposed. Include current generation and other expected fields.

## Universe

| Tool | Purpose |
|---|---|
| `quazonai.universe.show` | Active/pending/recovery roster and predicate |
| `quazonai.universe.revision_create` | Create narrowing or expansion revision |

Expansion still needs human Approval. Narrowing can trigger cancel and controlled Restart according to QZ semantics.

## Resources

Typical Resources:

```text
quazonai://manifest
quazonai://operations
quazonai://system/status
quazonai://plugin-releases/{id}
quazonai://runtime-bundles/{id}
quazonai://datasets/{id}
quazonai://strategies/{id}
quazonai://research/{id}
quazonai://experiments/{id}
quazonai://runs/{id}
quazonai://runs/{run_id}/reports/{report_id}
quazonai://approvals/{id}
quazonai://deployments/{id}
quazonai://deployments/{id}/risk
quazonai://deployments/{id}/universe
```

## Common operation sequences

### Complete research cycle

```text
quazonai://manifest
quazonai.system.status
quazonai.dataset.list
quazonai.strategy.list
quazonai.research.create
quazonai.research.section_set × required revisions
quazonai.research.activate
quazonai.experiment.create
quazonai.experiment.start
observe Task or quazonai://runs/{id}
quazonai.run.report
quazonai.research.show
quazonai.approval.prepare_decision
```

### Diagnose Recovery Blocked

```text
quazonai.deployment.show
quazonai.risk.show
quazonai.plugin.show for pinned releases
quazonai.execution_connection.show/preflight
quazonai.event.list with Deployment filter
```

Never issue a bypass, replacement bundle, raw order or direct start.

### Explicit Stop

```text
quazonai.deployment.show
quazonai.risk.show
quazonai.deployment.stop with expected generation and impact token
observe quazonai://deployments/{id}
quazonai.deployment.show
```

Report open positions separately; Stop is not liquidation.

### Human Approval handoff

```text
quazonai.approval.show
quazonai.approval.prepare_decision
→ return local CLI command and effect
→ stop
→ after human action, re-read Approval and Deployment
```

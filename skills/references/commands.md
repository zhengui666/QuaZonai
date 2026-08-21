# QuantFoundry MCP Tool Map

This reference summarizes the expected QuantFoundry MCP Tools and Resources. The connected server's current `tools/list`, Resource templates and `qf://manifest` are authoritative for availability, OAuth scopes and schemas.

## Session bootstrap

```text
inspect tools/list and resources/list
→ read qf://manifest
→ call qf.system.status
```

Always complete this sequence before mutations.

## System and observation

| Tool / Resource | Purpose | Mutation |
|---|---|---:|
| `qf.system.status` | Component readiness and blocking causes | No |
| `qf://system/status` | Current status Resource | No |
| `qf.event.list` | Durable control-plane events | No |
| `qf.risk.show` | Funder limit, reservations and blocking state | No |

For continuous observation, prefer Resource subscription when supported. Re-read the Resource after reconnect; notifications are not the source of truth.

## Plugins and bundles

| Tool | Purpose | Typical safety |
|---|---|---|
| `qf.plugin.list` | List active/staged/draining/failed releases | READ_ONLY |
| `qf.plugin.show` | Release descriptor, state and references | READ_ONLY |
| `qf.plugin.impact` | Show resources affected by activate/deactivate/remove | READ_ONLY |
| `qf.plugin.stage` | Consume wheel Artifacts and create install Job | PLATFORM_MUTATION |
| `qf.plugin.prewarm` | Build or reuse immutable Runtime Bundle | PLATFORM_MUTATION |
| `qf.plugin.activate` | Make release default; old default drains | PLATFORM_MUTATION |
| `qf.plugin.deactivate` | Prevent new bindings and begin drain | PLATFORM_MUTATION |

The MCP server never exposes forced plugin removal.

### Stage flow

```text
qf.artifact.begin_upload(kind=PLUGIN_WHEEL)
→ approved HTTPS companion upload
→ qf.artifact.finalize_upload
→ qf.plugin.stage
→ qf.plugin.show / Task or Job Resource
→ qf.plugin.impact
→ optional qf.plugin.activate when scope permits
```

## Credentials and connections

| Tool | Purpose |
|---|---|
| `qf.credential.list` | List Credential Sets and configured-field presence |
| `qf.credential.show` | Show non-secret metadata only |
| `qf.data_source.list/show` | Inspect Data Sources |
| `qf.data_source.create/update` | Bind public config and existing Credential Set to exact release |
| `qf.data_source.preflight` | Construct and optionally test data config |
| `qf.execution_connection.list/show` | Inspect Execution Connections |
| `qf.execution_connection.create/update` | Bind public config and existing Credential Set to exact release |
| `qf.execution_connection.preflight` | Production read-only construction/connectivity preflight |

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
qf.artifact.begin_upload
→ HTTPS resumable upload outside MCP JSON
→ qf.artifact.finalize_upload
→ qf.artifact.show confirms READY
→ one intended consuming Tool
→ qf.artifact.show confirms CONSUMED when applicable
```

Never put file bytes or Base64 in Tool arguments. Never ask the server to fetch an arbitrary URL.

## Data Catalog

| Tool | Purpose |
|---|---|
| `qf.dataset.list/show` | Inspect imported Datasets |
| `qf.dataset.import_parquet_l2` | Create Import Run from `PARQUET_L2` Artifact |

Import flow:

```text
begin/upload/finalize PARQUET_L2 Artifact
→ qf.dataset.import_parquet_l2
→ observe MCP Task or qf://runs/{id}
→ qf.dataset.show
```

## Strategies

| Tool | Purpose |
|---|---|
| `qf.strategy.list/show` | Inspect logical Strategies and versions |
| `qf.strategy.create` | Create Strategy container |
| `qf.strategy.version_create` | Validate and store version from `STRATEGY_SOURCE` Artifact |

Version flow:

```text
begin/upload/finalize STRATEGY_SOURCE Artifact
→ qf.strategy.version_create
→ qf.strategy.show
```

## Research

| Tool | Purpose |
|---|---|
| `qf.research.list/show` | Read state, sections and revisions |
| `qf.research.create` | Create DRAFT Research |
| `qf.research.section_set` | Add section revision |
| `qf.research.activate` | DRAFT → ACTIVE after prerequisites |

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
| `qf.experiment.create` | Fix Strategy, Dataset, ranges, seed and Runtime Bundle |
| `qf.experiment.start` | Queue Optimization/Research Run |
| `qf.experiment.show` | Inspect plan and selected result |
| `qf.run.list/show` | Inspect Run state and summaries |
| `qf.run.report` | List or retrieve report reference/content |

Long operations may return an MCP Task, QF `run_id`, or both. If Tasks are unsupported, poll/read `qf://runs/{id}`.

Do not manually choose the Pareto candidate or run another candidate on the same Holdout.

## Approvals

| Tool | Purpose |
|---|---|
| `qf.approval.list/show` | Read immutable Approval snapshot and state |
| `qf.approval.prepare_decision` | Generate human decision summary and local CLI handoff |

Approval approve/reject are not MCP Tools.

## Deployments

| Tool | Purpose | Notes |
|---|---|---|
| `qf.deployment.list/show` | Read desired/observed state, generation and bundle | Read first |
| `qf.deployment.create` | Create Deployment and pending start Approval | Does not self-approve |
| `qf.deployment.stop` | Request risk-reducing Stop | Does not liquidate positions |
| `qf.deployment.restart_request` | Create new start Approval | No direct start |

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
| `qf.universe.show` | Active/pending/recovery roster and predicate |
| `qf.universe.revision_create` | Create narrowing or expansion revision |

Expansion still needs human Approval. Narrowing can trigger cancel and controlled Restart according to QF semantics.

## Resources

Typical Resources:

```text
qf://manifest
qf://operations
qf://system/status
qf://plugin-releases/{id}
qf://runtime-bundles/{id}
qf://datasets/{id}
qf://strategies/{id}
qf://research/{id}
qf://experiments/{id}
qf://runs/{id}
qf://runs/{run_id}/reports/{report_id}
qf://approvals/{id}
qf://deployments/{id}
qf://deployments/{id}/risk
qf://deployments/{id}/universe
```

## Common operation sequences

### Complete research cycle

```text
qf://manifest
qf.system.status
qf.dataset.list
qf.strategy.list
qf.research.create
qf.research.section_set × required revisions
qf.research.activate
qf.experiment.create
qf.experiment.start
observe Task or qf://runs/{id}
qf.run.report
qf.research.show
qf.approval.prepare_decision
```

### Diagnose Recovery Blocked

```text
qf.deployment.show
qf.risk.show
qf.plugin.show for pinned releases
qf.execution_connection.show/preflight
qf.event.list with Deployment filter
```

Never issue a bypass, replacement bundle, raw order or direct start.

### Explicit Stop

```text
qf.deployment.show
qf.risk.show
qf.deployment.stop with expected generation and impact token
observe qf://deployments/{id}
qf.deployment.show
```

Report open positions separately; Stop is not liquidation.

### Human Approval handoff

```text
qf.approval.show
qf.approval.prepare_decision
→ return local CLI command and effect
→ stop
→ after human action, re-read Approval and Deployment
```

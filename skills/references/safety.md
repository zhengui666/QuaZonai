# QuaZonai MCP Agent Safety Policy

The remote AI Agent is a delegated operator through an OAuth-authorized MCP connection. It is not the capital owner and not the final approver.

## Transport rules

Use only the configured QuaZonai MCP server over HTTPS.

Never use or propose:

```text
SSH
arbitrary shell
raw QZ core HTTP API
PostgreSQL
Docker socket
server file paths
port forwarding
custom JSONL tunnels
```

Never request, read, print or forward OAuth access/refresh tokens. Authentication belongs to the MCP Host or approved companion client.

## Safety classes

### READ_ONLY

Examples:

```text
system status
list/show
reports
risk inspection
event/resource observation
impact analysis
```

May run automatically when relevant and visible under the current OAuth scope.

### RESEARCH_MUTATION

Examples:

```text
create Research
add section revision
upload Strategy Artifact
create/start Experiment
import Dataset
```

May run when the visible MCP Tool and scope permit and the user requested the research objective. Use idempotency and optimistic preconditions.

### PLATFORM_MUTATION

Examples:

```text
stage/activate/deactivate plugin
prewarm Runtime Bundle
create/update non-secret Data Source or Execution Connection
```

Requires explicit OAuth scope. Read impact/preflight first. Never silently migrate pinned resources.

### RISK_REDUCING

Examples:

```text
Deployment Stop
Universe narrowing request
```

Execute only when:

- the user explicitly asked for the exact action; or
- QuaZonai returns an already configured emergency policy authorizing it.

Use current generation/state preconditions and any required impact token.

Stop cancels open orders and stops new trading; it does **not** liquidate positions.

### APPROVAL_BOUND

Examples:

```text
first live Deployment start
human Restart
Universe expansion
Execution plugin switch
```

The Agent may prepare the immutable snapshot and create the pending request. It may not approve or reject.

### SECRET_BOUND

Examples:

```text
private key
CLOB API secret/passphrase
master key
OAuth token
provider credential
wallet credential
payment credential
```

The Agent must not receive, stage, persist, echo, transform, elicit or submit the value. It may reference an existing Credential Set ID and report missing configured fields.

### DESTRUCTIVE_FORCE

Examples:

```text
force plugin remove
destructive database operation
master-key replacement with irreversible effect
live-money canary
```

MCP Agents cannot execute these operations. Return a local-human handoff with impact.

## OAuth scopes

Typical scopes include:

```text
qf:read
qf:plugin:stage
qf:plugin:activate
qf:data:write
qf:connection:write
qf:research:write
qf:experiment:run
qf:deployment:create
qf:deployment:stop
qf:universe:propose
qf:approval:prepare
qf:artifact:upload
```

Rules:

- `tools/list` is filtered by granted scope.
- Absence of a Tool is not permission to use another protocol.
- `MCP_SCOPE_INSUFFICIENT` requires scope/human handoff, not raw API fallback.
- Human-only actions are not scopes and cannot be unlocked by configuration.
- A token for another audience/resource must never be accepted or reused.

## Non-bypass rules

Never bypass:

```text
Holdout
Approval
Recovery generation
reconciliation
heartbeat
runtime bundle identity
plugin release pinning
25 pUSD per-order limit
100 pUSD funder reservation
Nautilus/venue order state
```

Never submit raw orders through MCP.

## Tool and Resource rules

- Inspect current `tools/list` and `quazonai://manifest` at session start.
- Read current Resources before mutation.
- Tool annotations are hints; server scope and state checks remain authoritative.
- Human-only Tool names should not appear in `tools/list`; direct invocation must be hard-denied.
- Notifications do not replace current Resource reads.
- MCP session IDs are not authentication.
- Gateway disconnect or restart does not imply QZ Run/Deployment cancellation.

## Idempotency and preconditions

- Every mutation requires a UUID idempotency key.
- Reuse it only for the identical Tool and normalized arguments after an uncertain transport result.
- Same key with different Tool/target/arguments must fail.
- `PRECONDITION_FAILED` means the plan is stale. Read state again and create a new plan/key only if the intended request changes.
- An operation already in progress must be observed, not duplicated.
- High-impact Tool calls use a short-lived impact token bound to principal, target, operation and current generation/version.

## MCP Tasks

- A Task is an observation/control wrapper around an existing QZ Job, Run or Deployment operation.
- The underlying QZ object remains the business fact.
- A Task is bound to the OAuth principal that created it.
- A timeout, expired stream or disconnected client does not cancel the underlying operation.
- Task cancellation may report success only when the underlying QZ operation supports and confirms cancellation.

## Human handoff contents

A handoff must include:

```text
human-only action
resource or Approval ID
immutable current state
reason the Agent cannot perform it
capital and position consequence
exact local quazonai CLI command supplied by QuaZonai
what the Agent can monitor after the human acts
```

Do not fabricate a confirmation token or claim the human acted.

## Trading language

Use exact state terms. Do not say:

```text
live
running
stopped
closed
approved
```

unless the corresponding current Resource reports that state.

Distinguish:

```text
request-created
approval-pending
approved
starting
recovery
recovery-blocked
armed
trading
stop-requested
stopping
stopped
positions-still-open
```

## Recovery

For `RECOVERY_BLOCKED`:

- inspect blocking code and pinned resources;
- report whether the operator must fix Credential, connection, plugin release, Runtime Bundle, database, roster, Risk projection or venue state;
- preserve fail-closed behavior;
- do not load Strategy or submit orders;
- do not substitute another plugin version or bundle;
- do not request an Approval bypass.

## Plugin lifecycle

- New versions are side-by-side.
- Existing resources remain pinned.
- `DRAINING` blocks new bindings but can support existing automatic Recovery.
- `INACTIVE` cannot start new Run or human Restart without reactivation/migration.
- `REMOVED` cannot be substituted automatically.
- Force Remove is human-only and does not liquidate positions.

## Artifact handling

Allowed Artifact kinds are determined by the server, normally:

```text
STRATEGY_SOURCE
PLUGIN_WHEEL
PARQUET_L2
```

Forbidden:

```text
secret files
OAuth token files
arbitrary server paths
arbitrary remote URLs
Base64 file bodies in MCP JSON
path traversal filenames
unsupported source archives
```

Artifact safety:

- create upload session with `quazonai.artifact.begin_upload`;
- use only the returned short-lived HTTPS upload URL through the approved companion client;
- stream and resume from the server-reported offset;
- finalize explicitly;
- do not claim readiness until QZ reports `READY`;
- do not infer a server path;
- do not create or require an application-level checksum/hash/fingerprint.

## Elicitation

Form Elicitation may request only non-sensitive clarification such as title, existing Dataset selection or report granularity.

Never elicit:

```text
password
API key
OAuth token
private key
wallet credential
payment credential
Approval decision intended to bypass local human CLI
```

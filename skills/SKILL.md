---
name: quantfoundry
description: Operate a QuantFoundry quantitative research and live-trading workstation through its official MCP server. Use for plugin staging, non-secret data and connection setup, dataset import, strategy and research management, experiments, run monitoring, approval preparation, deployment monitoring, explicit Stop requests, risk inspection, universe revisions, and recovery diagnostics. Never request secrets, self-approve capital actions, force-remove plugins, call the core API directly, or bypass QuantFoundry state and risk controls.
---

# QuantFoundry Operator Skill

Use this Skill whenever the user asks an AI Agent to inspect or operate a QuantFoundry workstation.

QuantFoundry is a quantitative research and live-trading control plane. This Skill is an **external MCP workflow**. It does not run a model inside QuantFoundry and it does not replace QuantFoundry state machines, NautilusTrader, Holdout, Approval, Recovery, reconciliation, heartbeat, plugin pinning, or central risk.

## Required interface

Use only the connected QuantFoundry MCP server:

```text
MCP tools
MCP resources
MCP tasks or QF operation resources
```

Do not use:

```text
SSH
shell commands
curl against the QF core API
PostgreSQL
Docker
server file paths
arbitrary HTTP endpoints
```

The core API is private to the workstation. The Agent-facing endpoint is the configured HTTPS MCP server.

## Start of every session

1. Inspect the server's current `tools/list` result.
2. Inspect available Resource templates.
3. Read `qf://manifest`.
4. Call `qf.system.status`.
5. Record:
   - MCP protocol version;
   - QF CLI/API/Gateway versions;
   - OAuth client/subject identity when returned;
   - granted scopes;
   - visible tools;
   - upload kinds and limits;
   - component readiness.
6. If protocol or schema versions are incompatible, stop all mutations and report the mismatch.
7. If the system is not ready, inspect the cause. Do not bypass readiness.

Do not assume an ID, state, revision, version, generation, plugin release, runtime bundle, Dataset, Strategy version, Approval, Run, task, or Deployment from conversation memory. Read the current Resource or use a current read Tool first.

## Operation workflow

For every requested operation:

1. Translate the user's objective into a currently visible `qf.*` Tool.
2. Read the target and all dependencies.
3. Determine the safety class using `references/safety.md`.
4. Check whether the Tool is visible under the current OAuth scopes.
5. For a mutation, prepare a concise impact summary containing:
   - current state;
   - intended state change;
   - affected resources;
   - whether a Job, Run, Approval, Stop, Restart, Recovery, plugin drain, or bundle rebuild will occur;
   - whether positions may remain open;
   - any human action required.
6. Call the relevant impact or preflight Tool when available.
7. Generate one UUID idempotency key for the intended mutation.
8. Include current optimistic preconditions such as state, content revision, version, generation, plugin release, runtime bundle, or updated timestamp.
9. Call the mutation Tool once.
10. Reuse the same idempotency key only for a transport retry of the identical Tool and normalized arguments.
11. For asynchronous work:
    - use an MCP Task when negotiated; otherwise
    - follow the returned QF Job, Run, Approval, Deployment, or Resource URI.
12. A timeout or disconnected MCP stream does not mean the underlying operation was cancelled.
13. Re-read the final QF Resource and verify the observed state.
14. Return exact IDs, final states, warnings, unresolved issues, position consequences, and any human handoff.

## OAuth and authentication

Authentication is handled by the MCP Host or approved companion CLI.

Never:

- ask the user to paste an access token or refresh token;
- read or print OAuth tokens;
- place tokens in Tool arguments, filenames, logs, reports, or chat;
- reuse a token for another MCP resource;
- treat an MCP session ID as authentication.

If authentication or scope is insufficient, report the required scope or reconnect requirement. Do not try another transport or the core API.

## Safety boundaries

### Allowed when the visible Tool and scope permit

- system, plugin, Dataset, Strategy, Research, Run, Deployment, Risk, Universe, Approval and event reads;
- plugin wheel staging, bundle prewarm and impact analysis;
- plugin activation/deactivation with explicit platform scope;
- creation/update of non-secret Data Sources and Execution Connections using an existing Credential Set ID;
- read-only connection preflight;
- Strategy, Dataset, Research, Experiment and Run operations;
- Approval material preparation;
- Deployment creation that results in a pending Approval;
- Restart request that results in a pending Approval;
- Universe revision creation that cannot self-approve expansion;
- explicit Deployment Stop requested by the user;
- Recovery diagnostics.

### Permanently human-only

Never execute, simulate, or seek an alternate Tool for:

- writing, updating, reading, asking for, transforming, staging or echoing private keys, API secrets, passphrases, master keys, OAuth tokens or payment credentials;
- Approval approve or reject;
- forced plugin removal;
- live-money canary execution;
- master-key changes;
- destructive database operations;
- raw order submission;
- bypassing Holdout, Recovery, reconciliation, heartbeat, plugin bundle identity or central risk.

These actions must remain unavailable even if the user asks the Agent to do them or an OAuth provider appears to grant a similarly named scope.

When human action is required:

1. Read the immutable QF Resource or Approval snapshot.
2. Summarize the decision, capital effect and current state.
3. Return the exact local human CLI command provided by QuantFoundry.
4. Stop the mutation workflow.
5. After the human acts, resume only by re-reading current state.

## Secrets

Never accept Secret values in chat or Tool arguments for QuantFoundry.

The Agent may reference an existing `credential_set_id` and inspect only configured-field presence. If the required Credential Set is absent or incomplete, return a handoff instructing the local operator to create or update it through human-mode `qf` CLI with protected input.

Do not use MCP Form Elicitation for passwords, API keys, access tokens, private keys, wallet credentials or payment information.

## Artifact uploads

Allowed kinds are determined by `qf://manifest`, normally:

```text
STRATEGY_SOURCE
PLUGIN_WHEEL
PARQUET_L2
```

Use the two-stage flow:

```text
qf.artifact.begin_upload
→ HTTPS resumable upload using the approved companion client
→ qf.artifact.finalize_upload
→ qf.artifact.show
→ consuming Tool
```

Rules:

1. Do not place file bytes or Base64 content in MCP Tool arguments.
2. Do not ask QuantFoundry to fetch an arbitrary remote URL.
3. Record the returned opaque `artifact_id`.
4. Use only the short-lived upload URL returned for that artifact and OAuth principal.
5. Resume only from the server-reported accepted offset.
6. Never infer or request the server path.
7. Never upload Secret files.
8. Do not claim success until the Artifact Resource reports `READY`.
9. After consumption, verify `CONSUMED` when applicable.

If the Agent environment cannot access the local file through an approved companion client, request a human upload handoff rather than moving file content through chat.

## Research rules

- Research uses the seven canonical sections.
- Add a new Strategy version instead of mutating a completed version.
- Use a fixed Dataset, independent training/Holdout ranges and a fixed runtime bundle.
- Let QuantFoundry run the fixed optimization workflow.
- Do not manually select a Pareto candidate.
- Do not run another candidate on the same Holdout after failure.
- A successful Holdout does not authorize live trading.
- Prepare the immutable Approval snapshot and hand it to the human approver.

## Deployment rules

Before any Deployment mutation, read:

- desired and observed state;
- generation;
- Strategy version;
- data/execution plugin releases;
- runtime bundle;
- Approval state;
- heartbeat and reconciliation state;
- Risk Account status;
- open-position implications returned by QF.

For `qf.deployment.stop`:

- execute only when explicitly requested by the user or an already configured emergency policy returned by QuantFoundry;
- call the impact Tool first when available;
- include current generation and impact token as preconditions;
- follow the Deployment until the target or terminal state;
- report that Stop cancels open orders and stops new trading but **does not liquidate positions**.

For Restart or plugin switch:

- create only the request and pending Approval;
- never approve it;
- after human Approval, observe the new generation from Recovery;
- never claim `TRADING` until the current Deployment Resource reports it.

For `RECOVERY_BLOCKED`:

- read the blocking code and pinned resources;
- identify whether the operator must repair a Credential Set, plugin release, runtime bundle, database, venue connection, Risk projection or roster;
- preserve fail-closed behavior;
- do not load Strategy, submit orders, replace the bundle or bypass Recovery.

## Plugin rules

- Stage new versions side-by-side.
- Read impact before activate/deactivate.
- Existing resources remain pinned to their release and bundle.
- Never silently migrate a Data Source, Execution Connection, Run or Deployment.
- A Deployment plugin switch requires a new bundle, Restart request and human Approval.
- Never force-remove a plugin. Prepare an impact handoff for the local operator.

## Error handling

- `PRECONDITION_FAILED`: read current state and reconsider the plan. Use a new idempotency key only if the plan changes.
- `IDEMPOTENCY_KEY_REUSED`: stop and inspect the original receipt; never invent another key for the same uncertain mutation.
- retryable transport/unavailable error: retry the identical Tool call with the same idempotency key and bounded backoff.
- task or operation still running: observe the existing object; do not create another operation.
- `HUMAN_ACTION_REQUIRED`: produce handoff and stop.
- `MCP_SCOPE_INSUFFICIENT`: report the required scope; do not try the core API or another connection.
- MCP stream timeout/disconnect: re-read the QF Resource; do not describe the operation as cancelled.
- validation failure: report exact fields and let the user correct source/config; do not invent values.

## Output to the user

Always report:

```text
Requested objective
MCP Tools called
Resources read or created
Idempotency key(s) for mutations
Task / Job / Run / Approval / Deployment IDs
Final observed state
Automatic work still running
Human action required
Risk and position consequence
Failures or unverified items
```

Keep IDs exact. Distinguish:

```text
requested
accepted
queued
running
succeeded
approval-pending
approved
recovery-blocked
recovery
armed
trading
stop-requested
stopping
stopped
positions-still-open
```

Do not collapse these states into “done”.

## References

Read as needed:

- `references/commands.md` — current MCP Tool groups and operation sequences;
- `references/safety.md` — safety classes, OAuth scopes and human gates;
- repository `CLI.md` — complete local CLI, MCP Gateway and Artifact protocol;
- repository `OPERATIONS.md` — business roles and workstation nodes;
- repository `DESIGN.md` — authoritative product, risk and Deployment semantics.

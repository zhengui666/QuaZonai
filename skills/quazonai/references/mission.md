# Bound Mission operations

This procedure is for the Agent already launched inside a QuaZonai research Mission. It is not a way for an external assistant to create a Mission or gain its identity.

## Required context

Use the trusted launcher's exact project, Cycle, Run, active Attempt and frozen Brief IDs, the existing MCP connection and its tool schemas. Check the host's current tools/list instead of assuming a tool exists. If a binding is absent, return that missing context to the launcher; do not inspect environment variables, credential files or source code to reconstruct it.

## Workflow

1. Call `research.get_brief` with `{"brief_id":"<bound Brief ID>"}`. Read the frozen objective, inputs and budget. Call `run.get` with `{"run_id":"<bound Run ID>"}` when the current state or Attempt must be checked. A tool error is not an empty successful result.
2. Produce only research content within the workspace capability already granted by the launcher. The skill does not grant filesystem or shell tools. Keep outputs within the original budget; do not retrieve Sealed raw data or credentials.
3. Publish a permitted file using `artifact.submit`. Its fields are `schema_version`, `kind`, `workspace_relative_path` and `idempotency_key`. The kind is CODE, PARAMETERS or REPORT. The file is an ordinary UTF-8 file no larger than 2 MiB in the bound workspace; absolute, hidden and symlink paths are rejected. Do not provide `workspace_root`, project/Attempt overrides or secrets.
4. Call `experiment.propose` only with `EXPERIMENT_SUBMIT`, using `{"idempotency_key":"<retained key>","proposal":<native ExperimentProposalV1>}`. Use the Cycle and artifact references from this Mission. The installed tool schema defines the proposal; never copy an HTTP wrapper or invent a result/author field.
5. Return original artifact/experiment IDs, the observed Run state and unmet work. Submitted content remains research output, not independent scientific evidence or delivery permission.

Each tool rechecks current authorization and Attempt ownership. Tool discovery or a previous successful call is not permanent authority. After expiry, revocation or takeover, stop and return control to the trusted launcher. Do not obtain an Operator grant, start another server/CLI, change UUIDs or make an HTTP/SQL fallback.

## Uncertain results

A timeout or `MCP_CONTROL_UNAVAILABLE` after submission may mean the write committed. Retain the same key and unchanged file/proposal; `run.get` does not itself prove whether an artifact or proposal was committed. If current authority still permits retry, replay only that exact submission with its original key to retrieve the original receipt. With an unknown reason for `MCP_CONTROL_REJECTED` (including a bare HTTP 429), do not assume a retryable rate limit: return the code/status to the launcher for reconciliation.

Disconnecting MCP or reaching its limit does not cancel the remote Run. There is no cancellation, policy-editing, approval, Claim or ACK tool in this Mission surface.

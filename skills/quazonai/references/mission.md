# Bound Mission operations

Use this procedure only inside an existing research Mission.

## Required context

Use the launcher's existing MCP connection and current tool schemas. The adapter owns the project, Cycle, Run, active Attempt and frozen Brief binding. Missing tools/context return to the launcher; do not reconstruct credentials or bindings from environment, files or source.

## Workflow

1. Call `research.get_brief` with `{}` for the frozen objective, inputs and budget. Call `run.get` with `{}` when the state/Attempt is needed. Neither accepts an ID; tool errors are not empty successful results.
2. Produce research content within the granted workspace capability and original budget. This Skill grants no filesystem/shell tools or Sealed-data access.
3. `artifact.submit` takes only `kind`, `workspace_relative_path` and `idempotency_key`. Use the [research content rules](research.md). Pass an ordinary file's relative path in the bound workspace; absolute, hidden and symlink paths are rejected. Do not send a CLI wrapper, `schema_version`, workspace root or identity overrides.
4. With `EXPERIMENT_SUBMIT`, call `experiment.propose` with `{"idempotency_key":"<retained key>","proposal":{...}}`. Its proposal accepts `family_id`, `parent_experiment_id`, `hypothesis`, `expected_failure_modes`, `proposal_artifact_id`, `parameter_artifact_id` and `code_artifact_id`; use original Mission artifact references and the tool schema's required/nullable rules. The adapter supplies `schema_version` and `cycle_id`; do not submit them or copy the full HTTP DTO.
5. Return artifact/experiment IDs, observed Run state and unmet work. Submissions remain research output, not independent evidence or delivery permission.

Each tool rechecks authorization and current Attempt ownership. Expiry, revocation or takeover returns control to the launcher; never acquire grants, change identities or use an HTTP/SQL/CLI fallback.

## Uncertain results

A timeout or `MCP_CONTROL_UNAVAILABLE` may follow a committed write. Retain the key and unchanged file/proposal; `run.get` cannot prove submission outcome. If still authorized, retry only that original submission/key for its receipt. An unclassified `MCP_CONTROL_REJECTED`, including bare HTTP 429, returns to the launcher rather than an assumed retry loop.

Disconnecting MCP or reaching its limit does not cancel the remote Run. There is no cancellation, policy-editing, approval, Claim or ACK tool in this Mission surface.

# Inspect Alpha, portfolio and delivery evidence

These are scoped reads. Use the saved login with `quazonai client`, or retain provisioned connection flags for a scoped machine, before the table's native arguments. Resolve IDs from their original parent resources; preserve original versions and provenance.

| Question | Native arguments |
| --- | --- |
| What happened to the native experiment? | `experiment show EXPERIMENT_ID`; `experiment summary EXPERIMENT_ID`; when the original report is needed, `experiment result EXPERIMENT_ID` |
| Which Alphas and versions exist? | `alpha list --project-id PROJECT_ID --limit 20`; `alpha versions ALPHA_ID --limit 20`; `alpha show ALPHA_ID VERSION` |
| Why is an Alpha version qualified or not? | `alpha evaluations ALPHA_VERSION_ID --limit 20`; `alpha qualifications ALPHA_VERSION_ID --limit 20`; `alpha calibration ALPHA_VERSION_ID` |
| What does an evaluation actually report? | `evidence show EVALUATION_ID`; `evidence metrics EVALUATION_ID --limit 20` |
| What is in a portfolio candidate? | `portfolio candidate list PROJECT_ID --limit 20`; `portfolio candidate show CANDIDATE_ID`; `portfolio candidate evaluations CANDIDATE_ID --limit 20` |
| What does the accepted native portfolio report? | `portfolio candidate summary CANDIDATE_ID` |
| What assumptions and constraints apply? | `portfolio mandate show MANDATE_ID`; `portfolio assumptions show ASSUMPTIONS_ID` |
| What happened to the selected Cycle? | `cycle selection CYCLE_ID`; `cycle trials CYCLE_ID --limit 20` |
| What is the release/delivery state? | `release show RELEASE_ID`; `approval list RELEASE_ID --limit 20`; `handoff show HANDOFF_ID` |
| What output was produced? | `artifact show ARTIFACT_ID`; only when requested, `artifact export ARTIFACT_ID` |

Keep the three identifiers distinct: `ALPHA_ID` is `AlphaView.id`; `VERSION` is the decimal-string version number; `ALPHA_VERSION_ID` is the selected `AlphaVersionView.id` returned by `alpha versions` or `alpha show`. First resolve the Alpha and requested version, verify the version's `alpha_id` matches that Alpha, then pass the version object's `id` to evaluations, qualifications and calibration. Do not pass the parent `alpha_id` or the version number to those reads. When the user asks for the current version, a non-null `AlphaView.active_version_id` identifies that version; no active version is a missing prerequisite, not permission to invent an ID or choose a historical version silently.

Never infer qualification from an uploaded REPORT, an empty list or the exit code of a computation. Read the actual evaluation outcome, data origin, applicable versions/window, sample limitations and current qualification record. A missing metric is unknown or unavailable, not zero. Distinguish synthetic fixtures, unverified PIT and genuine native evidence. A real report can still fail qualification or be stale for the current decision.

For a portfolio explanation, keep the original candidate, member versions, target weights, frozen policy/mandate and evaluation together. Do not relabel Alpha-level returns as a portfolio equity curve or infer missing daily returns. Explain the actual public metrics and limitations without reimplementing a backtest or inventing a chart's underlying data.

Native experiment summaries keep fresh-capital folds separate; do not join their
equity or average their Sharpe ratios. Preserve each statistic's value, status,
reason and scope; previews are display-only. Native candidate summaries describe
either one shared-capital `HISTORICAL_REPLAY` or a target-only `CURRENT_DECISION`
with no simulated returns. Keep the original Run, accepted Attempt and report
bindings. These reads neither rerun science nor establish Forecast qualification.
Do not substitute a native summary for a Forecast candidate's evaluation records.

For a requested portfolio build, simulation, study or Alpha evaluation, discover unfamiliar fields with the exact `quazonai client portfolio ... --help` / `alpha evaluate --help` and native DTO, then follow [research submission](research.md). Authorized routine writes do not require preview. `alpha evaluate ALPHA_VERSION_ID` targets the version object's UUID. A successful read grants no authority for a new run, independent evaluation, approval or delivery.

Only report a release as approved or a handoff as acknowledged when its current record says so. Reading approval/Claim/ACK state does not authorize creating it. A stale or revoked authorization must not be treated as current. QuaZonai's package is target-only: no claim about real fills, positions, account NAV or active broker controls follows from it.

`NATIVE_TARGET_DECISION` delivery supports a fresh Paper account only; an approved
native target is not a Live launch or continuation of an existing account. Read
the original [account source, current state and history](account-transport.md)
separately. An ACK is not a fill, and account totals or balance differences are
not an individual Alpha's or Release's strategy return.

For exports, select a user-approved output file and check the command's exit status before using it; a failed redirection may have left an empty or incomplete file. Do not dump large artifacts into the conversation when metadata or a bounded summary answers the question.


## Active target delivery is V2 only

New downstream configurations use only `accepted_package_versions: ["2"]`. A requested Claim uses the existing `handoff claim HANDOFF_ID` command with `HandoffClaimV1` request fields `schema_version: 1`, the original `external_claim_id`, and `package_schema_version: "2"`. The DTO's `V1` suffix is not the target package version. Discover the exact request with `quazonai openapi --schema HandoffClaimV1`; keep the existing downstream identity, project/environment scope and Idempotency-Key. A local `--preview` checks wire shape without sending, checking server eligibility or granting authority. Invalid V1 Claim/configuration input is rejected before preview or transmission; do not retry it by relabeling an old package.

An active Claim contains a real package version `"2"` and selects its business body by `source_kind`:

- `FORECAST_EVALUATION` retains the original Forecast Alpha qualifications, independent evaluations, mandate, targets and capital assumptions. Its `source`, `current_weights`, `execution_settings` and `forward_dataset` freeze the original accepted Build and Forward metadata, including complete original instrument-definition histories for the portfolio assets and holdings. Consume these frozen values from the Claim; provenance artifact IDs are audit references, not permission to fetch arbitrary research artifacts. The Build environment is a research fact; the separately approved Handoff environment governs delivery
- `NATIVE_TARGET_DECISION` retains its separate native decision source and execution settings. Use its existing authorized metadata verification path; do not manufacture a Forecast metadata bundle or missing Native fields, or add research authority to make a host work

Historical V1 release records, downloads, settings and probe observations remain readable as their original history. They cannot be approved, offered or claimed as active V2 packages, and must not be rewritten or used as proof of V2 support. Select V2 explicitly for any settings update and obtain a current native readiness observation; configuration alone does not establish host support. V2 does not itself establish approval, usable funds, data eligibility, a running Paper account, or permission to publish Synthetic data. Preserve the original RMSE/Forecast and native target-weight research paths as separate models.


## Paper initialization and consumption

An explicitly authorized first Forecast Paper Build can create an immutable
model-funding root; it is not an observed account balance. Amount and currency remain the
original execution-settings/mandate values. Keep that original root through
subsequent Paper weights and measurements. SYNTHETIC economic lineage does not
change the required REAL/PIT market evidence or original qualified Forecast and
independent evaluation gates, and cannot be promoted to REAL/Live by relabeling.

Distinguish the initial-condition record, a qualified/approved Paper Claim, a
`CONSUMED` initialization attempt, native engine startup and actual complete
simulation evidence. `PaperInitialExecutionViewV1` reports only the consumption
fact and canonical original Claim; it is not a reusable launch permit. Inspect
its schema with `quazonai openapi --schema PaperInitialExecutionViewV1` when needed.
A saved Claim, an old fresh-looking receipt or `replayed: true` is never permission
to initialize another engine.

For that Forecast initialization, the foreground native host owns its fresh
authenticated consume request and its private one-use permit. Do not manually submit the consume endpoint, invent an
`owner_instance_id`, import a receipt as a permit or retry with another owner/key.
Timeout, uncertain consumption, restart or unsupported native recovery must remain
BLOCKED; do not reset the original root or assume an empty account. A no-market
constructor check is not a running Paper account or return evidence.

For that initial condition in the existing Polymarket service,
`initial_execution_authority` references only the existing QZ `origin`, absolute
`credential_file`, and optional existing
`ca_certificate`. It uses the original claiming Downstream credential, never the
local loopback service bearer. Report a missing reference to the authorized host
operator; do not create/save credentials, issue grants, change settings or start a
service under this read procedure. Service provisioning remains outside this
Skill. Preserve all source and transport checks, and report missing actual market,
claim or native-execution evidence rather than treating schema discovery as a run.


A separately authorized Native TargetDecision `FreshPaperCashV1` experiment uses
its own fresh engine/session and original handoff/release/source identity. Keep
that independent Synthetic simulation separate from continuation of a managed
Forecast initialization root, even if trader/account display labels repeat.
Do not merge their account sources, root identity, initial capital or return
series, and do not present a new experiment as a restart or recovery of the old
account. This distinction preserves both existing research models; it does not
permit consuming the same Forecast root twice.

# Inspect Alpha, portfolio and delivery evidence

These are scoped reads. Use the saved login with `quazonai client`, or retain provisioned connection flags for a scoped machine, before the table's native arguments. Resolve IDs from their original parent resources; preserve original versions and provenance.

| Question | Native arguments |
| --- | --- |
| Which Alphas and versions exist? | `alpha list --project-id PROJECT_ID --limit 20`; `alpha versions ALPHA_ID --limit 20`; `alpha show ALPHA_ID VERSION` |
| Why is an Alpha version qualified or not? | `alpha evaluations ALPHA_VERSION_ID --limit 20`; `alpha qualifications ALPHA_VERSION_ID --limit 20`; `alpha calibration ALPHA_VERSION_ID` |
| What does an evaluation actually report? | `evidence show EVALUATION_ID`; `evidence metrics EVALUATION_ID --limit 20` |
| What is in a portfolio candidate? | `portfolio candidate list PROJECT_ID --limit 20`; `portfolio candidate show CANDIDATE_ID`; `portfolio candidate evaluations CANDIDATE_ID --limit 20` |
| What assumptions and constraints apply? | `portfolio mandate show MANDATE_ID`; `portfolio assumptions show ASSUMPTIONS_ID` |
| What happened to the selected Cycle? | `cycle selection CYCLE_ID`; `cycle trials CYCLE_ID --limit 20` |
| What is the release/delivery state? | `release show RELEASE_ID`; `approval list RELEASE_ID --limit 20`; `handoff show HANDOFF_ID` |
| What output was produced? | `artifact show ARTIFACT_ID`; only when requested, `artifact export ARTIFACT_ID` |

Keep the three identifiers distinct: `ALPHA_ID` is `AlphaView.id`; `VERSION` is the decimal-string version number; `ALPHA_VERSION_ID` is the selected `AlphaVersionView.id` returned by `alpha versions` or `alpha show`. First resolve the Alpha and requested version, verify the version's `alpha_id` matches that Alpha, then pass the version object's `id` to evaluations, qualifications and calibration. Do not pass the parent `alpha_id` or the version number to those reads. When the user asks for the current version, a non-null `AlphaView.active_version_id` identifies that version; no active version is a missing prerequisite, not permission to invent an ID or choose a historical version silently.

Never infer qualification from an uploaded REPORT, an empty list or the exit code of a computation. Read the actual evaluation outcome, data origin, applicable versions/window, sample limitations and current qualification record. A missing metric is unknown or unavailable, not zero. Distinguish synthetic fixtures, unverified PIT and genuine native evidence. A real report can still fail qualification or be stale for the current decision.

For a portfolio explanation, keep the original candidate, member versions, target weights, frozen policy/mandate and evaluation together. Do not relabel Alpha-level returns as a portfolio equity curve or infer missing daily returns. Explain the actual public metrics and limitations without reimplementing a backtest or inventing a chart's underlying data.

For a requested portfolio build, simulation, study or Alpha evaluation, discover unfamiliar fields with the exact `quazonai client portfolio ... --help` / `alpha evaluate --help` and native DTO, then follow [research submission](research.md). Authorized routine writes do not require preview. `alpha evaluate ALPHA_VERSION_ID` targets the version object's UUID. A successful read grants no authority for a new run, independent evaluation, approval or delivery.

Only report a release as approved or a handoff as acknowledged when its current record says so. Reading approval/Claim/ACK state does not authorize creating it. A stale or revoked authorization must not be treated as current. QuaZonai's package is target-only: no claim about real fills, positions, account NAV or active broker controls follows from it.

For exports, select a user-approved output file and check the command's exit status before using it; a failed redirection may have left an empty or incomplete file. Do not dump large artifacts into the conversation when metadata or a bounded summary answers the question.

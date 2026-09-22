# Research preparation and submission

Use the connection flags from [connection](connection.md) with each `server client` command below. Table entries are the arguments after that prefix, not separate executable programs. Use only the current credential's authorized project and capabilities.

## Inspect before changing

| Need | Native arguments |
| --- | --- |
| Resolve project | `project list --limit 20`; then `project show ID` |
| Read the research objective | `brief list PROJECT_ID --limit 20`; then `brief show BRIEF_ID` |
| Inspect frozen inputs and policy | `input-set list --project-id PROJECT_ID --limit 20`; `input-set show ID`; `policy show ID` |
| Inspect registered source metadata | `data source show ID`; `data revision list --source-id ID --limit 20`; `data revision show ID` |
| Check execution readiness | `runtime readiness ID`; `codex models ID`; `codex account ID` |
| Find existing work | `cycle list PROJECT_ID --limit 20`; `cycle show ID`; `experiment list --project-id PROJECT_ID --limit 20` |

Read IDs referenced by the actual Brief/InputSet rather than scanning all sources. Explain missing or stale readiness as a blocker. Registration, metadata access, native profile discovery and previous readiness observations are not proof of data quality, PIT verification, a usable account or current scientific eligibility. Do not read Sealed raw data or native storage paths. Inspection must not refresh a probe, log in or alter data permissions behind the user's back.

For a question such as “can this Polymarket dataset support this Alpha study?”, return the selected project/InputSet/revision and the observed origin, licensing/PIT and coverage/readiness limitations. Do not infer compatibility from a provider name or successful registration alone.

## Prepare a research operation

| Requested operation | Native arguments | Request schema |
| --- | --- | --- |
| Save/replace a Brief | `brief create PROJECT_ID` / `brief update BRIEF_ID` | `BriefCreate` / `BriefUpdate` |
| Freeze the selected Brief | `brief freeze BRIEF_ID` | `BriefFreezeV1` |
| Start its research Cycle | `cycle start PROJECT_ID` | `CycleStartV1` |
| Validate a frozen InputSet | `data validate` | `DataValidateRequest` |
| Publish research content | `artifact submit` | `ArtifactCreate` |
| Propose an experiment | `experiment propose` | `ExperimentProposalV1` |

Discover the named schema from the installed binary. Populate only user-specified intent and values from the selected current service snapshots. Do not invent IDs, budgets, data roles, policy versions, assumptions or a runtime. Missing scientific decisions belong back with the user; missing authorization belongs with the human Operator. The server may require an exact Operator grant for preparation/start/validation commands. Do not self-issue it; a Mission cannot take this route at all.

An external delegated CLI operation uses an already provided exact grant only when the trusted host has explicitly authorized that specific request for that identity. It does not confer standing Operator status. Preserve any `expected_revision` from the snapshot; a conflict requires rereading and reconsidering intent, not silently substituting the latest revision.

Preview the final request and retain its original file and idempotency key. Send that request once. Read the resulting resource/Run IDs; a queued Run is followed using [runs](runs.md), never by invoking start again. Frozen objects are not edited in place. A saved draft is not frozen, and a frozen Brief is not an evaluation pass.

## Submit research artifacts and experiments

For an external client with `ARTIFACT_SUBMIT`, `ArtifactCreate` contains `schema_version`, the exact `project_id`, `kind` and UTF-8 `content`. Only CODE, PARAMETERS and REPORT are research submission kinds. Content must be nonblank, contain no NUL and be no larger than 2 MiB. CODE contains source text; PARAMETERS and REPORT must contain a JSON object with numeric `schema_version: 1`, not raw Markdown, a JSON array or a string-valued version. For example, a report's inner content may be `{"schema_version":1,"text":"research notes"}`; encode that document as the outer DTO's `content` string. A locally valid preview does not validate this inner document or confer scientific eligibility. Do not submit arbitrary paths, producer/origin claims, other Attempts or self-reported qualification.

For `EXPERIMENT_SUBMIT`, discover `ExperimentProposalV1`; use the original Cycle, inputs and registered artifact references. Proposed/PENDING is not executed, evaluated or qualified. Preserve the server's experiment ID and outcome. Return current output/experiment budget failures to the task; do not create another Attempt or key to evade them.

Inside a bound Mission, use [Mission operations](mission.md) instead. Its artifact API takes a workspace-relative filename rather than a raw HTTP content body, and its project/Run/Attempt binding belongs to the trusted launcher.

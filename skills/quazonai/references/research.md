# Research preparation and submission

Table entries follow `quazonai client` with the saved login or original scoped connection from [connection](connection.md).

## Inspect before changing

| Need | Native arguments |
| --- | --- |
| Resolve project | `project list --limit 20`; then `project show ID` |
| Read the research objective | `brief list PROJECT_ID --limit 20`; then `brief show BRIEF_ID` |
| Inspect frozen inputs and policy | `input-set list --project-id PROJECT_ID --limit 20`; `input-set show ID`; `policy show ID` |
| Inspect registered source metadata | `data source show ID`; `data revision list --source-id ID --limit 20`; `data revision show ID` |
| Check execution readiness | `runtime readiness ID`; `codex models ID`; `codex account ID` |
| Find existing work | `cycle list PROJECT_ID --limit 20`; `cycle show ID`; `experiment list --project-id PROJECT_ID --limit 20` |

Follow IDs from the Brief/InputSet rather than scanning all sources. Registration and prior readiness observations do not prove data quality, PIT, account usability or current eligibility. Report missing/stale prerequisites. Inspection does not authorize probes, login, permission changes or access to raw Sealed data/native storage paths.

For dataset suitability, report the selected project/InputSet/revision and observed origin, licensing/PIT, coverage and readiness limitations; a provider name alone proves no compatibility.

## Prepare a research operation

| Requested operation | Native arguments | Request schema |
| --- | --- | --- |
| Save/replace a Brief | `brief create PROJECT_ID` / `brief update BRIEF_ID` | `BriefCreate` / `BriefUpdate` |
| Freeze the selected Brief | `brief freeze BRIEF_ID` | `BriefFreezeV1` |
| Start its research Cycle | `cycle start PROJECT_ID` | `CycleStartV1` |
| Validate a frozen InputSet | `data validate` | `DataValidateRequest` |
| Publish research content | `artifact submit` | `ArtifactCreate` |
| Propose an experiment | `experiment propose` | `ExperimentProposalV1` |

Discover unfamiliar fields from the named schema. Populate user intent and current snapshot values; missing scientific decisions need the user, not invented IDs, budgets, data roles, policy versions, assumptions or runtime. Follow the entry Skill's authority rules; Missions use their bound tools.

Preserve `expected_revision` from the snapshot. A conflict requires rereading and reconsidering intent, not silently substituting the latest revision.

Retain the original request and one idempotency key. Authorized routine writes can execute directly; preview complex new requests or those needing inspection, destructive-action checks or human review. Send once and follow the returned Run using [runs](runs.md). Frozen objects are immutable; a draft is not frozen and freezing is not evaluation PASS.

## Submit research artifacts and experiments

With `ARTIFACT_SUBMIT`, populate native `ArtifactCreate` from the selected project and encode the research content below as its `content` string. Do not add arbitrary paths, producer/origin claims, other Attempts or self-reported qualification. Local preview does not validate the inner document or scientific eligibility.

With `EXPERIMENT_SUBMIT`, use `ExperimentProposalV1` with the original Cycle, inputs and registered artifact references. Preserve its ID and outcome; PENDING is not executed, evaluated or qualified. Budget failures return to the task owner; another Attempt/key must not evade them.

Inside a [bound Mission](mission.md), artifact submission takes a workspace-relative filename; the adapter supplies fixed identities.

## Research content

CODE, PARAMETERS and REPORT are the permitted research kinds. Content is nonblank UTF-8, contains no NUL and is at most 2 MiB. CODE contains source text. PARAMETERS and REPORT contain a JSON object with numeric `schema_version: 1`, for example `{"schema_version":1,"text":"research notes"}`. Raw Markdown, arrays and string-valued schema versions are invalid. These content rules apply to both CLI content strings and Mission files.

# Research preparation and submission

Table entries follow `quazonai client` with the saved login or original scoped connection from [connection](connection.md).

## Inspect before changing

| Need | Native arguments |
| --- | --- |
| Resolve project | `project list --limit 20`; then `project show ID` |
| Read the research objective | `brief list PROJECT_ID --limit 20`; then `brief show BRIEF_ID` |
| Recover the frozen execution bindings | `brief execution-context BRIEF_ID` |
| Read registered original feature parts | `data features list DATASET_REVISION_ID --project-id PROJECT_ID` |
| Inspect frozen inputs and policy | `input-set list --project-id PROJECT_ID --limit 20`; `input-set show ID`; `policy show ID` |
| Inspect registered source metadata | `data source show ID`; `data revision list --source-id ID --limit 20`; `data revision show ID` |
| Inspect registered evidence summary (owner, non-Sealed) | `data revision evidence ID` |
| Check execution readiness | `runtime readiness ID`; `codex models ID`; `codex account ID` |
| Find existing work | `cycle list PROJECT_ID --limit 20`; `cycle show ID`; `experiment list --project-id PROJECT_ID --limit 20` |

Follow IDs from the Brief/InputSet rather than scanning all sources. Registration and prior readiness observations do not prove data quality, PIT, account usability or current eligibility. Report missing/stale prerequisites. Inspection does not authorize probes, login, permission changes or access to raw Sealed data/native storage paths.

For dataset suitability, report the selected project/InputSet/revision and observed origin, licensing/PIT, coverage and readiness limitations; a provider name alone proves no compatibility.

`data revision evidence DATASET_REVISION_ID` reads only that Dataset's registered
metadata and quality documents, validates their stored associations, and returns
a typed registration/quality summary. Use the saved owner login; scoped Machine,
Mission and diagnostic CLI credentials cannot use this route. Sealed evidence is
not exposed (`DATASET_EVIDENCE_SEALED_UNSUPPORTED`); use `data revision show ID`
for its existing registration labels.
The summary includes fixed source/partition types, immutable UUID references,
counts and timestamps. It excludes free-text source explanations, URLs, native
paths, raw documents, instruments, BAR values and settlements. It retains the registered PIT/origin labels and current license
state, including expired or revoked licenses for historical audit. It does not
reopen the native snapshot, establish historical availability, extend a license,
or constitute fresh DATA_VALIDATE/PASS evidence. An artifact-route 404 alone
never proves that registered evidence bytes are missing. Report failed evidence
reads without re-registering or changing PIT labels.

## Prepare a research operation

| Requested operation | Native arguments | Request schema |
| --- | --- | --- |
| Save/replace a Brief | `brief create PROJECT_ID` / `brief update BRIEF_ID` | `BriefCreate` / `BriefUpdate` |
| Freeze the selected Brief | `brief freeze BRIEF_ID` | `BriefFreezeV1` |
| Start its research Cycle | `cycle start PROJECT_ID` | `CycleStartV1` |
| Validate a frozen InputSet | `data validate` | `DataValidateRequest` |
| Register an original recorded feature part | `data features register DATASET_REVISION_ID` | `RecordedFeatureRegisterV1` |
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

For scientific experiments, the generic JSON-object rule above is only the
artifact storage envelope. Before writing a parameter artifact, discover its
inner contract separately:

```sh
quazonai openapi --domain --schema ExperimentEvaluationParametersV1
quazonai openapi --domain --schema FeatureObservationsV1
```

`ExperimentEvaluationParametersV1` is the experiment's parameter document;
`FeatureObservationsV1` describes a feature-observation document. Keep each as
JSON encoded once in the outer `ArtifactCreate.content` string, not as extra
outer request fields. Follow the selected contract's transitive schemas for
settings, feature definitions, nullable values and decimal-string counters.
The installed contract describes wire structure, not approved scientific
choices or server eligibility. Do not infer model ABI or code requirements
from a JSON schema; report an unavailable contract or unsupported workflow.

For native recorded feature registration, discover `RecordedFeatureRegisterV1`
from the default HTTP scope and preserve the original attachment bytes. Schema
discovery never authorizes synthesizing or relabeling feature observations;
self-authored PARAMETERS uploads cannot establish recorded-source provenance.

## Preserve original recorded feature provenance

`brief execution-context BRIEF_ID` reads the existing `FrozenBriefV1`; it does not
freeze again. Draft Briefs have no frozen execution context. Preserve the returned
Runtime revision and Discovery/Validation/Sealed InputSet IDs rather than
reconstructing them from newer records.

Use `data features register DATASET_REVISION_ID` only for original recorded
feature bytes already described in that Dataset's frozen native metadata. The
body's `dataset_revision_id` must equal the route ID. Supply the original
`feature_part_key`, project and unmodified UTF-8 attachment as `content`; the
2 MiB encoded-content limit applies. Do not reserialize the inner JSON or replace
missing clocks/values. The server retains the actual Dataset origin, PIT and
license binding; callers cannot assert them. Raw Sealed feature registration is
not supported.

Keep the same original bytes and idempotency key for retries. Read back with
`data features list DATASET_REVISION_ID --project-id PROJECT_ID`; the bounded
result contains at most 16 registered part descriptors and source bindings, not
the original observations. An empty list is not permission to reconstruct an
attachment. Normal `artifact submit` research parameters remain self-authored
research artifacts and must not be substituted for native recorded feature
provenance. Registration proves neither verified PIT nor evaluation success.

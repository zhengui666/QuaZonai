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
| Start its internally managed research Cycle | `cycle start PROJECT_ID` | `CycleStartV1` |
| Start an external-Agent Cycle | `cycle start-external PROJECT_ID` | `ExternalCycleStartV1` |
| Close a settled external Cycle | `cycle finish-external CYCLE_ID` | `CycleFinishExternalV1` |
| Validate a frozen InputSet | `data validate` | `DataValidateRequest` |
| Register an original recorded feature part | `data features register DATASET_REVISION_ID` | `RecordedFeatureRegisterV1` |
| Publish research content | `artifact submit` | `ArtifactCreate` |
| Propose an experiment | `experiment propose` | `ExperimentProposalV1` |
| Compile and independently evaluate the original proposal | `experiment evaluate EXPERIMENT_ID` | `ExperimentEvaluateV1` |
| Adopt one accepted native fold as a target-weight Alpha | `experiment adopt-alpha EXPERIMENT_ID` | `StrategyAlphaAdoptV1` |
| Build the original Forecast portfolio | `portfolio build` | `PortfolioBuildEnvelopeV2` → `PortfolioBuildRequestV1` |
| Build the requested native target-weight portfolio | `portfolio build` | `PortfolioBuildEnvelopeV2` → `StrategyPortfolioBuildV1` |

Discover unfamiliar fields from the named schema. Populate user intent and current snapshot values; missing scientific decisions need the user, not invented IDs, budgets, data roles, policy versions, assumptions or runtime. Follow the entry Skill's authority rules; Missions use their bound tools.

Preserve `expected_revision` from the snapshot. A conflict requires rereading and reconsidering intent, not silently substituting the latest revision.

Retain the original request and one idempotency key. Authorized routine writes can execute directly; preview complex new requests or those needing inspection, destructive-action checks or human review. Send once and follow the returned Run using [runs](runs.md). Frozen objects are immutable; a draft is not frozen and freezing is not evaluation PASS.

## Submit research artifacts and experiments

With `ARTIFACT_SUBMIT`, populate native `ArtifactCreate` from the selected project and encode the research content below as its `content` string. Do not add arbitrary paths, producer/origin claims, other Attempts or self-reported qualification. Local preview does not validate the inner document or scientific eligibility.

With `EXPERIMENT_SUBMIT`, use `ExperimentProposalV1` with the original Cycle, inputs and registered artifact references. Preserve its ID and outcome; PENDING is not executed, evaluated or qualified. Budget failures return to the task owner; another Attempt/key must not evade them.

Inside a [bound Mission](mission.md), artifact submission takes a workspace-relative filename; the adapter supplies fixed identities.

## External native research sequence

Use this sequence only for requested external-Agent research; a bound Mission
retains its own tools. Each write takes the table's DTO as JSON on stdin and its
own retained `--idempotency-key`, as in [connection](connection.md).

1. `cycle start-external PROJECT_ID` uses the original frozen `brief_id` and
   Project `expected_revision`. It creates a Cycle without internal researcher or
   reviewer profiles; it does not invoke a model or create a scientific result.
2. Submit the original artifacts and `experiment propose`, then read
   `experiment show EXPERIMENT_ID`. `experiment evaluate EXPERIMENT_ID` uses that
   Experiment's `expected_revision` and approved `compile_limits` and
   `evaluation_limits`. Follow its returned Run with [runs](runs.md), then reread
   the Experiment for the evaluation Run/result; compilation success alone is
   not completed evaluation.
3. Read `experiment summary EXPERIMENT_ID` and, when needed,
   `experiment result EXPERIMENT_ID`. If adoption is requested, supply the current
   Experiment revision, requested `name` and original `source_fold_index` to
   `experiment adopt-alpha EXPERIMENT_ID`. Retain the returned Alpha/version and
   accepted source IDs; adoption is reusable research policy, not qualification.
4. For a native Build, use each returned version object's `id` as the member's
   `alpha_version_id`, with its requested `ensemble_weight` in
   `StrategyPortfolioBuildV1`. Retain the original Cycle,
   mandate, InputSet, Runtime revision and limits. Choose the requested purpose:
   `HISTORICAL_REPLAY` or `CURRENT_DECISION` with its original member inputs and
   `FreshPaperCashV1` account start. Native target delivery is fresh Paper only;
   this is separate from the Forecast funding procedure below. Follow the Build
   Run, then inspect its original `portfolio candidate summary CANDIDATE_ID` and
   [delivery/account evidence](results.md).
5. After all admitted scientific work settles, reread `cycle show CYCLE_ID` and
   use that Cycle's `expected_revision` for `cycle finish-external CYCLE_ID`.
   Closing preserves unexecuted proposals as history and returns an inconclusive
   batch outcome; it does not create a review, PASS, approval or Paper execution.

## Research content

CODE, PARAMETERS and REPORT are the permitted research kinds. Content is nonblank UTF-8 and contains no NUL. Original content bytes are retained without an application byte cap. CODE contains source text. PARAMETERS and REPORT contain a JSON object with numeric `schema_version: 1`, for example `{"schema_version":1,"text":"research notes"}`. Raw Markdown, arrays and string-valued schema versions are invalid. These content rules apply to both CLI content strings and Mission files.

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
An omitted or null `total_fuel` disables Wasmi fuel metering; an explicit budget
remains a decimal-string counter. Results report `consumed_fuel: null` for
unmetered execution, which is distinct from a measured `"0"`.
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
`feature_part_key`, project and unmodified UTF-8 attachment as `content`. There
is no application encoded-content byte cap. Do not reserialize the inner JSON or replace
missing clocks/values. The server retains the actual Dataset origin, PIT and
license binding; callers cannot assert them. Raw Sealed feature registration is
not supported.

Keep the same original bytes and idempotency key for retries. Read back with
`data features list DATASET_REVISION_ID --project-id PROJECT_ID`; the result
contains all registered part descriptors and source bindings, not
the original observations. An empty list is not permission to reconstruct an
attachment. Normal `artifact submit` research parameters remain self-authored
research artifacts and must not be substituted for native recorded feature
provenance. Registration proves neither verified PIT nor evaluation success.


## Explicit first Paper funding

Use the existing `portfolio build` only when first Paper initialization is in the
user's requested scope. It remains the original Forecast Build, not a different
Alpha model, an account observation, a Live operation or an approval. Discover
the current native fields before preparing the original request:

```sh
quazonai client portfolio build --help
quazonai openapi --schema PortfolioBuildEnvelopeV2
quazonai openapi --schema PortfolioBuildWeightsV1
```

Retain the original Cycle, Mandate/InputSet, Runtime revision, qualified members,
limits and frozen market bindings. Use Paper and this weight-source fragment only
with the verified existing native scope supplied by the authorized operator:

```json
{
  "environment": "PAPER",
  "current_weights_source": {
    "kind": "PAPER_INITIAL_CAPITAL",
    "downstream_id": "<verified downstream UUID>",
    "trader_id": "<original native trader ID>",
    "account_id": "<original native account ID>"
  }
}
```

This is only a fragment; keep the other required fields from the selected original
Forecast request, and never submit placeholders or invent scope IDs. Do not supply
an amount, currency, initialization/session ID or reset flag. Capital and currency
come only from the original frozen execution settings and matching mandate; a
missing or inconsistent original value is a blocker, not permission to fill one
in. Market inputs must still satisfy the original REAL/PIT rules, with the same
Alpha qualification and independent evaluation requirements.

Preserve one original request and idempotency key. An authorized Build uses the
existing command and connection procedure; `--preview` is local wire inspection,
not a server eligibility check or a funding event. Follow the original Run and
its evidence. A queued Build does not prove initialization, qualification or
native execution succeeded.

The same Forecast Paper holding, claim or initialization root cannot be made
empty by changing project, mandate, session, owner or output directory. Use its
original verified Paper snapshot/LastTarget lineage for continuation; never
request another funding event for that root or relabel its Synthetic economics
as REAL/Live. For an uncertain result, preserve the same inputs and follow
[recovery](recovery.md), without a new owner/key to evade the one-time boundary.
See [Paper delivery results](results.md) before interpreting a Claim or receipt.

An independently requested Native TargetDecision `FreshPaperCashV1` experiment
retains its existing fresh-session semantics. Repeated trader/account labels alone
do not make separate native sessions the same account source. Keep its new session,
source and handoff/release identities distinct; never substitute that separate
experiment for recovery of the original Forecast account or merge their returns.
Switching research models still requires the user's corresponding intent.

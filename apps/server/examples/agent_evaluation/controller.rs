//! Complete declaration replay, not an execution adapter or an attestation.
#[path = "io.rs"]
pub mod io;

use contracts::{agent_evaluation::*, SchemaV1, Timestamp};
use io::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Suite {
    schema_version: SchemaV1,
    recorded_at: Timestamp,
    runner: AgentEvaluationIdentityV1,
    subject: AgentEvaluationIdentityV1,
    requested: AgentEvaluationPolicyV1,
    source_revision: String,
    source_file: String,
    suite_id: String,
    tuning: Dataset,
    held_out: Dataset,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Dataset {
    id: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    scenario_file: String,
    assertions: Vec<Assertion>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Assertion {
    id: String,
    predicate: Predicate,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
enum Predicate {
    Equals { pointer: String, value: Value },
    Exists { pointer: String },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrozenLock {
    schema_version: SchemaV1,
    suite_sha256: EvaluationSha256,
    source_sha256: EvaluationSha256,
    tuning_sha256: EvaluationSha256,
    held_out_sha256: EvaluationSha256,
    scenarios: Vec<Scenario>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    case_id: String,
    sha256: EvaluationSha256,
}

#[derive(Serialize)]
struct DatasetManifest {
    schema_version: SchemaV1,
    split: AgentEvaluationSplit,
    id: String,
    cases: Vec<ManifestCase>,
}

#[derive(Serialize)]
struct ManifestCase {
    id: String,
    scenario_sha256: EvaluationSha256,
    assertions: Vec<Assertion>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationIndex {
    schema_version: SchemaV1,
    lock_sha256: EvaluationSha256,
    observations: Vec<ObservationRef>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRef {
    case_id: String,
    file: String,
    sha256: EvaluationSha256,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema_version: SchemaV1,
    case_id: String,
    source_sha256: EvaluationSha256,
    suite_sha256: EvaluationSha256,
    scenario_sha256: EvaluationSha256,
    requested: AgentEvaluationPolicyV1,
    observed: Option<AgentEvaluationObservedV1>,
    complete: bool,
    measurements: AgentEvaluationMeasurementsV1,
    data: Value,
}

struct Frozen {
    suite: Suite,
    lock: FrozenLock,
    manifests: [Vec<u8>; 2],
    report: AgentEvaluationReportV1,
}

fn digest(bytes: &[u8]) -> EvaluationSha256 {
    // The SHA implementation always emits a lowercase 64-character digest.
    io::hash(bytes).try_into().expect("SHA-256 format")
}

fn unknown_measurements() -> AgentEvaluationMeasurementsV1 {
    AgentEvaluationMeasurementsV1 {
        input_tokens: None,
        output_tokens: None,
        elapsed_ms: None,
        tool_calls: None,
        cost: None,
    }
}

fn valid_pointer(pointer: &str) -> bool {
    if pointer.len() > 512 || (!pointer.is_empty() && !pointer.starts_with('/')) {
        return false;
    }
    let mut bytes = pointer.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'~' && !matches!(bytes.next(), Some(b'0' | b'1')) {
            return false;
        }
    }
    true
}

fn prepare(input: &Path, budget: &mut io::Budget) -> Result<Frozen> {
    io::directory(input)?;
    let suite_bytes = budget.relative(input, "suite.json")?;
    let suite: Suite = io::parse(&suite_bytes)?;
    let _ = suite.schema_version;
    let count = suite.tuning.cases.len() + suite.held_out.cases.len();
    if count < 2
        || suite.tuning.cases.is_empty()
        || suite.held_out.cases.is_empty()
    {
        return Err("case count limit");
    }
    let source = budget.relative(input, &suite.source_file)?;
    let source_sha256 = digest(&source);
    drop(source);
    let mut manifests = Vec::new();
    let mut scenarios = Vec::new();
    let mut cases = Vec::new();
    for (split, dataset) in [
        (AgentEvaluationSplit::Tuning, &suite.tuning),
        (AgentEvaluationSplit::HeldOut, &suite.held_out),
    ] {
        let mut manifest = DatasetManifest {
            schema_version: SchemaV1,
            split,
            id: dataset.id.clone(),
            cases: Vec::new(),
        };
        for case in &dataset.cases {
            if case.assertions.is_empty() {
                return Err("assertion count limit");
            }
            for assertion in &case.assertions {
                let pointer = match &assertion.predicate {
                    Predicate::Equals { pointer, .. } | Predicate::Exists { pointer } => pointer,
                };
                if !valid_pointer(pointer) {
                    return Err("invalid JSON pointer");
                }
            }
            let scenario = budget.relative(input, &case.scenario_file)?;
            let sha256 = digest(&scenario);
            scenarios.push(Scenario {
                case_id: case.id.clone(),
                sha256: sha256.clone(),
            });
            manifest.cases.push(ManifestCase {
                id: case.id.clone(),
                scenario_sha256: sha256.clone(),
                assertions: case.assertions.clone(),
            });
            cases.push(AgentEvaluationCaseV1 {
                id: case.id.clone(),
                split,
                scenario_sha256: sha256,
                status: AgentEvaluationStatus::Unrun,
                reason: "Protocol only. No observation supplied; no invocation attested.".into(),
                observed: None,
                measurements: unknown_measurements(),
                required_assertions: case.assertions.iter().map(|a| a.id.clone()).collect(),
                assertions: Vec::new(),
            });
        }
        manifests.push(io::json(&manifest)?);
    }
    let [tuning_manifest, held_out_manifest]: [Vec<u8>; 2] =
        manifests.try_into().map_err(|_| "dataset count")?;
    let lock = FrozenLock {
        schema_version: SchemaV1,
        suite_sha256: digest(&suite_bytes),
        source_sha256,
        tuning_sha256: digest(&tuning_manifest),
        held_out_sha256: digest(&held_out_manifest),
        scenarios,
    };
    let dataset = |definition: &Dataset, sha256: &EvaluationSha256| AgentEvaluationDatasetV1 {
        id: definition.id.clone(),
        sha256: sha256.clone(),
        case_ids: definition.cases.iter().map(|c| c.id.clone()).collect(),
    };
    let report = AgentEvaluationReportV1 {
        schema_version: SchemaV1,
        report_kind: AgentEvaluationReportKind::AgentEvaluation,
        mode: AgentEvaluationMode::ProtocolOnly,
        status: AgentEvaluationStatus::Unrun,
        recorded_at: suite.recorded_at,
        runner: suite.runner.clone(),
        subject: suite.subject.clone(),
        requested: suite.requested.clone(),
        source_revision: suite.source_revision.clone(),
        source_sha256: lock.source_sha256.clone(),
        suite_id: suite.suite_id.clone(),
        suite_sha256: lock.suite_sha256.clone(),
        tuning: dataset(&suite.tuning, &lock.tuning_sha256),
        held_out: dataset(&suite.held_out, &lock.held_out_sha256),
        cases,
    };
    domain::agent_evaluation::validate(&report).map_err(|_| "native report contract")?;
    Ok(Frozen {
        suite,
        lock,
        manifests: [tuning_manifest, held_out_manifest],
        report,
    })
}

pub fn freeze(input: &Path, output: &Path) -> Result<String> {
    let frozen = prepare(input, &mut io::Budget::default())?;
    let lock = io::json(&frozen.lock)?;
    let report = io::json(&frozen.report)?;
    io::create_directory(output)?;
    io::write_new(&output.join("lock.json"), &lock)?;
    io::write_new(
        &output.join("lock.sha256"),
        format!("{}\n", io::hash(&lock)).as_bytes(),
    )?;
    io::write_new(&output.join("tuning.manifest.json"), &frozen.manifests[0])?;
    io::write_new(&output.join("held-out.manifest.json"), &frozen.manifests[1])?;
    io::write_new(&output.join("report.json"), &report)?;
    Ok("PROTOCOL_ONLY UNRUN: frozen declarations; no invocation or model quality attested".into())
}

pub fn replay(
    input: &Path,
    lock_path: &Path,
    expected_lock: &str,
    observations: &Path,
    output: &Path,
) -> Result<String> {
    let expected: EvaluationSha256 = expected_lock
        .to_owned()
        .try_into()
        .map_err(|_| "invalid lock digest")?;
    let mut budget = io::Budget::default();
    let lock_bytes = budget.read(lock_path)?;
    if digest(&lock_bytes) != expected {
        return Err("lock digest mismatch");
    }
    let _: FrozenLock = io::parse(&lock_bytes)?;
    let mut frozen = prepare(input, &mut budget)?;
    if io::json(&frozen.lock)? != lock_bytes {
        return Err("frozen input mismatch");
    }
    let lock_root = lock_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    for (name, expected) in ["tuning.manifest.json", "held-out.manifest.json"]
        .into_iter()
        .zip(&frozen.manifests)
    {
        if budget.relative(lock_root, name)? != *expected {
            return Err("dataset manifest mismatch");
        }
    }
    io::directory(observations)?;
    let index_bytes = budget.relative(observations, "observations.json")?;
    let index: ObservationIndex = io::parse(&index_bytes)?;
    let _ = index.schema_version;
    if index.lock_sha256 != expected || index.observations.len() > frozen.report.cases.len() {
        return Err("observation index binding");
    }
    let definitions: BTreeMap<_, _> = frozen
        .suite
        .tuning
        .cases
        .iter()
        .chain(&frozen.suite.held_out.cases)
        .map(|c| (c.id.as_str(), c))
        .collect();
    let mut seen = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut invocations = BTreeSet::new();
    let mut retained = Vec::new();
    for reference in index.observations {
        if !seen.insert(reference.case_id.clone()) || !files.insert(reference.file.clone()) {
            return Err("duplicate observation");
        }
        let case = frozen
            .report
            .cases
            .iter_mut()
            .find(|c| c.id == reference.case_id)
            .ok_or("unknown observation case")?;
        let bytes = budget.relative(observations, &reference.file)?;
        if digest(&bytes) != reference.sha256 {
            return Err("observation digest mismatch");
        }
        let observation: Observation = io::parse(&bytes)?;
        let _ = observation.schema_version;
        if observation.case_id != case.id
            || observation.source_sha256 != frozen.lock.source_sha256
            || observation.suite_sha256 != frozen.lock.suite_sha256
            || observation.scenario_sha256 != case.scenario_sha256
            || observation.requested.model != frozen.suite.requested.model
            || observation.requested.reasoning_effort != frozen.suite.requested.reasoning_effort
        {
            return Err("observation provenance mismatch");
        }
        if let Some(identity) = &observation.observed {
            if !invocations.insert(identity.invocation_id.clone()) {
                return Err("duplicate invocation identity");
            }
        }
        // No partial stream is interpreted as a total tool-call count.
        if !observation.complete && observation.measurements.tool_calls.is_some() {
            return Err("incomplete observation tool count");
        }
        let matches = observation.observed.as_ref().is_some_and(|v| {
            v.settings.model == frozen.suite.requested.model
                && v.settings.reasoning_effort == frozen.suite.requested.reasoning_effort
        });
        case.observed = observation.observed;
        case.measurements = observation.measurements;
        if !matches || !observation.complete {
            case.status = AgentEvaluationStatus::Blocked;
            case.reason = "Protocol declaration blocked: incomplete observation or absent/mismatched identity; no invocation attested.".into();
        } else {
            case.assertions = definitions[case.id.as_str()]
                .assertions
                .iter()
                .map(|assertion| {
                    let passed = match &assertion.predicate {
                        Predicate::Equals { pointer, value } => observation
                            .data
                            .pointer(pointer)
                            .is_some_and(|actual| actual == value),
                        Predicate::Exists { pointer } => {
                            observation.data.pointer(pointer).is_some()
                        }
                    };
                    AgentEvaluationAssertionV1 {
                        id: assertion.id.clone(),
                        passed,
                        evidence_sha256: reference.sha256.clone(),
                    }
                })
                .collect();
            case.status = if case.assertions.iter().all(|a| a.passed) {
                AgentEvaluationStatus::Pass
            } else {
                AgentEvaluationStatus::Fail
            };
            case.reason = "Protocol only: frozen assertions scored against replayed declarations; no invocation or model quality attested.".into();
        }
        retained.push((String::from(reference.sha256), bytes));
    }
    frozen.report.status = domain::agent_evaluation::aggregate(&frozen.report.cases);
    domain::agent_evaluation::validate(&frozen.report).map_err(|_| "native report contract")?;
    let report = io::json(&frozen.report)?;
    io::create_directory(output)?;
    io::write_new(&output.join("lock.json"), &lock_bytes)?;
    io::write_new(
        &output.join("lock.sha256"),
        format!("{expected_lock}\n").as_bytes(),
    )?;
    io::write_new(&output.join("tuning.manifest.json"), &frozen.manifests[0])?;
    io::write_new(&output.join("held-out.manifest.json"), &frozen.manifests[1])?;
    io::write_new(&output.join("observations.index.json"), &index_bytes)?;
    io::create_directory(&output.join("evidence"))?;
    for (hash, bytes) in retained {
        io::write_new(
            &output.join("evidence").join(format!("{hash}.json")),
            &bytes,
        )?;
    }
    // Publish the report only after all retained evidence has been written.
    io::write_new(&output.join("report.json"), &report)?;
    Ok(format!(
        "PROTOCOL_ONLY {:?}: replayed declarations; no invocation or model quality attested",
        frozen.report.status
    )
    .to_uppercase())
}

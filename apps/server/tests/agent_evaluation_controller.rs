//! Public, synthetic controller self-tests. These are not held-out model evidence.
#[path = "../examples/agent_evaluation/controller.rs"]
mod controller;

use controller::{freeze, io, replay};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

struct Pack {
    temp: TempDir,
    input: PathBuf,
    frozen: PathBuf,
    observations: PathBuf,
    suite: Value,
}

fn write(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}
fn read(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

impl Pack {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("input");
        let observations = temp.path().join("observations");
        fs::create_dir(&input).unwrap();
        fs::create_dir(&observations).unwrap();
        fs::write(
            input.join("source.txt"),
            b"public controller source fixture, not a candidate\n",
        )
        .unwrap();
        let cases: Vec<_> = (0..4)
            .map(|n| {
                let name = format!("scenario-{n}.txt");
                fs::write(
                    input.join(&name),
                    format!("public self-test scenario {n}\n"),
                )
                .unwrap();
                json!({"id":format!("case-{n}"),"scenario_file":name,"assertions":[
                    {"id":"answer","predicate":{"kind":"EQUALS","pointer":"/answer","value":42}},
                    {"id":"presence","predicate":{"kind":"EXISTS","pointer":"/a~1b/~0"}}
                ]})
            })
            .collect();
        let suite = json!({
            "schema_version":1,"recorded_at":"2026-09-30T00:00:00Z",
            "runner":{"name":"public-offline-self-test","version":"1"},
            "subject":{"name":"codex","version":"unobserved"},
            "requested":{"model":"gpt-6-luna","reasoning_effort":"max"},
            "source_revision":"public-self-test-not-a-candidate","source_file":"source.txt",
            "suite_id":"public-self-test-v1",
            "tuning":{"id":"public-tuning","cases":[cases[0],cases[1]]},
            "held_out":{"id":"public-held-out-shaped-fixture","cases":[cases[2],cases[3]]}
        });
        write(&input.join("suite.json"), &suite);
        let frozen = temp.path().join("frozen");
        Self {
            temp,
            input,
            frozen,
            observations,
            suite,
        }
    }
    fn save_suite(&self) {
        write(&self.input.join("suite.json"), &self.suite);
    }
    fn freeze(&self) {
        freeze(&self.input, &self.frozen).unwrap();
        write(
            &self.observations.join("observations.json"),
            &json!({
                "schema_version":1,"lock_sha256":self.lock_hash(),"observations":[]
            }),
        );
    }
    fn lock_hash(&self) -> String {
        io::hash(&fs::read(self.frozen.join("lock.json")).unwrap())
    }
    fn observation(&self, n: usize) -> Value {
        let report = read(&self.frozen.join("report.json"));
        json!({
            "schema_version":1,"case_id":format!("case-{n}"),
            "source_sha256":report["source_sha256"],"suite_sha256":report["suite_sha256"],
            "scenario_sha256":report["cases"][n]["scenario_sha256"],
            "requested":report["requested"],
            "observed":{"settings":report["requested"],"invocation_id":format!("public-declaration-{n}")},
            "complete":true,"measurements":{
                "input_tokens":null,"output_tokens":null,"elapsed_ms":null,"tool_calls":null,"cost":null
            },"data":{"answer":42,"a/b":{"~":null}}
        })
    }
    fn add(&self, n: usize, observation: &Value) {
        let name = format!("observation-{n}.json");
        let path = self.observations.join(&name);
        write(&path, observation);
        let index_path = self.observations.join("observations.json");
        let mut index = read(&index_path);
        index["observations"].as_array_mut().unwrap().push(json!({
            "case_id":format!("case-{n}"),"file":name,"sha256":io::hash(&fs::read(path).unwrap())
        }));
        write(&index_path, &index);
    }
    fn run(&self, name: &str) -> io::Result<String> {
        replay(
            &self.input,
            &self.frozen.join("lock.json"),
            &self.lock_hash(),
            &self.observations,
            &self.temp.path().join(name),
        )
    }
    fn report(&self, name: &str) -> Value {
        read(&self.temp.path().join(name).join("report.json"))
    }
}

#[test]
fn deterministic_freeze_retains_exact_hashed_manifests_and_unknowns() {
    let pack = Pack::new();
    pack.freeze();
    let second = pack.temp.path().join("second");
    freeze(&pack.input, &second).unwrap();
    for name in [
        "lock.json",
        "lock.sha256",
        "report.json",
        "tuning.manifest.json",
        "held-out.manifest.json",
    ] {
        assert_eq!(
            fs::read(pack.frozen.join(name)).unwrap(),
            fs::read(second.join(name)).unwrap()
        );
    }
    let report = read(&pack.frozen.join("report.json"));
    assert_eq!(report["mode"], "PROTOCOL_ONLY");
    assert_eq!(report["status"], "UNRUN");
    assert_eq!(report["cases"].as_array().unwrap().len(), 4);
    for (split, filename) in [
        ("tuning", "tuning.manifest.json"),
        ("held_out", "held-out.manifest.json"),
    ] {
        assert_eq!(
            report[split]["sha256"],
            io::hash(&fs::read(pack.frozen.join(filename)).unwrap())
        );
    }
    assert_eq!(
        report["suite_sha256"],
        io::hash(&fs::read(pack.input.join("suite.json")).unwrap())
    );
    assert_eq!(
        report["source_sha256"],
        io::hash(&fs::read(pack.input.join("source.txt")).unwrap())
    );
    assert!(report["cases"][0]["measurements"]
        .as_object()
        .unwrap()
        .values()
        .all(Value::is_null));
}

#[test]
fn mixed_outcomes_keep_both_splits_and_actual_identity_and_evidence() {
    let pack = Pack::new();
    pack.freeze();
    pack.add(0, &pack.observation(0));
    let mut failed = pack.observation(1);
    failed["data"]["answer"] = json!(0);
    pack.add(1, &failed);
    let mut blocked = pack.observation(2);
    blocked["observed"]["settings"]["model"] = json!("actual-other-model");
    blocked["measurements"]["input_tokens"] = json!("9007199254740993");
    blocked["measurements"]["tool_calls"] = json!("0");
    blocked["measurements"]["cost"] = json!({"amount":"0.000000000000000001","currency":"EUR"});
    pack.add(2, &blocked);
    let summary = pack.run("result").unwrap();
    assert!(summary.contains("FAIL"));
    let report = pack.report("result");
    assert_eq!(report["status"], "FAIL");
    assert_eq!(report["mode"], "PROTOCOL_ONLY");
    for (n, status) in ["PASS", "FAIL", "BLOCKED", "UNRUN"].into_iter().enumerate() {
        assert_eq!(report["cases"][n]["status"], status);
    }
    assert_eq!(report["tuning"]["case_ids"], json!(["case-0", "case-1"]));
    assert_eq!(report["held_out"]["case_ids"], json!(["case-2", "case-3"]));
    assert_eq!(report["cases"][2]["observed"], blocked["observed"]);
    assert_eq!(report["cases"][2]["measurements"], blocked["measurements"]);
    assert_eq!(report["cases"][2]["assertions"], json!([]));
    for n in 0..2 {
        let sha = report["cases"][n]["assertions"][0]["evidence_sha256"]
            .as_str()
            .unwrap();
        let raw = fs::read(
            pack.temp
                .path()
                .join("result/evidence")
                .join(format!("{sha}.json")),
        )
        .unwrap();
        assert_eq!(io::hash(&raw), sha);
        assert_eq!(
            raw,
            fs::read(pack.observations.join(format!("observation-{n}.json"))).unwrap()
        );
    }
    domain::agent_evaluation::parse(&serde_json::to_vec(&report).unwrap()).unwrap();
    pack.run("result2").unwrap();
    assert_eq!(
        fs::read(pack.temp.path().join("result/report.json")).unwrap(),
        fs::read(pack.temp.path().join("result2/report.json")).unwrap()
    );
}

#[test]
fn absent_and_incomplete_observations_never_become_pass() {
    let pack = Pack::new();
    pack.freeze();
    pack.run("absent").unwrap();
    assert_eq!(pack.report("absent")["status"], "UNRUN");
    let mut observation = pack.observation(0);
    observation["observed"] = Value::Null;
    pack.add(0, &observation);
    let mut observation = pack.observation(1);
    observation["complete"] = json!(false);
    pack.add(1, &observation);
    let mut observation = pack.observation(2);
    observation["observed"]["settings"]["reasoning_effort"] = json!("low");
    pack.add(2, &observation);
    pack.run("blocked").unwrap();
    let report = pack.report("blocked");
    assert_eq!(report["status"], "BLOCKED");
    for n in 0..3 {
        assert_eq!(report["cases"][n]["status"], "BLOCKED");
    }
    assert_eq!(report["cases"][3]["status"], "UNRUN");
}

#[test]
fn missing_json_pointer_fails_while_present_null_exists() {
    let pack = Pack::new();
    pack.freeze();
    let mut observation = pack.observation(0);
    observation["data"] = json!({"a/b":{"~":null}});
    pack.add(0, &observation);
    pack.run("result").unwrap();
    let report = pack.report("result");
    assert_eq!(report["cases"][0]["status"], "FAIL");
    assert_eq!(report["cases"][0]["assertions"][0]["passed"], false);
    assert_eq!(report["cases"][0]["assertions"][1]["passed"], true);
}

#[test]
fn all_cases_must_pass_before_aggregate_pass() {
    let pack = Pack::new();
    pack.freeze();
    for n in 0..4 {
        pack.add(n, &pack.observation(n));
    }
    pack.run("result").unwrap();
    assert_eq!(pack.report("result")["status"], "PASS");
}

#[test]
fn exact_byte_mutations_fail_closed_without_creating_output() {
    for file in [
        "suite.json",
        "source.txt",
        "scenario-0.txt",
        "scenario-3.txt",
    ] {
        let pack = Pack::new();
        pack.freeze();
        let path = pack.input.join(file);
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b'\n');
        fs::write(path, bytes).unwrap();
        assert!(pack.run("result").is_err(), "{file}");
        assert!(!pack.temp.path().join("result").exists());
    }
    for file in ["tuning.manifest.json", "held-out.manifest.json"] {
        let pack = Pack::new();
        pack.freeze();
        let path = pack.frozen.join(file);
        let mut bytes = fs::read(&path).unwrap();
        bytes.push(b' ');
        fs::write(path, bytes).unwrap();
        assert!(pack.run("result").is_err(), "{file}");
    }
}

#[test]
fn external_lock_digest_and_canonical_lock_are_required() {
    let pack = Pack::new();
    pack.freeze();
    for sha in ["0".repeat(64), "A".repeat(64), "".into()] {
        assert!(replay(
            &pack.input,
            &pack.frozen.join("lock.json"),
            &sha,
            &pack.observations,
            &pack.temp.path().join("result")
        )
        .is_err());
    }
    let lock = pack.frozen.join("lock.json");
    let mut bytes = fs::read(&lock).unwrap();
    bytes.push(b' ');
    fs::write(&lock, bytes).unwrap();
    assert!(pack.run("result").is_err());
}

#[test]
fn rejects_swapped_missing_or_changed_observation_bytes() {
    for action in ["swap", "missing", "whitespace"] {
        let pack = Pack::new();
        pack.freeze();
        pack.add(0, &pack.observation(0));
        let path = pack.observations.join("observation-0.json");
        match action {
            "swap" => write(&path, &pack.observation(1)),
            "missing" => fs::remove_file(path).unwrap(),
            _ => {
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(b'\n');
                fs::write(path, bytes).unwrap();
            }
        }
        assert!(pack.run("result").is_err(), "{action}");
    }
}

#[test]
fn observation_cross_case_source_suite_scenario_and_requested_policy_are_bound() {
    for (pointer, replacement) in [
        ("/case_id", json!("case-1")),
        ("/source_sha256", json!("0".repeat(64))),
        ("/suite_sha256", json!("0".repeat(64))),
        ("/scenario_sha256", json!("0".repeat(64))),
        ("/requested/model", json!("other")),
        ("/requested/reasoning_effort", json!("low")),
    ] {
        let pack = Pack::new();
        pack.freeze();
        let mut observation = pack.observation(0);
        *observation.pointer_mut(pointer).unwrap() = replacement;
        pack.add(0, &observation);
        assert!(pack.run("result").is_err(), "{pointer}");
    }
}

#[test]
fn rejects_duplicate_or_unknown_index_entries_and_reused_invocation_identity() {
    for action in ["duplicate", "unknown", "lock", "file", "invocation"] {
        let pack = Pack::new();
        pack.freeze();
        pack.add(0, &pack.observation(0));
        let mut observation = pack.observation(1);
        if action == "invocation" {
            observation["observed"]["invocation_id"] = json!("public-declaration-0");
        }
        pack.add(1, &observation);
        let path = pack.observations.join("observations.json");
        let mut index = read(&path);
        match action {
            "duplicate" => index["observations"][1]["case_id"] = json!("case-0"),
            "unknown" => index["observations"][1]["case_id"] = json!("not-frozen"),
            "lock" => index["lock_sha256"] = json!("0".repeat(64)),
            "file" => index["observations"][1]["file"] = json!("observation-0.json"),
            _ => {}
        }
        write(&path, &index);
        assert!(pack.run("result").is_err(), "{action}");
    }
}

#[test]
fn suite_rejects_overlap_duplicate_ids_empty_assertions_bad_pointers_and_invalid_policy() {
    for (pointer, replacement) in [
        ("/requested/model", json!("")),
        ("/requested/reasoning_effort", json!(" ")),
        ("/requested/model", json!("x".repeat(201))),
        ("/held_out/id", json!("public-tuning")),
        ("/held_out/cases/0/id", json!("case-0")),
        ("/held_out/cases/0/scenario_file", json!("scenario-0.txt")),
        ("/tuning/cases/0/assertions", json!([])),
        ("/tuning/cases/0/assertions/1/id", json!("answer")),
        (
            "/tuning/cases/0/assertions/0/predicate/pointer",
            json!("answer"),
        ),
        (
            "/tuning/cases/0/assertions/0/predicate/pointer",
            json!("/bad~2"),
        ),
        (
            "/tuning/cases/0/assertions/0/predicate/pointer",
            json!("/bad~"),
        ),
        (
            "/tuning/cases/0/assertions/0/predicate/kind",
            json!("SCRIPT"),
        ),
        ("/held_out/cases", json!([])),
        ("/schema_version", json!(2)),
    ] {
        let mut pack = Pack::new();
        *pack.suite.pointer_mut(pointer).unwrap() = replacement;
        pack.save_suite();
        assert!(freeze(&pack.input, &pack.frozen).is_err(), "{pointer}");
    }
}

#[test]
fn rejects_unknown_fields_at_all_typed_boundaries() {
    for pointer in [
        "",
        "/runner",
        "/subject",
        "/requested",
        "/tuning",
        "/tuning/cases/0",
        "/tuning/cases/0/assertions/0",
        "/tuning/cases/0/assertions/0/predicate",
    ] {
        let mut pack = Pack::new();
        pack.suite.pointer_mut(pointer).unwrap()["extra"] = json!(true);
        pack.save_suite();
        assert!(freeze(&pack.input, &pack.frozen).is_err(), "{pointer}");
    }
    for pointer in [
        "",
        "/requested",
        "/observed",
        "/observed/settings",
        "/measurements",
    ] {
        let pack = Pack::new();
        pack.freeze();
        let mut observation = pack.observation(0);
        observation.pointer_mut(pointer).unwrap()["extra"] = json!(true);
        pack.add(0, &observation);
        assert!(pack.run("result").is_err(), "{pointer}");
    }
}

#[test]
fn strict_json_rejects_duplicate_keys_nested_duplicates_and_trailing_data() {
    for bytes in [
        br#"{"schema_version":1,"schema_version":1}"#.as_slice(),
        br#"{"data":{"a":1,"a":2}}"#,
        br#"{"a":[{"b":1,"b":2}]}"#,
        br#"{} {}"#,
    ] {
        assert!(io::parse::<Value>(bytes).is_err());
    }
    let pack = Pack::new();
    pack.freeze();
    let bytes = serde_json::to_string(&pack.observation(0))
        .unwrap()
        .replace("\"answer\":42", "\"answer\":42,\"answer\":42");
    fs::write(pack.observations.join("duplicated.json"), &bytes).unwrap();
    write(
        &pack.observations.join("observations.json"),
        &json!({"schema_version":1,"lock_sha256":pack.lock_hash(),"observations":[{"case_id":"case-0","file":"duplicated.json","sha256":io::hash(bytes.as_bytes())}]}),
    );
    assert!(pack.run("result").is_err());
}

#[test]
fn exact_metrics_and_incomplete_totals_are_checked_by_native_contract() {
    for (pointer, replacement) in [
        ("/measurements/input_tokens", json!(12)),
        ("/measurements/input_tokens", json!("-1")),
        (
            "/measurements/cost",
            json!({"amount":"-1","currency":"USD"}),
        ),
        ("/measurements/cost", json!({"amount":"1","currency":"ZZZ"})),
        ("/observed/invocation_id", json!("")),
    ] {
        let pack = Pack::new();
        pack.freeze();
        let mut observation = pack.observation(0);
        *observation.pointer_mut(pointer).unwrap() = replacement;
        pack.add(0, &observation);
        assert!(pack.run("result").is_err(), "{pointer}");
    }
    let pack = Pack::new();
    pack.freeze();
    let mut observation = pack.observation(0);
    observation["complete"] = json!(false);
    observation["measurements"]["tool_calls"] = json!("0");
    pack.add(0, &observation);
    assert!(pack.run("result").is_err());
}

#[test]
fn path_traversal_absolute_paths_and_ambiguous_components_are_rejected() {
    for path in [
        "../source.txt",
        "/etc/passwd",
        "sub/../source.txt",
        "./source.txt",
        "sub//file",
        "C:\\secret",
        "",
        "sub\\file",
    ] {
        let mut pack = Pack::new();
        pack.suite["source_file"] = json!(path);
        pack.save_suite();
        assert!(freeze(&pack.input, &pack.frozen).is_err(), "{path}");
    }
    let pack = Pack::new();
    pack.freeze();
    write(
        &pack.observations.join("observations.json"),
        &json!({"schema_version":1,"lock_sha256":pack.lock_hash(),"observations":[{"case_id":"case-0","file":"../input/source.txt","sha256":"0".repeat(64)}]}),
    );
    assert!(pack.run("result").is_err());
}

#[test]
fn rejects_oversized_json_scenario_source_and_aggregate_reads() {
    assert!(io::parse::<Value>(&vec![b' '; io::MAX_JSON + 1]).is_err());
    for (file, size) in [
        ("suite.json", io::MAX_JSON + 1),
        ("scenario-0.txt", 256 * 1024 + 1),
        ("source.txt", io::MAX_SOURCE + 1),
    ] {
        let pack = Pack::new();
        let file = fs::File::create(pack.input.join(file)).unwrap();
        file.set_len(size as u64).unwrap();
        assert!(freeze(&pack.input, &pack.frozen).is_err());
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("bounded");
    fs::write(&path, vec![b'x'; 1024 * 1024]).unwrap();
    let mut budget = io::Budget::default();
    for _ in 0..96 {
        budget.read(&path, io::MAX_JSON).unwrap();
    }
    assert!(budget.read(&path, io::MAX_JSON).is_err());
    let path = temp.path().join("small");
    fs::write(&path, b"x").unwrap();
    let mut budget = io::Budget::default();
    for _ in 0..1100 {
        budget.read(&path, 1).unwrap();
    }
    assert!(budget.read(&path, 1).is_err());
}

#[test]
fn output_collision_never_overwrites_existing_files() {
    let pack = Pack::new();
    pack.freeze();
    let before = fs::read(pack.frozen.join("report.json")).unwrap();
    assert!(freeze(&pack.input, &pack.frozen).is_err());
    assert_eq!(before, fs::read(pack.frozen.join("report.json")).unwrap());
    pack.run("result").unwrap();
    let before = fs::read(pack.temp.path().join("result/report.json")).unwrap();
    assert!(pack.run("result").is_err());
    assert_eq!(
        before,
        fs::read(pack.temp.path().join("result/report.json")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn symlink_files_roots_lock_and_output_parents_are_rejected() {
    use std::os::unix::fs::symlink;
    for file in ["source.txt", "scenario-0.txt", "suite.json"] {
        let pack = Pack::new();
        let original = pack.input.join(file);
        let moved = pack.temp.path().join("original");
        fs::rename(&original, &moved).unwrap();
        symlink(moved, original).unwrap();
        assert!(freeze(&pack.input, &pack.frozen).is_err());
    }
    let pack = Pack::new();
    let linked = pack.temp.path().join("linked");
    symlink(&pack.input, &linked).unwrap();
    assert!(freeze(&linked, &pack.frozen).is_err());
    assert!(freeze(&pack.input, &linked.join("output")).is_err());
    pack.freeze();
    let linked_lock = pack.temp.path().join("linked-lock.json");
    symlink(pack.frozen.join("lock.json"), &linked_lock).unwrap();
    assert!(replay(
        &pack.input,
        &linked_lock,
        &pack.lock_hash(),
        &pack.observations,
        &pack.temp.path().join("result")
    )
    .is_err());
}

#[cfg(unix)]
#[test]
fn output_permissions_are_private_on_unix() {
    use std::os::unix::fs::PermissionsExt;
    let pack = Pack::new();
    pack.freeze();
    assert_eq!(
        fs::metadata(&pack.frozen).unwrap().permissions().mode() & 0o077,
        0
    );
    for file in [
        "lock.json",
        "lock.sha256",
        "report.json",
        "tuning.manifest.json",
        "held-out.manifest.json",
    ] {
        assert_eq!(
            fs::metadata(pack.frozen.join(file))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
}

#[test]
fn strict_lock_and_index_contracts_reject_unknown_fields() {
    for pointer in ["", "/observations/0"] {
        let pack = Pack::new();
        pack.freeze();
        pack.add(0, &pack.observation(0));
        let path = pack.observations.join("observations.json");
        let mut index = read(&path);
        index.pointer_mut(pointer).unwrap()["extra"] = json!(true);
        write(&path, &index);
        assert!(pack.run("result").is_err());
    }
    let pack = Pack::new();
    pack.freeze();
    let path = pack.frozen.join("lock.json");
    let mut lock = read(&path);
    lock["extra"] = json!(true);
    write(&path, &lock);
    assert!(pack.run("result").is_err());
}

#[test]
fn bounded_case_and_assertion_work_is_enforced_before_reporting() {
    let mut pack = Pack::new();
    let case = pack.suite["tuning"]["cases"][0].clone();
    pack.suite["tuning"]["cases"] = json!(vec![case.clone(); 500]);
    pack.save_suite();
    assert!(freeze(&pack.input, &pack.frozen).is_err());

    let mut pack = Pack::new();
    let assertion = pack.suite["tuning"]["cases"][0]["assertions"][0].clone();
    pack.suite["tuning"]["cases"][0]["assertions"] = json!(vec![assertion.clone(); 101]);
    pack.save_suite();
    assert!(freeze(&pack.input, &pack.frozen).is_err());

    let mut pack = Pack::new();
    let mut cases = Vec::new();
    for n in 0..101 {
        let mut case = case.clone();
        case["id"] = json!(format!("many-{n}"));
        case["assertions"] = json!((0..100)
            .map(|n| {
                let mut assertion = assertion.clone();
                assertion["id"] = json!(format!("assertion-{n}"));
                assertion
            })
            .collect::<Vec<_>>());
        cases.push(case);
    }
    pack.suite["tuning"]["cases"] = json!(cases);
    pack.save_suite();
    assert_eq!(
        freeze(&pack.input, &pack.frozen).unwrap_err(),
        "assertion count limit"
    );
}

#[test]
fn floating_point_and_overflowing_json_cannot_collapse_into_false_matches() {
    for literal in [
        "1.0",
        "1e0",
        "0.10000000000000000001",
        "0.10000000000000000002",
        "18446744073709551616",
        "18446744073709551617",
        "-9223372036854775809",
    ] {
        let bytes = format!("{{\"nested\":[{{\"value\":{literal}}}]}}");
        assert!(io::parse::<Value>(bytes.as_bytes()).is_err(), "{literal}");
        let pack = Pack::new();
        let raw = serde_json::to_string(&pack.suite)
            .unwrap()
            .replace("\"value\":42", &format!("\"value\":{literal}"));
        fs::write(pack.input.join("suite.json"), raw).unwrap();
        assert!(
            freeze(&pack.input, &pack.frozen).is_err(),
            "predicate {literal}"
        );

        let pack = Pack::new();
        pack.freeze();
        let raw = serde_json::to_string(&pack.observation(0))
            .unwrap()
            .replace("\"answer\":42", &format!("\"answer\":{literal}"));
        fs::write(pack.observations.join("case.json"), &raw).unwrap();
        write(
            &pack.observations.join("observations.json"),
            &json!({"schema_version":1,"lock_sha256":pack.lock_hash(),"observations":[{"case_id":"case-0","file":"case.json","sha256":io::hash(raw.as_bytes())}]}),
        );
        assert!(pack.run("result").is_err(), "observation {literal}");
        assert!(!pack.temp.path().join("result").exists());
    }
    for literal in [
        "18446744073709551615",
        "-9223372036854775808",
        "9007199254740993",
    ] {
        let value: Value = io::parse(literal.as_bytes()).unwrap();
        assert_eq!(serde_json::to_string(&value).unwrap(), literal);
    }
    let first: Value = io::parse(b"9007199254740992").unwrap();
    let second: Value = io::parse(b"9007199254740993").unwrap();
    assert_ne!(first, second);
}

#[test]
fn generic_frozen_policy_is_bound_without_any_model_execution() {
    // Deliberately fictional settings demonstrate offline policy-agnostic binding,
    // not selection, discovery or execution of a different live subject.
    let mut pack = Pack::new();
    let requested = json!({"model":"synthetic-offline-policy-id","reasoning_effort":"synthetic-declared-effort"});
    pack.suite["requested"] = requested.clone();
    pack.save_suite();
    pack.freeze();
    for n in 0..4 {
        pack.add(n, &pack.observation(n));
    }
    pack.run("generic").unwrap();
    let report = pack.report("generic");
    assert_eq!(report["requested"], requested);
    assert_eq!(report["status"], "PASS");
    assert_eq!(report["mode"], "PROTOCOL_ONLY");

    let path = pack.observations.join("observation-0.json");
    let mut observation = read(&path);
    observation["observed"]["settings"]["model"] = json!("synthetic-mismatched-identity");
    write(&path, &observation);
    let index_path = pack.observations.join("observations.json");
    let mut index = read(&index_path);
    index["observations"][0]["sha256"] = json!(io::hash(&fs::read(&path).unwrap()));
    write(&index_path, &index);
    pack.run("mismatched").unwrap();
    let blocked = pack.report("mismatched");
    assert_eq!(blocked["status"], "BLOCKED");
    assert_eq!(blocked["cases"][0]["observed"], observation["observed"]);
    assert_eq!(blocked["cases"][0]["assertions"], json!([]));

    observation["requested"]["model"] = json!("synthetic-other-request");
    write(&path, &observation);
    index["observations"][0]["sha256"] = json!(io::hash(&fs::read(&path).unwrap()));
    write(&index_path, &index);
    assert_eq!(
        pack.run("wrong-request").unwrap_err(),
        "observation provenance mismatch"
    );
    assert!(!pack.temp.path().join("wrong-request").exists());

    pack.suite["requested"]["model"] = json!("synthetic-replaced-frozen-policy");
    pack.save_suite();
    assert_eq!(
        pack.run("changed-suite").unwrap_err(),
        "frozen input mismatch"
    );
}

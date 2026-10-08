//! Actual Wasmi traps and limits; WAT is a dev-only native fixture compiler.
//! This does not replace host-process/compiler filesystem isolation acceptance.
use job::signals::{SignalModule, WasmSignal};

const FEATURES: [f64; 8] = [110.0, 100.0, 105.0, 102.0, 25.0, 100.0, 112.0, 98.0];
fn module(prefix: &str, body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        "(module {prefix} (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) {body}))"
    )).unwrap()
}
fn signal(prefix: &str, body: &str) -> WasmSignal {
    WasmSignal::new(&module(prefix, body), 100, 1_000_000).unwrap()
}

#[test]
fn native_prediction_uses_only_explicit_features_and_debits_fuel() {
    let mut vm = signal("", "local.get 0 local.get 1 f64.div f64.const 1 f64.sub");
    let before = vm.remaining_fuel();
    let first = vm.predict(FEATURES).unwrap();
    assert!((first - 0.1).abs() < 1e-12);
    assert!(vm.remaining_fuel() < before);
    let mut changed = FEATURES;
    changed[0] = 120.0;
    assert!((vm.predict(changed).unwrap() - 0.2).abs() < 1e-12);
}

#[test]
fn modules_cannot_import_host_io_environment_network_or_clocks() {
    for (namespace, name) in [
        ("wasi_snapshot_preview1", "fd_read"),
        ("wasi_snapshot_preview1", "environ_get"),
        ("env", "read_file"),
        ("env", "http_get"),
        ("env", "clock"),
    ] {
        let bytes = module(
            &format!("(import \"{namespace}\" \"{name}\" (func))"),
            "f64.const 1",
        );
        assert!(WasmSignal::new(&bytes, 1, 100_000).is_err());
    }
}

#[test]
fn start_functions_and_wrong_abis_are_rejected_before_prediction() {
    let bytes = module("(func $start) (start $start)", "f64.const 1");
    assert!(WasmSignal::new(&bytes, 1, 100_000).is_err());
    let bytes =
        wat::parse_str("(module (func (export \"predict\") (result f64) f64.const 1))").unwrap();
    assert!(WasmSignal::new(&bytes, 1, 100_000).is_err());
    let bytes =
        wat::parse_str("(module (func (export \"different\") (result f64) f64.const 1))").unwrap();
    assert!(WasmSignal::new(&bytes, 1, 100_000).is_err());
}

#[test]
fn fuel_exhaustion_stops_infinite_code_and_permanently_invalidates_the_instance() {
    let mut vm = signal("", "(loop $again (br $again)) f64.const 0");
    assert!(vm.predict(FEATURES).is_err());
    assert!(vm.predict(FEATURES).is_err());
}

#[test]
fn prediction_count_is_a_hard_limit_not_a_resettable_hint() {
    let bytes = module("", "f64.const 0.25");
    let mut vm = WasmSignal::new(&bytes, 2, 100_000).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 0.25);
    assert_eq!(vm.predict(FEATURES).unwrap(), 0.25);
    assert!(vm.predict(FEATURES).is_err());
    assert!(vm.predict(FEATURES).is_err());
}

#[test]
fn total_fuel_is_not_refilled_between_successful_predictions() {
    let bytes = module("", "local.get 0 local.get 1 f64.div");
    let mut vm = WasmSignal::new(&bytes, 1_000_000, 100).unwrap();
    let mut completed = 0;
    while vm.predict(FEATURES).is_ok() {
        completed += 1;
        assert!(completed < 1000, "native fuel must be consumed");
    }
    assert!(completed < 1000);
    assert!(vm.predict(FEATURES).is_err());
}

#[test]
fn memory_allocation_and_growth_can_exceed_old_product_caps() {
    let bytes = module("(memory 257)", "memory.size f64.convert_i32_u");
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 257.0);
    let bytes = module(
        "(memory 1)",
        "i32.const 512 memory.grow drop memory.size f64.convert_i32_u",
    );
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 513.0);
}

#[test]
fn explicit_fuel_stops_recursive_code() {
    let mut vm = signal(
        "(func $recurse (result f64) call $recurse)",
        "call $recurse",
    );
    assert!(vm.predict(FEATURES).is_err());
}

#[test]
fn nonfinite_inputs_and_results_never_become_zero_or_null_signals() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut vm = signal("", "f64.const 1");
        let mut features = FEATURES;
        features[3] = invalid;
        assert!(vm.predict(features).is_err());
        assert!(vm.predict(FEATURES).is_err());
    }
    for body in ["f64.const nan", "f64.const inf", "f64.const -inf"] {
        let mut vm = signal("", body);
        assert!(vm.predict(FEATURES).is_err());
    }
}

#[test]
fn independent_instruments_and_folds_do_not_share_mutable_model_memory() {
    let prefix = "(global $counter (mut f64) (f64.const 0))";
    let body = "global.get $counter f64.const 1 f64.add global.set $counter global.get $counter";
    let compiled = SignalModule::new(&module(prefix, body), true).unwrap();
    let mut first = compiled.instantiate(100, 1_000_000).unwrap();
    let mut second = compiled.instantiate(100, 1_000_000).unwrap();
    assert_eq!(first.predict(FEATURES).unwrap(), 1.0);
    assert_eq!(first.predict(FEATURES).unwrap(), 2.0);
    assert_eq!(second.predict(FEATURES).unwrap(), 1.0);
}

#[test]
fn invalid_binary_and_explicit_zero_budgets_are_rejected() {
    for bytes in [
        Vec::new(),
        b"(module)".to_vec(),
        vec![0; 2 * 1024 * 1024 + 1],
    ] {
        assert!(WasmSignal::new(&bytes, 1, 100).is_err());
    }
    let bytes = module("", "f64.const 1");
    for (calls, fuel) in [(0, 100), (1, 0)] {
        assert!(WasmSignal::new(&bytes, calls, fuel).is_err());
    }
}

#[test]
fn compiled_module_warmth_never_changes_prediction_or_fuel_limits() {
    let bytes = module("", "local.get 0 local.get 1 f64.div f64.const 1 f64.sub");
    let compiled = SignalModule::new(&bytes, true).unwrap();
    for budget in [1, 64, 100_000] {
        // Exercise both first use and already translated code, including exhaustion.
        for instance in 0..3 {
            let mut reused = compiled.instantiate(32, budget).unwrap();
            let mut fresh = WasmSignal::new(&bytes, 32, budget).unwrap();
            for ordinal in 0..33 {
                let mut features = FEATURES;
                features[0] += f64::from(ordinal);
                let expected = fresh.predict(features);
                let actual = reused.predict(features);
                assert_eq!(
                    actual.is_ok(),
                    expected.is_ok(),
                    "instance={instance} budget={budget} ordinal={ordinal}"
                );
                assert_eq!(
                    reused.remaining_fuel(),
                    fresh.remaining_fuel(),
                    "instance={instance} budget={budget} ordinal={ordinal}"
                );
                match (actual, expected) {
                    (Ok(actual), Ok(expected)) => assert_eq!(actual.to_bits(), expected.to_bits()),
                    (Err(_), Err(_)) => {
                        assert!(reused.predict(FEATURES).is_err());
                        assert!(fresh.predict(FEATURES).is_err());
                        break;
                    }
                    _ => unreachable!("success status checked above"),
                }
            }
        }
    }
}

#[test]
fn reused_code_preserves_linear_memory_fuel_results_and_failure_isolation() {
    let bytes = module(
        "(memory 1)",
        "i32.const 0 i32.const 0 f64.load f64.const 1 f64.add f64.store i32.const 0 f64.load",
    );
    let compiled = SignalModule::new(&bytes, true).unwrap();
    let mut first = compiled.instantiate(3, 100_000).unwrap();
    let mut second = compiled.instantiate(3, 100_000).unwrap();
    let mut fresh = WasmSignal::new(&bytes, 3, 100_000).unwrap();
    for expected in [1.0, 2.0, 3.0] {
        assert_eq!(first.predict(FEATURES).unwrap(), expected);
        assert_eq!(fresh.predict(FEATURES).unwrap(), expected);
        assert_eq!(first.remaining_fuel(), fresh.remaining_fuel());
    }
    assert!(first.predict(FEATURES).is_err());
    assert_eq!(second.predict(FEATURES).unwrap(), 1.0);
    for (calls, fuel) in [(0, 100), (1, 0)] {
        assert!(compiled.instantiate(calls, fuel).is_err());
    }
}

#[test]
fn no_budget_means_no_metering_or_prediction_counter() {
    let bytes = module("", "f64.const 0.25");
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    for _ in 0..1_000_001 {
        assert_eq!(vm.predict(FEATURES).unwrap(), 0.25);
    }
    assert_eq!(vm.remaining_fuel(), None);
    let mut explicit = WasmSignal::new(&bytes, 1_000_001, 1_000_000_001).unwrap();
    assert_eq!(explicit.predict(FEATURES).unwrap(), 0.25);
    assert!(explicit.remaining_fuel().unwrap() < 1_000_000_001);
}

#[test]
fn one_prediction_can_consume_more_than_the_old_per_call_cap() {
    let body = "(local $n i32) i32.const 100000 local.set $n
        (loop $again local.get $n i32.const 1 i32.sub local.tee $n br_if $again)
        f64.const 0.5";
    let bytes = module("", body);
    let mut vm = WasmSignal::new(&bytes, 1, 1_000_000).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 0.5);
    assert!(1_000_000 - vm.remaining_fuel().unwrap() > 100_000);
    let mut unmetered = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(unmetered.predict(FEATURES).unwrap(), 0.5);
    assert_eq!(unmetered.remaining_fuel(), None);
}

#[test]
fn module_tables_memories_and_recursion_can_exceed_old_product_caps() {
    let padding = "x".repeat(2 * 1024 * 1024 + 1);
    let bytes = module(
        &format!("(memory 33) (data (i32.const 0) \"{padding}\")"),
        "f64.const 1",
    );
    assert!(bytes.len() > 2 * 1024 * 1024);
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 1.0);
    let bytes = module(
        "(table 4097 funcref) (table 1 funcref) (memory 1) (memory 1)",
        "f64.const 1",
    );
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 1.0);
    let bytes = module(
        "(func $recurse (param $n i32) (result i32)
            local.get $n i32.eqz if (result i32) i32.const 0 else
            local.get $n i32.const 1 i32.sub call $recurse i32.const 1 i32.add end)",
        "i32.const 1100 call $recurse f64.convert_i32_u",
    );
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 1100.0);
}

#[test]
fn optional_budget_modes_cannot_be_mismatched_or_silently_reset() {
    let bytes = module("", "f64.const 1");
    let metered = SignalModule::new(&bytes, true).unwrap();
    assert!(metered.instantiate(None, None).is_err());
    let unmetered = SignalModule::new(&bytes, false).unwrap();
    assert!(unmetered.instantiate(None, Some(100)).is_err());
    let mut bounded_count = unmetered.instantiate(Some(1), None).unwrap();
    assert_eq!(bounded_count.predict(FEATURES).unwrap(), 1.0);
    assert!(bounded_count.predict(FEATURES).is_err());
}

#[test]
#[ignore = "subprocess fixture for unmetered host cancellation"]
fn unmetered_loop_child() {
    use std::io::Write;
    assert!(std::env::var_os("QZ_UNMETERED_CHILD").is_some());
    let bytes = module("", "(loop $again br $again) f64.const 1");
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.remaining_fuel(), None);
    println!("UNMETERED_CHILD_READY");
    std::io::stdout().flush().unwrap();
    vm.predict(FEATURES).unwrap();
    panic!("unmetered loop unexpectedly finished");
}

#[test]
fn unmetered_execution_remains_terminable_by_its_owning_host_process() {
    use std::{
        io::{BufRead, BufReader},
        process::{Command, Stdio},
        time::Duration,
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "unmetered_loop_child",
            "--ignored",
            "--nocapture",
        ])
        .env("QZ_UNMETERED_CHILD", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let ready = stdout
        .lines()
        .any(|line| line.unwrap().contains("UNMETERED_CHILD_READY"));
    if !ready {
        let status = child.wait().unwrap();
        panic!("child failed before the cancellation check: {status}");
    }
    std::thread::sleep(Duration::from_millis(20));
    assert!(child.try_wait().unwrap().is_none());
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
}

#[test]
fn value_stack_can_exceed_both_old_product_and_native_default_quotas() {
    let locals = "f64 ".repeat(128);
    let initialize = (1..=128)
        .map(|i| format!("local.get 0 f64.convert_i32_u local.set {i} "))
        .collect::<String>();
    let sum = (1..=128)
        .map(|i| format!("local.get {i} f64.add "))
        .collect::<String>();
    // Each recursive frame retains 128 live f64 values across the recursive
    // call, exceeding 1 MiB in total as well as QZ's former 64 KiB quota.
    let prefix = format!(
        "(func $recurse (param i32) (result f64) (local {locals})
        {initialize} local.get 0 i32.eqz if (result f64) f64.const 0 else
        local.get 0 i32.const 1 i32.sub call $recurse {sum} end)"
    );
    let bytes = module(&prefix, "i32.const 1100 call $recurse");
    let mut vm = WasmSignal::new(&bytes, None, None).unwrap();
    assert_eq!(vm.predict(FEATURES).unwrap(), 128.0 * 1100.0 * 1101.0 / 2.0);
}

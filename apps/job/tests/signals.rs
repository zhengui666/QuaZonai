//! Actual Wasmi traps and limits; WAT is a dev-only native fixture compiler.
//! This does not replace host-process/compiler filesystem isolation acceptance.
use job::signals::{WasmSignal, MAX_SIGNAL_FUEL, MAX_SIGNAL_MODULE_BYTES};

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
fn native_memory_limits_cover_both_instantiation_and_growth() {
    let oversized = module("(memory 257)", "f64.const 1");
    assert!(WasmSignal::new(&oversized, 1, 100_000).is_err());
    let mut vm = signal("(memory 1)", "i32.const 512 memory.grow drop f64.const 1");
    assert!(vm.predict(FEATURES).is_err());
    assert!(vm.predict(FEATURES).is_err());
}

#[test]
fn native_recursion_limit_traps_without_growing_an_unbounded_host_stack() {
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
    let mut first = signal(prefix, body);
    let mut second = signal(prefix, body);
    assert_eq!(first.predict(FEATURES).unwrap(), 1.0);
    assert_eq!(first.predict(FEATURES).unwrap(), 2.0);
    assert_eq!(second.predict(FEATURES).unwrap(), 1.0);
}

#[test]
fn byte_and_budget_limits_reject_untrusted_inputs_before_compilation() {
    for bytes in [
        Vec::new(),
        b"(module)".to_vec(),
        vec![0; MAX_SIGNAL_MODULE_BYTES + 1],
    ] {
        assert!(WasmSignal::new(&bytes, 1, 100).is_err());
    }
    let bytes = module("", "f64.const 1");
    for (calls, fuel) in [(0, 100), (1_000_001, 100), (1, 0), (1, MAX_SIGNAL_FUEL + 1)] {
        assert!(WasmSignal::new(&bytes, calls, fuel).is_err());
    }
}

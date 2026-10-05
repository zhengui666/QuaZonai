//! Actual v2 Wasmi ABI and metering. Native process tests cover callback failure.
use contracts::{science::*, DbCounter};
use job::{
    feature_model::FeatureModel,
    signals::{SignalModule, SIGNAL_FUEL_PER_PREDICTION},
};
use wasmi::{CompilationMode, Config, Engine, Linker, Module, Store, TrapCode};

fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
fn input() -> FeatureValueV1 {
    FeatureValueV1 {
        value: Some(0.0),
        missing_reason: None,
        event_ns: Some(count(7)),
        observed_available_ns: Some(count(8)),
        effective_available_ns: Some(count(8)),
        sequence: Some(count(1)),
    }
}
fn module_bytes(prefix: &str, set: &str, predict: &str) -> Vec<u8> {
    wat::parse_str(format!(
        "(module {prefix}
        (func (export \"qz_set_feature_v2\") (param i32 f64 i32 i64 i64) {set})
        (func (export \"qz_predict_v2\") (param i64 i64 i32 i32 i32) (result f64) {predict}))"
    ))
    .unwrap()
}
fn module(prefix: &str, set: &str, predict: &str) -> SignalModule {
    SignalModule::new(&module_bytes(prefix, set, predict)).unwrap()
}

#[test]
fn version_two_exposes_frozen_identity_and_source_freshness() {
    let module = module(
        "",
        "local.get 0 i32.const 0 i32.ne if unreachable end
        local.get 1 f64.const 0 f64.ne if unreachable end
        local.get 2 i32.const 0 i32.ne if unreachable end
        local.get 3 i64.const 7 i64.ne if unreachable end
        local.get 4 i64.const 8 i64.ne if unreachable end",
        "
        local.get 0 i64.const 10 i64.ne if unreachable end
        local.get 1 i64.const 9 i64.ne if unreachable end
        local.get 2 i32.const 0 i32.ne if unreachable end
        local.get 3 i32.const 4 i32.ne if unreachable end
        local.get 4 i32.const 1 i32.ne if unreachable end f64.const 0.25",
    );
    let mut model = FeatureModel::new(&module, 1, 100_000).unwrap();
    assert_eq!(model.predict(10, 9, 4, &[input()]).unwrap(), 0.25);
    assert!(model.remaining_fuel() < 100_000);
    assert!(model.predict(10, 9, 4, &[input()]).is_err());
}

#[test]
fn all_missing_masks_remain_distinct_from_observed_zero() {
    let module = module(
        "(global $mask (mut i32) (i32.const 0))",
        "local.get 2 global.set $mask",
        "global.get $mask f64.convert_i32_s",
    );
    let mut model = FeatureModel::new(&module, 4, 100_000).unwrap();
    assert_eq!(model.predict(10, 9, 0, &[input()]).unwrap(), 0.0);
    for (expected, missing) in [
        (1, FeatureMissingReasonV1::NotYetAvailable),
        (2, FeatureMissingReasonV1::Expired),
        (3, FeatureMissingReasonV1::SourceMissing),
    ] {
        let mut value = input();
        value.value = None;
        value.missing_reason = Some(missing);
        if missing == FeatureMissingReasonV1::NotYetAvailable {
            value.event_ns = None;
            value.observed_available_ns = None;
            value.effective_available_ns = None;
            value.sequence = None;
        }
        assert_eq!(
            model.predict(10, 9, 0, &[value]).unwrap(),
            f64::from(expected)
        );
    }
}

#[test]
fn setter_work_is_metered_and_a_failed_instance_cannot_resume() {
    let bytes = module_bytes("", "(loop $again br $again)", "f64.const 0.25");
    // Wasmi debits whole instruction blocks. OutOfFuel may leave a remainder
    // smaller than the next block's cost, so it does not promise a zero balance.
    let mut config = Config::default();
    config
        .compilation_mode(CompilationMode::Eager)
        .consume_fuel(true);
    let engine = Engine::new(&config);
    let native_module = Module::new(&engine, &bytes).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Linker::new(&engine)
        .instantiate_and_start(&mut store, &native_module)
        .unwrap();
    let setter = instance
        .get_typed_func::<(i32, f64, i32, i64, i64), ()>(&store, "qz_set_feature_v2")
        .unwrap();
    store.set_fuel(SIGNAL_FUEL_PER_PREDICTION).unwrap();
    let error = setter.call(&mut store, (0, 0.0, 0, 7, 8)).unwrap_err();
    assert_eq!(error.as_trap_code(), Some(TrapCode::OutOfFuel));
    let consumed = SIGNAL_FUEL_PER_PREDICTION - store.get_fuel().unwrap();
    assert!(consumed > 0);

    let module = SignalModule::new(&bytes).unwrap();
    let mut model = FeatureModel::new(&module, 10, 1_000_000).unwrap();
    let before = model.remaining_fuel();
    assert_eq!(
        model.predict(10, 9, 0, &[input()]).unwrap_err().to_string(),
        "FEATURE_MODEL_EXECUTION_FAILED"
    );
    assert_eq!(model.remaining_fuel(), before - consumed);
    let after_failure = model.remaining_fuel();
    assert_eq!(
        model.predict(10, 9, 0, &[input()]).unwrap_err().to_string(),
        "FEATURE_MODEL_INSTANCE_FAILED"
    );
    assert_eq!(model.remaining_fuel(), after_failure);
}

#[test]
fn fresh_v2_instances_do_not_share_globals_or_memory() {
    let module = module(
        "(global $n (mut f64) (f64.const 0))",
        "",
        "global.get $n f64.const 0.25 f64.add global.set $n global.get $n",
    );
    let mut first = FeatureModel::new(&module, 10, 100_000).unwrap();
    let mut second = FeatureModel::new(&module, 10, 100_000).unwrap();
    assert_eq!(first.predict(10, 9, 0, &[input()]).unwrap(), 0.25);
    assert_eq!(first.predict(10, 9, 1, &[input()]).unwrap(), 0.5);
    assert_eq!(second.predict(10, 9, 0, &[input()]).unwrap(), 0.25);
}

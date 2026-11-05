use super::*;

fn stateful_module(trap: bool) -> Vec<u8> {
    let trap = if trap {
        "global.get $n i32.const 2 i32.eq if unreachable end"
    } else {
        ""
    };
    wat::parse_str(format!(
        r#"(module (global $n (mut i32) (i32.const 0))
        (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64)
        global.get $n i32.const 1 i32.add global.set $n
        {trap} global.get $n f64.convert_i32_s))"#
    ))
    .unwrap()
}

#[test]
fn cached_code_preserves_fresh_state_metering_and_trap_isolation() {
    let bytes = stateful_module(true);
    let mut cache = SignalModuleCache::default();
    let compiled = cache.get_or_compile(&bytes, true).unwrap();
    let reused = cache.get_or_compile(&bytes, true).unwrap();
    assert!(Rc::ptr_eq(&compiled, &reused));
    let mut first = compiled.instantiate(3, 10_000).unwrap();
    assert_eq!(first.predict([1.0; 8]).unwrap(), 1.0);
    assert!(first.predict([1.0; 8]).is_err());
    assert_eq!(
        first.predict([1.0; 8]).unwrap_err().to_string(),
        "SIGNAL_INSTANCE_FAILED"
    );
    let mut next = reused.instantiate(3, 10_000).unwrap();
    let mut cold = WasmSignal::new(&bytes, 3, 10_000).unwrap();
    assert_eq!(
        next.predict([1.0; 8]).unwrap(),
        cold.predict([1.0; 8]).unwrap()
    );
    assert_eq!(next.remaining_fuel(), cold.remaining_fuel());
    assert_eq!(
        next.predict([1.0; 8]).unwrap_err().to_string(),
        cold.predict([1.0; 8]).unwrap_err().to_string()
    );
    assert_eq!(next.remaining_fuel(), cold.remaining_fuel());

    let unmetered = cache.get_or_compile(&bytes, false).unwrap();
    assert!(!Rc::ptr_eq(&compiled, &unmetered));
    let mut model = unmetered.instantiate(3, None).unwrap();
    assert_eq!(model.predict([1.0; 8]).unwrap(), 1.0);
    assert_eq!(model.remaining_fuel(), None);
    assert!(compiled.instantiate(3, None).is_err());
}

#[test]
fn cache_retention_limits_do_not_thrash_repeated_scans_or_restrict_execution() {
    let bytes = stateful_module(false);
    let mut cache = SignalModuleCache {
        max_entries: 1,
        max_source_bytes: bytes.len(),
        ..SignalModuleCache::default()
    };
    let first = cache.get_or_compile(&bytes, true).unwrap();
    let weak = Rc::downgrade(&first);
    drop(first);
    cache.get_or_compile(&bytes, false).unwrap();
    assert!(Rc::ptr_eq(
        &weak.upgrade().unwrap(),
        &cache.get_or_compile(&bytes, true).unwrap()
    ));
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.source_bytes, bytes.len());
    drop(cache);
    assert!(weak.upgrade().is_none(), "task end drops the owning Engine");

    let mut cache = SignalModuleCache {
        max_source_bytes: bytes.len() - 1,
        ..SignalModuleCache::default()
    };
    let module = cache.get_or_compile(&bytes, true).unwrap();
    let mut model = module.instantiate(2, 10_000).unwrap();
    assert_eq!(model.predict([1.0; 8]).unwrap(), 1.0);
    assert_eq!(model.predict([1.0; 8]).unwrap(), 2.0);
    assert!(
        cache.entries.is_empty(),
        "oversized code is executed uncached"
    );
}

#[test]
fn invalid_modules_are_not_cached_or_replaced_with_previous_code() {
    let bytes = stateful_module(false);
    let mut cache = SignalModuleCache::default();
    let original = cache.get_or_compile(&bytes, true).unwrap();
    assert!(cache.get_or_compile(b"invalid", true).is_err());
    assert_eq!(cache.entries.len(), 1);
    assert!(Rc::ptr_eq(
        &original,
        &cache.get_or_compile(&bytes, true).unwrap()
    ));
}

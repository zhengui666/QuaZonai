//! Synthetic kernel measurement; no market or end-to-end performance claim.
use anyhow::{ensure, Result};
#[path = "../../../../../../../apps/job/src/signals.rs"]
mod signals;
use signals::{SignalModule, SignalModuleCache};
use std::{
    hint::black_box,
    rc::{Rc, Weak},
    time::Instant,
};

struct Case {
    name: &'static str,
    models: usize,
    cutoffs: usize,
    assets: usize,
    predictions: u64,
    metered: bool,
}
struct Sample {
    nanos: u128,
    checksum: f64,
    fuel: Option<u64>,
    calls: usize,
    reused: usize,
}

fn run(case: &Case, modules: &[Vec<u8>], cached: bool) -> Result<Sample> {
    let begin = Instant::now();
    let mut cache = SignalModuleCache::default();
    let mut previous: Vec<Weak<SignalModule>> = (0..case.models).map(|_| Weak::new()).collect();
    let mut remaining = vec![case.metered.then_some(10_000_000_u64); case.models];
    let mut checksum = 0.0;
    let mut reused = 0;
    let mut calls = 0;
    for _ in 0..case.cutoffs {
        for (i, bytes) in modules.iter().enumerate() {
            let module = if cached {
                cache.get_or_compile(black_box(bytes), case.metered)?
            } else {
                Rc::new(SignalModule::new(black_box(bytes), case.metered)?)
            };
            calls += 1;
            if previous[i]
                .upgrade()
                .is_some_and(|prior| Rc::ptr_eq(&prior, &module))
            {
                reused += 1;
            }
            previous[i] = Rc::downgrade(&module);
            for _ in 0..case.assets {
                let mut model = module.instantiate(case.predictions, remaining[i])?;
                for _ in 0..case.predictions {
                    checksum += black_box(
                        model.predict(black_box([110., 100., 105., 102., 25., 100., 112., 98.]))?,
                    );
                }
                remaining[i] = model.remaining_fuel();
            }
        }
    }
    let fuel = case
        .metered
        .then(|| remaining.iter().map(|r| 10_000_000 - r.unwrap()).sum());
    // Task teardown belongs to the measurement: compiled entries and Engines
    // retained by the cached arm must be released before its timer stops.
    drop(cache);
    Ok(Sample {
        nanos: begin.elapsed().as_nanos(),
        checksum,
        fuel,
        calls,
        reused,
    })
}

fn validate(case: &Case, modules: &[Vec<u8>]) -> Result<()> {
    // Outside the timer, compare every prediction bit and every remaining-fuel
    // value, including the state reset at each asset/cutoff boundary.
    let mut cache = SignalModuleCache::default();
    let mut remaining = vec![case.metered.then_some(10_000_000_u64); case.models];
    for _ in 0..case.cutoffs {
        for (index, bytes) in modules.iter().enumerate() {
            let fresh = SignalModule::new(bytes, case.metered)?;
            let cached = cache.get_or_compile(bytes, case.metered)?;
            for _ in 0..case.assets {
                let mut fresh = fresh.instantiate(case.predictions, remaining[index])?;
                let mut cached = cached.instantiate(case.predictions, remaining[index])?;
                for prediction in 0..case.predictions {
                    let features = [110., 100., 105., 102., 25., 100., 112., 98.];
                    let expected = ((prediction + 1) as f64 + index as f64) + (110.0 / 100.0);
                    let observed = fresh.predict(features)?;
                    ensure!(observed.to_bits() == cached.predict(features)?.to_bits());
                    ensure!(observed.to_bits() == expected.to_bits());
                    ensure!(fresh.remaining_fuel() == cached.remaining_fuel());
                }
                remaining[index] = fresh.remaining_fuel();
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    ensure!(
        !cfg!(debug_assertions),
        "benchmark requires release profile"
    );
    let cases = [
        Case {
            name: "short_metered",
            models: 8,
            cutoffs: 16,
            assets: 2,
            predictions: 16,
            metered: true,
        },
        Case {
            name: "long_metered",
            models: 8,
            cutoffs: 2,
            assets: 2,
            predictions: 4096,
            metered: true,
        },
        Case {
            name: "saturated_metered",
            models: 64,
            cutoffs: 4,
            assets: 2,
            predictions: 16,
            metered: true,
        },
        Case {
            name: "short_unmetered",
            models: 8,
            cutoffs: 16,
            assets: 2,
            predictions: 16,
            metered: false,
        },
    ];
    println!("case,round,models,cutoffs,assets,predictions_per_instance,metered,fresh_ns,cached_ns,checksum,fuel,module_calls,reused_modules");
    for case in cases {
        let modules = (0..case.models)
            .map(|index| {
                wat::parse_str(format!(
                    r#"(module (global $n (mut i32) (i32.const 0))
            (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64)
            global.get $n i32.const 1 i32.add global.set $n
            global.get $n f64.convert_i32_s f64.const {index} f64.add
            local.get 0 local.get 1 f64.div f64.add))"#
                ))
            })
            .collect::<Result<Vec<_>, _>>()?;
        validate(&case, &modules)?;
        for _ in 0..5 {
            run(&case, &modules, false)?;
            run(&case, &modules, true)?;
        }
        for round in 0..30 {
            let (fresh, cached) = if round % 2 == 0 {
                (run(&case, &modules, false)?, run(&case, &modules, true)?)
            } else {
                let cached = run(&case, &modules, true)?;
                (run(&case, &modules, false)?, cached)
            };
            ensure!(
                fresh.checksum == cached.checksum && fresh.fuel == cached.fuel,
                "state/fuel changed"
            );
            ensure!(fresh.reused == 0, "fresh arm reused modules");
            ensure!(
                cached.reused == case.models.min(32) * (case.cutoffs - 1),
                "compiled reuse count changed"
            );
            println!(
                "{},{round},{},{},{},{},{},{},{},{},{},{},{}",
                case.name,
                case.models,
                case.cutoffs,
                case.assets,
                case.predictions,
                case.metered,
                fresh.nanos,
                cached.nanos,
                fresh.checksum,
                fresh
                    .fuel
                    .map_or_else(|| "unmetered".into(), |v| v.to_string()),
                cached.calls,
                cached.reused
            );
        }
    }
    Ok(())
}

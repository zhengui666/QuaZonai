//! Manual microbenchmark; timings are observations, never pass/fail thresholds.
use anyhow::{ensure, Result};
use job::signals::{SignalModule, WasmSignal};
use std::{hint::black_box, time::Instant};

fn main() -> Result<()> {
    let bytes = wat::parse_str(
        r#"(module
        (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64)
        local.get 0 local.get 1 f64.div f64.const 1 f64.sub))"#,
    )?;
    let iterations = 256;
    let run = |reuse: bool| -> Result<(std::time::Duration, f64, u64)> {
        let started = Instant::now();
        let compiled = reuse
            .then(|| SignalModule::new(black_box(&bytes)))
            .transpose()?;
        let mut checksum = 0.0;
        let mut fuel = 0;
        for _ in 0..iterations {
            let mut model = match &compiled {
                Some(module) => module.instantiate(16, 100_000)?,
                None => WasmSignal::new(black_box(&bytes), 16, 100_000)?,
            };
            for _ in 0..16 {
                checksum += black_box(model.predict(black_box([
                    110.0, 100.0, 105.0, 102.0, 25.0, 100.0, 112.0, 98.0,
                ]))?);
            }
            fuel += 100_000 - model.remaining_fuel();
        }
        Ok((started.elapsed(), checksum, fuel))
    };
    run(false)?;
    run(true)?;
    println!(
        "profile={} instances={iterations} predictions_per_instance=16",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    for round in 0..5 {
        let (fresh, reused) = if round % 2 == 0 {
            (run(false)?, run(true)?)
        } else {
            let reused = run(true)?;
            (run(false)?, reused)
        };
        ensure!(
            fresh.1 == reused.1 && fresh.2 == reused.2,
            "reuse changed results or fuel: fresh_checksum={} reused_checksum={} fresh_fuel={} reused_fuel={}",
            fresh.1, reused.1, fresh.2, reused.2
        );
        println!(
            "round={round} compile_each_us={} reuse_us={} checksum={} fuel={}",
            fresh.0.as_micros(),
            reused.0.as_micros(),
            fresh.1,
            fresh.2
        );
    }
    Ok(())
}

use anyhow::{ensure,Result};
#[path = "../../../../../../apps/job/src/signals.rs"]
mod signals;
use signals::{SignalModule,WasmSignal};
use std::{hint::black_box,time::Instant};
fn main() -> Result<()> {
let bytes=wat::parse_str(r#"(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) local.get 0 local.get 1 f64.div f64.const 1 f64.sub))"#)?;
println!("case,round,instances,predictions_per_instance,fresh_ns,reuse_ns,checksum,fuel");
for (case,instances,predictions) in [("small",256u32,16u32),("long",32,4096)] {
let run=|reuse:bool| -> Result<(u128,f64,u64)> {
let began=Instant::now();
let compiled = reuse.then(||SignalModule::new(black_box(&bytes))).transpose()?;
let mut checksum=0.0;
let mut fuel=0;
for _ in 0..instances {
let mut model=match &compiled {Some(module)=>module.instantiate(predictions,10_000_000)?,None=>WasmSignal::new(black_box(&bytes),predictions,10_000_000)?};
for _ in 0..predictions {checksum+=black_box(model.predict(black_box([110.,100.,105.,102.,25.,100.,112.,98.]))?);}
fuel += 10_000_000-model.remaining_fuel();
}
Ok((began.elapsed().as_nanos(),checksum,fuel))
};
for _ in 0..5 {run(false)?;run(true)?;}
for round in 0..30 {
let (fresh,reuse)= if round%2==0 {(run(false)?,run(true)?)} else {let b=run(true)?;(run(false)?,b)};
ensure!(fresh.1==reuse.1&&fresh.2==reuse.2,"checksum/fuel mismatch");
println!("{case},{round},{instances},{predictions},{},{},{},{}",fresh.0,reuse.0,fresh.1,fresh.2);
}
}
Ok(())
}

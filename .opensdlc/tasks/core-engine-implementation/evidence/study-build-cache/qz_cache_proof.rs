//! Untimed configuration proof, installed only into temporary worktrees.
//! Directly includes each arm's production file; no public job API is added.
#[path = "../src/signals.rs"]
#[allow(dead_code)]
mod signals;

use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::Path,
    rc::{Rc, Weak},
};

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    ensure!(
        args.len() == 1,
        "cache proof requires a frozen case directory"
    );
    let root = Path::new(&args[0]);
    let request: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("study-request.json"))?)?;
    let modules = request["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| {
            fs::read(
                root.join("objects")
                    .join(member["model_artifact_id"].as_str().unwrap()),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut cache = signals::SignalModuleCache::default();
    let mut previous = (0..modules.len()).map(|_| Weak::new()).collect::<Vec<_>>();
    let mut reuse = 0;
    let mut values = Vec::new();
    let mut fuels = Vec::new();
    for _ in 0..2 {
        for (index, bytes) in modules.iter().enumerate() {
            let module = cache.get_or_compile(bytes, true)?;
            if previous[index]
                .upgrade()
                .is_some_and(|old| Rc::ptr_eq(&old, &module))
            {
                reuse += 1;
            }
            previous[index] = Rc::downgrade(&module);
            let mut instance = module.instantiate(1, 10_000)?;
            values.push(instance.predict([0.0; 8])?.to_bits());
            fuels.push(instance.remaining_fuel());
        }
    }
    drop(cache);
    let released = previous.iter().all(|item| item.upgrade().is_none());
    ensure!(
        released,
        "compiled modules must be released with their task"
    );
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "source_sha256": format!("{:x}", Sha256::digest(include_bytes!("../src/signals.rs"))),
            "module_source_sha256": modules.iter().map(|bytes| format!("{:x}",Sha256::digest(bytes))).collect::<Vec<_>>(),
            "cache_lookups": modules.len()*2,
            "compiled_modules": modules.len()*2-reuse,
            "reused_compiled_identity": reuse,
            "prediction_bits": values,
            "remaining_fuel": fuels,
            "released_after_task": released,
            "scope": "untimed direct inclusion of arm's exact production signals.rs; not E2E instrumentation"
        }))?
    );
    Ok(())
}

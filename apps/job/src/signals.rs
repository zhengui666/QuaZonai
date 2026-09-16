//! Pure, bounded research computation using the native Wasmi interpreter.
//! This module has no WASI, host imports, file, network, clock or credential access.
use anyhow::{ensure, Result};
use wasmi::{
    Config, EnforcedLimits, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

pub const MAX_SIGNAL_MODULE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SIGNAL_MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_SIGNAL_PREDICTIONS: u32 = 1_000_000;
pub const MAX_SIGNAL_FUEL: u64 = 1_000_000_000;
pub const SIGNAL_FUEL_PER_PREDICTION: u64 = 100_000;

type Arguments = (f64, f64, f64, f64, f64, f64, f64, f64);

/// The caller supplies only observations already available at the decision cut-off.
/// Construct a fresh instance for each instrument and each independent fold.
pub struct WasmSignal {
    store: Store<StoreLimits>,
    predict: TypedFunc<Arguments, f64>,
    remaining_predictions: u32,
    remaining_fuel: u64,
    failed: bool,
}

impl WasmSignal {
    pub fn new(bytes: &[u8], max_predictions: u32, total_fuel: u64) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_SIGNAL_MODULE_BYTES,
            "SIGNAL_MODULE_TOO_LARGE"
        );
        ensure!(
            bytes.starts_with(b"\0asm\x01\0\0\0"),
            "SIGNAL_REQUIRES_WASM_BINARY"
        );
        ensure!(
            (1..=MAX_SIGNAL_PREDICTIONS).contains(&max_predictions),
            "SIGNAL_PREDICTION_LIMIT"
        );
        ensure!(
            (1..=MAX_SIGNAL_FUEL).contains(&total_fuel),
            "SIGNAL_FUEL_LIMIT"
        );
        let mut config = Config::default();
        config
            .consume_fuel(true)
            .allow_start_fn(false)
            .ignore_custom_sections(true)
            .enforced_limits(EnforcedLimits::strict())
            .set_max_recursion_depth(64)
            .set_min_stack_height(1024)
            .set_max_stack_height(64 * 1024)
            .set_max_cached_stacks(1)
            .wasm_multi_memory(false);
        let engine = Engine::new(&config);
        let module =
            Module::new(&engine, bytes).map_err(|_| anyhow::anyhow!("SIGNAL_MODULE_INVALID"))?;
        ensure!(
            module.imports().next().is_none(),
            "SIGNAL_IMPORTS_FORBIDDEN"
        );
        let limits = StoreLimitsBuilder::new()
            .memory_size(MAX_SIGNAL_MEMORY_BYTES)
            .table_elements(4096)
            .instances(1)
            .tables(1)
            .memories(1)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(&engine, limits);
        store.limiter(|limits| limits);
        // Instantiation also uses the same total fuel budget; no free start work.
        store.set_fuel(total_fuel)?;
        let instance = Linker::<StoreLimits>::new(&engine)
            .instantiate_and_start(&mut store, &module)
            .map_err(|_| anyhow::anyhow!("SIGNAL_INSTANTIATION_REJECTED"))?;
        let predict = instance
            .get_typed_func::<Arguments, f64>(&store, "predict")
            .map_err(|_| anyhow::anyhow!("SIGNAL_ABI_MISMATCH"))?;
        let remaining_fuel = store.get_fuel()?;
        Ok(Self {
            store,
            predict,
            remaining_predictions: max_predictions,
            remaining_fuel,
            failed: false,
        })
    }

    /// ABI order: close, previous_close, fast EMA, slow EMA, volume, open, high, low.
    /// No repair, zero substitution, fuel reset or retry is possible after a failed call.
    pub fn predict(&mut self, features: [f64; 8]) -> Result<f64> {
        ensure!(!self.failed, "SIGNAL_INSTANCE_FAILED");
        // Any early return, including native metering errors, invalidates the instance.
        self.failed = true;
        ensure!(
            features.iter().all(|value| value.is_finite()),
            "SIGNAL_INPUT_NONFINITE"
        );
        ensure!(
            self.remaining_predictions > 0 && self.remaining_fuel > 0,
            "SIGNAL_BUDGET_EXHAUSTED"
        );
        self.remaining_predictions -= 1;
        let fuel = self.remaining_fuel.min(SIGNAL_FUEL_PER_PREDICTION);
        self.store.set_fuel(fuel)?;
        let [close, previous, fast, slow, volume, open, high, low] = features;
        let result = self.predict.call(
            &mut self.store,
            (close, previous, fast, slow, volume, open, high, low),
        );
        let unused = self.store.get_fuel()?;
        ensure!(unused <= fuel, "SIGNAL_FUEL_ACCOUNTING_INVALID");
        self.remaining_fuel -= fuel - unused;
        match result {
            Ok(value) if value.is_finite() => {
                self.failed = false;
                Ok(value)
            }
            _ => Err(anyhow::anyhow!("SIGNAL_EXECUTION_FAILED")),
        }
    }

    pub fn remaining_fuel(&self) -> u64 {
        self.remaining_fuel
    }
}

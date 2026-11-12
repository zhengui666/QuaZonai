//! Pure research computation using the native Wasmi interpreter.
//! This module has no WASI, host imports, file, network, clock or credential access.
use anyhow::{ensure, Result};
use wasmi::{CompilationMode, Config, Engine, Instance, Linker, Module, Store, TypedFunc};

type Arguments = (f64, f64, f64, f64, f64, f64, f64, f64);

/// The caller supplies only observations already available at the decision cut-off.
/// Construct a fresh instance for each instrument and each independent fold.
pub struct WasmSignal {
    store: Store<()>,
    predict: TypedFunc<Arguments, f64>,
    remaining_predictions: Option<u64>,
    remaining_fuel: Option<u64>,
    failed: bool,
}

/// Task-local compiled code. Never shares globals, memory, failure state or fuel.
/// Drop with the task rather than retaining an unbounded process-wide module cache.
pub struct SignalModule {
    engine: Engine,
    module: Module,
    fuel_metering: bool,
}

impl SignalModule {
    /// Meter only an explicitly supplied budget. Wasmi 2.0.0 disables metering
    /// in its compiler when `fuel_metering` is false; no sentinel fuel is used.
    pub fn new(bytes: &[u8], fuel_metering: bool) -> Result<Self> {
        ensure!(
            bytes.starts_with(b"\0asm\x01\0\0\0"),
            "SIGNAL_REQUIRES_WASM_BINARY"
        );
        let mut config = Config::default();
        // Eager compilation keeps explicit fuel independent of cache warmth.
        // Default enforced module limits are None. Do not install StoreLimits:
        // memory/table allocation is governed by Wasm declarations and the host.
        // Wasmi's stack API has no disabled setting. Use the host's representable
        // allocation boundary, not its arbitrary default recursion/stack quotas.
        // This is a finite platform boundary, not a promise of infinite memory.
        config
            .compilation_mode(CompilationMode::Eager)
            .consume_fuel(fuel_metering)
            .allow_start_fn(false)
            .ignore_custom_sections(true)
            .set_max_recursion_depth(isize::MAX as usize)
            .set_max_stack_height(isize::MAX as usize);
        let engine = Engine::new(&config);
        let module =
            Module::new(&engine, bytes).map_err(|_| anyhow::anyhow!("SIGNAL_MODULE_INVALID"))?;
        ensure!(
            module.imports().next().is_none(),
            "SIGNAL_IMPORTS_FORBIDDEN"
        );
        Ok(Self {
            engine,
            module,
            fuel_metering,
        })
    }

    pub fn instantiate(
        &self,
        max_predictions: impl Into<Option<u64>>,
        total_fuel: impl Into<Option<u64>>,
    ) -> Result<WasmSignal> {
        let max_predictions = max_predictions.into();
        let total_fuel = total_fuel.into();
        let (store, instance) = self.instantiate_store(max_predictions, total_fuel)?;
        let predict = instance
            .get_typed_func::<Arguments, f64>(&store, "predict")
            .map_err(|_| anyhow::anyhow!("SIGNAL_ABI_MISMATCH"))?;
        let remaining_fuel = total_fuel.map(|_| store.get_fuel()).transpose()?;
        Ok(WasmSignal {
            store,
            predict,
            remaining_predictions: max_predictions,
            remaining_fuel,
            failed: false,
        })
    }

    pub(crate) fn instantiate_store(
        &self,
        max_predictions: Option<u64>,
        total_fuel: Option<u64>,
    ) -> Result<(Store<()>, Instance)> {
        prediction_budget(max_predictions, total_fuel)?;
        ensure!(
            self.fuel_metering == total_fuel.is_some(),
            "SIGNAL_FUEL_MODE_MISMATCH"
        );
        let mut store = Store::new(&self.engine, ());
        if let Some(fuel) = total_fuel {
            // Instantiation shares the same explicit budget; no free start work.
            store.set_fuel(fuel)?;
        }
        let instance = Linker::new(&self.engine)
            .instantiate_and_start(&mut store, &self.module)
            .map_err(|_| anyhow::anyhow!("SIGNAL_INSTANTIATION_REJECTED"))?;
        Ok((store, instance))
    }
}

fn prediction_budget(max_predictions: Option<u64>, total_fuel: Option<u64>) -> Result<()> {
    ensure!(max_predictions != Some(0), "SIGNAL_PREDICTION_LIMIT");
    ensure!(total_fuel != Some(0), "SIGNAL_FUEL_LIMIT");
    Ok(())
}

impl WasmSignal {
    pub fn new(
        bytes: &[u8],
        max_predictions: impl Into<Option<u64>>,
        total_fuel: impl Into<Option<u64>>,
    ) -> Result<Self> {
        let max_predictions = max_predictions.into();
        let total_fuel = total_fuel.into();
        prediction_budget(max_predictions, total_fuel)?;
        SignalModule::new(bytes, total_fuel.is_some())?.instantiate(max_predictions, total_fuel)
    }

    /// ABI order: close, previous_close, fast EMA, slow EMA, volume, open, high, low.
    /// No repair, zero substitution, fuel reset or retry is possible after a failed call.
    pub fn predict(&mut self, features: [f64; 8]) -> Result<f64> {
        ensure!(!self.failed, "SIGNAL_INSTANCE_FAILED");
        self.failed = true;
        ensure!(
            features.iter().all(|value| value.is_finite()),
            "SIGNAL_INPUT_NONFINITE"
        );
        ensure!(
            self.remaining_predictions != Some(0) && self.remaining_fuel != Some(0),
            "SIGNAL_BUDGET_EXHAUSTED"
        );
        if let Some(remaining) = &mut self.remaining_predictions {
            *remaining -= 1;
        }
        let [close, previous, fast, slow, volume, open, high, low] = features;
        let result = self.predict.call(
            &mut self.store,
            (close, previous, fast, slow, volume, open, high, low),
        );
        if let Some(remaining) = &mut self.remaining_fuel {
            let unused = self.store.get_fuel()?;
            ensure!(unused <= *remaining, "SIGNAL_FUEL_ACCOUNTING_INVALID");
            *remaining = unused;
        }
        match result {
            Ok(value) if value.is_finite() => {
                self.failed = false;
                Ok(value)
            }
            _ => Err(anyhow::anyhow!("SIGNAL_EXECUTION_FAILED")),
        }
    }

    /// None means fuel metering was disabled, never an invented zero measurement.
    pub fn remaining_fuel(&self) -> Option<u64> {
        self.remaining_fuel
    }
}

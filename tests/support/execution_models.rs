//! Explicit controlled native model parameters, never production defaults.
use contracts::{portfolio::*, DbCounter, SchemaV1};

pub fn fill() -> NativeModelRefV1 {
    NativeModelRefV1::NautilusDefaultFill {
        schema_version: SchemaV1,
        upstream_class: NAUTILUS_FILL_CLASS.into(),
        upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters: NautilusFillParametersV1 {
            prob_fill_on_limit: "1".parse().unwrap(),
            prob_slippage: "0".parse().unwrap(),
            random_seed: DbCounter::new(7).unwrap(),
        },
    }
}
pub fn fee() -> NativeModelRefV1 {
    NativeModelRefV1::NautilusMakerTaker {
        schema_version: SchemaV1,
        upstream_class: NAUTILUS_FEE_CLASS.into(),
        upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters: NautilusFeeParametersV1 {},
    }
}
pub fn latency(insert: u64) -> NativeModelRefV1 {
    NativeModelRefV1::NautilusStaticLatency {
        schema_version: SchemaV1,
        upstream_class: NAUTILUS_LATENCY_CLASS.into(),
        upstream_version: NAUTILUS_EXECUTION_VERSION.into(),
        parameters: NautilusLatencyParametersV1 {
            base_latency_ns: DbCounter::ZERO,
            insert_latency_ns: DbCounter::new(insert).unwrap(),
            update_latency_ns: DbCounter::ZERO,
            cancel_latency_ns: DbCounter::ZERO,
        },
    }
}

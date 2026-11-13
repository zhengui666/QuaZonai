//! Retired output admission remains rejected; historical contracts remain readable.
use contracts::{
    DbCounter, Id, Revision,
    runtime_jobs::{RuntimeOutputV1, native_output_contract},
};

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
#[test]
fn retired_allocation_and_study_outputs_are_not_accepted() {
    for name in [
        "qz.native_portfolio",
        "qz.portfolio_study",
        "qz.native_simulation",
        "qz.strategy_portfolio",
    ] {
        let contract = native_output_contract(name, "1").unwrap();
        let bytes = b"{}";
        let descriptor = RuntimeOutputV1 {
            kind: contract.kind,
            schema: contracts::runtime::RuntimeArtifactSchemaV1 {
                name: name.into(),
                version: "1".into(),
            },
            storage_ref: Id::new(),
            storage_version: Revision::INITIAL,
            byte_count: count(bytes.len() as u64),
            media_type: contract.media_type.into(),
        };
        assert_eq!(
            domain::execution::output_shape(&descriptor, bytes),
            Err(domain::research::invalid(
                "native_output.schema",
                "NATIVE_OUTPUT_CONTRACT_INVALID"
            ))
        );
    }
}

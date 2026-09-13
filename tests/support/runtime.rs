//! Protocol-only native runtime fixture. This does not prove OCI execution,
//! genuine market data, a qualified Alpha, or the complete fresh-user workflow.
#![allow(dead_code)]
use chrono::{DateTime, Utc};
use contracts::runtime::RuntimeCapabilitiesV1;

pub fn capabilities(now: DateTime<Utc>) -> RuntimeCapabilitiesV1 {
    let mut value: RuntimeCapabilitiesV1 = serde_json::from_str(include_str!(
        "../contracts/runtime-capabilities.fixture.json"
    ))
    .unwrap();
    value.checked_at = now;
    // Controlled protocol capabilities, not an actual image verification.
    for (name, version) in [
        ("solow-cv", "0.7.3"),
        ("ndarray-stats", "0.7.0"),
        ("linregress", "0.5.4"),
    ] {
        value.engine_versions.insert(name.into(), version.into());
    }
    value
        .artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.alpha_validation".into(),
            version: "1".into(),
        });
    value
        .artifact_schemas
        .push(contracts::runtime::RuntimeArtifactSchemaV1 {
            name: "qz.alpha_sealed".into(),
            version: "1".into(),
        });
    value
}

pub fn portfolio_capabilities(now: DateTime<Utc>) -> RuntimeCapabilitiesV1 {
    use contracts::{portfolio::*, runs::RunKind, runtime::*};
    let mut value = capabilities(now);
    for kind in [RunKind::PortfolioBuild, RunKind::PortfolioSimulate] {
        value.job_kinds.push(kind);
        let mut image = value.image_refs[0].clone();
        image.job_kind = kind;
        value.image_refs.push(image);
    }
    for (name, version) in [
        ("portfolio-models", "4"),
        ("portfolio-weights", "1"),
        ("portfolio-liquidity", "1"),
        ("portfolio-cost-source", "1"),
        ("portfolio-slippage", "1"),
        ("bar-notional", "1"),
        ("simulation-models", "1"),
        ("nautilus", NAUTILUS_EXECUTION_VERSION),
        ("clarabel", CLARABEL_VERSION),
        ("ndarray", FIXED_ENSEMBLE_VERSION),
    ] {
        value.engine_versions.insert(name.into(), version.into());
    }
    value.artifact_schemas.push(RuntimeArtifactSchemaV1 {
        name: "qz.native_portfolio".into(),
        version: "1".into(),
    });
    value.solver_capabilities = vec!["CONVEX_QP".into()];
    value.venues = vec![RuntimeVenueV1 {
        venue: "SIM".into(),
        instrument_classes: vec!["CurrencyPair".into()],
        data_kinds: vec![RuntimeDataKind::Bar],
        expiry_and_settlement: false,
    }];
    value
}

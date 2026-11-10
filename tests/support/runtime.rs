//! Protocol-only native runtime fixture. This does not prove OCI execution,
//! genuine market data, a qualified Alpha, or the complete fresh-user workflow.
#![allow(dead_code)]
use chrono::{DateTime, Utc};
use contracts::runtime::RuntimeCapabilitiesV1;

pub fn capabilities(now: DateTime<Utc>) -> RuntimeCapabilitiesV1 {
    let mut value: RuntimeCapabilitiesV1 = serde_json::from_str(r###"{"schema_version":1,"protocol_versions":[1],"runtime_version":"protocol-fixture-not-a-production-runtime","engine_versions":{"fixture-engine":"1"},"image_refs":[{"job_kind":"DATA_VALIDATE","image_ref":"example.invalid/fixture@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},{"job_kind":"ALPHA_EVALUATE","image_ref":"example.invalid/fixture@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}],"job_kinds":["DATA_VALIDATE","ALPHA_EVALUATE"],"artifact_schemas":[{"name":"qz.job_result","version":"1"}],"data_kinds":["BAR"],"venues":[{"venue":"FIXTURE","instrument_classes":["EQUITY"],"data_kinds":["BAR"],"expiry_and_settlement":false}],"label_interval_support":{"fixed_bars":true,"fixed_duration":false,"variable_interval":false},"solver_capabilities":[],"max_cpu":2,"max_memory_mib":4096,"max_output_bytes":null,"max_wall_seconds":null,"isolation_profile":"OCI_RESEARCH_V1","checked_at":"2026-09-09T00:00:00Z"}"###)
    .unwrap();
    value.checked_at = now;
    // Controlled protocol capabilities, not an actual image verification.
    for (name, version) in [
        ("optional-wall-time", "1"),
        ("optional-cpu-budget", "1"),
        ("optional-cpu-rate", "1"),
        ("optional-memory-limit", "1"),
        ("optional-output-budget", "1"),
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
        ("portfolio-rolling-liquidity", "1"),
        ("portfolio-build-rolling", "1"),
        ("portfolio-cost-source", "1"),
        ("portfolio-slippage", "1"),
        ("bar-notional", "1"),
        ("simulation-models", "1"),
        ("candidate-simulation", "2"),
        ("portfolio-study", "6"),
        ("portfolio-calendar", "2"),
        ("portfolio-history", "1"),
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
    value.artifact_schemas.push(RuntimeArtifactSchemaV1 {
        name: "qz.native_simulation".into(),
        version: "1".into(),
    });
    value.artifact_schemas.push(RuntimeArtifactSchemaV1 {
        name: "qz.data_quality".into(),
        version: "1".into(),
    });
    value.solver_capabilities = vec!["CONVEX_QP".into()];
    for name in ["qz.portfolio_study", "qz.portfolio_history"] {
        value.artifact_schemas.push(RuntimeArtifactSchemaV1 {
            name: name.into(),
            version: "1".into(),
        });
    }
    value.venues = vec![RuntimeVenueV1 {
        venue: "SIM".into(),
        instrument_classes: vec!["CurrencyPair".into()],
        data_kinds: vec![RuntimeDataKind::Bar],
        expiry_and_settlement: false,
    }];
    value
}

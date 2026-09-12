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
}

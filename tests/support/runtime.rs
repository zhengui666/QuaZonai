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
    value
}

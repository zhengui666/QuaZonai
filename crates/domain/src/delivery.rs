//! Borrowed immutable target fields used by the native Sandbox adapter.
/// Common transport fields retain their typed source branch. This is a borrowed
/// view, never a conversion of strategy evidence into forecast qualifications.
pub struct PackageDeliveryRef<'a> {
    pub release_id: contracts::Id,
    pub project_id: contracts::Id,
    pub candidate_id: contracts::Id,
    pub mandate_id: contracts::Id,
    pub package_schema_version: contracts::settings::PackageSchemaVersion,
    pub asof: chrono::DateTime<chrono::Utc>,
    pub valid_from: chrono::DateTime<chrono::Utc>,
    pub valid_until: chrono::DateTime<chrono::Utc>,
    pub base_currency: &'a String,
    pub capital_assumption: &'a contracts::DecimalValue,
    pub exposure_tolerance: &'a contracts::DecimalValue,
    pub cost_assumption_ref: contracts::Id,
    pub cash_weight: &'a contracts::DecimalValue,
    pub targets: &'a [contracts::delivery::PackageTargetV1],
    pub compatible_market_capabilities: &'a [String],
    pub input_revision_refs: &'a [contracts::Id],
}

pub fn package_delivery(
    package: &contracts::strategy_portfolio::TargetPackageEnvelopeV2,
) -> PackageDeliveryRef<'_> {
    use contracts::{settings::PackageSchemaVersion, strategy_portfolio::TargetPackageEnvelopeV2};
    macro_rules! common {
        ($p:expr, $version:expr) => {
            PackageDeliveryRef {
                release_id: $p.release_id,
                project_id: $p.project_id,
                candidate_id: $p.candidate_id,
                mandate_id: $p.mandate_id,
                package_schema_version: $version,
                asof: $p.asof,
                valid_from: $p.valid_from,
                valid_until: $p.valid_until,
                base_currency: &$p.base_currency,
                capital_assumption: &$p.capital_assumption,
                exposure_tolerance: &$p.exposure_tolerance,
                cost_assumption_ref: $p.cost_assumption_ref,
                cash_weight: &$p.cash_weight,
                targets: &$p.targets,
                compatible_market_capabilities: &$p.compatible_market_capabilities,
                input_revision_refs: &$p.input_revision_refs,
            }
        };
    }
    match package {
        TargetPackageEnvelopeV2::Forecast(p) => common!(p, PackageSchemaVersion::V2),
        TargetPackageEnvelopeV2::TargetDecision(p) => common!(p, PackageSchemaVersion::V2),
    }
}

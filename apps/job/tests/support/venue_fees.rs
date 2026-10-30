//! Controlled venue policy, independently written before constructing requests.
//! These exact fixture rates are not provider/account commission evidence.
use contracts::{
    catalogs::{FrozenMakerTakerFeesV1, MAKER_TAKER_FEES_FILE},
    research::DataOrigin,
    science::NativeFeeRateV1,
    DbCounter, SchemaV1,
};
use nautilus_model::types::Currency;
use std::path::Path;

pub fn write(
    root: &Path,
    venue: &str,
    currency: Currency,
    valid_until_ns: u64,
    rates: Vec<NativeFeeRateV1>,
) {
    let source = FrozenMakerTakerFeesV1 {
        schema_version: SchemaV1,
        source_reference: "Controlled original venue fee fixture; not market fee evidence".into(),
        source_version: "synthetic-venue-fee-policy-v1".into(),
        origin: DataOrigin::Fixture,
        venue: venue.into(),
        currency: currency.to_string(),
        currency_precision: currency.precision,
        available_at_ns: DbCounter::ZERO,
        valid_from_ns: DbCounter::ZERO,
        valid_until_ns: DbCounter::new(valid_until_ns).unwrap(),
        rates,
    };
    domain::catalogs::maker_taker_fee_source(&source).unwrap();
    std::fs::write(
        root.join(MAKER_TAKER_FEES_FILE),
        serde_json::to_vec(&source).unwrap(),
    )
    .unwrap();
}

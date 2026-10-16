//! Optional operator command implementations; excluded from default scientific builds.
#[cfg(feature = "catalog-prepare")]
pub mod catalog_prepare;
#[cfg(feature = "polymarket-history")]
pub mod polymarket_history;

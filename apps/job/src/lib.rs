//! Native research computations over registered inputs.
pub const NAUTILUS_VERSION: &str = env!("QZ_NAUTILUS_VERSION");
pub const NAUTILUS_REVISION: &str = env!("QZ_NAUTILUS_REVISION");

pub fn build_identity() -> serde_json::Value {
    serde_json::json!({
        "nautilusVersion": NAUTILUS_VERSION,
        "nautilusRevision": NAUTILUS_REVISION,
        "precision": if cfg!(feature = "high-precision") { "high" } else { "standard" },
        "jobVersion": env!("CARGO_PKG_VERSION"),
    })
}
pub mod account_observation_cli;
pub mod account_observer;
#[cfg(feature = "account-runtime")]
mod account_order_preview;
#[cfg(feature = "account-runtime")]
pub mod account_runtime;
#[cfg(feature = "account-runtime")]
mod account_runtime_cache;
pub mod bounded;
pub mod capital_curve;
pub mod capital_exit_gate;
pub mod catalog;
pub mod catalog_prefix;
mod fee_models;
pub mod forward;
pub mod managed;
#[cfg(feature = "native-sandbox-test")]
pub mod native_capital_exit;
#[cfg(feature = "native-node-observer")]
pub mod native_node_observer;
pub mod native_walk_forward;
#[cfg(feature = "catalog-prepare")]
pub mod operator;
mod report;
pub use report::write_probe_report;
pub mod source_market;
#[cfg(feature = "hyperliquid-offline")]
pub mod spot_cash_source;
pub mod spot_fees;
pub mod ticks;
pub mod validation;

pub mod native_strategy;

#[cfg(test)]
#[path = "../tests/support/native_portfolio.rs"]
mod native_test_portfolio;

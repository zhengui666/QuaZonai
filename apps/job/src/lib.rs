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
pub mod capacity;
pub mod capital_curve;
pub mod capital_exit_gate;
pub mod catalog;
pub mod native_walk_forward;
pub mod catalog_prefix;
pub mod experiment;
pub mod feature_model;
mod fee_models;
pub mod forecast;
pub mod forward;
mod indicator_strategy;
mod live_progress;
pub mod managed;
#[cfg(feature = "native-sandbox-test")]
pub mod native_capital_exit;
#[cfg(feature = "native-node-observer")]
pub mod native_node_observer;
pub mod observed_spot;
#[cfg(feature = "catalog-prepare")]
pub mod operator;
mod optimization;
mod performance;
pub mod portfolio;
pub use optimization::allocate;
mod report;
pub use report::write_probe_report;
pub mod signals;
pub mod simulation;
pub mod source_market;
pub mod spot_cash_capture;
mod spot_cash_runtime;
#[cfg(feature = "hyperliquid-offline")]
pub mod spot_cash_source;
pub mod spot_fees;
pub mod strategy;
pub mod study;
pub mod ticks;
pub mod validation;

pub mod native_strategy;

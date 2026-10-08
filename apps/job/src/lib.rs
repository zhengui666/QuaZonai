//! Native research computations over registered inputs.
pub mod account_observation_cli;
pub mod account_observer;
pub mod bounded;
pub mod catalog;
pub mod capital_exit_gate;
#[cfg(any(feature = "native-sandbox-test", feature = "native-paper"))]
pub mod native_capital_exit;
#[cfg(feature = "native-paper")]
pub mod capital_exit_transport;
#[cfg(feature = "native-paper")]
mod paper_capital_exit;
pub mod experiment;
pub mod feature_model;
pub mod forecast;
pub mod forward;
pub mod managed;
#[cfg(feature = "native-node-observer")]
pub mod native_node_observer;
#[cfg(any(feature = "catalog-prepare", feature = "polymarket-history"))]
pub mod operator;
mod optimization;
#[cfg(feature = "native-paper")]
mod paper_claim_store;
#[cfg(feature = "native-paper")]
pub mod paper_node;
#[cfg(feature = "native-paper")]
pub mod paper_service;
#[cfg(feature = "native-node-observer")]
pub mod polymarket_data_probe;
pub mod polymarket_execution_preflight;
#[cfg(feature = "native-paper")]
pub mod polymarket_paper_host;
#[cfg(feature = "native-paper")]
pub mod polymarket_paper_service;
#[cfg(any(feature = "native-paper", feature = "catalog-prepare"))]
mod polymarket_source_record;
#[cfg(feature = "native-paper")]
pub mod polymarket_streaming_paper;
pub mod portfolio;
mod prediction;
pub use domain::codex::verified_codex_version;
pub use optimization::allocate;
mod report;
pub use report::write_probe_report;
pub mod signals;
pub mod simulation;
pub mod spot_cash_capture;
mod spot_cash_runtime;
#[cfg(feature = "hyperliquid-offline")]
pub mod spot_cash_source;
pub mod spot_fees;
pub mod strategy;
pub mod study;
pub mod validation;

//! Native research computations over registered inputs.
pub mod account_observation_cli;
pub mod account_observer;
pub mod bounded;
pub mod catalog;
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
pub mod paper_node;
#[cfg(feature = "native-paper")]
pub mod paper_service;
#[cfg(feature = "native-node-observer")]
pub mod polymarket_data_probe;
#[cfg(feature = "native-paper")]
pub mod polymarket_paper_host;
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
pub mod strategy;
pub mod study;
pub mod validation;

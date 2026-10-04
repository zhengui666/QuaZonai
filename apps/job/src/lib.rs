//! Native research computations over registered inputs.
pub mod account_observation_cli;
pub mod account_observer;
pub mod bounded;
pub mod catalog;
pub mod forecast;
pub mod forward;
pub mod managed;
#[cfg(any(feature = "catalog-prepare", feature = "polymarket-history"))]
pub mod operator;
mod optimization;
pub mod portfolio;
mod prediction;
pub use domain::codex::verified_codex_version;
pub use optimization::allocate;
mod report;
pub use report::write_probe_report;
pub mod signals;
pub mod simulation;
pub mod study;
pub mod validation;

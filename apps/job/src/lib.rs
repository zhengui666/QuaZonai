//! Native research computations over registered inputs.
pub mod bounded;
pub mod catalog;
pub mod forecast;
pub mod forward;
pub mod managed;
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

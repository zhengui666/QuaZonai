//! Pure native scientific input, evidence and numerical adapter rules.
#![forbid(unsafe_code)]

pub mod account_observation;
pub mod catalogs;
pub mod control;
pub mod data;
pub mod delivery;
pub mod evidence;
pub mod execution;
pub mod execution_limits;
pub mod factors;
pub mod forward;
pub mod observed_spot;
pub mod portfolio;
pub mod prediction;
pub mod research;
pub mod runtime;
pub mod runtime_jobs;
pub mod spot_cash;

use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    #[error("research contract fields are invalid")]
    Fields(Vec<contracts::research::FieldIssue>),
    #[error("invalid contract field: {0}")]
    Invalid(&'static str),
    #[error("capability unavailable: {0}")]
    CapabilityUnavailable(&'static str),
}

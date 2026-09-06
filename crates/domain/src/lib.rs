//! QZ-owned decisions only. No HTTP, SQL, queue, crypto, model loop or optimizer.
//!
//! Callers must fetch authoritative facts and persist every resulting transition
//! under the same database transaction/CAS. These pure rules are not a substitute
//! for authentication, cross-project FKs, leases, native isolation or acceptance.
#![forbid(unsafe_code)]

pub mod admission;
pub mod codex;
pub mod control;
pub mod evidence;
pub mod runs;

use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DomainError {
    #[error("invalid contract field: {0}")]
    Invalid(&'static str),
    #[error("project does not admit new research")]
    AdmissionClosed,
    #[error("budget exhausted: {0}")]
    BudgetExhausted(&'static str),
    #[error("capability unavailable: {0}")]
    CapabilityUnavailable(&'static str),
    #[error("object revision changed")]
    RevisionConflict,
    #[error("run is already terminal")]
    TerminalRun,
    #[error("state does not allow this transition")]
    InvalidTransition,
    #[error("external cancellation has not been confirmed")]
    CancelNotConfirmed,
    #[error("stale attempt or expired owner lease")]
    StaleAttempt,
}

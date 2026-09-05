//! Transition decisions must be applied using the locked row's revision and seq.
use qz_contracts::{runs::RunState, DbCounter, Revision, Timestamp};

use crate::DomainError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptLease {
    pub attempt_no: u32,
    pub worker_owner_id: String,
    pub owner_epoch: Revision,
    pub lease_expires_at: Timestamp,
}

/// A fact returned by the trusted runtime adapter after querying the exact job.
/// "Cancel accepted"/network timeout/RUNNING are intentionally not variants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteTerminal {
    Succeeded,
    Failed,
    Cancelled,
    ConfirmedAbsent,
}

pub fn next_event(
    current_revision: Revision,
    expected_revision: Revision,
    last_event_seq: DbCounter,
) -> Result<(Revision, DbCounter), DomainError> {
    if expected_revision != current_revision {
        return Err(DomainError::RevisionConflict);
    }
    Ok((
        current_revision
            .next()
            .ok_or(DomainError::Invalid("revision_exhausted"))?,
        last_event_seq
            .checked_add(1)
            .ok_or(DomainError::Invalid("event_sequence_exhausted"))?,
    ))
}

pub fn validate_owner(
    current: &AttemptLease,
    presented: &AttemptLease,
    database_now: Timestamp,
) -> Result<(), DomainError> {
    if current.attempt_no == 0
        || current.attempt_no != presented.attempt_no
        || current.worker_owner_id.is_empty()
        || current.worker_owner_id != presented.worker_owner_id
        || current.owner_epoch != presented.owner_epoch
        || current.lease_expires_at <= database_now
    {
        return Err(DomainError::StaleAttempt);
    }
    // Presented expiry is never authority; only the locked database row counts.
    Ok(())
}

pub fn request_cancel(state: RunState) -> Result<RunState, DomainError> {
    match state {
        RunState::Queued => Ok(RunState::Cancelled),
        RunState::Dispatching
        | RunState::Running
        | RunState::Reconciling
        | RunState::CancelRequested => Ok(RunState::CancelRequested),
        RunState::Succeeded | RunState::Failed | RunState::Cancelled => {
            Err(DomainError::TerminalRun)
        }
    }
}

pub fn begin_dispatch(state: RunState) -> Result<RunState, DomainError> {
    if state == RunState::Queued {
        Ok(RunState::Dispatching)
    } else {
        Err(DomainError::InvalidTransition)
    }
}

pub fn reconcile(state: RunState) -> Result<RunState, DomainError> {
    match state {
        RunState::Dispatching | RunState::Running | RunState::Reconciling => {
            Ok(RunState::Reconciling)
        }
        // Keep cancellation intent while querying a lost submit/cancel response.
        RunState::CancelRequested => Ok(RunState::CancelRequested),
        _ => Err(DomainError::InvalidTransition),
    }
}

pub fn confirm_running(state: RunState) -> Result<RunState, DomainError> {
    match state {
        RunState::Dispatching | RunState::Running | RunState::Reconciling => Ok(RunState::Running),
        RunState::CancelRequested => Ok(RunState::CancelRequested),
        _ => Err(DomainError::InvalidTransition),
    }
}

pub fn accept_terminal(
    state: RunState,
    terminal: Option<RemoteTerminal>,
    current: &AttemptLease,
    presented: &AttemptLease,
    database_now: Timestamp,
) -> Result<RunState, DomainError> {
    validate_owner(current, presented, database_now)?;
    if state.is_terminal() {
        return Err(DomainError::TerminalRun);
    }
    if state == RunState::Queued {
        return Err(DomainError::InvalidTransition);
    }
    let terminal = terminal.ok_or(DomainError::CancelNotConfirmed)?;
    if state == RunState::CancelRequested {
        // Cancellation won the database CAS. A remotely completed but unadopted
        // result stays diagnostic only; never publish success as well.
        return Ok(RunState::Cancelled);
    }
    match terminal {
        RemoteTerminal::Succeeded => Ok(RunState::Succeeded),
        RemoteTerminal::Failed => Ok(RunState::Failed),
        RemoteTerminal::Cancelled => Ok(RunState::Cancelled),
        // Missing job is a reconciliation fact, not fabricated success/failure.
        RemoteTerminal::ConfirmedAbsent => Err(DomainError::InvalidTransition),
    }
}

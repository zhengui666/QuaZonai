//! One reserved native Turn, never a second model/tool loop.
use super::{MissionConnection, WorkerFailure};
use crate::codex_native::{NativeFailure, Observation, Turn, TurnStatus};
use contracts::Id;
use integrations::artifacts::ArtifactStore;
use std::{sync::Arc, time::Duration};
use store::{
    lifecycle::NextRuntimeAction,
    turns::{DispatchDecision, TurnOutcome, UsageReceipt, WorkerFence},
    Store, StoreError,
};
use tokio::{sync::watch, time::Instant};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnProgress {
    Settled(UsageReceipt),
    /// No ACK or final usage. Keep the original reservation and native identity.
    Unresolved,
}

fn native(reason: NativeFailure) -> WorkerFailure {
    WorkerFailure::Codex("TURN", reason)
}

fn outcome(turn: &Turn) -> Option<(TurnOutcome, &'static str)> {
    match turn.status {
        TurnStatus::Completed => Some((TurnOutcome::Succeeded, "NATIVE_TURN_COMPLETED")),
        TurnStatus::Failed => Some((TurnOutcome::Failed, "NATIVE_TURN_FAILED")),
        TurnStatus::Interrupted => Some((TurnOutcome::Cancelled, "NATIVE_TURN_INTERRUPTED")),
        TurnStatus::InProgress => None,
    }
}

impl MissionConnection {
    /// The owning worker renews the Attempt while this future runs. On any exit,
    /// it must close this connection; closing is not evidence of remote cancel.
    pub async fn drive_turn(
        &mut self,
        store: &Store,
        objects: Arc<ArtifactStore>,
        run: Id,
        fence: &WorkerFence,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<TurnProgress, WorkerFailure> {
        if *shutdown.borrow() || shutdown.has_changed().is_err() {
            return Err(WorkerFailure::LostAuthority);
        }
        let checkpoint = store.mission_turn_checkpoint(run, fence).await?;
        let latest = checkpoint.latest.ok_or(WorkerFailure::Contract)?;
        let item = &latest.reservation;
        if item.session_id != self.session.id || item.attempt_id != fence.attempt_id {
            return Err(WorkerFailure::Contract);
        }
        if let Some(receipt) = latest.receipt {
            return Ok(TurnProgress::Settled(receipt));
        }
        // Native model/catalog metadata has no authoritative price. Never send
        // against a made-up rate or settle using the originally reserved amount.
        if !latest.sent && item.reserved_cost.is_some() {
            return Err(WorkerFailure::Codex(
                "COST_UNAVAILABLE",
                NativeFailure::Configuration,
            ));
        }
        let thread = &self.session.native.thread_id;
        let prompt = if !latest.sent {
            Some(
                store
                    .mission_turn_prompt(run, fence, item.id, move |id, size| async move {
                        tokio::task::spawn_blocking(move || objects.read(id, size))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    })
                    .await?,
            )
        } else {
            None
        };
        let mut job = store.mission_job(run, fence).await?;
        let mut actual = match store.claim_turn_dispatch(item.id, fence).await? {
            DispatchDecision::Settled => return Ok(TurnProgress::Unresolved),
            DispatchDecision::Send { rpc_request_id } => {
                // The stored intent stays unknown if shutdown or I/O loses the
                // response. No second turn/start, including after process restart.
                if *shutdown.borrow() || shutdown.has_changed().is_err() {
                    return Err(WorkerFailure::LostAuthority);
                }
                let turn = self
                    .client
                    .start_turn(
                        &rpc_request_id,
                        thread,
                        prompt.as_deref().ok_or(WorkerFailure::Contract)?,
                    )
                    .await
                    .map_err(|reason| WorkerFailure::Codex("START_TURN", reason))?;
                store.bind_native_turn(item.id, fence, &turn.id).await?;
                turn
            }
            DispatchDecision::Reconcile {
                native_turn_id: Some(id),
            } => {
                let Some(turn) = self
                    .client
                    .turns(thread)
                    .await
                    .map_err(native)?
                    .into_iter()
                    .find(|turn| turn.id == id)
                else {
                    return Ok(TurnProgress::Unresolved);
                };
                turn
            }
            DispatchDecision::Reconcile {
                native_turn_id: None,
            } => return Ok(TurnProgress::Unresolved),
        };
        // turn/start acknowledges queued input before the native Turn is active.
        // Only its started event/timestamp permits a normal turn/interrupt.
        let mut started = actual.started_at.is_some();
        if started {
            store
                .observe_run_running(run, fence, &job.lease.external_job_id)
                .await?;
        }
        let mut tokens = None;
        let mut terminal_at = None;
        let mut interrupted_at = None;
        loop {
            if *shutdown.borrow() || shutdown.has_changed().is_err() {
                return Err(WorkerFailure::LostAuthority);
            }
            // Re-read the current fence and the committed DB-clock cancel intent.
            job = store.mission_job(run, fence).await?;
            if let Some((outcome, reason)) = outcome(&actual) {
                store
                    .observe_mission_turn_terminal(item.id, fence, outcome, reason)
                    .await?;
                if let Some(actual_tokens) = tokens {
                    // Usage is updated after each native model response, not an
                    // authoritative final receipt for a failed/interrupted Turn.
                    // A later tool continuation may have spent unreported tokens.
                    if outcome == TurnOutcome::Succeeded && item.reserved_cost.is_none() {
                        let receipt = UsageReceipt {
                            outcome,
                            actual_tokens,
                            actual_cost: None,
                            currency: None,
                            reason_code: reason.into(),
                        };
                        store.settle_turn(item.id, fence, &receipt).await?;
                        return Ok(TurnProgress::Settled(receipt));
                    }
                }
                let since = terminal_at.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_secs(2) {
                    return Ok(TurnProgress::Unresolved);
                }
            } else if started
                && job.lease.action == NextRuntimeAction::Cancel
                && interrupted_at.is_none()
            {
                match self.client.interrupt_turn(thread, &actual.id).await {
                    Ok(()) => {}
                    Err(NativeFailure::Rejected(-32600)) => {
                        // Completion can win after our last observation. The
                        // rejection is not a terminal: reconcile only this ID.
                        let Some(terminal) = self
                            .client
                            .turns(thread)
                            .await
                            .map_err(native)?
                            .into_iter()
                            .find(|turn| turn.id == actual.id && turn.status.terminal())
                        else {
                            return Ok(TurnProgress::Unresolved);
                        };
                        actual = terminal;
                    }
                    Err(reason) => return Err(WorkerFailure::Codex("INTERRUPT_TURN", reason)),
                }
                interrupted_at = Some(Instant::now());
            }
            if interrupted_at.is_some_and(|time| time.elapsed() >= Duration::from_secs(30)) {
                return Ok(TurnProgress::Unresolved);
            }
            for event in self
                .client
                .observations(Duration::from_millis(200))
                .await
                .map_err(native)?
            {
                match event {
                    Observation::TurnStarted { thread_id, turn }
                    | Observation::TurnCompleted { thread_id, turn } => {
                        if thread_id != *thread || turn.id != actual.id {
                            return Err(native(NativeFailure::Correlation));
                        }
                        if actual.status.terminal() && turn.status != actual.status {
                            // start_turn can already return a terminal while its
                            // earlier started notification remains in the queue.
                            if turn.status == TurnStatus::InProgress {
                                continue;
                            }
                            return Err(native(NativeFailure::Contract));
                        }
                        if !started
                            && (turn.status == TurnStatus::InProgress || turn.started_at.is_some())
                        {
                            store
                                .observe_run_running(run, fence, &job.lease.external_job_id)
                                .await?;
                            started = true;
                        }
                        actual = turn;
                    }
                    Observation::Usage {
                        thread_id,
                        turn_id,
                        total,
                    } => {
                        if thread_id != *thread || turn_id != actual.id {
                            return Err(native(NativeFailure::Correlation));
                        }
                        let cumulative = u64::try_from(total.total)
                            .map_err(|_| native(NativeFailure::Contract))?;
                        let delta = cumulative
                            .checked_sub(checkpoint.accounted_tokens.get())
                            .ok_or_else(|| native(NativeFailure::Contract))?;
                        let delta = contracts::DbCounter::new(delta)
                            .map_err(|_| native(NativeFailure::Contract))?;
                        if tokens.is_some_and(|prior| prior > delta) {
                            return Err(native(NativeFailure::Contract));
                        }
                        tokens = Some(delta);
                    }
                    Observation::ModelRerouted { .. } => {
                        return Err(native(NativeFailure::ModelUnavailable))
                    }
                    Observation::ThreadClosed { .. } => return Ok(TurnProgress::Unresolved),
                    Observation::LoginCompleted { .. } => {
                        return Err(native(NativeFailure::Correlation))
                    }
                }
            }
        }
    }
}

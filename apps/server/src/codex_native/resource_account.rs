//! Durable accounting for a single exact native resource, not a supervisor.
use super::{NativeFailure, Result};
use contracts::Id;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use store::{lifecycle::mission::resources::MissionResource, turns::WorkerFence, Store};

#[derive(Clone)]
pub(super) struct ResourceAccount {
    pub store: Store,
    pub fence: WorkerFence,
    pub resource: MissionResource,
    pub prior_nanoseconds: Option<u64>,
    pub grant_nanoseconds: Option<u64>,
    pub gate: Arc<tokio::sync::Mutex<()>>,
    pub closed: Arc<AtomicBool>,
}
impl ResourceAccount {
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
    pub fn mark_closed(&self) {
        self.closed.store(true, Ordering::Release);
    }
    pub fn exceeds(&self, nanos: u64) -> Result<bool> {
        budget_reached(self.prior_nanoseconds, nanos, self.grant_nanoseconds)
    }
    pub async fn begin_launch(&self) -> Result<()> {
        tokio::time::timeout(
            Duration::from_secs(2),
            self.store.begin_mission_resource_launch(
                self.resource.run_id,
                &self.fence,
                self.resource.id,
            ),
        )
        .await
        .map_err(|_| NativeFailure::Unavailable)?
        .map_err(|_| NativeFailure::Unavailable)
    }
    pub async fn begin_execution(&self) -> Result<()> {
        tokio::time::timeout(
            Duration::from_secs(2),
            self.store.begin_mission_resource_execution(
                self.resource.run_id,
                &self.fence,
                self.resource.id,
            ),
        )
        .await
        .map_err(|_| NativeFailure::Unavailable)?
        .map_err(|_| NativeFailure::Unavailable)
    }
    pub async fn abort_before_spawn(&self) -> Result<()> {
        self.checkpoint(Some(0), true, true).await?;
        self.mark_closed();
        Ok(())
    }
    pub async fn bind(&self, physical: &str) -> Result<()> {
        tokio::time::timeout(
            Duration::from_secs(2),
            self.store.bind_mission_resource(
                self.resource.run_id,
                &self.fence,
                self.resource.id,
                physical,
            ),
        )
        .await
        .map_err(|_| NativeFailure::Unavailable)?
        .map_err(|_| NativeFailure::Unavailable)
    }
    pub async fn checkpoint(
        &self,
        nanos: Option<u64>,
        final_accounted: bool,
        closed: bool,
    ) -> Result<bool> {
        use super::close_diagnostics::{failure, Phase};
        tokio::time::timeout(
            Duration::from_secs(2),
            self.store.checkpoint_mission_resource(
                self.resource.run_id,
                &self.fence,
                self.resource.id,
                nanos,
                final_accounted,
                closed,
            ),
        )
        .await
        .map_err(|_| failure(Phase::AccountCheckpointTimeout, NativeFailure::Unavailable))?
        .map_err(|_| failure(Phase::AccountCheckpointRejected, NativeFailure::Unavailable))
    }
}

pub(super) async fn pending(
    store: &Store,
    run: Id,
    fence: &WorkerFence,
) -> Result<Vec<MissionResource>> {
    tokio::time::timeout(
        Duration::from_secs(2),
        store.mission_open_resources(run, fence),
    )
    .await
    .map_err(|_| NativeFailure::Unavailable)?
    .map_err(|_| NativeFailure::Unavailable)
}

fn budget_reached(prior: Option<u64>, current: u64, grant: Option<u64>) -> Result<bool> {
    let Some(grant) = grant else {
        return Ok(false);
    };
    let prior = prior.ok_or(NativeFailure::Unavailable)?;
    Ok(prior.checked_add(current).is_none_or(|used| used >= grant))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequential_children_and_reopens_cannot_reset_the_cpu_grant() {
        // Aggregate kernel counters include already-exited child processes.
        let grant = 1_000_000_000;
        assert!(!budget_reached(None, u64::MAX, None).unwrap());
        assert!(budget_reached(None, 0, Some(grant)).is_err());
        assert!(!budget_reached(Some(0), 600_000_000, Some(grant)).unwrap());
        assert!(budget_reached(Some(0), 1_200_000_000, Some(grant)).unwrap());
        assert!(budget_reached(Some(600_000_000), 400_000_000, Some(grant)).unwrap());
        assert!(!budget_reached(Some(600_000_000), 399_999_999, Some(grant)).unwrap());
        assert!(budget_reached(Some(u64::MAX), 1, Some(grant)).unwrap());
    }
}

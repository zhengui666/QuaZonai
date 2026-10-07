//! A handle polled by the existing Mission driver. No spawned task or daemon.
use super::{container::Container, resources::ProcessGroup, Result};
use std::{sync::Arc, time::Duration};
use tokio::sync::{watch, Mutex};

enum Target {
    Host(ProcessGroup),
    Docker(Container),
}
#[derive(Clone)]
pub struct ResourceMonitor(Arc<Mutex<Target>>);
impl ResourceMonitor {
    pub(super) fn host(group: ProcessGroup) -> Self {
        Self(Arc::new(Mutex::new(Target::Host(group))))
    }
    pub(super) fn docker(container: Container) -> Self {
        Self(Arc::new(Mutex::new(Target::Docker(container))))
    }
    pub async fn enforce(&self) -> Result<()> {
        match &mut *self.0.lock().await {
            Target::Host(group) => group.check_cpu().await,
            Target::Docker(container) => container.check_cpu().await,
        }
    }
    pub async fn watch(mut receiver: watch::Receiver<Option<Self>>) -> Result<()> {
        loop {
            let current = receiver.borrow_and_update().clone();
            if let Some(current) = current {
                current.enforce().await?;
            }
            tokio::select! {
                _=tokio::time::sleep(Duration::from_millis(200))=>{},
                changed=receiver.changed()=>{ if changed.is_err() { return Err(super::NativeFailure::Closed); } },
            }
        }
    }
}

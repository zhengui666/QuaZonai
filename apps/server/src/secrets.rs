//! Local, purpose-limited reconciliation after failed or interrupted issuance.
use contracts::Id;
use integrations::secrets::SecretVault;
use std::sync::Arc;
use store::{Store, StoreError};

pub async fn reconcile_verifier(
    store: &Store,
    vault: Arc<SecretVault>,
    id: Id,
) -> Result<bool, StoreError> {
    store
        .reconcile_unpublished_verifier(id, move || async move {
            tokio::task::spawn_blocking(move || vault.remove_unpublished_verifier(id))
                .await
                .map_err(|_| StoreError::SecretCleanup)?
                .map_err(|_| StoreError::SecretCleanup)
        })
        .await
}

pub async fn prune_unpublished_verifiers(
    store: &Store,
    vault: Arc<SecretVault>,
) -> Result<usize, StoreError> {
    let scan = vault.clone();
    let candidates = tokio::task::spawn_blocking(move || scan.machine_verifier_ids())
        .await
        .map_err(|_| StoreError::SecretCleanup)?
        .map_err(|_| StoreError::SecretCleanup)?;
    let mut removed = 0;
    for id in candidates {
        removed += usize::from(reconcile_verifier(store, vault.clone(), id).await?);
    }
    Ok(removed)
}

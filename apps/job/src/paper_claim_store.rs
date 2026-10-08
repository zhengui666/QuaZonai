//! Durable claim admission, not native account recovery or execution evidence.
//!
//! The configured root is a stable runtime volume, independent of output paths.
//! Each adapter/project/downstream/handoff owns one OS lock and immutable claim.
//! A missing terminal observation after admission always requires reconciliation;
//! releasing the OS lock after a crash never grants permission to execute again.
use contracts::{strategy_portfolio::HandoffClaimViewV2, Id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const MAX_RECORD: u64 = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StoreError {
    Busy,
    Conflict,
    RecoveryRequired,
    LegacyUnsupported,
    Unavailable,
}

impl StoreError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Busy => "paper_claim_in_progress",
            Self::Conflict => "paper_claim_conflict",
            Self::RecoveryRequired => "paper_claim_recovery_required",
            Self::LegacyUnsupported => "paper_legacy_claim_not_executable",
            Self::Unavailable => "paper_claim_store_unavailable",
        }
    }
}

type Result<T> = std::result::Result<T, StoreError>;

#[derive(Clone)]
pub(crate) struct ClaimStore {
    root: PathBuf,
    adapter: &'static str,
}

pub(crate) enum Admission {
    Reserved(ClaimLease),
    Replay(Vec<u8>),
}

pub(crate) struct ClaimLease {
    path: PathBuf,
    handoff: Id,
    release: Id,
    external_claim: Option<String>,
    // Never unlink/recreate this lock: another process may already hold its inode.
    _lock: File,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginalClaim {
    schema_version: u8,
    adapter: String,
    claim: Value,
}

fn private_directory(path: &Path) -> Result<()> {
    private_directory_with_sync(path, sync_directory)
}

fn private_directory_with_sync(
    path: &Path,
    sync_parent: impl FnOnce(&Path) -> Result<()>,
) -> Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(StoreError::Unavailable),
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| StoreError::Unavailable)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(StoreError::Unavailable);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(StoreError::Unavailable);
        }
    }
    // An existing directory is not proof that its link is durable: a previous
    // caller may have failed fsync, or another opener may have just created it.
    // Every successful use syncs its parent. `open` and `claim_path` apply this
    // to the root and every adapter/project/downstream/handoff link in order.
    sync_parent(path.parent().ok_or(StoreError::Unavailable)?)
}

fn sync_directory(path: &Path) -> Result<()> {
    // The existing scientific runtime is Unix-only. Directory durability must
    // not be silently downgraded on unsupported filesystems/platforms.
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| StoreError::Unavailable)
}

fn read_record(path: &Path) -> Result<Option<Vec<u8>>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(StoreError::RecoveryRequired),
    };
    let metadata = file.metadata().map_err(|_| StoreError::RecoveryRequired)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_RECORD {
        return Err(StoreError::RecoveryRequired);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
            return Err(StoreError::RecoveryRequired);
        }
    }
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| StoreError::RecoveryRequired)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(StoreError::RecoveryRequired);
    }
    Ok(Some(bytes))
}

fn publish(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_RECORD {
        return Err(StoreError::Unavailable);
    }
    let parent = path.parent().ok_or(StoreError::Unavailable)?;
    let mut staged =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| StoreError::Unavailable)?;
    staged
        .write_all(bytes)
        .map_err(|_| StoreError::Unavailable)?;
    staged
        .as_file()
        .sync_all()
        .map_err(|_| StoreError::Unavailable)?;
    // Same no-replace atomic publication as managed Runtime outputs. A staging
    // file alone cannot authorize execution; publication and directory fsync
    // must both succeed before the request is handed to the native owner.
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged.path(),
        rustix::fs::CWD,
        path,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(|_| StoreError::Unavailable)?;
    sync_directory(parent)
}

impl ClaimStore {
    pub(crate) fn open(root: &Path, adapter: &'static str) -> Result<Self> {
        if !root.is_absolute()
            || root
                .components()
                .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
            || !matches!(adapter, "binance" | "polymarket")
        {
            return Err(StoreError::Unavailable);
        }
        private_directory(root)?;
        if fs::canonicalize(root).map_err(|_| StoreError::Unavailable)? != root {
            return Err(StoreError::Unavailable);
        }
        Ok(Self {
            root: root.to_owned(),
            adapter,
        })
    }

    fn claim_path(&self, claim: &HandoffClaimViewV2) -> Result<PathBuf> {
        let mut path = self.root.clone();
        for component in [
            self.adapter.to_owned(),
            claim.handoff.project_id.to_string(),
            claim.handoff.downstream_id.to_string(),
            claim.handoff.id.to_string(),
        ] {
            path.push(component);
            private_directory(&path)?;
        }
        Ok(path)
    }

    /// Lookup is allowed for expired originals so receipt replay remains useful.
    /// `allow_new` is calculated by the existing claim/capability validation.
    pub(crate) fn admit(
        &self,
        claim: &HandoffClaimViewV2,
        allow_new: bool,
    ) -> Result<Option<Admission>> {
        let path = self.claim_path(claim)?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let lock = options
            .open(path.join("owner.lock"))
            .map_err(|_| StoreError::Unavailable)?;
        let metadata = lock.metadata().map_err(|_| StoreError::Unavailable)?;
        if !metadata.is_file() {
            return Err(StoreError::Unavailable);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 || metadata.mode() & 0o077 != 0 {
                return Err(StoreError::Unavailable);
            }
        }
        match rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(rustix::io::Errno::WOULDBLOCK) => return Err(StoreError::Busy),
            Err(_) => return Err(StoreError::Unavailable),
        }
        let lease = ClaimLease {
            path,
            handoff: claim.handoff.id,
            release: claim.handoff.release_id,
            external_claim: claim.handoff.external_claim_id.clone(),
            _lock: lock,
        };
        let value = serde_json::to_value(claim).map_err(|_| StoreError::Unavailable)?;
        if let Some(original) = read_record(&lease.path.join("claim.json"))? {
            let original: OriginalClaim =
                serde_json::from_slice(&original).map_err(|_| StoreError::RecoveryRequired)?;
            if original.schema_version != 1 || original.adapter != self.adapter {
                return Err(StoreError::RecoveryRequired);
            }
            // Preserve original bytes for diagnosis. A successful old terminal
            // record is never upgraded into permission to replay/execute V2.
            if original.claim["package"]["package_schema_version"] == "1" {
                return Err(StoreError::LegacyUnsupported);
            }
            if original.claim["package"]["package_schema_version"] != "2" {
                return Err(StoreError::RecoveryRequired);
            }
            if original.claim != value {
                return Err(StoreError::Conflict);
            }
            let receipt = read_record(&lease.path.join("terminal-status.json"))?
                .ok_or(StoreError::RecoveryRequired)?;
            lease.validate_receipt(&receipt)?;
            sync_directory(&lease.path)?;
            return Ok(Some(Admission::Replay(receipt)));
        }
        // An orphan terminal file is corruption, never a reason to re-execute.
        if read_record(&lease.path.join("terminal-status.json"))?.is_some() {
            return Err(StoreError::RecoveryRequired);
        }
        if !allow_new {
            return Ok(None);
        }
        let original = serde_json::to_vec(&OriginalClaim {
            schema_version: 1,
            adapter: self.adapter.into(),
            claim: value,
        })
        .map_err(|_| StoreError::Unavailable)?;
        publish(&lease.path.join("claim.json"), &original)?;
        Ok(Some(Admission::Reserved(lease)))
    }
}

impl ClaimLease {
    fn validate_receipt(&self, bytes: &[u8]) -> Result<()> {
        // Validate the complete, versioned wire type, not only identity keys.
        // Deserialization requires every field (including explicit null options)
        // and rejects unknown fields, invalid enum values, dates and types.
        let status: crate::paper_service::PaperStatus =
            serde_json::from_slice(bytes).map_err(|_| StoreError::RecoveryRequired)?;
        if !matches!(
            status.state,
            crate::paper_service::PaperState::Stopped | crate::paper_service::PaperState::Failed
        ) || status.handoff_id != Some(self.handoff)
            || status.release_id != Some(self.release)
            || status.external_claim_id != self.external_claim
        {
            return Err(StoreError::RecoveryRequired);
        }
        Ok(())
    }

    /// Only the owner calls this after native termination/join. It records the
    /// actual terminal lifecycle response, including failure and cancellation.
    pub(crate) fn complete(&self, receipt: &[u8]) -> Result<()> {
        self.validate_receipt(receipt)?;
        let path = self.path.join("terminal-status.json");
        if let Some(existing) = read_record(&path)? {
            return if existing == receipt {
                sync_directory(&self.path)
            } else {
                Err(StoreError::Conflict)
            };
        }
        publish(&path, receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_service::{
        tests::{claim, private_state_directory},
        PaperProfile, PaperState, PaperStatus,
    };
    use std::{
        process::Command,
        sync::{Arc, Barrier},
    };

    fn reserve(store: &ClaimStore, claim: &HandoffClaimViewV2) -> ClaimLease {
        match store.admit(claim, true).unwrap().unwrap() {
            Admission::Reserved(lease) => lease,
            Admission::Replay(_) => panic!("unexpected replay"),
        }
    }

    fn receipt(claim: &HandoffClaimViewV2, state: PaperState) -> Vec<u8> {
        serde_json::to_vec(&PaperStatus {
            state,
            handoff_id: Some(claim.handoff.id),
            release_id: Some(claim.handoff.release_id),
            external_claim_id: claim.handoff.external_claim_id.clone(),
            native_session_id: Some("observed-original-session".into()),
            target_points_consumed: 3,
            ..PaperStatus::idle_with_profile(PaperProfile::Binance)
        })
        .unwrap()
    }

    #[test]
    fn existing_directory_retries_failed_parent_sync_at_every_scope_link() {
        let root = private_state_directory();
        let mut path = root.path().to_owned();
        // The same primitive protects the stable root and every claim scope.
        for component in ["state", "polymarket", "project", "downstream", "handoff"] {
            path.push(component);
            let parent = path.parent().unwrap().to_owned();
            assert_eq!(
                private_directory_with_sync(&path, |actual| {
                    assert_eq!(actual, parent);
                    Err(StoreError::Unavailable)
                }),
                Err(StoreError::Unavailable)
            );
            assert!(path.is_dir());
            // A visible directory from the failed attempt must still invoke
            // parent fsync; another failure must not be accepted as durable.
            assert_eq!(
                private_directory_with_sync(&path, |actual| {
                    assert_eq!(actual, parent);
                    Err(StoreError::Unavailable)
                }),
                Err(StoreError::Unavailable)
            );
            let mut synced = false;
            private_directory_with_sync(&path, |actual| {
                assert_eq!(actual, parent);
                sync_directory(actual)?;
                synced = true;
                Ok(())
            })
            .unwrap();
            assert!(synced);
        }
    }

    #[test]
    fn concurrent_opener_syncs_parent_while_creator_has_not_completed_sync() {
        let root = private_state_directory();
        let path = root.path().join("concurrent-directory");
        let barrier = Arc::new(Barrier::new(2));
        let creator_path = path.clone();
        let creator_barrier = barrier.clone();
        let creator = std::thread::spawn(move || {
            private_directory_with_sync(&creator_path, |_| {
                creator_barrier.wait();
                creator_barrier.wait();
                Err(StoreError::Unavailable)
            })
        });
        barrier.wait();
        let mut synced = false;
        let result = private_directory_with_sync(&path, |parent| {
            assert_eq!(parent, root.path());
            sync_directory(parent)?;
            synced = true;
            Ok(())
        });
        barrier.wait();
        assert_eq!(creator.join().unwrap(), Err(StoreError::Unavailable));
        result.unwrap();
        assert!(synced);
    }

    #[test]
    fn full_versioned_status_is_required_even_with_matching_identity_fields() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let lease = reserve(&store, &original);
        let full: Value = serde_json::from_slice(&receipt(&original, PaperState::Stopped)).unwrap();
        assert_eq!(full["schema_version"], 1);
        let mut invalid = Vec::new();
        for key in full.as_object().unwrap().keys() {
            let mut missing = full.clone();
            missing.as_object_mut().unwrap().remove(key);
            invalid.push(missing);
            let mut wrong_type = full.clone();
            wrong_type[key] = serde_json::json!({"unexpected":true});
            invalid.push(wrong_type);
        }
        for (key, value) in [
            ("schema_version", serde_json::json!(2)),
            ("schema_version", serde_json::json!("1")),
            ("updated_at", serde_json::json!("not-a-timestamp")),
            ("state", serde_json::json!("complete")),
            ("target_points_consumed", serde_json::json!(-1)),
            ("stop_requested", serde_json::json!("false")),
            ("unexpected_extension", serde_json::json!(true)),
        ] {
            let mut changed = full.clone();
            changed[key] = value;
            invalid.push(changed);
        }
        invalid.push(serde_json::json!({
            "state":"stopped", "handoff_id":original.handoff.id,
            "release_id":original.handoff.release_id,
            "external_claim_id":original.handoff.external_claim_id,
        }));
        for changed in &invalid {
            assert_eq!(
                lease.complete(&serde_json::to_vec(changed).unwrap()),
                Err(StoreError::RecoveryRequired)
            );
        }
        assert!(!lease.path.join("terminal-status.json").exists());
        let path = lease.path.join("terminal-status.json");
        lease.complete(&serde_json::to_vec(&full).unwrap()).unwrap();
        drop(lease);
        // Existing corrupted records receive the same complete validation.
        for changed in invalid {
            fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
            assert!(matches!(
                store.admit(&original, true),
                Err(StoreError::RecoveryRequired)
            ));
        }
    }

    #[test]
    fn complete_status_validation_keeps_original_response_bytes() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let lease = reserve(&store, &original);
        let full: Value = serde_json::from_slice(&receipt(&original, PaperState::Failed)).unwrap();
        let bytes = format!("{}\n", serde_json::to_string_pretty(&full).unwrap()).into_bytes();
        lease.complete(&bytes).unwrap();
        drop(lease);
        match store.admit(&original, false).unwrap().unwrap() {
            Admission::Replay(actual) => assert_eq!(actual, bytes),
            Admission::Reserved(_) => panic!("must replay original bytes"),
        }
    }

    #[test]
    fn restart_of_incomplete_claim_requires_recovery_but_other_claims_are_independent() {
        let root = private_state_directory();
        let original = claim();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let lease = reserve(&store, &original);
        assert!(matches!(
            store.admit(&original, true),
            Err(StoreError::Busy)
        ));
        let other = claim();
        let other_lease = reserve(&store, &other);
        drop(lease);
        let restarted = ClaimStore::open(root.path(), "binance").unwrap();
        assert!(matches!(
            restarted.admit(&original, true),
            Err(StoreError::RecoveryRequired)
        ));
        drop(other_lease);
    }

    #[test]
    fn completed_receipt_replays_exact_bytes_after_restart_even_when_claim_is_expired() {
        for state in [PaperState::Stopped, PaperState::Failed] {
            let root = private_state_directory();
            let original = claim();
            let store = ClaimStore::open(root.path(), "polymarket").unwrap();
            let lease = reserve(&store, &original);
            let bytes = receipt(&original, state);
            lease.complete(&bytes).unwrap();
            lease.complete(&bytes).unwrap();
            drop(lease);
            for _ in 0..3 {
                let restarted = ClaimStore::open(root.path(), "polymarket").unwrap();
                match restarted.admit(&original, false).unwrap().unwrap() {
                    Admission::Replay(actual) => assert_eq!(actual, bytes),
                    Admission::Reserved(_) => panic!("replay must never reserve an execution"),
                }
            }
        }
    }

    #[test]
    fn changed_original_is_conflict_and_cannot_replace_completed_receipt() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let lease = reserve(&store, &original);
        let bytes = receipt(&original, PaperState::Stopped);
        lease.complete(&bytes).unwrap();
        assert_eq!(
            lease.complete(&receipt(&original, PaperState::Failed)),
            Err(StoreError::Conflict)
        );
        drop(lease);
        let mut changed = original;
        changed.handoff.external_claim_id = Some("changed".into());
        assert!(matches!(
            store.admit(&changed, true),
            Err(StoreError::Conflict)
        ));
    }

    #[test]
    fn completion_rejects_nonterminal_or_unrelated_observations() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let lease = reserve(&store, &original);
        for state in [PaperState::Idle, PaperState::Starting, PaperState::Running] {
            assert_eq!(
                lease.complete(&receipt(&original, state)),
                Err(StoreError::RecoveryRequired)
            );
        }
        assert_eq!(
            lease.complete(&receipt(&claim(), PaperState::Stopped)),
            Err(StoreError::RecoveryRequired)
        );
        assert!(!lease.path.join("terminal-status.json").exists());
    }

    #[test]
    fn malformed_or_partial_claim_and_terminal_records_fail_closed() {
        for (name, bytes) in [
            ("claim.json", b"{\"schema_version\":".as_slice()),
            ("claim.json", b"".as_slice()),
            (
                "terminal-status.json",
                b"{\"state\":\"stopped\"}".as_slice(),
            ),
            ("terminal-status.json", b"{\"state\":\"stop".as_slice()),
        ] {
            let root = private_state_directory();
            let store = ClaimStore::open(root.path(), "binance").unwrap();
            let original = claim();
            let lease = reserve(&store, &original);
            let path = lease.path.join(name);
            drop(lease);
            let mut options = OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path).unwrap().write_all(bytes).unwrap();
            assert!(matches!(
                store.admit(&original, true),
                Err(StoreError::RecoveryRequired)
            ));
        }
    }

    #[test]
    fn abandoned_staging_file_is_not_admission_or_a_receipt() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let path = store.claim_path(&original).unwrap();
        fs::write(path.join(".tmp-abandoned"), b"{partial").unwrap();
        let lease = reserve(&store, &original);
        drop(lease);
        fs::write(
            path.join(".tmp-abandoned-terminal"),
            receipt(&original, PaperState::Stopped),
        )
        .unwrap();
        assert!(matches!(
            store.admit(&original, true),
            Err(StoreError::RecoveryRequired)
        ));
    }

    #[test]
    fn final_publication_failure_preserves_reservation() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let lease = reserve(&store, &original);
        fs::create_dir(lease.path.join("terminal-status.json")).unwrap();
        assert!(lease
            .complete(&receipt(&original, PaperState::Stopped))
            .is_err());
        drop(lease);
        assert!(matches!(
            store.admit(&original, true),
            Err(StoreError::RecoveryRequired)
        ));
    }

    #[test]
    fn concurrent_openers_admit_exactly_one_native_owner() {
        let root = private_state_directory();
        let original = claim();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (store, original, barrier) = (store.clone(), original.clone(), barrier.clone());
                std::thread::spawn(move || {
                    barrier.wait();
                    let result = store.admit(&original, true);
                    // Keep the winner's lock alive until every contender tried it.
                    barrier.wait();
                    match result {
                        Ok(Some(Admission::Reserved(_))) => true,
                        Err(StoreError::Busy) => false,
                        _ => panic!("unexpected concurrent admission outcome"),
                    }
                })
            })
            .collect();
        assert_eq!(
            threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .filter(|won| *won)
                .count(),
            1
        );
    }

    #[test]
    fn adapter_project_and_downstream_scope_are_separate() {
        let root = private_state_directory();
        let original = claim();
        let binance = ClaimStore::open(root.path(), "binance").unwrap();
        let polymarket = ClaimStore::open(root.path(), "polymarket").unwrap();
        let a = reserve(&binance, &original);
        let b = reserve(&polymarket, &original);
        let mut other = original.clone();
        other.handoff.project_id = Id::new();
        let c = reserve(&binance, &other);
        other = original;
        other.handoff.downstream_id = Id::new();
        let d = reserve(&binance, &other);
        drop((a, b, c, d));
    }

    #[test]
    fn invalid_new_claim_does_not_create_a_durable_reservation() {
        let root = private_state_directory();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        assert!(store.admit(&original, false).unwrap().is_none());
        assert!(!store
            .claim_path(&original)
            .unwrap()
            .join("claim.json")
            .exists());
        drop(reserve(&store, &original));
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_roots_and_records_are_rejected() {
        use std::os::unix::fs::symlink;
        let root = private_state_directory();
        let alias = root.path().join("alias");
        symlink(root.path(), &alias).unwrap();
        assert!(ClaimStore::open(&alias, "binance").is_err());
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let original = claim();
        let path = store.claim_path(&original).unwrap();
        let target = root.path().join("external");
        fs::write(&target, b"{}").unwrap();
        symlink(&target, path.join("claim.json")).unwrap();
        assert!(matches!(
            store.admit(&original, true),
            Err(StoreError::RecoveryRequired)
        ));
        assert_eq!(fs::read(&target).unwrap(), b"{}");
    }

    #[test]
    fn legacy_success_journal_is_retained_but_never_replayed_or_upgraded() {
        let root = private_state_directory();
        let claim = claim();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let path = store.claim_path(&claim).unwrap();
        let mut old = serde_json::to_value(&claim).unwrap();
        old["package"]["package_schema_version"] = serde_json::json!("1");
        let original = serde_json::to_vec(&OriginalClaim {
            schema_version: 1,
            adapter: "binance".into(),
            claim: old,
        })
        .unwrap();
        let terminal = receipt(&claim, PaperState::Stopped);
        publish(&path.join("claim.json"), &original).unwrap();
        publish(&path.join("terminal-status.json"), &terminal).unwrap();
        for _ in 0..2 {
            let restarted = ClaimStore::open(root.path(), "binance").unwrap();
            assert!(matches!(
                restarted.admit(&claim, true),
                Err(StoreError::LegacyUnsupported)
            ));
            assert_eq!(fs::read(path.join("claim.json")).unwrap(), original);
            assert_eq!(
                fs::read(path.join("terminal-status.json")).unwrap(),
                terminal
            );
        }
    }

    #[test]
    fn subprocess_lock_probe() {
        let Some(root) = std::env::var_os("QZ_CLAIM_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let original: HandoffClaimViewV2 =
            serde_json::from_slice(&fs::read(root.join("input.json")).unwrap()).unwrap();
        let store = ClaimStore::open(&root, "binance").unwrap();
        match std::env::var("QZ_CLAIM_TEST_MODE").unwrap().as_str() {
            "busy" => assert!(matches!(
                store.admit(&original, true),
                Err(StoreError::Busy)
            )),
            "recover" => assert!(matches!(
                store.admit(&original, true),
                Err(StoreError::RecoveryRequired)
            )),
            "crash" => {
                let _lease = reserve(&store, &original);
                // Process exit skips Rust destructors, as an interrupted owner does.
                std::process::exit(0);
            }
            _ => panic!("invalid test mode"),
        }
    }

    fn child(root: &Path, mode: &str) {
        assert!(Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "paper_claim_store::tests::subprocess_lock_probe",
                "--nocapture"
            ])
            .env("QZ_CLAIM_TEST_ROOT", root)
            .env("QZ_CLAIM_TEST_MODE", mode)
            .status()
            .unwrap()
            .success());
    }

    #[test]
    fn actual_process_lock_and_crash_recovery_are_fail_closed() {
        let root = private_state_directory();
        let original = claim();
        fs::write(
            root.path().join("input.json"),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        let store = ClaimStore::open(root.path(), "binance").unwrap();
        let lease = reserve(&store, &original);
        child(root.path(), "busy");
        drop(lease);
        child(root.path(), "recover");
        let crashed_root = private_state_directory();
        fs::write(
            crashed_root.path().join("input.json"),
            serde_json::to_vec(&original).unwrap(),
        )
        .unwrap();
        child(crashed_root.path(), "crash");
        child(crashed_root.path(), "recover");
    }
}

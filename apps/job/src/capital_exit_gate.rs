//! Durable owner-thread capital fence. This is control metadata, never an order
//! ledger or native recovery engine. Existing records always restart quarantined.
use anyhow::{Result, anyhow, ensure};
use contracts::{DbCounter, Id, capital_exit::CapitalExitViewV1};
use nautilus_model::orders::{Order, OrderAny};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone)]
pub struct CapitalExitGate(Rc<RefCell<Gate>>);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    managed_account_key: String,
    owner_binding_ref: String,
    native_account_id: String,
    /// The full original claimed server control, not a locally recomputed budget.
    control: Option<CapitalExitViewV1>,
    /// Original IDs written before submit. Missing native recovery never retries.
    issued_order_ids: Vec<String>,
    target_claim_refs: Vec<String>,
    /// IDs captured before the first fence ACK, never a parallel order state.
    #[serde(default)]
    opening_order_ids: Option<Vec<String>>,
    #[serde(default)]
    paper_source_id: Option<Id>,
}

struct Gate {
    path: PathBuf,
    _lock: File,
    record: Record,
    recovery_required: bool,
    write_failed: bool,
}

impl CapitalExitGate {
    /// Trusted host configuration only; not a network DTO. Use one stable private
    /// volume for the physical account, independent of source/project/session.
    /// This permits a fresh controlled Sandbox only. A previous boot requires
    /// official native reconciliation, which this slice deliberately does not fake.
    pub fn open_fresh_sandbox(
        root: &Path,
        managed_account_key: &str,
        owner_binding_ref: &str,
        native_account_id: &str,
    ) -> Result<Self> {
        Self::open_fresh(
            root,
            managed_account_key,
            owner_binding_ref,
            native_account_id,
            native_account_id,
        )
    }

    /// A Paper engine is an isolated simulated account. The original claimed
    /// handoff is its durable boot scope; account-label reuse by another claim
    /// must not merge independent simulations. The host supplies the checked
    /// original claim, never a user-selected control request or filesystem alias.
    pub(crate) fn open_fresh_paper(
        root: &Path,
        managed_account_key: &str,
        owner_binding_ref: &str,
        native_account_id: &str,
        claim: &contracts::strategy_portfolio::HandoffClaimViewV2,
    ) -> Result<Self> {
        ensure!(
            claim.handoff.environment == contracts::forward::ForwardEnvironmentV1::Paper
                && claim.handoff.state == contracts::delivery::HandoffStateV1::Claimed
                && claim
                    .handoff
                    .external_claim_id
                    .as_ref()
                    .is_some_and(|id| !id.is_empty()),
            "capital_exit_original_target_claim_required"
        );
        Self::open_fresh(
            root,
            managed_account_key,
            owner_binding_ref,
            native_account_id,
            &format!("paper-claim:{}:{}", claim.handoff.id, native_account_id),
        )
    }

    fn open_fresh(
        root: &Path,
        managed_account_key: &str,
        owner_binding_ref: &str,
        native_account_id: &str,
        storage_identity: &str,
    ) -> Result<Self> {
        ensure!(root.is_absolute(), "capital_exit_stable_volume_required");
        for value in [managed_account_key, owner_binding_ref, native_account_id] {
            ensure!(
                !value.is_empty() && value.len() <= 200,
                "capital_exit_owner_binding_invalid"
            );
        }
        let metadata = fs::symlink_metadata(root)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "capital_exit_stable_volume_required"
        );
        // A different key/session/project cannot get a new allowance for this account.
        let physical = storage_identity
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let path = root.join(format!("sandbox-cash-{physical}.json"));
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
        }
        let lock = options.open(root.join(format!("sandbox-cash-{physical}.lock")))?;
        lock.try_lock()
            .map_err(|_| anyhow!("capital_exit_owner_already_running"))?;
        let existing = path.try_exists()?;
        let record = if existing {
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= 4 * 1024 * 1024,
                "capital_exit_owner_recovery_required"
            );
            let record: Record = serde_json::from_slice(&fs::read(&path)?)?;
            ensure!(
                record.managed_account_key == managed_account_key
                    && record.owner_binding_ref == owner_binding_ref
                    && record.native_account_id == native_account_id,
                "capital_exit_owner_binding_conflict"
            );
            record
        } else {
            Record {
                managed_account_key: managed_account_key.into(),
                owner_binding_ref: owner_binding_ref.into(),
                native_account_id: native_account_id.into(),
                control: None,
                issued_order_ids: vec![],
                target_claim_refs: vec![],
                opening_order_ids: None,
                paper_source_id: None,
            }
        };
        let mut gate = Gate {
            path,
            _lock: lock,
            record,
            recovery_required: existing,
            write_failed: false,
        };
        // Even a crash before the first fence prevents replacing the account.
        gate.persist()?;
        Ok(Self(Rc::new(RefCell::new(gate))))
    }

    pub fn epoch(&self) -> DbCounter {
        self.0
            .borrow()
            .record
            .control
            .as_ref()
            .map_or(DbCounter::ZERO, |v| v.account_control_epoch)
    }
    pub fn recovery_required(&self) -> bool {
        let gate = self.0.borrow();
        gate.recovery_required || gate.write_failed
    }
    pub fn control(&self) -> Option<CapitalExitViewV1> {
        self.0.borrow().record.control.clone()
    }

    /// Only the checked Paper adapter calls this after the authenticated native
    /// intake echoes its retained original observation. A source is bound once,
    /// before capital commands, and survives in the same original-claim journal.
    pub(crate) fn bind_paper_source(&self, source: Id) -> Result<()> {
        let mut gate = self.0.borrow_mut();
        ensure!(
            !gate.recovery_required && !gate.write_failed,
            "capital_exit_owner_recovery_required"
        );
        if let Some(original) = gate.record.paper_source_id {
            ensure!(original == source, "capital_exit_source_binding_conflict");
            return Ok(());
        }
        ensure!(
            gate.record.control.is_none() && gate.record.issued_order_ids.is_empty(),
            "capital_exit_source_binding_conflict"
        );
        gate.record.paper_source_id = Some(source);
        gate.record.managed_account_key = format!("paper-native:{source}");
        gate.record.owner_binding_ref = format!("nautilus-paper:{source}");
        gate.persist()
    }
    /// Existing host claim validation and V2 qualification remain mandatory. This
    /// stores only original authority references for invalidation, never a target.
    pub fn capture_target_claim(
        &self,
        claim: &contracts::strategy_portfolio::HandoffClaimViewV2,
    ) -> Result<DbCounter> {
        ensure!(
            claim.handoff.state == contracts::delivery::HandoffStateV1::Claimed,
            "capital_exit_original_target_claim_required"
        );
        let external = claim
            .handoff
            .external_claim_id
            .as_ref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("capital_exit_original_target_claim_required"))?;
        let mut gate = self.0.borrow_mut();
        ensure!(
            !gate.recovery_required && !gate.write_failed && gate.record.control.is_none(),
            "capital_exit_remaining_target_unavailable"
        );
        let reference = format!("{}:{}", claim.handoff.id, external);
        if !gate.record.target_claim_refs.contains(&reference) {
            ensure!(
                gate.record.target_claim_refs.len() < 1024,
                "capital_exit_target_claim_limit"
            );
            gate.record.target_claim_refs.push(reference);
            gate.persist()?;
        }
        Ok(DbCounter::ZERO)
    }
    pub(crate) fn original_target_claim_refs(&self) -> Vec<String> {
        self.0.borrow().record.target_claim_refs.clone()
    }

    pub(crate) fn opening_order_ids(&self) -> Option<Vec<String>> {
        self.0.borrow().record.opening_order_ids.clone()
    }
    pub(crate) fn retain_original_openers(&self, ids: Vec<String>) -> Result<()> {
        let mut gate = self.0.borrow_mut();
        ensure!(
            !gate.recovery_required && !gate.write_failed,
            "capital_exit_owner_recovery_required"
        );
        if gate.record.opening_order_ids.is_some() {
            return Ok(());
        }
        ensure!(ids.len() <= 4096, "capital_exit_native_order_scope_limit");
        gate.record.opening_order_ids = Some(ids);
        gate.persist()
    }

    pub(crate) fn retained_control_report(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.0.borrow().record)?)
    }

    pub fn issued_order_ids(&self) -> Vec<String> {
        self.0.borrow().record.issued_order_ids.clone()
    }

    /// Every old/new target callback uses its captured epoch immediately before
    /// submit. No pause/cancel/resume method clears this fence or allows buyback.
    pub fn check_target(&self, claimed_epoch: DbCounter, order: &OrderAny) -> Result<()> {
        let gate = self.0.borrow();
        ensure!(
            !gate.recovery_required && !gate.write_failed,
            "capital_exit_owner_recovery_required"
        );
        let epoch = gate
            .record
            .control
            .as_ref()
            .map_or(DbCounter::ZERO, |v| v.account_control_epoch);
        ensure!(claimed_epoch == epoch, "capital_exit_obsolete_target_epoch");
        ensure!(
            order.exec_algorithm_id().is_none()
                && order.exec_spawn_id().is_none()
                && order.emulation_trigger().is_none()
                && order.parent_order_id().is_none(),
            "capital_exit_child_path_unsupported"
        );
        if gate.record.control.is_some() {
            // A new qualified remaining target is not yet supported. Previously
            // submitted native protection is left untouched by this admission gate.
            return Err(anyhow!("capital_exit_remaining_target_unavailable"));
        }
        Ok(())
    }

    /// Original owner evidence is durable before network transmission. These
    /// immutable reports are an outbox for exact replay, never an order ledger.
    pub(crate) fn retain_original_message(
        &self,
        kind: &str,
        sequence: DbCounter,
        value: &impl Serialize,
    ) -> Result<()> {
        ensure!(
            matches!(kind, "assessment" | "evidence"),
            "capital_exit_evidence_kind"
        );
        let mut gate = self.0.borrow_mut();
        ensure!(
            !gate.recovery_required && !gate.write_failed,
            "capital_exit_owner_recovery_required"
        );
        let path = gate
            .path
            .with_extension(format!("{kind}-{}.json", sequence.get()));
        let bytes = serde_json::to_vec(value)?;
        ensure!(
            bytes.len() <= 8 * 1024 * 1024,
            "capital_exit_evidence_size_limit"
        );
        if path.try_exists()? {
            ensure!(
                fs::read(path)? == bytes,
                "capital_exit_evidence_replay_conflict"
            );
            return Ok(());
        }
        let result = (|| -> Result<()> {
            let root = gate
                .path
                .parent()
                .ok_or_else(|| anyhow!("capital_exit_stable_volume_required"))?;
            let mut temporary = tempfile::NamedTempFile::new_in(root)?;
            temporary.write_all(&bytes)?;
            temporary.as_file().sync_all()?;
            temporary.persist_noclobber(path).map_err(|e| e.error)?;
            File::open(root)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            gate.write_failed = true;
        }
        result
    }

    pub(crate) fn apply_control(&self, control: &CapitalExitViewV1) -> Result<()> {
        let mut gate = self.0.borrow_mut();
        ensure!(
            !gate.recovery_required && !gate.write_failed,
            "capital_exit_owner_recovery_required"
        );
        ensure!(
            control.managed_account_key == gate.record.managed_account_key
                && control.owner_binding_ref == gate.record.owner_binding_ref
                && control
                    .external_claim_id
                    .as_ref()
                    .is_some_and(|v| !v.is_empty())
                && control.account_control_epoch.get() > 0,
            "capital_exit_claim_binding_mismatch"
        );
        if let Some(previous) = &gate.record.control {
            ensure!(
                previous.id == control.id
                    && control.account_control_epoch >= previous.account_control_epoch,
                "capital_exit_control_epoch_regression"
            );
            if previous.account_control_epoch == control.account_control_epoch {
                ensure!(
                    same_control_binding(previous, control),
                    "capital_exit_control_replay_conflict"
                );
                // An exact command replay can carry newer progress, but must not
                // overwrite the original immutable authority with a full view.
                return Ok(());
            }
        }
        gate.record.control = Some(control.clone());
        gate.persist()
    }

    pub(crate) fn record_order_before_submit(&self, intent: Id, id: &str) -> Result<()> {
        let mut gate = self.0.borrow_mut();
        ensure!(
            !gate.recovery_required
                && !gate.write_failed
                && gate.record.control.as_ref().is_some_and(|v| v.id == intent),
            "capital_exit_owner_recovery_required"
        );
        ensure!(
            !gate.record.issued_order_ids.iter().any(|v| v == id)
                && gate.record.issued_order_ids.len() < 1024,
            "capital_exit_original_order_reconciliation_required"
        );
        gate.record.issued_order_ids.push(id.into());
        gate.persist()
    }
}

/// One authority predicate for both durable refencing and synchronous submit.
/// View revision/state/reasons/evidence and financial *progress* are not authority.
pub(crate) fn same_control_binding(a: &CapitalExitViewV1, b: &CapitalExitViewV1) -> bool {
    a.id == b.id
        && a.project_id == b.project_id
        && a.account_source_id == b.account_source_id
        && a.environment == b.environment
        && a.managed_account_key == b.managed_account_key
        && a.owner_binding_ref == b.owner_binding_ref
        && a.command_id == b.command_id
        && a.account_control_epoch == b.account_control_epoch
        && a.account_control_revision == b.account_control_revision
        && a.owner_command == b.owner_command
        && a.external_claim_id == b.external_claim_id
        && a.scope == b.scope
        && a.policy == b.policy
        && a.preview_id == b.preview_id
        && a.plan_artifact_id == b.plan_artifact_id
        && a.funds.requested_amount == b.funds.requested_amount
        && a.funds.reserved_amount == b.funds.reserved_amount
}

impl Gate {
    fn persist(&mut self) -> Result<()> {
        let result = (|| -> Result<()> {
            let root = self
                .path
                .parent()
                .ok_or_else(|| anyhow!("capital_exit_stable_volume_required"))?;
            let mut temporary = tempfile::NamedTempFile::new_in(root)?;
            temporary.write_all(&serde_json::to_vec(&self.record)?)?;
            temporary.as_file().sync_all()?;
            temporary.persist(&self.path).map_err(|e| e.error)?;
            File::open(root)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            self.write_failed = true;
        }
        result
    }
}

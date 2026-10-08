//! DTO dispatch on the existing Polymarket engine thread. The asynchronous
//! transport never receives a native handle; the native side never waits on HTTP.
use anyhow::{Result, anyhow, ensure};
use contracts::{
    DbCounter, Id, SchemaV1, account_observation::*, capital_exit::*,
    strategy_portfolio::HandoffClaimViewV2,
};
use nautilus_backtest::engine::BacktestEngine;
use nautilus_common::actor::registry::try_get_actor_unchecked;
use nautilus_execution::models::fee::FeeModelHandle;
use nautilus_model::identifiers::{AccountId, ClientId, InstrumentId, StrategyId};
use std::{fs::File, path::Path, sync::mpsc};

use crate::{
    account_observer::NativeAccountObserver, native_capital_exit::NativeCapitalExitOwner,
    simulation::TargetReplay,
};

pub(crate) enum NativeRequest {
    Observe,
    Bind(Box<AccountObservationReceiptV2>),
    Assess(Box<CapitalExitPreviewV1>),
    Fence(Box<CapitalExitViewV1>),
    Advance(Box<CapitalExitViewV1>),
    Availability(Box<CapitalExitViewV1>),
}

pub(crate) enum NativeResponse {
    Observation(AccountObservationSubmitV2),
    Bound(Id),
    Assessment(CapitalExitOwnerAssessmentV1),
    Evidence(CapitalExitOwnerEvidenceV1),
}

pub(crate) struct NativePaperCapitalExit {
    owner: NativeCapitalExitOwner,
    observer: NativeAccountObserver,
    account_id: AccountId,
    client_id: ClientId,
    observation_output: File,
    pending_observation: Option<AccountObservationSubmitV2>,
    source_observation_id: Option<Id>,
    sequence: u64,
    deadline_requested: Option<Id>,
    downstream_id: Id,
}

impl NativePaperCapitalExit {
    pub(crate) fn attach(
        engine: &BacktestEngine,
        instrument: InstrumentId,
        settings: &contracts::science::NativeSimulationSettingsV1,
        fee_model: FeeModelHandle,
        claim: &HandoffClaimViewV2,
        stable_volume: &Path,
    ) -> Result<Self> {
        let owner = NativeCapitalExitOwner::attach_streaming_paper(
            engine,
            instrument,
            settings,
            fee_model,
            claim,
            stable_volume,
        )?;
        let client_id = ClientId::from("POLYMARKET");
        let account_id = engine
            .kernel()
            .exec_engine
            .borrow()
            .get_client(&client_id)
            .ok_or_else(|| anyhow!("capital_exit_owner_client_unavailable"))?
            .account_id();
        let binding = NativeAccountBindingV1 {
            schema_version: SchemaV1,
            project_id: claim.handoff.project_id,
            environment: contracts::forward::ForwardEnvironmentV1::Paper,
            native_trader_id: engine.trader_id().to_string(),
            native_session_id: engine.instance_id().to_string(),
            native_account_id: account_id.to_string(),
            native_version: NATIVE_ACCOUNT_VERSION.into(),
        };
        let observer = NativeAccountObserver::new(binding, DbCounter::ZERO, DbCounter::ZERO)?;
        let observation_output = crate::account_observation_cli::new_segment(
            &stable_volume.join(format!("paper-source-{}.ndjson", claim.handoff.id)),
        )?;
        Ok(Self {
            owner,
            observer,
            account_id,
            client_id,
            observation_output,
            pending_observation: None,
            source_observation_id: None,
            sequence: 0,
            deadline_requested: None,
            downstream_id: claim.handoff.downstream_id,
        })
    }

    pub(crate) fn gate(&self) -> crate::capital_exit_gate::CapitalExitGate {
        self.owner.gate()
    }

    fn next_sequence(&mut self) -> Result<DbCounter> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow!("capital_exit_evidence_sequence_exhausted"))?;
        DbCounter::new(self.sequence).map_err(anyhow::Error::msg)
    }

    pub(crate) fn handle(
        &mut self,
        engine: &BacktestEngine,
        request: NativeRequest,
        source_healthy: bool,
    ) -> Result<NativeResponse> {
        // The exact type was registered by PolymarketStreamingPaper::new_inner.
        // Calls occur between engine.run invocations on that same owning thread.
        let mut strategy =
            try_get_actor_unchecked::<TargetReplay>(&StrategyId::from("QZ-PAPER-001").inner())
                .ok_or_else(|| anyhow!("capital_exit_native_strategy_unavailable"))?;
        match request {
            NativeRequest::Observe => {
                let execution = engine.kernel().exec_engine.borrow();
                let client = execution
                    .get_client(&self.client_id)
                    .ok_or_else(|| anyhow!("capital_exit_native_client_unavailable"))?;
                ensure!(
                    client.client_id() == self.client_id && client.account_id() == self.account_id,
                    "capital_exit_owner_binding_mismatch"
                );
                let connection = if source_healthy && client.is_connected() {
                    AccountConnectionV1::Connected
                } else {
                    AccountConnectionV1::Disconnected
                };
                drop(execution);
                let snapshot = engine
                    .kernel()
                    .portfolio
                    .borrow_mut()
                    .build_snapshot(&self.account_id)
                    .ok_or_else(|| anyhow!("capital_exit_native_portfolio_unavailable"))?;
                let (sender, receiver) = mpsc::sync_channel(1);
                ensure!(
                    self.observer.try_emit(
                        Some(&snapshot),
                        crate::account_observation_cli::now()?,
                        connection,
                        &sender
                    )?,
                    "capital_exit_native_observation_dropped"
                );
                let observation = AccountObservationSubmitV2 {
                    schema_version: NativeClientObservationSchemaV2,
                    native_client_id: self.client_id.to_string(),
                    observation: receiver.try_recv()?,
                };
                crate::account_observation_cli::retain_client_bound_envelope(
                    &mut self.observation_output,
                    &observation,
                )?;
                self.pending_observation = Some(observation.clone());
                Ok(NativeResponse::Observation(observation))
            }
            NativeRequest::Bind(receipt) => {
                let original = self
                    .pending_observation
                    .as_ref()
                    .ok_or_else(|| anyhow!("capital_exit_original_observation_required"))?;
                ensure!(
                    receipt.native_client_id == original.native_client_id
                        && receipt.resource.downstream_id == self.downstream_id
                        && serde_json::to_value(&receipt.resource.observation)?
                            == serde_json::to_value(&original.observation)?
                        && !receipt.resource.gap_before,
                    "capital_exit_authenticated_receipt_mismatch"
                );
                let observation_id = self.owner.bind_authenticated_source(&receipt)?;
                self.source_observation_id = Some(observation_id);
                strategy.authenticated_capital_exit_source();
                Ok(NativeResponse::Bound(receipt.resource.source_id))
            }
            NativeRequest::Assess(preview) => {
                ensure!(
                    Some(preview.original_observation_id) == self.source_observation_id,
                    "capital_exit_preview_stale"
                );
                let sequence = self.next_sequence()?;
                let revision = preview
                    .expected_account_control_revision
                    .ok_or_else(|| anyhow!("capital_exit_owner_registration_unavailable"))?;
                let request = CapitalExitPreviewRequestV1 {
                    schema_version: SchemaV1,
                    account_source_id: preview.account_source_id,
                    expected_source_observation_id: preview.original_observation_id,
                    scope: preview.scope,
                    policy: preview.policy,
                };
                Ok(NativeResponse::Assessment(
                    self.owner.assess(&*strategy, request, revision, sequence)?,
                ))
            }
            NativeRequest::Fence(view) => {
                let evidence = self.control(&mut *strategy, 0, &view)?;
                // Stopping original owned work does not wait on a valuation or
                // a remote ACK. Its original outcomes are retained separately.
                if crate::native_capital_exit::stopping(&view) {
                    self.control(&mut *strategy, 1, &view)?;
                }
                Ok(evidence)
            }
            NativeRequest::Advance(view) => self.control(&mut *strategy, 1, &view),
            NativeRequest::Availability(view) => self.control(&mut *strategy, 2, &view),
        }
    }

    fn control(
        &mut self,
        strategy: &mut TargetReplay,
        kind: u8,
        view: &CapitalExitViewV1,
    ) -> Result<NativeResponse> {
        let source = self
            .source_observation_id
            .ok_or_else(|| anyhow!("capital_exit_authenticated_source_unavailable"))?;
        let sequence = self.next_sequence()?;
        let value = match kind {
            0 => self.owner.fence(strategy, view, source, sequence)?,
            1 => self.owner.advance(strategy, view, source, sequence)?,
            _ => self
                .owner
                .simulated_availability(strategy, view, source, sequence)?,
        };
        Ok(NativeResponse::Evidence(value))
    }

    /// A retained, authenticated deadline is also checked during a silent source
    /// wait. This queues cancellation once; it never manufactures native terminal
    /// events or revalues the old book. Original evidence stays in the gate outbox.
    pub(crate) fn deadline(&mut self) -> Result<()> {
        let Some(view) = self.owner.gate().control() else {
            return Ok(());
        };
        let CapitalExitPolicyV1::BoundedLimit { deadline, .. } = &view.policy else {
            return Ok(());
        };
        if *deadline > chrono::Utc::now() || self.deadline_requested == Some(view.command_id) {
            return Ok(());
        }
        let mut strategy =
            try_get_actor_unchecked::<TargetReplay>(&StrategyId::from("QZ-PAPER-001").inner())
                .ok_or_else(|| anyhow!("capital_exit_native_strategy_unavailable"))?;
        self.control(&mut *strategy, 1, &view)?;
        self.deadline_requested = Some(view.command_id);
        Ok(())
    }
}

pub(crate) struct NativeEnvelope {
    request: NativeRequest,
    reply: tokio::sync::oneshot::Sender<std::result::Result<NativeResponse, String>>,
}

pub(crate) struct NativeInbox {
    root: std::path::PathBuf,
    receiver: mpsc::Receiver<NativeEnvelope>,
}

impl NativeInbox {
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn service(
        &self,
        session: &mut crate::polymarket_streaming_paper::PolymarketStreamingPaper,
    ) -> Result<()> {
        session.capital_exit_deadline()?;
        // A bounded batch preserves source progress under an abusive/slow peer.
        for _ in 0..8 {
            let Ok(envelope) = self.receiver.try_recv() else {
                break;
            };
            let result = session
                .control_capital_exit(envelope.request)
                .map_err(|error| error.to_string());
            let _ = envelope.reply.send(result);
        }
        Ok(())
    }
}

pub(crate) fn channel(root: std::path::PathBuf) -> (NativeInbox, mpsc::SyncSender<NativeEnvelope>) {
    let (sender, receiver) = mpsc::sync_channel(8);
    (NativeInbox { root, receiver }, sender)
}

async fn native(
    sender: &mpsc::SyncSender<NativeEnvelope>,
    request: NativeRequest,
) -> Result<NativeResponse> {
    let (reply, response) = tokio::sync::oneshot::channel();
    sender
        .try_send(NativeEnvelope { request, reply })
        .map_err(|_| anyhow!("capital_exit_native_dispatch_unavailable"))?;
    // If the HTTP process is stopped with this request outstanding, its exact
    // native control/evidence is still retained. A timeout never generates a new
    // intent, claim or native order identity.
    response
        .await
        .map_err(|_| anyhow!("capital_exit_native_dispatch_unavailable"))?
        .map_err(anyhow::Error::msg)
}

enum Pending {
    Observation(AccountObservationSubmitV2),
    Assessment(Id, CapitalExitOwnerAssessmentV1),
    Claim(Id, String, CapitalExitClaimV1),
    Evidence(CapitalExitOwnerEvidenceV1),
}

/// This task uses the already configured QZ machine connection. No credentials
/// or native handles are placed in a message, journal, report, or task status.
pub(crate) struct Poller {
    transport: crate::capital_exit_transport::CapitalExitOwnerTransport,
    sender: mpsc::SyncSender<NativeEnvelope>,
    root: std::path::PathBuf,
    source: Option<Id>,
    receipt: Option<AccountObservationReceiptV2>,
    pending: Option<Pending>,
    acknowledged_command: Option<Id>,
    fence_attempted: Option<Id>,
    progress_ready_command: Option<Id>,
    stopped_command: Option<Id>,
    assessment_due: bool,
    assessed_previews: Vec<Id>,
    observation_at: Option<std::time::Instant>,
}

impl Poller {
    pub(crate) fn new(
        transport: crate::capital_exit_transport::CapitalExitOwnerTransport,
        sender: mpsc::SyncSender<NativeEnvelope>,
        root: std::path::PathBuf,
    ) -> Self {
        Self {
            transport,
            sender,
            root,
            source: None,
            receipt: None,
            pending: None,
            acknowledged_command: None,
            fence_attempted: None,
            progress_ready_command: None,
            stopped_command: None,
            assessment_due: false,
            assessed_previews: Vec::new(),
            observation_at: None,
        }
    }

    async fn send_pending(&mut self) -> Result<()> {
        let Some(pending) = &self.pending else {
            return Ok(());
        };
        let result: Result<()> = match pending {
            Pending::Observation(request) => {
                let receipt = self.transport.submit_observation(request).await?;
                let NativeResponse::Bound(source) =
                    native(&self.sender, NativeRequest::Bind(Box::new(receipt.clone()))).await?
                else {
                    return Err(anyhow!("capital_exit_native_response_kind"));
                };
                self.source = Some(source);
                if self.receipt.as_ref().is_none_or(|old| old.resource.id != receipt.resource.id) {
                    self.assessed_previews.clear();
                }
                self.receipt = Some(receipt);
                self.observation_at = Some(std::time::Instant::now());
                Ok(())
            }
            Pending::Assessment(preview_id, request) => {
                self.transport.submit_assessment(request).await?;
                // The immutable blocked preview remains in discovery after its
                // assessment succeeds. Do not let that receipt starve Observe.
                self.assessed_previews.push(*preview_id);
                Ok(())
            }
            Pending::Claim(id, key, request) => {
                self.transport.claim(*id, key, request).await?;
                Ok(())
            }
            Pending::Evidence(request) => {
                let response = self.transport.submit_evidence(request).await?;
                ensure!(
                    response.resource.id == request.intent_id
                        && response.resource.command_id == request.command_id
                        && response.resource.account_source_id == request.account_source_id
                        && response.resource.account_control_epoch == request.account_control_epoch
                        && response.resource.owner_binding_ref == request.owner_binding_ref
                        && response.resource.external_claim_id.as_deref()
                            == Some(request.external_claim_id.as_str())
                        && !response.resource.evidence_refs.is_empty(),
                    "capital_exit_transport_outcome_unknown_reconcile_original_identity"
                );
                if matches!(
                    request.evidence,
                    CapitalExitEvidenceKindV1::FenceApplied { .. }
                ) {
                    self.acknowledged_command = Some(request.command_id);
                } else {
                    // Availability can remain stable while a new Resume preview
                    // arrives later. Give discovery a bounded turn after every
                    // accepted non-fence control outcome, not only Progress.
                    self.assessment_due = true;
                }
                if let CapitalExitEvidenceKindV1::NativeProgress { phase, .. } = &request.evidence {
                    // A FENCE also projects WAITING_EVIDENCE in Store, but does
                    // not prove Advance cancelled/reduced the original work.
                    self.progress_ready_command = if *phase == CapitalExitStateV1::WaitingEvidence {
                        Some(request.command_id)
                    } else {
                        None
                    };
                    self.stopped_command = if matches!(
                        phase,
                        CapitalExitStateV1::Paused | CapitalExitStateV1::CancelledReserved
                    ) {
                        Some(request.command_id)
                    } else {
                        None
                    };
                    self.assessment_due = true;
                }
                Ok(())
            }
        };
        result?;
        self.pending = None;
        Ok(())
    }

    async fn queue_native(&mut self, request: NativeRequest) -> Result<()> {
        let preview_id = match &request {
            NativeRequest::Assess(preview) => Some(preview.id),
            _ => None,
        };
        self.pending = Some(match native(&self.sender, request).await? {
            NativeResponse::Observation(value) => Pending::Observation(value),
            NativeResponse::Assessment(value) => Pending::Assessment(
                preview_id.ok_or_else(|| anyhow!("capital_exit_native_response_kind"))?,
                value,
            ),
            NativeResponse::Evidence(value) => Pending::Evidence(value),
            _ => return Err(anyhow!("capital_exit_native_response_kind")),
        });
        self.send_pending().await
    }

    async fn assessments(&mut self) -> Result<bool> {
        let Some(source) = self.source else {
            return Ok(false);
        };
        let mut cursor = None;
        loop {
            let page = self.transport.pending_assessments(cursor).await?;
            for preview in page.items {
                if preview.account_source_id == source
                    && !self.assessed_previews.contains(&preview.id)
                    && self
                        .receipt
                        .as_ref()
                        .is_some_and(|r| r.resource.id == preview.original_observation_id)
                    && preview.expected_account_control_revision.is_some()
                {
                    // Discovery returns immutable BLOCKED requests whose
                    // valid_until is deliberately not an executable plan TTL.
                    // Preserve the exact source/request and validate its policy;
                    // native inspection and Store admission still enforce real
                    // observation, quote, assessment TTL and revision freshness.
                    let request = CapitalExitPreviewRequestV1 {
                        schema_version: SchemaV1,
                        account_source_id: preview.account_source_id,
                        expected_source_observation_id: preview.original_observation_id,
                        scope: preview.scope.clone(),
                        policy: preview.policy.clone(),
                    };
                    if domain::capital_exit::preview_request(&request, chrono::Utc::now()).is_err() {
                        continue;
                    }
                    // A stale/unpriceable preview cannot starve later controls.
                    // Its existing server preview remains BLOCKED until a fresh
                    // request matches an actual original source observation.
                    return match self
                        .queue_native(NativeRequest::Assess(Box::new(preview)))
                        .await
                    {
                        Ok(()) => Ok(true),
                        Err(error) if self.pending.is_some() => Err(error),
                        Err(_) => Ok(false),
                    };
                }
            }
            let Some(next) = page.next_cursor else {
                return Ok(false);
            };
            ensure!(Some(next) != cursor, "capital_exit_transport_cursor_cycle");
            cursor = Some(next);
        }
    }

    async fn intent(&self) -> Result<Option<CapitalExitViewV1>> {
        let Some(source) = self.source else {
            return Ok(None);
        };
        let mut cursor = None;
        loop {
            let page = self.transport.pending_intents(cursor).await?;
            if let Some(view) = page
                .items
                .into_iter()
                .find(|view| view.account_source_id == source)
            {
                return Ok(Some(view));
            }
            let Some(next) = page.next_cursor else {
                return Ok(None);
            };
            ensure!(Some(next) != cursor, "capital_exit_transport_cursor_cycle");
            cursor = Some(next);
        }
    }

    async fn step(&mut self) -> Result<()> {
        if self.pending.is_some() {
            return self.send_pending().await;
        }
        let Some(view) = self.intent().await? else {
            // Assess against the request's exact original observation before
            // publishing a newer one. Controls always take priority over this.
            if self.assessments().await? {
                return Ok(());
            }
            if self.source.is_none()
                || self
                    .observation_at
                    .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(1))
            {
                return self.queue_native(NativeRequest::Observe).await;
            }
            return Ok(());
        };
        let external_claim_id = format!(
            "paper-capital:{}:{}",
            view.account_source_id, view.command_id
        );
        if view.external_claim_id.is_none() {
            let request = CapitalExitClaimV1 {
                schema_version: SchemaV1,
                expected_revision: view.revision,
                command_id: view.command_id,
                account_control_epoch: view.account_control_epoch,
                account_source_id: view.account_source_id,
                owner_binding_ref: view.owner_binding_ref.clone(),
                external_claim_id,
            };
            let key = format!(
                "paper-capital-claim:{}:{}",
                view.command_id,
                view.revision.get()
            );
            let path = self.root.join(format!(
                "capital-claim-{}-{}.json",
                view.command_id,
                view.revision.get()
            ));
            let original = serde_json::to_vec(&request)?;
            if path.try_exists()? {
                ensure!(
                    std::fs::read(&path)? == original,
                    "capital_exit_claim_replay_conflict"
                );
            } else {
                use std::io::Write;
                let mut file = crate::account_observation_cli::new_segment(&path)?;
                file.write_all(&original)?;
                file.sync_all()?;
            }
            self.pending = Some(Pending::Claim(view.id, key, request));
            return self.send_pending().await;
        }
        ensure!(
            view.external_claim_id.as_deref() == Some(external_claim_id.as_str()),
            "capital_exit_claim_owned_by_another_process"
        );
        if self.fence_attempted != Some(view.command_id) {
            self.fence_attempted = Some(view.command_id);
            self.assessment_due = false;
            return self
                .queue_native(NativeRequest::Fence(Box::new(view)))
                .await;
        }
        if self
            .observation_at
            .is_none_or(|at| at.elapsed() >= std::time::Duration::from_secs(1))
        {
            return self.queue_native(NativeRequest::Observe).await;
        }
        if self.acknowledged_command != Some(view.command_id) {
            return self
                .queue_native(NativeRequest::Fence(Box::new(view)))
                .await;
        }
        // A terminal native stop was accepted for this exact command. Keep
        // checking for later resume assessments without resubmitting terminal
        // CANCEL progress (which Store correctly rejects after completion).
        // A read-side PAUSED projection alone is not proof of terminal orders.
        if self.stopped_command == Some(view.command_id) {
            self.assessment_due = false;
            self.assessments().await?;
            return Ok(());
        }
        // New command claim/fence and the original control outcome have
        // priority. Thereafter service at most one assessment per accepted
        // progress receipt, including while a paused intent remains active.
        if self.assessment_due {
            self.assessment_due = false;
            if self.assessments().await? {
                return Ok(());
            }
        }
        if matches!(
            view.owner_command.instruction,
            CapitalExitOwnerInstructionV1::Start {} | CapitalExitOwnerInstructionV1::Resume {}
        ) && view.state == CapitalExitStateV1::WaitingEvidence
            && self.progress_ready_command == Some(view.command_id)
        {
            self.queue_native(NativeRequest::Availability(Box::new(view)))
                .await
        } else {
            self.queue_native(NativeRequest::Advance(Box::new(view)))
                .await
        }
    }

    pub(crate) async fn run(
        mut self,
        status: tokio::sync::watch::Sender<crate::paper_service::PaperStatus>,
    ) {
        loop {
            let result = self.step().await;
            if let Err(error) = &result {
                let code = error.to_string();
                // A definite rejection permits obtaining a new original native
                // observation/report. Ambiguous writes retain their exact body/key.
                if code.starts_with("capital_exit_transport_write_status:4")
                    && !matches!(self.pending, Some(Pending::Observation(_)))
                {
                    self.pending = None;
                }
                status.send_modify(|value| {
                    if value
                        .reason_code
                        .as_ref()
                        .is_none_or(|reason| reason.starts_with("CAPITAL_EXIT_"))
                    {
                        value.reason_code =
                            Some("CAPITAL_EXIT_CONTROL_WAITING_FOR_ORIGINAL_EVIDENCE".into());
                    }
                });
            } else {
                status.send_modify(|value| {
                    if value
                        .reason_code
                        .as_ref()
                        .is_some_and(|reason| reason.starts_with("CAPITAL_EXIT_"))
                    {
                        value.reason_code = None;
                    }
                });
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
}

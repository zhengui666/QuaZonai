import { boundScienceTarget } from './science-binding.ts';
import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, Tx } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { existingDataset, type ServiceConfig } from './config.ts';
import type { workbench } from './workbench.ts';
import { Application, json, nowNs, object, observation, Problem, text } from './facts.ts';
import { captureAccount } from './pm-account.ts';
import { retirementAccountFlat } from './pm-account-native.ts';
import { captureDataset } from './pm-datasets.ts';
import { PmDocument, assertPmNotDeleted, visiblePmRecord, digest, ensureRevision, fact, gap, identity, pmRecord, pmState, revise } from './pm-facts.ts';
import type { PmRecord, PmState } from './pm-facts.ts';
import { assessStage } from './pm-registry.ts';
import { pmView } from './pm-view.ts';
import { deletePmMandate, pmMandateTasks } from './pm-deletion.ts';
import { reconcileNativeRemoteTasks } from '../../research/src/native-remote-receipts.ts';
import { readSourceRevision } from './pm-source.ts';
import { PmTargetTask } from './pm-target.ts';
import { activeAcknowledgedReceipt } from './account-target-control.ts';
import { accountKey } from './capital-exit.ts';
import { observePmOutcome } from './pm-outcomes.ts';
import { assertBusinessAdmission, maintenancePark, maintenanceState, resumeMaintenance } from './pm-maintenance.ts';
import type { MaintenanceCheckpoint } from './pm-maintenance.ts';
import { pmControls } from './pm-controls.ts';
import { retirementOutstandingTargets } from './pm-retirement.ts';
import { historicalSourceRecovery, markPmSourceFailure, recordOperatingRecoveryWait, recoverPmSource } from './pm-source-recovery.ts';
import { verifyOriginalPrefix } from './pm-science.ts';

const context = BACKGROUND_CONTEXT;
type Work = ReturnType<typeof workbench>;
type Dependencies = { host: ResearchHost; work: Work };
/** One official task follows source facts; it never schedules its own workers. */
export function createPmPlatform(config: ServiceConfig) {
  let dependencies!: Dependencies;
  let connect!: (value: Dependencies) => void;
  const ready = new Promise<Dependencies>(resolve => { connect = value => { dependencies = value; resolve(value); }; });

  const controls = pmControls(ready);

  async function trackAcknowledgedRetirement(tx: Tx, record: PmRecord) {
    if (!Object.values(record.retirements).some(item => item.reconciliationRequired)) return;
    for (const command of Object.values(record.commands)) {
      const key = `retirement-target-task:${command.id}`;
      if (command.state !== 'ACKNOWLEDGED' || BigInt(command.validUntilNs) <= BigInt(nowNs()) || record.artifacts[key]) continue;
      const taskId = await tx.createTask(PmTargetTask, { mandateId: record.mandate.id, commandId: command.id,
        reconcileOnly: true, trackAcknowledgedUntilExpiry: true },
      { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
      record.artifacts[key] = { taskId, commandId: command.id, originalValidUntilNs: command.validUntilNs };
    }
  }

  async function enqueue(tx: Tx, _state: PmState, record: PmRecord, event: Pm.BusinessEvent) {
    assertPmNotDeleted(record);
    if (record.events[event.id]) return record.cycles[record.latestCycleId ?? ''] ?? null;
    assertBusinessAdmission(await tx.doc(Application));
    const atNs = nowNs();
    const forward = record.mandate.capitalSource.kind === 'ACCOUNT' && ['ACCOUNT_CHANGED', 'FRESHNESS_DEADLINE', 'SOURCE_RECOVERED'].includes(event.kind);
    if (!forward) {
      // A source or intent change invalidates the old result. It must not
      // launch the removed custom fitting/selection workflow.
      record.events[event.id] = event;
      if (record.currentDecisionId) { const previous = record.decisions[record.currentDecisionId]!; revise(previous); previous.status = 'SUPERSEDED'; previous.reasonCodes.push('dependency_revision_changed'); record.currentDecisionId = null; }
      const old = record.latestCycleId ? record.cycles[record.latestCycleId] : null;
      if (old && !['CONCLUDED', 'FAILED', 'SUPERSEDED'].includes(old.state)) { revise(old); old.state = 'SUPERSEDED'; old.reasonCodes.push('dependency_revision_changed'); }
      record.basisRevision = digest([record.basisRevision, event.id, record.datasets]);
      return old ?? null;
    }
    let accountBasis: Pm.AccountBasis | null = null;
    let capturedAccount: ReturnType<typeof captureAccount> | null = null;
    if (record.mandate.capitalSource.kind === 'ACCOUNT') {
      try {
        capturedAccount = captureAccount(await tx.doc(Application), record.mandate.capitalSource.accountBindingId, atNs);
        accountBasis = capturedAccount.basis;
      } catch (error) { gap(record, 'authoritative_account_basis', error instanceof Problem ? error.message : 'account_basis_unavailable', 'REFRESH_SOURCE'); }
    }
    // Account observation cannot invent or replace an operating method.
    const coalesce = () => {
      // Application retains the latest authoritative receipt. This one bounded
      // pending entry is not a new scientific fact/cycle for every heartbeat.
      if (capturedAccount) record.artifacts['latest-account-pending'] = json({ capture: capturedAccount, eventKind: event.kind });
      if (record.currentDecisionId && accountBasis) {
        const previous = record.decisions[record.currentDecisionId]!;
        if (record.comparisons[previous.comparisonRef]?.evaluationContext === 'FORWARD_MODEL_ESTIMATE'
          && previous.accountBasis?.positionsEvidenceRef !== accountBasis.positionsEvidenceRef) {
          revise(previous); previous.status = 'SUPERSEDED'; previous.reasonCodes.push('current_account_receipt_changed');
          record.currentDecisionId = null;
        }
      }
      return record.latestCycleId ? record.cycles[record.latestCycleId]! : null;
    };
    recordOperatingRecoveryWait(record, event, 'pm_custom_forward_removed');
    return coalesce();
  }

  const MonitorMandate = defineTask<{ mandateId: string }, { phase: 'start' } | { phase: 'observe' } | { phase: 'wait'; wakeAt: number } | MaintenanceCheckpoint<{ phase: 'start' } | { phase: 'observe' }>, null>({
    name: 'qz.pm.mandate-monitor', version: 1, initial: () => ({ phase: 'start' }),
    phases: {
      async start(task, runtime, ctx) {
        const { work } = await ready;
        let suspended = false;
        await runtime.commit(async tx => {
          const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
          if (stored.deletedAt) { suspended = true; return { status: 'terminal', outcome: { status: 'completed', result: null } }; }
          if (Object.values(stored.retirements).some(item => item.strategyVersionId === stored.latestStrategyId)) { suspended = true; return { status: 'running', checkpoint: { phase: 'observe' } }; }
          const app = await tx.doc(Application); const maintenance = maintenanceState(app);
          if (!maintenance || maintenance.state === 'RUNNING') return undefined;
          suspended = true;
          const obligations = Object.values(stored.commands).some(item => ['PENDING', 'UNCERTAIN'].includes(item.state)) || Object.values(stored.retirements).some(item => item.reconciliationRequired);
          return obligations ? { status: 'running', checkpoint: { phase: 'observe' } } : await maintenancePark(tx, runtime.taskId, 'qz.pm.mandate-monitor', { phase: 'start' } as const) ?? undefined;
        }, ctx);
        if (suspended) return;
        await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'observe' } }), ctx);
      },
      async observe(task, runtime, ctx) {
        const { work } = await ready;
        let parked = false; let maintenanceOnly = false;
        await runtime.commit(async tx => {
          const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
          if (stored.deletedAt) { parked = true; return { status: 'terminal', outcome: { status: 'completed', result: null } }; }
          const maintenance = maintenanceState(await tx.doc(Application));
          maintenanceOnly = !!maintenance && maintenance.state !== 'RUNNING';
          if (!maintenanceOnly) return undefined;
          const obligations = Object.values(stored.commands).some(item => ['PENDING', 'UNCERTAIN'].includes(item.state)) || Object.values(stored.retirements).some(item => item.reconciliationRequired);
          if (obligations) return undefined;
          const waiting = await maintenancePark(tx, runtime.taskId, 'qz.pm.mandate-monitor', { phase: 'observe' } as const); parked = waiting !== null; return waiting ?? undefined;
        }, ctx);
        if (parked) return;
        const state = pmState(await runtime.snapshot(PmDocument, ctx)); const record = pmRecord(state, task.input.mandateId);
        const campaign = Object.values(record.campaigns).at(-1)!;
        const retired = Object.values(record.retirements).some(item => item.strategyVersionId === record.latestStrategyId);
        if (retired && record.mandate.capitalSource.kind !== 'ACCOUNT') { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx); return; }
        let source: Awaited<ReturnType<typeof readSourceRevision>> = null; let sourceError: string | null = null;
        const configured = existingDataset(config, record.input.datasetId)!;
        try { if (!maintenanceOnly) source = await readSourceRevision(configured); } catch (error) { sourceError = error instanceof Problem ? error.message : 'source_revision_unavailable'; }
        if (source) {
          const previous = state.datasets[record.datasets.at(-1)!]!;
          if (previous.data.catalog === source.configured.data.catalog && previous.revision.contentRef !== source.contentDigest) { sourceError = 'source_catalog_must_be_immutable'; source = null; }
          else configured.data = source.configured.data;
        }
        // Reopen only an already complete historical result. Prefix validation
        // is read-only and outside the business transaction; commit rechecks
        // all original identities before restoring its display pointer.
        let verifiedRecovery: { signature: string; prefix: JsonObject } | null = null;
        if (source && !retired && !maintenanceOnly && record.artifacts['source-health']?.state === 'FAILED') {
          const registered = state.datasets[record.datasets.at(-1)!];
          const candidate = registered?.revision.contentRef === source.contentDigest ? historicalSourceRecovery(record, registered.revision) : null;
          if (candidate) try {
            const frozenData = object(object(record.artifacts[candidate.cycleId]!.configured).data);
            const catalog = String(source.configured.data.catalog);
            const prefix = await withNativeRemoteTracking(runtime.taskId, runtime, ctx,
              () => verifyOriginalPrefix(candidate.source, catalog, boundScienceTarget(config, frozenData), config.dataDir, ctx));
            const checked = await readSourceRevision(configured);
            if (!checked || checked.contentDigest !== source.contentDigest) throw new Problem('source_changed_during_recovery_verification');
            verifiedRecovery = { signature: candidate.signature, prefix };
          } catch (error) {
            await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, ctx);
            if (runtime.signal.aborted) throw error;
            sourceError = error instanceof Error && /^[a-z][a-z0-9_]+$/.test(error.message) ? error.message : 'source_recovery_prefix_verification_failed'; source = null;
          }
        }
        if (record.mandate.capitalSource.kind === 'ACCOUNT') {
          try { await work.refreshObservation(record.mandate.capitalSource.accountBindingId); } catch { /* Original account facts retain transport failures. */ }
        }
        await runtime.commit(async tx => {
          const latest = pmState(await tx.doc(PmDocument)); const stored = pmRecord(latest, task.input.mandateId);
          if (stored.deletedAt) return { status: 'terminal', outcome: { status: 'completed', result: null } };
          const current = stored.campaigns[campaign.id]!;
          // An owner clarification may replace the campaign while this task was
          // reading its original conversation. Older intake cannot restore it.
          if (Object.values(stored.campaigns).at(-1)!.id !== campaign.id) return { status: 'running', checkpoint: { phase: 'observe' } };
          const maintenance = maintenanceState(await tx.doc(Application)); maintenanceOnly = !!maintenance && maintenance.state !== 'RUNNING';
          if (maintenanceOnly && !Object.values(stored.commands).some(item => ['PENDING', 'UNCERTAIN'].includes(item.state)) && !Object.values(stored.retirements).some(item => item.reconciliationRequired)) {
            return await maintenancePark(tx, runtime.taskId, 'qz.pm.mandate-monitor', { phase: 'observe' } as const) ?? { status: 'running', checkpoint: { phase: 'observe' } };
          }
          if (sourceError && !maintenanceOnly) {
            if (!retired) markPmSourceFailure(stored, sourceError, nowNs());
          }
          if (source && !retired && !maintenanceOnly) {
            let captured: ReturnType<typeof captureDataset> | null = null;
            try { captured = captureDataset(source.configured, latest, nowNs(), source); } catch (error) {
              const reason = error instanceof Problem ? error.message : 'source_revision_admission_failed'; markPmSourceFailure(stored, reason, nowNs());
            }
            if (captured) {
              const sourceEvidenceId = identity('source-evidence', captured.revision.id); const artifact = json(source.manifest);
              stored.artifacts[sourceEvidenceId] = artifact;
              stored.evidence[sourceEvidenceId] = { id: sourceEvidenceId, sourceKind: 'DATASET', role: 'CONTROL', runtime: { conversationId: stored.conversationId, taskId: runtime.taskId },
                objectKey: sourceEvidenceId, contentDigest: digest(artifact), mediaType: 'application/json', sourceRevision: captured.revision.sourceRevision,
                eventInterval: captured.revision.eventInterval, availableAtNs: source.availableAtNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.dataset-revision-manifest/1', visibility: 'OWNER_ONLY' };
              for (const missing of Object.values(stored.gaps)) if (missing.requiredCapability === 'source_revision' && missing.state !== 'RESOLVED') { revise(missing); missing.state = 'RESOLVED'; missing.resolutionEvidenceRefs = [sourceEvidenceId]; }
              const recovery = recoverPmSource(stored, captured.revision, sourceEvidenceId, nowNs(), runtime.taskId, verifiedRecovery);
              if (recovery?.forward) await enqueue(tx, latest, stored, recovery.event);
            }
            if (captured && !stored.datasets.includes(captured.revision.id)) {
              const prior = latest.datasets[stored.datasets.at(-1)!]!;
              if (prior.data.catalog === captured.data.catalog && prior.revision.contentRef !== captured.revision.contentRef) throw new Problem('source_catalog_must_be_immutable');
              stored.datasets = [captured.revision.id];
              await enqueue(tx, latest, stored, { id: identity('source-event', captured.revision.id), kind: source.revisionKind === 'RETRACTION' ? 'EVIDENCE_INVALIDATED' : 'DATA_REVISION',
                sourceRevisionRef: captured.revision.id, availableAtNs: source.availableAtNs, affectedMandateIds: [stored.mandate.id], supersedesEventId: null });
            }
          }
          if (stored.mandate.capitalSource.kind === 'ACCOUNT') {
            try {
              const account = captureAccount(await tx.doc(Application), stored.mandate.capitalSource.accountBindingId);
              observePmOutcome(stored, account);
              await trackAcknowledgedRetirement(tx, stored);
              for (const command of Object.values(stored.commands)) if (['PENDING', 'UNCERTAIN'].includes(command.state)) {
                const reference = stored.artifacts[`target-task:${command.id}`];
                const prior = typeof reference?.taskId === 'number' ? await tx.task(reference.taskId as TaskId) : undefined;
                if (!prior || prior.state.status === 'terminal') {
                  const taskId = await tx.createTask(PmTargetTask, { mandateId: stored.mandate.id, commandId: command.id, reconcileOnly: true },
                    { conversationId: stored.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
                  stored.artifacts[`target-task:${command.id}`] = { taskId };
                }
              }
              if (!retired && !maintenanceOnly) await enqueue(tx, latest, stored, { id: identity('event', account.evidence.sourceRevision), kind: 'ACCOUNT_CHANGED', sourceRevisionRef: account.evidence.sourceRevision,
                availableAtNs: account.basis.availableAtNs, affectedMandateIds: [stored.mandate.id], supersedesEventId: null });
              else {
                // Read-only reconciliation still observes every authoritative
                // receipt, but a heartbeat alone is not another historical
                // retirement event. Keep the latest pending capture bounded.
                stored.artifacts['latest-account-pending'] = json({ capture: account, eventKind: retired ? 'RETIREMENT_RECONCILIATION' : 'MAINTENANCE_RECONCILIATION' });
                const snapshot = object(account.artifact.snapshot), native = snapshot.nativeObservation ? object(object(snapshot.nativeObservation).snapshot) : null;
                const flatness = retirementAccountFlat(account);
                const outstanding = retirementOutstandingTargets(Object.values(stored.commands), account, nowNs(), stored.artifacts);
                const application = await tx.doc(Application);
                const connection = observation(application.observations[account.basis.accountBindingId]).connection;
                const physicalAccount = accountKey(connection, application.accountControls);
                const legacyReceipts = Object.values(application.observations).map(value => observation(value))
                  .filter(value => accountKey(value.connection, application.accountControls) === physicalAccount)
                  .flatMap(value => [value.view.receipt, value.view.allocation?.receipt]).filter(value => value != null)
                  .filter(receipt => ['PENDING', 'UNCERTAIN'].includes(receipt.status)
                    || activeAcknowledgedReceipt(receipt) || activeAcknowledgedReceipt(receipt, account.basis.observedAtNs)
                    || activeAcknowledgedReceipt(receipt, account.basis.availableAtNs));
                const retirementBasis = digest(json({ accountSourceId: account.basis.accountSourceId,
                  nativeAccountId: native?.account_id ?? null,
                  positions: Array.isArray(snapshot.positions) ? snapshot.positions.map(value => { const row = object(value); return { instrumentId: row.instrumentId ?? null, quantity: row.quantity ?? null }; }) : null,
                  balances: native?.balances ?? null,
                  orderCount: snapshot.openOrders ? object(snapshot.openOrders).count ?? null : null,
                  flat: flatness.flat, flatnessReasons: flatness.reasonCodes, reserved: account.basis.reserved, outstanding,
                  legacyReceipts: legacyReceipts.map(receipt => ({ id: receipt.id, status: receipt.status, expiresAt: receipt.expiresAt })) }));
                for (const disposition of Object.values(stored.retirements)) if (disposition.reconciliationRequired) {
                  const markerKey = `retirement-observation:${disposition.id}`;
                  if (stored.artifacts[markerKey]?.basis !== retirementBasis) {
                    stored.evidence[account.evidence.id] = account.evidence; stored.artifacts[account.evidence.objectKey] = account.artifact;
                    revise(disposition); disposition.residualPositionsEvidenceRef = account.evidence.id;
                    disposition.outstandingTargetCommandIds = outstanding;
                    stored.artifacts[markerKey] = { basis: retirementBasis, evidenceRef: account.evidence.id, legacyReceipts: json(legacyReceipts) };
                  }
                  for (const reason of flatness.flat === null ? flatness.reasonCodes : []) gap(stored, 'retirement_account_flatness', reason, 'REFRESH_SOURCE', disposition.id);
                  if (flatness.flat === true && disposition.outstandingTargetCommandIds.length === 0 && legacyReceipts.length === 0) {
                    // A final closure always names an immutable original
                    // receipt, even if the economic values were unchanged.
                    stored.evidence[account.evidence.id] = account.evidence; stored.artifacts[account.evidence.objectKey] = account.artifact;
                    revise(disposition); disposition.positionDisposition = 'FLAT_CONFIRMED'; disposition.reconciliationRequired = false;
                    disposition.residualPositionsEvidenceRef = account.evidence.id; disposition.closureEvidenceRefs.push(account.evidence.id);
                  }
                }
              }
            } catch (error) { gap(stored, 'account', error instanceof Problem ? error.message : 'account_basis_unavailable', 'REFRESH_SOURCE'); }
          }
          stored.searchFamily.trialIds = Object.keys(stored.derivedTrials);
          stored.searchFamily.countedAttempts = Object.values(stored.derivedTrials).filter(item => item.countedAttempt).length.toString();
          stored.searchFamily.failedAttempts = Object.values(stored.derivedTrials).filter(item => item.countedAttempt && item.state === 'FAILED').length.toString();
          return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, ctx);
      },
      async wait(task, runtime, ctx) { await runtime.sleepUntil(task.state.checkpoint.wakeAt, { phase: 'observe' }, ctx); },
      async maintenance(task, runtime, ctx) { await runtime.commit(async tx => pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId).deletedAt
        ? { status: 'terminal', outcome: { status: 'completed', result: null } } : resumeMaintenance(tx, runtime.taskId, 'qz.pm.mandate-monitor', task.state.checkpoint), ctx); },
    },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });

  async function start(_value: unknown): Promise<Awaited<ReturnType<typeof get>>> { throw new Problem('pm_custom_research_removed', 410); }
  async function get(id: string) { await ready; const state = pmState(await dependencies.host.harness.snapshot(PmDocument, context)); return pmView(visiblePmRecord(state, id), state); }
  async function list() { await ready; const state = pmState(await dependencies.host.harness.snapshot(PmDocument, context)); return Object.values(state?.mandates ?? {}).filter(record => !record.deletedAt).map(record => pmView(record, state)); }
  async function clarifyIntent(_id: string, _value: unknown): Promise<Awaited<ReturnType<typeof get>>> { throw new Problem('pm_custom_research_removed', 410); }
  async function command(id: string, kind: 'recompute' | 'retire' | 'refresh-data', value: unknown) {
    await ready; const body = object(value); const requestId = text(body.requestId, 'request_id'); const original = json(value);
    if (kind === 'recompute') {
      visiblePmRecord(pmState(await dependencies.host.harness.snapshot(PmDocument, context)), id);
      throw new Problem('pm_custom_research_removed', 410);
    }
    const before = pmState(await dependencies.host.harness.snapshot(PmDocument, context)); assertPmNotDeleted(pmRecord(before, id)); const existing = before.commands[`${id}:${kind}:${requestId}`];
    if (existing) { if (canonical(existing.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); if (kind !== 'retire') return get(id); }
    const configuredSource = kind === 'refresh-data' ? existingDataset(config, pmRecord(before, id).input.datasetId)! : null;
    const revisionSource = configuredSource ? await readSourceRevision(configuredSource) : null;
    if (revisionSource && configuredSource) configuredSource.data = revisionSource.configured.data;

    await dependencies.host.harness.commit(async tx => {
      const state = pmState(await tx.doc(PmDocument)); const record = pmRecord(state, id); assertPmNotDeleted(record); const key = `${id}:${kind}:${requestId}`; const replay = state.commands[key];
      if (replay) { if (canonical(replay.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return; }
      assertBusinessAdmission(await tx.doc(Application));
      ensureRevision(record, body.expectedRevision);
      if (kind === 'retire') {
        const strategyId = body.strategyVersionId === undefined ? record.latestStrategyId : text(body.strategyVersionId, 'strategy_version');
        const strategy = record.strategies[strategyId]; if (!strategy) throw new Problem('strategy_not_found', 404);
        const assessment: Pm.PipelineAssessment = { ...assessStage(strategy, 'RETIRED', []), outcome: 'RETIRED', reasonCodes: ['owner_retirement'], nextPermittedActions: ['RECONCILE_RESIDUAL_POSITIONS'] };
        record.assessments[assessment.id] = assessment;
        const disposition: Pm.RetirementDisposition = { ...fact(identity('retirement', [strategyId, requestId])), strategyVersionId: strategyId, pipelineAssessmentId: assessment.id,
          signalGeneration: 'STOPPED', positionDisposition: record.mandate.capitalSource.kind === 'ACCOUNT' ? 'MONITORING_EXISTING' : 'NONE', residualPositionsEvidenceRef: null,
          outstandingTargetCommandIds: retirementOutstandingTargets(Object.values(record.commands)), exitDecisionId: null,
          reconciliationRequired: record.mandate.capitalSource.kind === 'ACCOUNT', closureEvidenceRefs: [] };
        record.retirements[disposition.id] = disposition;
        await trackAcknowledgedRetirement(tx, record);
        if (record.currentDecisionId && record.decisions[record.currentDecisionId]!.strategyVersionId === strategyId) { const current = record.decisions[record.currentDecisionId]!; revise(current); current.status = 'REVOKED'; current.reasonCodes.push('strategy_retired_not_position_liquidation'); record.currentDecisionId = null; }
        if (record.latestCycleId && record.cycles[record.latestCycleId]!.strategyVersionId === strategyId) { const cycle = record.cycles[record.latestCycleId]!; if (!['CONCLUDED', 'FAILED', 'SUPERSEDED'].includes(cycle.state)) { revise(cycle); cycle.state = 'SUPERSEDED'; cycle.reasonCodes.push('strategy_retired'); } }
        if (record.latestStrategyId === strategyId) record.basisRevision = digest([record.basisRevision, 'retired', strategyId]);
      } else {
        if (Object.values(record.retirements).some(item => item.strategyVersionId === record.latestStrategyId)) throw new Problem('strategy_retired', 409);
        let sourceRevisionRef = requestId;
        if (kind === 'refresh-data') {
          const configured = revisionSource?.configured ?? configuredSource!; const captured = captureDataset(configured, state, nowNs(), revisionSource);
          if (record.datasets.includes(captured.revision.id)) { state.commands[key] = { original, mandateId: id, resourceId: record.latestCycleId ?? id }; return; }
          record.datasets = [captured.revision.id]; sourceRevisionRef = captured.revision.id;
        }
        await enqueue(tx, state, record, { id: identity('event', [id, kind, requestId]), kind: 'DATA_REVISION',
          sourceRevisionRef, availableAtNs: nowNs(), affectedMandateIds: [id], supersedesEventId: null });
      }
      state.commands[key] = { original, mandateId: id, resourceId: record.latestCycleId ?? id };
    }, context);
    if (kind === 'retire') { const view = await get(id); const strategyId = body.strategyVersionId === undefined ? view.strategies.at(-1)!.id : text(body.strategyVersionId, 'strategy_version'); if (view.strategies.at(-1)!.id === strategyId) await controls.strategyCommand(strategyId, 'pause', value); }
    return get(id);
  }
  async function offerTarget(_id: string, _value: unknown): Promise<Awaited<ReturnType<typeof get>>> { throw new Problem('native_strategy_only', 410); }
  async function remove(id: string) {
    await ready;
    const host = dependencies.host;
    const record = pmRecord(pmState(await host.harness.snapshot(PmDocument, context)), id);
    const conversations = [...new Set([record.conversationId, ...Object.values(record.campaigns).map(item => item.runtime.conversationId)])].sort((left, right) => left - right);
    function assertLockedConversations(current: PmRecord) {
      const ids = [...new Set([current.conversationId, ...Object.values(current.campaigns).map(item => item.runtime.conversationId)])].sort((left, right) => left - right);
      if (ids.length !== conversations.length || ids.some((value, index) => value !== conversations[index])) throw new Problem('pm_delete_tasks_active', 409);
    }
    async function locked(index: number): Promise<import('../../../packages/contracts/src/index.ts').DeletionReceipt> {
      if (index < conversations.length) return host.withOwnerChange(conversations[index]!, () => locked(index + 1));
      const tasks = await host.harness.commit(async tx => {
        const current = pmRecord(pmState(await tx.doc(PmDocument)), id); assertLockedConversations(current);
        return pmMandateTasks(tx, current);
      }, context);
      await reconcileNativeRemoteTasks(host.harness, tasks, context);
      return host.harness.commit(async tx => {
        const state = pmState(await tx.doc(PmDocument)); assertLockedConversations(pmRecord(state, id));
        return deletePmMandate(tx, state, id);
      }, context);
    }
    return locked(0);
  }
  return { extension: defineExtension({ name: 'qz.pm', tasks: [MonitorMandate, PmTargetTask, controls.task] }), connect,
    work: { start, get, list, delete: remove, readResource: controls.readResource, campaignCommand: controls.campaignCommand, strategyCommand: controls.strategyCommand, startCampaign: start, recompute: (id: string, value: unknown) => command(id, 'recompute', value), retire: (id: string, value: unknown) => command(id, 'retire', value),
      refreshData: (id: string, value: unknown) => command(id, 'refresh-data', value), offerTarget, clarifyIntent,
      events: async (id: string) => Object.values(visiblePmRecord(pmState(await dependencies.host.harness.snapshot(PmDocument, context)), id).events),
      snapshot: () => dependencies.host.harness.snapshot(PmDocument, context) } };
}

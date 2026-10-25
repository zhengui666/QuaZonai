import { boundScienceTarget, pmScienceData } from './science-binding.ts';
import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, Tx } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical, Study } from '../../research/src/documents.ts';
import { CampaignResearchDoc } from '../../research/src/pm-campaign.ts';
import { readNativeEvidence } from '../../research/plugins/native-evidence.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { canonicalOwnerBoundary } from '../../../packages/contracts/src/pm-intent.ts';
import type { ResearchCampaignBinding } from '../../research/src/pm-campaign.ts';
import type { ServiceConfig } from './config.ts';
import type { workbench } from './workbench.ts';
import { Application, counter, decimal, decimalUnits, json, nowNs, object, observation, Problem, text } from './facts.ts';
import { captureAccount } from './pm-account.ts';
import { admitCampaignAccount, campaignAccount, freezeCampaignAccount } from './pm-campaign-account.ts';
import { retirementAccountFlat } from './pm-account-native.ts';
import { captureDataset } from './pm-datasets.ts';
import { PmDocument, assertPmNotDeleted, visiblePmRecord, currentStrategy, digest, ensureRevision, fact, gap, identity, pmRecord, pmState, revise } from './pm-facts.ts';
import type { PmRecord, PmState } from './pm-facts.ts';
import { assessStage, PM_CAPABILITY_ID, pmMetricDefinitions, selectionRule } from './pm-registry.ts';
import { pmView } from './pm-view.ts';
import { deletePmMandate, pmMandateTasks } from './pm-deletion.ts';
import { reconcileNativeRemoteTasks } from '../../research/src/native-remote-receipts.ts';
import { computePmDecision } from './pm-native.ts';
import { pmQualificationProtocol } from './pm-qualification.ts';
import { pmAllocationMethod } from './pm-method.ts';
import { readSourceRevision } from './pm-source.ts';
import { PmTargetTask, pmTargetAllocation, pmTargetPayload, targetEligibility } from './pm-target.ts';
import { activeAcknowledgedReceipt, assertAccountTargetControl } from './account-target-control.ts';
import { accountKey } from './capital-exit.ts';
import { observePmOutcome } from './pm-outcomes.ts';
import { assertBusinessAdmission, maintenancePark, maintenanceState, resumeMaintenance } from './pm-maintenance.ts';
import type { MaintenanceCheckpoint } from './pm-maintenance.ts';
import { pmControls } from './pm-controls.ts';
import { pmScienceCheckpoint, readPmArtifact, retainPmArtifacts } from './pm-evidence.ts';
import { pmIndependentTask } from './pm-independent.ts';
import { clarificationInput, mergeOwnerBoundaries, retainIntentEvidence, reviseOwnerIntent } from './pm-intent-state.ts';
import { computePmForward, forwardSeedDecision } from './pm-forward.ts';
import { pmOperatingRule } from './pm-operating-rule.ts';
import { retirementOutstandingTargets } from './pm-retirement.ts';
import { historicalSourceRecovery, markPmSourceFailure, recordOperatingRecoveryWait, recoverPmSource } from './pm-source-recovery.ts';
import { verifyOriginalPrefix } from './pm-science.ts';

const context = BACKGROUND_CONTEXT;
type Work = ReturnType<typeof workbench>;
type Dependencies = { host: ResearchHost; work: Work };
const supportedKeys = new Set(['requestId', 'goal', 'datasetId', 'funding', 'model', 'thinkingLevel', 'serviceTier', 'maxTrials', 'operatingAuthority', 'businessPreference']);
/** Two original factors, one fit and one joint validation must remain searchable. */
export const downstreamTrialReservation = (total: number) => total >= 8 ? Math.min(6, Math.max(4, Math.floor(total / 2))) : 0;
export function mandateStart(value: unknown): Pm.PmMandateStart {
  const body = object(value);
  if (Object.keys(body).some(key => !supportedKeys.has(key))) throw new Problem('pm_mandate_field_unsupported');
  text(body.requestId, 'request_id'); text(body.goal, 'goal'); text(body.datasetId, 'dataset_id');
  if (!Number.isSafeInteger(body.maxTrials) || Number(body.maxTrials) < 1) throw new Problem('max_trials_invalid');
  const model = object(body.model, 'model'); text(model.provider, 'provider'); text(model.modelId, 'model_id');
  if (body.thinkingLevel !== undefined && !['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'].includes(String(body.thinkingLevel))) throw new Problem('invalid_thinking_level');
  if (body.serviceTier !== undefined && !['default', 'priority'].includes(String(body.serviceTier))) throw new Problem('invalid_service_tier');
  if (body.businessPreference !== undefined && !['NET_RETURN', 'DOWNSIDE_FIRST', 'FRONTIER'].includes(String(body.businessPreference))) throw new Problem('business_preference_invalid');
  if (body.operatingAuthority !== undefined && !['RESEARCH_ONLY', 'SHADOW_TARGETS', 'AUTHORIZED_TARGETS'].includes(String(body.operatingAuthority))) throw new Problem('operating_authority_invalid');
  const funding = object(body.funding, 'funding');
  if (funding.kind === 'RESEARCH_SCENARIO') {
    const capital = object(funding.capital, 'capital'); text(capital.currency, 'currency');
    if (decimalUnits(decimal(capital.amount, 'capital')) <= 0n) throw new Problem('capital_invalid');
  } else if (funding.kind === 'ACCOUNT') text(funding.accountBindingId, 'account_binding');
  else throw new Problem('funding_kind_unsupported');
  return json(body) as unknown as Pm.PmMandateStart;
}

/** One official task follows source facts; it never schedules its own workers. */
export function createPmPlatform(config: ServiceConfig) {
  const IndependentEvaluation = pmIndependentTask(config);
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

  async function enqueue(tx: Tx, state: PmState, record: PmRecord, event: Pm.BusinessEvent) {
    assertPmNotDeleted(record);
    if (record.events[event.id]) return record.cycles[record.latestCycleId ?? ''] ?? null;
    assertBusinessAdmission(await tx.doc(Application));
    const atNs = nowNs();
    const campaign = Object.values(record.campaigns).at(-1)!;
    const forward = record.mandate.capitalSource.kind === 'ACCOUNT' && ['ACCOUNT_CHANGED', 'FRESHNESS_DEADLINE', 'SOURCE_RECOVERED'].includes(event.kind);
    if (!forward && canonical(json(campaign.datasetRevisionIds)) !== canonical(json(record.datasets))) {
      // The new revision invalidates the old result, but cannot re-label an old
      // campaign's research or account start. Its own new campaign is admitted
      // below by the monitor, with a separately captured starting state.
      record.events[event.id] = event;
      if (record.currentDecisionId) { const previous = record.decisions[record.currentDecisionId]!; revise(previous); previous.status = 'SUPERSEDED'; previous.reasonCodes.push('dependency_revision_changed'); record.currentDecisionId = null; }
      const old = record.latestCycleId ? record.cycles[record.latestCycleId] : null;
      if (old && !['CONCLUDED', 'FAILED', 'SUPERSEDED'].includes(old.state)) { revise(old); old.state = 'SUPERSEDED'; old.reasonCodes.push('new_data_requires_new_campaign'); }
      record.basisRevision = digest([record.basisRevision, event.id, record.datasets]);
      return old ?? null;
    }
    let accountBasis: Pm.AccountBasis | null = null;
    let capturedAccount: ReturnType<typeof captureAccount> | null = null;
    if (record.mandate.capitalSource.kind === 'ACCOUNT') {
      try {
        capturedAccount = forward ? captureAccount(await tx.doc(Application), record.mandate.capitalSource.accountBindingId, atNs) : campaignAccount(record, campaign);
        accountBasis = capturedAccount.basis;
      } catch (error) { gap(record, 'authoritative_account_basis', error instanceof Problem ? error.message : 'account_basis_unavailable', 'REFRESH_SOURCE'); }
    }
    const seed = forward ? forwardSeedDecision(record) : null;
    // Historical matched replay keeps its captured starting state. A new
    // observation is not permission to run another unbudgeted research trial
    // or cancel the first independent method evaluation before a seed exists.
    const coalesce = () => {
      // Application retains the latest authoritative receipt. This one bounded
      // pending entry is not a new scientific fact/cycle for every heartbeat.
      if (capturedAccount) record.artifacts['latest-account-pending'] = json({ capture: capturedAccount, eventKind: event.kind });
      if (record.currentDecisionId && accountBasis) {
        const previous = record.decisions[record.currentDecisionId]!;
        if (record.comparisons[previous.comparisonRef]?.evaluationContext === 'FORWARD_MODEL_ESTIMATE'
          && previous.accountBasis?.positionsEvidenceRef !== accountBasis.positionsEvidenceRef) {
          revise(previous); previous.status = 'SUPERSEDED'; previous.reasonCodes.push('current_account_receipt_changed_waiting_for_next_registered_slot');
          record.currentDecisionId = null;
        }
      }
      return record.latestCycleId ? record.cycles[record.latestCycleId]! : null;
    };
    if (forward && !seed) { recordOperatingRecoveryWait(record, event, 'source_recovered_without_qualified_current_policy_seed'); return coalesce(); }
    const basisRevision = digest({ mandate: record.mandate, policy: record.policy.id, strategy: seed?.strategyVersionId ?? record.latestStrategyId,
      datasets: record.datasets, accountBasis, eventId: event.id });
    const cycleId = identity('cycle', [record.mandate.id, basisRevision]);
    let operatingSlotAlreadyAdmitted = false;
    if (seed) {
      const explanation = object(seed.explanationFacts); const anchor = BigInt(String(explanation.forwardAnchorNs));
      const ttl = BigInt(String(explanation.forwardTtlSeconds)) * 1_000_000_000n;
      const slot = (BigInt(atNs) - anchor) / ttl; const slotKey = `forward-slot:${seed.id}:${slot}`;
      operatingSlotAlreadyAdmitted = !!record.artifacts[slotKey] && record.artifacts[slotKey]!.cycleId !== cycleId;
      if (operatingSlotAlreadyAdmitted) { recordOperatingRecoveryWait(record, event, 'source_recovered_waiting_for_next_registered_operating_slot'); return coalesce(); }
      record.artifacts[slotKey] ??= { cycleId, seedDecisionId: seed.id, slot: slot.toString(), policyRevisionId: record.policy.id };
    }
    record.events[event.id] = event;
    if (forward) for (const missing of Object.values(record.gaps)) if (missing.requiredCapability === 'source_recovery_operating' && missing.state !== 'RESOLVED') {
      revise(missing); missing.state = 'RESOLVED'; missing.resolutionEvidenceRefs = [event.sourceRevisionRef];
    }
    if (capturedAccount) {
      record.evidence[capturedAccount.evidence.id] = capturedAccount.evidence;
      record.artifacts[capturedAccount.evidence.objectKey] = capturedAccount.artifact;
    }
    const old = record.latestCycleId ? record.cycles[record.latestCycleId] : null;
    if (old && !['CONCLUDED', 'FAILED', 'SUPERSEDED'].includes(old.state)) { revise(old); old.state = 'SUPERSEDED'; old.supersededByCycleId = cycleId; old.reasonCodes.push('newer_business_basis'); }
    if (record.currentDecisionId) {
      const previous = record.decisions[record.currentDecisionId]!;
      if (['DATA_REVISION', 'COST_OR_RULE_CHANGED', 'EVIDENCE_INVALIDATED', 'MANDATE_CHANGED', 'ACCOUNT_CHANGED', 'FRESHNESS_DEADLINE'].includes(event.kind)) {
        revise(previous); previous.status = 'SUPERSEDED'; previous.reasonCodes.push('dependency_revision_changed'); record.currentDecisionId = null;
      }
    }
    const cycle: Pm.DecisionCycle = { ...fact(cycleId, atNs), mandateRevisionId: record.mandate.id,
      runtime: { conversationId: record.conversationId, taskId: null }, state: 'PENDING', triggerEventIds: [event.id], basisRevision,
      datasetRevisionIds: [...record.datasets], accountBasis, policyRevisionId: record.policy.id, strategyVersionId: seed?.strategyVersionId ?? record.latestStrategyId,
      asOfNs: atNs, availableCutoffNs: atNs, dependencyRevisionRefs: [record.policy.id, record.latestStrategyId, ...record.datasets, event.sourceRevisionRef],
      outputDecisionId: null, supersededByCycleId: null, reasonCodes: [] };
    const research = campaign.state !== 'PLANNED' && campaign.runtime.conversationId !== record.conversationId ? await tx.doc(Study, campaign.runtime.conversationId as ConversationId) : null;
    // Freeze the exact observed research revision, never read a later trial in this cycle.
    const registered = state.datasets[record.datasets.at(-1)!]!;
    record.artifacts[cycleId] = json({ research, campaignId: campaign.id, configured: { ...registered, data: pmScienceData(record, registered.data) }, incumbentDecisionId: record.currentDecisionId,
      operatingForward: forward, forwardSeedDecisionId: seed?.id ?? null, operatingSlotAlreadyAdmitted });
    cycle.runtime.taskId = await tx.createTask(ComputeCycle, { mandateId: record.mandate.id, cycleId }, { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
    record.cycles[cycleId] = cycle; record.latestCycleId = cycleId; record.basisRevision = basisRevision;
    return cycle;
  }

  const ComputeCycle = defineTask<{ mandateId: string; cycleId: string }, { phase: 'compute' | 'offer' }, null>({
    name: 'qz.pm.decision-cycle', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      await ready;
      let captured!: PmRecord;
      let cycle!: Pm.DecisionCycle;
      let stale = false;
      await runtime.commit(async tx => {
        const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); cycle = record.cycles[task.input.cycleId]!;
        stale = cycle.state === 'SUPERSEDED' || record.basisRevision !== cycle.basisRevision;
        if (stale) return { status: 'terminal', outcome: { status: 'completed', result: null } };
        revise(cycle); cycle.state = 'COMPUTING'; captured = json(record) as unknown as PmRecord; cycle = json(cycle) as unknown as Pm.DecisionCycle;
        return undefined;
      }, ctx);
      if (stale) return;
      try {
        const frozen = captured.artifacts[cycle.id]!;
        const isForward = frozen.operatingForward === true;
        const study = frozen.research ? object(frozen.research) : null;
        const final = study?.final ? object(study.final) : null;
        const trials = Array.isArray(study?.trials) ? study.trials.map(item => object(item)) : [];
        const selected = final && trials.find(item => item.taskId === final.trialId);
        const selectedResult = selected?.result ? object(selected.result) : null;
        if (!isForward && selectedResult?.kind === 'FITTED_ENSEMBLE_REPLAY' && typeof selectedResult.evidenceTaskId === 'number') {
          const source = await runtime.getTask(selectedResult.evidenceTaskId as TaskId<JsonObject>, ctx);
          if (!source || source.state.status !== 'terminal' || source.state.outcome.status !== 'completed') throw new Problem('native_evidence_unavailable');
          const output = object(source.state.outcome.result, 'native_evidence_result');
          const originalSource = await readNativeEvidence(runtime, source.conversationId, source.id, object(output.native, 'native_evidence'), ctx);
          // Original simulation/application tapes remain in their official child
          // evidence. The next scientific stage needs only these frozen inputs.
          const sourceBody: JsonObject = {};
          for (const key of ['fit', 'fitRequest', 'members', 'request', 'simulationRequest', 'policy', 'data', 'catalogPrefix', 'datasetBridge']) {
            if (originalSource[key] !== undefined) sourceBody[key] = originalSource[key]!;
          }
          const originalEvidenceId = identity('original-native-evidence', [source.conversationId, source.id]);
          sourceBody.originalEvidenceRef = originalEvidenceId;
          frozen.nativeSource = sourceBody;
          const sourceTaskData = object(object(source.input, 'native_source_task_input').data, 'native_source_task_data');
          const originalResearchData = object(object(study!.input, 'frozen_research_input').data, 'frozen_research_data');
          if (canonical(sourceTaskData) !== canonical(originalResearchData)
            || sourceBody.data !== undefined && canonical(object(sourceBody.data)) !== canonical(sourceTaskData)) throw new Problem('native_source_task_data_mismatch');
          sourceBody.data = json(sourceTaskData);
          const proposal = selected?.proposal ? object(selected.proposal) : null;
          const sourceInput = object(source.input, 'native_source_task_input');
          const fitTaskId = sourceInput.fitTaskId ?? proposal?.fitTaskId;
          if (sourceInput.fitTaskId !== undefined && proposal?.fitTaskId !== undefined && sourceInput.fitTaskId !== proposal.fitTaskId) throw new Problem('native_fit_source_identity_mismatch');
          if (typeof fitTaskId === 'number') {
            const fitted = await runtime.getTask(fitTaskId as TaskId<JsonObject>, ctx);
            if (fitted?.state.status === 'terminal' && fitted.state.outcome.status === 'completed') {
              const originalFit = await readNativeEvidence(runtime, fitted.conversationId, fitted.id, object(object(fitted.state.outcome.result).native), ctx);
              for (const key of ['fitRequest', 'catalogPrefix']) if (originalFit[key] !== undefined) {
                if (sourceBody[key] !== undefined && canonical(sourceBody[key]!) !== canonical(originalFit[key]!)) throw new Problem('native_fit_evidence_identity_mismatch');
                sourceBody[key] = originalFit[key]!;
              }
            }
          }
          frozen.sourceTaskId = source.id; frozen.sourceConversationId = source.conversationId;
          // Native science is admitted before execution; failures consume the same identities.
          await runtime.commit(async tx => {
            const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
            record.artifacts[originalEvidenceId] = { kind: 'PM_ORIGINAL_NATIVE_EVIDENCE_REFERENCE', conversationId: source.conversationId, taskId: source.id };
            record.evidence[originalEvidenceId] = { id: originalEvidenceId, sourceKind: 'NATIVE_RESULT', role: 'SELECTION', runtime: { conversationId: source.conversationId, taskId: source.id },
              objectKey: originalEvidenceId, contentDigest: null, mediaType: 'application/json', sourceRevision: cycle.basisRevision, eventInterval: null,
              availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'qz.native-fitted-ensemble-original/1', visibility: 'OWNER_ONLY' };
            for (const campaign of Object.values(record.campaigns)) if (campaign.runtime.conversationId !== record.conversationId) {
              const ledger = await tx.doc(CampaignResearchDoc, campaign.runtime.conversationId as ConversationId);
              for (const trial of ledger.trials) record.derivedTrials[trial.id] = trial;
              for (const movement of ledger.budgetMovements) record.budgetMovements[movement.id] = movement;
            }
            if (Object.values(record.gaps).some(item => item.state !== 'RESOLVED' && item.permittedRemedy === 'CLARIFY_INTENT')) return undefined;
            const used = Object.values(record.derivedTrials).filter(item => item.countedAttempt && item.proposalRef !== cycle.id).length;
            const reserved = downstreamTrialReservation(record.budget.maximumTrials);
            if (reserved < 4 || used + reserved > record.budget.maximumTrials) { frozen.budgetUnavailable = true; return undefined; }
            frozen.maximumTrials = reserved - 2; // The frozen configuration and its matched independent baseline each consume an attempt.
            const campaign = Object.values(record.campaigns).at(-1)!;
            for (let index = 0; index < reserved - 2; index++) {
              const purpose = record.mandate.capitalSource.kind === 'ACCOUNT' ? index % 2 === 1 ? 'ROBUSTNESS' : 'CAPITAL_SCALE' : index === reserved - 3 ? 'ROBUSTNESS' : 'CAPITAL_SCALE';
              const trialId = identity('pm-trial', [cycle.id, purpose, index]);
              const parent = Object.values(record.derivedTrials).find(item => item.runtime.taskId === selected!.taskId && item.campaignId === campaign.id);
              if (!parent) throw new Problem('selected_business_trial_identity_missing');
              record.derivedTrials[trialId] ??= { ...fact(trialId), campaignId: campaign.id, hypothesisId: parent.hypothesisId, parentTrialIds: [parent.id],
                runtime: { conversationId: record.conversationId, taskId: runtime.taskId }, state: 'ADMITTED', purpose,
                proposalRef: cycle.id, datasetRevisionIds: [...cycle.datasetRevisionIds], protocolRef: record.policy.trainingProtocolRef, methodVersion: record.policy.methodFamily,
                countedAttempt: true, resourceUsageRef: null, evidenceRefs: [], errorCode: null };
              const movementId = identity('budget-movement', trialId);
              record.budgetMovements[movementId] ??= { ...fact(movementId), budgetId: record.budget.id, campaignId: campaign.id, hypothesisId: record.definition.id, trialId,
                kind: 'CONSUME', resource: 'TRIAL', amount: '1', currency: null, fromReservationId: record.budget.reservationForClosureRef, toReservationId: null,
                measurementEvidenceRef: null, reasonCode: purpose === 'CAPITAL_SCALE' ? 'automatic_capital_configuration_replay' : record.mandate.capitalSource.kind === 'ACCOUNT' ? 'same_observed_state_hold_replay' : 'same_scope_cash_baseline' };
            }
            record.artifacts[cycle.id] = json(frozen);
            return undefined;
          }, ctx);
        }
        const result = await withNativeRemoteTracking(runtime.taskId, runtime, ctx, async () => isForward
          ? await computePmForward(captured, cycle, config, ctx, pmScienceCheckpoint(runtime, 'pm-forward', ctx), (reference, stored) => readPmArtifact(runtime, reference, stored, ctx))
          : await computePmDecision(captured, cycle, config, ctx, pmScienceCheckpoint(runtime, 'pm-science', ctx)));
        const inlineRefs = isForward ? ['nativeTargetEvidenceRef', 'executionQualificationEvidenceRef']
          .map(field => object(result.decision.explanationFacts)[field]).filter((value): value is string => typeof value === 'string') : [];
        const retainedArtifacts = await retainPmArtifacts(runtime, result.artifacts, ctx, inlineRefs);
        await runtime.commit(async tx => {
          const state = pmState(await tx.doc(PmDocument)); const record = pmRecord(state, task.input.mandateId); const current = record.cycles[cycle.id]!;
          // Completed scientific evidence is retained even if a later basis wins.
          Object.assign(record.evidence, result.evidence); Object.assign(record.artifacts, retainedArtifacts);
          Object.assign(record.capacities, result.capacities); Object.assign(record.ensembles, result.ensembles);
          if ('assessments' in result) Object.assign(record.assessments, result.assessments);
          Object.assign(record.derivedTrials, result.trials); Object.assign(record.budgetMovements, result.budgetMovements);
          const admitted = Object.values(record.derivedTrials).filter(item => item.proposalRef === cycle.id);
          for (const [index, trial] of admitted.entries()) {
            const settlement = result.trialSettlements[index]; revise(trial);
            trial.state = settlement?.state ?? (result.scienceCompleted ? 'REJECTED' : 'FAILED');
            trial.countedAttempt = settlement?.countedAttempt ?? !result.scienceCompleted;
            trial.evidenceRefs = settlement?.evidenceRefs ?? [];
            trial.errorCode = settlement?.errorCode ?? (result.scienceCompleted ? 'planned_configuration_not_executed' : result.decision.reasonCodes[0] ?? 'science_unavailable');
            if (!trial.countedAttempt) {
              const movementId = identity('budget-release', trial.id);
              record.budgetMovements[movementId] = { ...fact(movementId), budgetId: record.budget.id, campaignId: trial.campaignId, hypothesisId: trial.hypothesisId, trialId: trial.id,
                kind: 'RELEASE', resource: 'TRIAL', amount: '1', currency: null, fromReservationId: record.budget.reservationForClosureRef, toReservationId: null,
                measurementEvidenceRef: null, reasonCode: 'native_result_proves_configuration_not_executed' };
            }
          }
          record.comparisons[result.comparison.id] = result.comparison;
          const decision = result.decision;
          if (!isForward && decision.action === 'REBALANCE' && decision.ensembleRevisionId) {
            const ensemble = record.ensembles[decision.ensembleRevisionId]!; const strategy = record.strategies[decision.strategyVersionId]!;
            strategy.fittedEvidenceRefs = [...new Set([...strategy.fittedEvidenceRefs, ...ensemble.trainingEvidenceRefs])];
            strategy.modelRefs = ensemble.members.map(item => item.factorVersionId);
            const originalCampaign = typeof frozen.campaignId === 'string' ? record.campaigns[frozen.campaignId] : undefined;
            if (originalCampaign) { revise(originalCampaign); originalCampaign.selectedStrategyVersionId = strategy.id; }
          }
          if (record.basisRevision !== cycle.basisRevision || current.state === 'SUPERSEDED') {
            decision.status = 'SUPERSEDED'; decision.reasonCodes.push('newer_business_basis'); current.state = 'SUPERSEDED';
          } else {
            if (isForward && cycle.accountBasis && record.mandate.capitalSource.kind === 'ACCOUNT') {
              const currentAccount = captureAccount(await tx.doc(Application), record.mandate.capitalSource.accountBindingId);
              if (currentAccount.basis.positionsEvidenceRef !== cycle.accountBasis.positionsEvidenceRef
                || currentAccount.basis.outstandingCommandsRef !== cycle.accountBasis.outstandingCommandsRef) {
                decision.status = 'SUPERSEDED'; decision.reasonCodes.push('account_changed_during_current_inference'); current.state = 'SUPERSEDED';
                revise(current); current.outputDecisionId = decision.id; record.decisions[decision.id] = decision;
                return { status: 'terminal', outcome: { status: 'completed', result: null } };
              }
            }
            if (record.currentDecisionId) { const old = record.decisions[record.currentDecisionId]!; revise(old); old.status = 'SUPERSEDED'; }
            decision.status = 'CURRENT'; record.currentDecisionId = decision.id;
            if ('assessments' in result) {
              const assessment = Object.values(result.assessments as Record<string, Pm.PipelineAssessment>).find(item => item.strategyVersionId === decision.strategyVersionId);
              if (assessment) record.strategies[decision.strategyVersionId]!.pipelineAssessmentId = assessment.id;
            }
            current.state = ['NEEDS_EVIDENCE', 'NO_FEASIBLE_SOLUTION'].includes(decision.action) ? 'WAITING_EVIDENCE' : 'CONCLUDED';
          }
          revise(current); current.outputDecisionId = decision.id; current.reasonCodes = [...decision.reasonCodes]; record.decisions[decision.id] = decision;
          for (const reason of result.gaps) gap(record, reason.capability, reason.reason, reason.remedy, decision.id);
          if (!isForward && decision.status === 'CURRENT' && result.scienceCompleted && decision.action === 'REBALANCE' && decision.capital) {
            current.state = 'EVALUATING';
            const taskId = await tx.createTask(IndependentEvaluation, { mandateId: record.mandate.id, cycleId: cycle.id, decisionId: decision.id },
              { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
            record.artifacts[`independent-task:${decision.id}`] = { taskId };
          }
          if (isForward && decision.status === 'CURRENT' && decision.action === 'REBALANCE' && record.mandate.operatingAuthority === 'AUTHORIZED_TARGETS'
            && 'assessments' in result && Object.values(result.assessments as Record<string, Pm.PipelineAssessment>).some(item => item.outcome === 'READY')) {
            return { status: 'running', checkpoint: { phase: 'offer' } };
          }
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, ctx);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, ctx);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const current = record.cycles[task.input.cycleId]!;
          if (current.state !== 'SUPERSEDED') current.state = 'FAILED'; revise(current);
          const reason = error instanceof Error && /^[a-zA-Z][a-zA-Z0-9_]+$/.test(error.message) ? error.message : 'pm_native_computation_failed';
          const errorId = identity('pm-error', current.id); const detail = json({ name: error instanceof Error ? error.name : 'Error', message: error instanceof Error ? error.message : String(error), stage: 'PM_OPERATING_DECISION' });
          record.artifacts[errorId] = detail; record.evidence[errorId] = { id: errorId, sourceKind: 'DERIVED', role: 'CONTROL', runtime: current.runtime, objectKey: errorId,
            contentDigest: digest(detail), mediaType: 'application/json', sourceRevision: current.basisRevision, eventInterval: null, availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'qz.pm.error/1', visibility: 'OWNER_ONLY' };
          current.reasonCodes = [reason]; gap(record, 'scientific_decision', reason, 'RECOMPUTE', current.id);
          for (const trial of Object.values(record.derivedTrials).filter(item => item.proposalRef === current.id && item.state === 'ADMITTED')) { revise(trial); trial.state = 'FAILED'; trial.errorCode = reason; }
          return { status: 'terminal', outcome: { status: 'failed', error: { message: reason } } };
        }, ctx);
      }
    }, async offer(task, runtime, ctx) {
      await ready;
      const record = pmRecord(pmState(await runtime.snapshot(PmDocument, ctx)), task.input.mandateId);
      const decisionId = record.cycles[task.input.cycleId]!.outputDecisionId;
      if (decisionId) {
        try { await offerTarget(record.mandate.id, { requestId: identity('auto-forward-target', decisionId), expectedRevision: record.mandate.revision, decisionId }); }
        catch (error) { await runtime.commit(async tx => { const current = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
          gap(current, 'target_delivery', error instanceof Error ? error.message : 'current_target_delivery_rejected', 'REFRESH_SOURCE', decisionId); return undefined; }, ctx); }
      }
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
    } },
    async abort(task, runtime, ctx) { await runtime.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const cycle = record.cycles[task.input.cycleId]!;
      if (cycle.state !== 'SUPERSEDED') { revise(cycle); cycle.state = 'FAILED'; cycle.reasonCodes = ['owner_stopped_local_compute']; }
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, ctx); },
  });

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
        const state = pmState(await runtime.snapshot(PmDocument, ctx)); const record = pmRecord(state, task.input.mandateId);
        const campaign = Object.values(record.campaigns).at(-1)!;
        if (campaign.state === 'PAUSED' && campaign.conclusionReason === 'intent_revision_requires_new_unexposed_data') {
          await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } }), ctx); return;
        }
        const consumed = Object.values(record.derivedTrials).filter(item => item.countedAttempt).length;
        const researchAllowance = record.budget.maximumTrials - consumed - downstreamTrialReservation(record.budget.maximumTrials);
        if (researchAllowance < 1) {
          await runtime.commit(async tx => { const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); stored.campaigns[campaign.id]!.state = 'PAUSED';
            stored.campaigns[campaign.id]!.conclusionReason = 'shared_trial_budget_exhausted'; gap(stored, 'shared_research_budget', 'shared_trial_budget_exhausted', 'CLARIFY_INTENT');
            return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
          }, ctx); return;
        }
        if (record.budget.maximumTrials < 8) {
          await runtime.commit(async tx => { const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
            gap(stored, 'shared_research_budget', 'needs_budget_for_search_fit_joint_replay_and_capital_comparison', 'CLARIFY_INTENT');
            return undefined;
          }, ctx);
        }
        if (record.artifacts[`campaign-control:${campaign.id}`]?.action === 'pause') { await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } }), ctx); return; }
        if (campaign.runtime.conversationId !== record.conversationId) { await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'observe' } }), ctx); return; }
        const registered = state.datasets[record.datasets.at(-1)!]!;
        const configured = { ...registered, data: pmScienceData(record, registered.data) };
        const raw = object(configured.data.catalog_ensemble_request ?? configured.data.catalog_scan_request ?? configured.data.request);
        const trainEnd = counter(configured.data.train_end_ns ?? raw.train_end_ns, 'train_end');
        const validationEnd = counter(configured.data.validation_end_ns ?? raw.validation_end_ns, 'validation_end');
        const researchSettings = configured.strategy?.simulationSettings ? object(configured.strategy.simulationSettings) : raw.settings ? object(raw.settings) : null;
        const binding: ResearchCampaignBinding = { campaignId: campaign.id, mandateRevisionId: record.mandate.id,
          policy: json(record.policy) as unknown as ResearchCampaignBinding['policy'], intakeRequired: true, independentConsumer: 'PM_PORTFOLIO',
          ownerBoundaries: json(record.mandate.ownerBoundaries) as unknown as NonNullable<ResearchCampaignBinding['ownerBoundaries']>,
          ownerClarificationStatements: (object(record.policy.validDomain).ownerClarificationStatements ?? []) as string[],
          budget: json({ ...record.budget, maximumTrials: researchAllowance }) as unknown as ResearchCampaignBinding['budget'],
          scopes: [{ id: identity('scope', [campaign.id, 'SELECTION']), allowedUniverseRef: record.mandate.allowedUniverseRef, datasetRevisionIds: [...record.datasets],
            interval: { startNs: trainEnd, endExclusiveNs: validationEnd }, role: 'SELECTION', calendarId: 'UTC_COMPLETE_DAYS', labelDefinitionId: null,
            accountingPolicyRef: 'qz.single-currency-shared-account/1', costModelRef: identity('cost', configured.strategy), startingStateRef: identity('fresh-research', record.mandate.capitalSource),
            capital: researchSettings && typeof researchSettings.starting_capital === 'string' && typeof researchSettings.base_currency === 'string' ? { amount: researchSettings.starting_capital, currency: researchSettings.base_currency } : null }],
          definitions: pmMetricDefinitions(), frontierDefinitionIds: pmMetricDefinitions().map(item => item.id), datasetRevisionIds: [...record.datasets], protocolRef: campaign.evaluationProtocolRef };
        try {
          const target = boundScienceTarget(config, configured.data);
          const receipt = await work.startResearch({ requestId: `pm:${campaign.id}`, input: {
            goal: record.input.goal, model: record.input.model, data: configured.data,
            ...(record.input.thinkingLevel !== undefined ? { thinkingLevel: record.input.thinkingLevel } : {}),
            ...(record.input.serviceTier !== undefined ? { serviceTier: record.input.serviceTier } : {}),
            policy: { ...configured.policy, max_trials: researchAllowance, ...(configured.strategy ? { strategy: configured.strategy } : {}) }, campaign: binding,
          } }, typeof target === 'string' ? undefined : target);
          await runtime.commit(async tx => {
            const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const current = stored.campaigns[campaign.id]!;
            revise(current); current.runtime.conversationId = receipt.conversationId; current.state = 'SEARCHING';
            return { status: 'running', checkpoint: { phase: 'observe' } };
          }, ctx);
        } catch (error) {
          await retainNativeRemoteUncertainty(error, runtime.taskId, runtime, ctx);
          if (runtime.signal.aborted) throw error;
          await runtime.commit(async tx => { const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
            const reason = error instanceof Error && /^[a-z][a-z0-9_]+$/.test(error.message) ? error.message : 'campaign_start_failed';
            gap(stored, 'research_runtime', reason, 'RECOMPUTE'); stored.campaigns[campaign.id]!.state = 'PAUSED'; stored.campaigns[campaign.id]!.conclusionReason = reason;
            return { status: 'terminal', outcome: { status: 'failed', error: { message: reason } } };
          }, ctx);
        }
      },
      async observe(task, runtime, ctx) {
        const { host, work } = await ready;
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
        if (!maintenanceOnly && !retired && campaign.state === 'PLANNED') { await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'start' } }), ctx); return; }
        let source: Awaited<ReturnType<typeof readSourceRevision>> = null; let sourceError: string | null = null;
        const configured = config.datasets.find(item => item.id === record.input.datasetId)!;
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
        const original = campaign.runtime.conversationId !== record.conversationId ? await host.state(campaign.runtime.conversationId) : null;
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
          const research = original?.research; const scientific = original?.campaignResearch;
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
          if (scientific?.binding && !maintenanceOnly) {
            current.state = stored.artifacts[`campaign-control:${current.id}`]?.action === 'pause' ? 'PAUSED' : scientific.state; current.trialIds = scientific.trials.map(item => item.id); current.hypothesisIds = scientific.hypotheses.map(item => item.id);
            current.latestComparisonId = scientific.comparisons.at(-1)?.id ?? null; current.conclusionReason = scientific.closingReason;
            for (const item of scientific.trials) stored.derivedTrials[item.id] = item;
            for (const movement of scientific.budgetMovements) stored.budgetMovements[movement.id] = movement;
            if (scientific.intake) {
              if (scientific.intake.sourceGoal !== stored.mandate.ownerGoal) throw new Problem('intake_owner_goal_mismatch');
              const intakeEvidenceId = identity('strategy-intake', current.id);
              retainIntentEvidence(stored, intakeEvidenceId, json(scientific.intake), nowNs(), 'qz.pm.strategy-intake/1');
              const acceptedBoundaries = mergeOwnerBoundaries(stored.mandate.ownerBoundaries, scientific.intake.ownerBoundaries as Pm.OwnerBoundary[]);
              if (canonical(json(acceptedBoundaries)) !== canonical(json(stored.mandate.ownerBoundaries))) reviseOwnerIntent(stored, acceptedBoundaries,
                (object(stored.policy.validDomain).ownerClarificationStatements ?? []) as string[], intakeEvidenceId, nowNs());
              const currentReasons = new Set(scientific.intake.missingCapabilities);
              if (scientific.intake.support === 'NEEDS_CLARIFICATION') currentReasons.add('strategy_intake_needs_clarification');
              if (scientific.intake.support === 'UNSUPPORTED') currentReasons.add('strategy_archetype_unsupported');
              for (const priorGap of Object.values(stored.gaps)) if ((['owner_boundary_normalization_required', 'strategy_intake_needs_clarification', 'strategy_archetype_unsupported', 'pm_account_consumer_not_requested'].includes(priorGap.reasonCode) || priorGap.reasonCode.startsWith('strategy_archetype:'))
                && !currentReasons.has(priorGap.reasonCode) && priorGap.state !== 'RESOLVED') { revise(priorGap); priorGap.state = 'RESOLVED'; priorGap.resolutionEvidenceRefs = [intakeEvidenceId]; }
              stored.definition = scientific.intake.strategy; current.strategyDefinitionId = stored.definition.id;
              if (currentStrategy(stored).definitionId !== stored.definition.id) {
                const version: Pm.StrategyVersion = { ...currentStrategy(stored), ...fact(identity('strategy', [stored.mandate.id, stored.datasets, stored.definition.id])), definitionId: stored.definition.id };
                const assessment = assessStage(version, 'RESEARCH', [], ['frozen_strategy_evidence_pending']); version.pipelineAssessmentId = assessment.id;
                stored.strategies[version.id] = version; stored.assessments[assessment.id] = assessment; stored.latestStrategyId = version.id;
              }
              if (scientific.intake.support === 'UNSUPPORTED') gap(stored, 'strategy_archetype', 'strategy_archetype_unsupported', 'UNSUPPORTED');
              if (scientific.intake.support === 'NEEDS_CLARIFICATION') gap(stored, 'strategy_intake', 'strategy_intake_needs_clarification', 'CLARIFY_INTENT');
              for (const missing of scientific.intake.missingCapabilities) gap(stored, missing, missing, ['owner_boundary_normalization_required', 'pm_account_consumer_not_requested'].includes(missing) ? 'CLARIFY_INTENT' : 'UNSUPPORTED');
            }
          }
          if (!maintenanceOnly && research?.final && current.state !== 'PAUSED' && stored.artifacts[`strategy-control:${stored.latestStrategyId}`]?.action !== 'pause' && !retired && (!scientific?.binding || scientific.state === 'CONCLUDED')) {
            if (!scientific?.binding) { current.state = 'CONCLUDED'; current.conclusionReason = 'legacy_research_has_no_pm_comparison'; }
            const source = identity('research-evidence', [campaign.runtime.conversationId, research.trials, research.final]);
            await enqueue(tx, latest, stored, { id: identity('event', source), kind: 'RESEARCH_EVIDENCE', sourceRevisionRef: source,
              availableAtNs: nowNs(), affectedMandateIds: [stored.mandate.id], supersedesEventId: null });
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
              if (!retired && !maintenanceOnly) {
                const seed = forwardSeedDecision(stored);
                if (seed) {
                  const explanation = object(seed.explanationFacts); const anchor = BigInt(String(explanation.forwardAnchorNs));
                  const ttl = BigInt(String(explanation.forwardTtlSeconds)) * 1_000_000_000n; const atNs = nowNs();
                  if (BigInt(atNs) >= anchor) {
                    const slot = ((BigInt(atNs) - anchor) / ttl).toString();
                    if (!stored.artifacts[`forward-slot:${seed.id}:${slot}`]) await enqueue(tx, latest, stored, {
                      id: identity('forward-clock-event', [seed.id, slot]), kind: 'FRESHNESS_DEADLINE', sourceRevisionRef: `${seed.id}:${slot}`,
                      availableAtNs: atNs, affectedMandateIds: [stored.mandate.id], supersedesEventId: null });
                  }
                }
              }
            } catch (error) { gap(stored, 'account', error instanceof Problem ? error.message : 'account_basis_unavailable', 'REFRESH_SOURCE'); }
          }
          stored.searchFamily.trialIds = Object.keys(stored.derivedTrials);
          stored.searchFamily.countedAttempts = Object.values(stored.derivedTrials).filter(item => item.countedAttempt).length.toString();
          stored.searchFamily.failedAttempts = Object.values(stored.derivedTrials).filter(item => item.countedAttempt && item.state === 'FAILED').length.toString();
          const changedSource = canonical(json(current.datasetRevisionIds)) !== canonical(json(stored.datasets));
          if (!maintenanceOnly && !retired && latest.datasets[stored.datasets.at(-1)!]!.revision.revisionKind !== 'RETRACTION' && stored.artifacts[`strategy-control:${stored.latestStrategyId}`]?.action !== 'pause' && changedSource && ['CONCLUDED', 'PAUSED'].includes(current.state)) {
            const nextId = identity('campaign', [stored.mandate.id, stored.datasets]);
            if (!stored.campaigns[nextId]) {
              const fresh = { ...currentStrategy(stored), ...fact(identity('strategy', [stored.mandate.id, stored.datasets])), modelRefs: [], fittedEvidenceRefs: [] };
              const assessment = assessStage(fresh, 'RESEARCH', [], ['new_data_requires_refit_and_independent_protocol']); fresh.pipelineAssessmentId = assessment.id;
              stored.strategies[fresh.id] = fresh; stored.latestStrategyId = fresh.id; stored.assessments[assessment.id] = assessment;
              stored.campaigns[nextId] = { ...current, ...fact(nextId), runtime: { conversationId: stored.conversationId, taskId: stored.monitorTaskId }, state: 'PLANNED', datasetRevisionIds: [...stored.datasets],
                hypothesisIds: [], trialIds: [], latestComparisonId: null, selectedStrategyVersionId: null, conclusionReason: null };
              admitCampaignAccount(stored, stored.campaigns[nextId]!, await tx.doc(Application));
              stored.searchFamily.campaignIds.push(nextId);
              return { status: 'running', checkpoint: { phase: 'start' } };
            }
          }
          return { status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 5000 } };
        }, ctx);
      },
      async wait(task, runtime, ctx) { await runtime.sleep(task.state.checkpoint.wakeAt, ctx); await runtime.commit(async tx => pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId).deletedAt
        ? { status: 'terminal', outcome: { status: 'completed', result: null } } : { status: 'running', checkpoint: { phase: 'observe' } }, ctx); },
      async maintenance(task, runtime, ctx) { await runtime.commit(async tx => pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId).deletedAt
        ? { status: 'terminal', outcome: { status: 'completed', result: null } } : resumeMaintenance(tx, runtime.taskId, 'qz.pm.mandate-monitor', task.state.checkpoint), ctx); },
    },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });

  async function start(value: unknown) {
    await ready; const input = mandateStart(value); const original = json(value);
    const replay = pmState(await dependencies.host.harness.snapshot(PmDocument, context)).commands[`mandate:${input.requestId}`];
    if (replay) { if (canonical(replay.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return get(replay.mandateId); }
    const configured = config.datasets.find(item => item.id === input.datasetId); if (!configured) throw new Problem('dataset_not_found', 404);
    const selectedModel = (await dependencies.host.availableModels(input.model.provider)).find(model => model.modelId === input.model.modelId);
    if (!selectedModel) throw new Problem('model_unavailable', 409);
    const preferences = await dependencies.work.getSettings();
    const preferredModel = preferences.defaultModel?.provider === input.model.provider && preferences.defaultModel?.modelId === input.model.modelId;
    input.thinkingLevel ??= (preferredModel ? preferences.thinkingLevel : null) ?? selectedModel.effectiveThinkingLevel;
    const serviceTier = input.serviceTier ?? (preferredModel ? preferences.serviceTier : null);
    if (serviceTier !== null && serviceTier !== undefined) input.serviceTier = serviceTier;
    if (!selectedModel.thinkingLevels.includes(input.thinkingLevel)) throw new Problem('thinking_level_unsupported');
    if (input.serviceTier !== undefined && !selectedModel.serviceTiers.includes(input.serviceTier)) throw new Problem('service_tier_unsupported');
    const source = await readSourceRevision(configured); if (source) configured.data = source.configured.data;
    const id = await dependencies.host.harness.commit(async tx => {
      const state = pmState(await tx.doc(PmDocument)); const key = `mandate:${input.requestId}`; const replay = state.commands[key];
      if (replay) { if (canonical(replay.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); assertPmNotDeleted(pmRecord(state, replay.mandateId)); return replay.mandateId; }
      assertBusinessAdmission(await tx.doc(Application));
      const atNs = nowNs(); const id = identity('mandate', input.requestId); const policyId = identity('policy', [id, 'pm-method/1']);
      const dataset = captureDataset(configured, state, atNs, source); let currency: string;
      let initialAccount: ReturnType<typeof captureAccount> | null = null;
      if (input.funding.kind === 'ACCOUNT') {
        const application = await tx.doc(Application);
        initialAccount = captureAccount(application, input.funding.accountBindingId, atNs); currency = initialAccount.basis.currency;
        if (input.operatingAuthority === 'AUTHORIZED_TARGETS') assertAccountTargetControl(application, state,
          observation(application.observations[input.funding.accountBindingId]).connection, { kind: 'PM', mandateId: id, accountBindingId: input.funding.accountBindingId });
      }
      else currency = input.funding.capital.currency;
      if (configured.currency !== currency) throw new Problem('mandate_currency_mismatch');
      const selection = selectionRule(input.businessPreference);
      const budget: Pm.ResearchBudget = { ...fact(identity('budget', id), atNs), maximumTrials: input.maxTrials, nativeComputeSeconds: null, modelSpend: null, dataBytes: null,
        reservationForClosureRef: identity('pm-reservation', id), usageEvidenceRefs: [] };
      const mandate: Pm.MandateRevision = validatePmEntity('MandateRevision', { ...fact(id, atNs), ownerGoal: input.goal, interpretation: input.goal,
        baseCurrency: currency, capitalSource: input.funding, allowedUniverseRef: identity('registered-source-universe', [configured.id, configured.source]), ownerBoundaries: [], selection,
        operatingAuthority: input.operatingAuthority ?? 'RESEARCH_ONLY', researchBudgetRef: budget.id,
        unresolvedIntent: selection.mode === 'FRONTIER_ONLY' ? ['investment_tradeoff_not_selected'] : [] });
      const policy: Pm.DecisionPolicyRevision = { ...fact(policyId, atNs), mandateRevisionId: id, methodFamily: 'qz.pm.train-only-ensemble-shared-account/1',
        nativeImplementationVersion: 'qz-job/1', learnedParametersRef: null, trainingProtocolRef: 'qz.pm.expanding-train-purged-oof/1', validationEvidenceRefs: [],
        applicableCapabilityIds: [PM_CAPABILITY_ID], validDomain: { data: 'BAR', accounting: 'single-currency-fresh-research-account', strategy: configured.strategy?.kind ?? null, ownerBoundaries: [], ownerClarificationStatements: [], jointAllocationMethod: json(pmAllocationMethod()), qualificationProtocol: pmQualificationProtocol(selection), operatingRule: pmOperatingRule() },
        numericalToleranceRef: 'qz.native.solver-tolerances/1', selectionRule: selection,
        limitations: ['historical_replay_is_not_economic_capacity', 'no_real_orders', 'unverified_receive_clock', 'account_operation_requires_matched_live_evidence'] };
      const definition: Pm.StrategyDefinition = { ...fact(identity('strategy-definition', id), atNs), name: input.goal.slice(0, 120), archetype: 'FACTOR_RESEARCH', economicMechanism: input.goal,
        falsificationConditions: ['net_account_objective_not_supported_in_matched_validation'], opportunityUniverseRef: mandate.allowedUniverseRef,
        requiredCapabilities: [PM_CAPABILITY_ID], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['selection_bias', 'cost_and_capacity_uncertainty'] };
      const strategy: Pm.StrategyVersion = { ...fact(identity('strategy', [id, 'initial']), atNs), definitionId: definition.id, mandateRevisionId: id, policyRevisionId: policyId,
        modelRefs: [], featureSchemaRef: 'qz.native.registered-factor-features/1', signalContractRef: null, stateSchemaRef: null, entryRuleRef: null, holdingRuleRef: null,
        invalidationRuleRef: 'qz.pm.dependency-revision/1', portfolioRuleRef: null, executionRuleRef: null, exitRuleRef: null, componentStrategyVersionIds: [],
        capabilityBindingIds: [PM_CAPABILITY_ID], fittedEvidenceRefs: [], pipelineAssessmentId: identity('assessment-pending', id) };
      const assessment = assessStage(strategy, 'RESEARCH', [], ['frozen_strategy_evidence_pending']); strategy.pipelineAssessmentId = assessment.id;
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const campaignId = identity('campaign', id);
      const campaign: Pm.ResearchCampaign = { ...fact(campaignId, atNs), mandateRevisionId: id, strategyDefinitionId: definition.id, state: 'PLANNED',
        runtime: { conversationId: conversation.id, taskId: null }, datasetRevisionIds: [dataset.revision.id], evaluationProtocolRef: 'qz.pm.validation-plus-independent/1', budgetId: budget.id,
        hypothesisIds: [], trialIds: [], latestComparisonId: null, selectedStrategyVersionId: null, conclusionReason: null };
      const record: PmRecord = { input, conversationId: conversation.id, mandate, policy, budget, definition,
        strategies: { [strategy.id]: strategy }, assessments: { [assessment.id]: assessment }, datasets: [dataset.revision.id], events: {}, campaigns: { [campaignId]: campaign },
        cycles: {}, decisions: {}, comparisons: {}, capacities: {}, ensembles: {}, evidence: {}, artifacts: {}, outcomes: {}, attributions: {}, retirements: {}, commands: {}, gaps: {},
        currentDecisionId: null, latestCycleId: null, latestStrategyId: strategy.id, basisRevision: 'initial', monitorTaskId: null, derivedTrials: {}, budgetMovements: {},
        searchFamily: { ...fact(identity('search-family', id), atNs), economicFamilyId: definition.id, campaignIds: [campaignId], trialIds: [], methodVariantRefs: [policyId],
          countedAttempts: '0', failedAttempts: '0', costEvidenceRefs: [], selectionExposureRefs: [], effectiveTrialEstimate: null } };
      if (config.nautilus?.backtest) record.artifacts['nautilus-binding'] = json({ target: config.nautilus.backtest });
      if (initialAccount) freezeCampaignAccount(record, campaign, initialAccount);
      const reservationId = budget.reservationForClosureRef;
      const reserve = downstreamTrialReservation(input.maxTrials);
      if (reserve > 0) record.budgetMovements[reservationId] = { ...fact(reservationId, atNs), budgetId: budget.id, campaignId, hypothesisId: null, trialId: null,
        kind: 'RESERVE', resource: 'TRIAL', amount: String(reserve), currency: null, fromReservationId: null, toReservationId: reservationId,
        measurementEvidenceRef: null, reasonCode: 'reserve_native_capital_configurations_and_cash_baseline' };
      record.monitorTaskId = await tx.createTask(MonitorMandate, { mandateId: id }, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
      campaign.runtime.taskId = record.monitorTaskId;
      state.mandates[id] = record; state.commands[key] = { original, mandateId: id, resourceId: id }; return id;
    }, context);
    return get(id);
  }
  async function get(id: string) { await ready; const state = pmState(await dependencies.host.harness.snapshot(PmDocument, context)); return pmView(visiblePmRecord(state, id), state); }
  async function list() { await ready; const state = pmState(await dependencies.host.harness.snapshot(PmDocument, context)); return Object.values(state?.mandates ?? {}).filter(record => !record.deletedAt).map(record => pmView(record, state)); }
  async function clarifyIntent(id: string, value: unknown) {
    await ready; const input = clarificationInput(value); const original = json(value);
    await dependencies.host.harness.commit(async tx => {
      const state = pmState(await tx.doc(PmDocument)); const record = pmRecord(state, id); assertPmNotDeleted(record);
      const key = `${id}:intent-clarification:${input.requestId}`; const replay = state.commands[key];
      if (replay) { if (canonical(replay.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return; }
      assertBusinessAdmission(await tx.doc(Application)); ensureRevision(record, input.expectedRevision);
      if (Object.values(record.retirements).some(item => item.strategyVersionId === record.latestStrategyId)) throw new Problem('strategy_retired', 409);
      const previous = Object.values(record.campaigns).at(-1)!;
      if (!['CONCLUDED', 'PAUSED'].includes(previous.state)) throw new Problem('intent_clarification_requires_concluded_or_paused_campaign', 409);
      const conversations = Object.values(record.campaigns).map(item => item.runtime.conversationId).filter(item => item !== record.conversationId);
      for (const conversationId of conversations) {
        const research = await tx.doc(CampaignResearchDoc, conversationId as ConversationId);
        if (conversationId === previous.runtime.conversationId && research.state !== 'CONCLUDED') throw new Problem('intent_clarification_requires_research_conclusion', 409);
        for (const trial of research.trials) record.derivedTrials[trial.id] = trial;
        for (const movement of research.budgetMovements) record.budgetMovements[movement.id] = movement;
      }
      const previousIntake = record.artifacts[identity('strategy-intake', previous.id)];
      const ambiguityList = Array.isArray(previousIntake?.ambiguities) ? previousIntake.ambiguities as string[] : [];
      if (input.resolvedAmbiguities.some(item => !ambiguityList.includes(item))) throw new Problem('intent_ambiguity_not_recorded');
      const oldStatements = (object(record.policy.validDomain).ownerClarificationStatements ?? []) as string[];
      const statements = [...new Set([...oldStatements, input.statement])];
      const sourceStatements = [record.mandate.ownerGoal, ...statements]; const atNs = nowNs();
      const proposed = input.boundaries.map(boundary => {
        if (!sourceStatements.some(source => source.includes(boundary.sourceStatement))) throw new Problem('owner_boundary_source_not_in_owner_statement');
        return canonicalOwnerBoundary({ ...boundary, id: identity('owner-boundary', [id, boundary.kind, boundary.sourceStatement, boundary.value]), effectiveFromNs: atNs });
      });
      const boundaries = mergeOwnerBoundaries(record.mandate.ownerBoundaries, proposed);
      const evidenceId = identity('intent-clarification', [id, input.requestId]);
      const priorMandateRevision = record.mandate.revision; const priorPolicyId = record.policy.id;
      const priorRevisionEvidenceRef = reviseOwnerIntent(record, boundaries, statements, evidenceId, atNs);
      retainIntentEvidence(record, evidenceId, json({ ...input, priorMandateRevision, priorPolicyId, priorRevisionEvidenceRef }), atNs);
      // The owner can add information, but only a new structured intake resolves
      // earlier ambiguity. No command string erases missing scientific evidence.
      const allRelated = new Set([...conversations, record.conversationId]);
      const exposure = await tx.doc(Exposures);
      const hasExposure = exposure.windows.some(item => allRelated.has(Number(item.conversationId)));
      const consumed = Object.values(record.derivedTrials).filter(item => item.countedAttempt).length;
      const canRestart = consumed === 0 && !hasExposure;
      const campaignId = identity('campaign', [id, record.mandate.revision, record.policy.id, record.datasets]);
      record.campaigns[campaignId] = { ...previous, ...fact(campaignId, atNs), supersedesId: previous.id, state: canRestart ? 'PLANNED' : 'PAUSED',
        runtime: { conversationId: record.conversationId, taskId: record.monitorTaskId }, datasetRevisionIds: [...record.datasets], hypothesisIds: [], trialIds: [],
        latestComparisonId: null, selectedStrategyVersionId: null, conclusionReason: canRestart ? null : 'intent_revision_requires_new_unexposed_data' };
      admitCampaignAccount(record, record.campaigns[campaignId]!, await tx.doc(Application), atNs);
      record.searchFamily.campaignIds.push(campaignId); record.searchFamily.trialIds = Object.keys(record.derivedTrials);
      record.searchFamily.countedAttempts = String(consumed);
      if (!canRestart) gap(record, 'owner_intent_research_window', 'intent_revision_requires_new_unexposed_data', 'COLLECT_AUTHORIZED_DATA', campaignId);
      const event: Pm.BusinessEvent = { id: identity('event', evidenceId), kind: 'MANDATE_CHANGED', sourceRevisionRef: evidenceId, availableAtNs: atNs,
        affectedMandateIds: [id], supersedesEventId: null };
      record.events[event.id] = event;
      const priorMonitor = record.monitorTaskId ? await tx.task(record.monitorTaskId as TaskId) : null;
      if (!priorMonitor || priorMonitor.state.status === 'terminal') record.monitorTaskId = await tx.createTask(MonitorMandate, { mandateId: id },
        { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
      record.campaigns[campaignId]!.runtime.taskId = record.monitorTaskId;
      state.commands[key] = { original, mandateId: id, resourceId: campaignId };
    }, context);
    return get(id);
  }
  async function command(id: string, kind: 'recompute' | 'retire' | 'refresh-data', value: unknown) {
    await ready; const body = object(value); const requestId = text(body.requestId, 'request_id'); const original = json(value);
    const before = pmState(await dependencies.host.harness.snapshot(PmDocument, context)); assertPmNotDeleted(pmRecord(before, id)); const existing = before.commands[`${id}:${kind}:${requestId}`];
    if (existing) { if (canonical(existing.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); if (kind !== 'retire') return get(id); }
    const configuredSource = kind === 'refresh-data' ? config.datasets.find(item => item.id === pmRecord(before, id).input.datasetId)! : null;
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
        await enqueue(tx, state, record, { id: identity('event', [id, kind, requestId]), kind: kind === 'refresh-data' ? 'DATA_REVISION' : forwardSeedDecision(record) ? 'FRESHNESS_DEADLINE' : 'RESEARCH_EVIDENCE',
          sourceRevisionRef, availableAtNs: nowNs(), affectedMandateIds: [id], supersedesEventId: null });
      }
      state.commands[key] = { original, mandateId: id, resourceId: record.latestCycleId ?? id };
    }, context);
    if (kind === 'retire') { const view = await get(id); const strategyId = body.strategyVersionId === undefined ? view.strategies.at(-1)!.id : text(body.strategyVersionId, 'strategy_version'); if (view.strategies.at(-1)!.id === strategyId) await controls.strategyCommand(strategyId, 'pause', value); }
    return get(id);
  }
  async function offerTarget(id: string, value: unknown) {
    await ready; const body = object(value); const requestId = text(body.requestId, 'request_id');
    await dependencies.host.harness.commit(async tx => {
      const state = pmState(await tx.doc(PmDocument)); const record = pmRecord(state, id); assertPmNotDeleted(record); const key = `${id}:target:${requestId}`;
      const original = json(value); const replay = state.commands[key];
      if (replay) { if (canonical(replay.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return; }
      assertBusinessAdmission(await tx.doc(Application));
      ensureRevision(record, body.expectedRevision);
      const decisionId = text(body.decisionId, 'decision_id'); const decision = record.decisions[decisionId]; if (!decision) throw new Problem('decision_not_found', 404);
      if (record.mandate.capitalSource.kind !== 'ACCOUNT') throw new Problem('target_account_binding_required', 409);
      const account = captureAccount(await tx.doc(Application), record.mandate.capitalSource.accountBindingId);
      if (decision.targetCommandId) throw new Problem('target_decision_already_offered', 409);
      const application = await tx.doc(Application); const source = observation(application.observations[account.basis.accountBindingId]);
      assertAccountTargetControl(application, state, source.connection, { kind: 'PM', mandateId: id, accountBindingId: account.basis.accountBindingId });
      targetEligibility(record, decision, account.basis, source.connection.mode);
      const targetId = identity('target-command', [id, requestId]); const payloadRef = identity('target-payload', targetId);
      const target = pmTargetPayload(record, decision);
      const allocation = pmTargetAllocation(record, decision, source, application.accountControls);
      record.artifacts[payloadRef] = target;
      const allocationRef = `target-allocation:${targetId}`;
      record.artifacts[allocationRef] = allocation;
      record.evidence[allocationRef] = { id: allocationRef, sourceKind: 'DERIVED', role: 'CONTROL', runtime: { conversationId: record.conversationId, taskId: null }, objectKey: allocationRef,
        contentDigest: digest(allocation), mediaType: 'application/json', sourceRevision: decision.basisRevision, eventInterval: null, availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'qz.revision-bound-allocation/1', visibility: 'OWNER_ONLY' };
      record.evidence[payloadRef] = { id: payloadRef, sourceKind: 'DERIVED', role: 'CONTROL', runtime: { conversationId: record.conversationId, taskId: null }, objectKey: payloadRef, contentDigest: digest(target), mediaType: 'application/json', sourceRevision: decision.basisRevision, eventInterval: null, availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'qz.target-payload/1', visibility: 'OWNER_ONLY' };
      record.commands[targetId] = { ...fact(targetId), decisionId, idempotencyKey: targetId, accountBindingId: account.basis.accountBindingId, basisRevision: decision.basisRevision,
        controlRevision: account.basis.controlRevision, payloadEvidenceRef: payloadRef, validUntilNs: decision.validUntilNs!, state: 'PENDING', externalReceiptRef: null };
      decision.targetCommandId = targetId;
      const taskId = await tx.createTask(PmTargetTask, { mandateId: id, commandId: targetId }, { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
      record.artifacts[`target-task:${targetId}`] = { taskId };
      state.commands[key] = { original, mandateId: id, resourceId: targetId };
    }, context); return get(id);
  }
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
  return { extension: defineExtension({ name: 'qz.pm', tasks: [MonitorMandate, ComputeCycle, IndependentEvaluation, PmTargetTask, controls.task] }), connect,
    work: { start, get, list, delete: remove, readResource: controls.readResource, campaignCommand: controls.campaignCommand, strategyCommand: controls.strategyCommand, startCampaign: controls.startCampaign, recompute: (id: string, value: unknown) => command(id, 'recompute', value), retire: (id: string, value: unknown) => command(id, 'retire', value),
      refreshData: (id: string, value: unknown) => command(id, 'refresh-data', value), offerTarget, clarifyIntent,
      events: async (id: string) => Object.values(visiblePmRecord(pmState(await dependencies.host.harness.snapshot(PmDocument, context)), id).events),
      snapshot: () => dependencies.host.harness.snapshot(PmDocument, context) } };
}

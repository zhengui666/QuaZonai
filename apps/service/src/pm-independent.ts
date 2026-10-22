import { defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { checkExposure } from '../../research/plugins/native-exposure.ts';
import { researchRuntime } from '../../research/src/research-runtime.ts';
import { evaluatePmIndependent } from './pm-science.ts';
import type { ServiceConfig } from './config.ts';
import { json, nowNs, object, Problem } from './facts.ts';
import { PmDocument, digest, fact, gap, identity, pmRecord, pmState, revise } from './pm-facts.ts';
import { projectNativeIndependentMetrics } from '../../../packages/contracts/src/decision.ts';
import { assessStage, pmMetricDefinitions, pmDiagnosticDefinitions } from './pm-registry.ts';
import { assertPmDataAvailable } from './pm-datasets.ts';
import { evaluatePmQualification } from './pm-qualification.ts';
import { validatePmAllocationMethod } from './pm-method.ts';
import { pmArtifactDigest, pmScienceCheckpoint, retainPmArtifacts } from './pm-evidence.ts';
import type { CapturedAccount } from './pm-account.ts';
import { campaignAccount } from './pm-campaign-account.ts';
import { canonical } from '../../research/src/documents.ts';
import { validatePmEntity } from '../../../packages/contracts/src/pm-validation.ts';

/** The selected method/capital are immutable before this owner-only task sees FINAL. */
export function pmIndependentTask(config: ServiceConfig) {
  return defineTask<{ mandateId: string; cycleId: string; decisionId: string }, { phase: 'evaluate' }, null>({
    name: 'qz.pm.independent-evaluation', version: 1, initial: () => ({ phase: 'evaluate' }),
    phases: { async evaluate(task, runtime, ctx) {
      const trialId = identity('pm-independent-trial', task.input.decisionId);
      const baselineTrialId = identity('pm-independent-baseline-trial', task.input.decisionId);
      try {
        let admitted = false;
        await runtime.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
          const cycle = record.cycles[task.input.cycleId]!; const decision = record.decisions[task.input.decisionId]!;
          if (record.derivedTrials[trialId] && record.derivedTrials[baselineTrialId]) { admitted = true; return undefined; }
          if (cycle.state === 'SUPERSEDED' || decision.status === 'SUPERSEDED' || record.basisRevision !== cycle.basisRevision) return { status: 'terminal', outcome: { status: 'completed', result: null } };
          if (record.policy.id !== decision.policyRevisionId || cycle.policyRevisionId !== decision.policyRevisionId) throw new Problem('independent_selected_policy_mismatch');
          const parents = Object.values(record.derivedTrials).filter(item => item.proposalRef === cycle.id && item.state === 'COMPUTED');
          const parent = parents.find(item => item.evidenceRefs.includes(decision.jointReplayEvidenceRef!));
          if (!parent || !decision.capital) throw new Problem('independent_selected_configuration_missing');
          const newIds = [trialId, baselineTrialId].filter(id => !record.derivedTrials[id]);
          if (Object.values(record.derivedTrials).filter(item => item.countedAttempt).length + newIds.length > record.budget.maximumTrials) throw new Problem('independent_shared_trial_budget_exhausted');
          for (const id of newIds) {
          record.derivedTrials[id] = { ...fact(id), campaignId: parent.campaignId, hypothesisId: parent.hypothesisId, parentTrialIds: [parent.id],
            runtime: { conversationId: record.conversationId, taskId: runtime.taskId }, state: 'ADMITTED', purpose: 'ROBUSTNESS', proposalRef: decision.id,
            datasetRevisionIds: [...cycle.datasetRevisionIds], protocolRef: record.campaigns[parent.campaignId]!.evaluationProtocolRef,
            methodVersion: record.policy.methodFamily, countedAttempt: true, resourceUsageRef: null, evidenceRefs: [], errorCode: null };
          const movementId = identity('budget-movement', id);
          record.budgetMovements[movementId] = { ...fact(movementId), budgetId: record.budget.id, campaignId: parent.campaignId, hypothesisId: parent.hypothesisId, trialId: id,
            kind: 'CONSUME', resource: 'TRIAL', amount: '1', currency: null, fromReservationId: record.budget.reservationForClosureRef,
            toReservationId: null, measurementEvidenceRef: null, reasonCode: id === trialId ? 'one_frozen_selected_portfolio_independent_evaluation' : 'one_matched_independent_baseline_evaluation' };
          }
          const capture = cycle.accountBasis ? campaignAccount(record, record.campaigns[parent.campaignId]!) : null;
          if (capture && canonical(json(capture.basis)) !== canonical(json(cycle.accountBasis))) throw new Problem('independent_campaign_account_start_mismatch');
          record.artifacts[`independent-input:${decision.id}`] = json({ cycle: cycle.id, basisRevision: cycle.basisRevision, decisionId: decision.id,
            selectedCapital: decision.capital.committedCapital.amount, capital: decision.capital.equity, policy: record.policy,
            source: object(record.artifacts[cycle.id]!).nativeSource!, configured: object(record.artifacts[cycle.id]!).configured!,
            data: object(object(record.artifacts[cycle.id]!).configured).data!,
            capture });
          admitted = true; return undefined;
        }, ctx);
        if (!admitted) return;
        const snapshot = pmRecord(pmState(await runtime.snapshot(PmDocument, ctx)), task.input.mandateId);
        const frozen = object(snapshot.artifacts[`independent-input:${task.input.decisionId}`]);
        const policy = validatePmEntity('DecisionPolicyRevision', json(frozen.policy));
        const originalDecision = snapshot.decisions[task.input.decisionId]!;
        const data = object(frozen.data); const heldout = object(data.sealed, 'registered_independent_data');
        const frozenCycle = snapshot.cycles[task.input.cycleId]!;
        if (frozen.decisionId !== originalDecision.id || frozen.cycle !== frozenCycle.id || frozen.basisRevision !== frozenCycle.basisRevision
          || policy.id !== originalDecision.policyRevisionId || policy.id !== frozenCycle.policyRevisionId
          || policy.mandateRevisionId !== originalDecision.mandateRevisionId) throw new Problem('independent_frozen_policy_identity_mismatch');
        const allocationMethod = validatePmAllocationMethod(object(policy.validDomain).jointAllocationMethod);
        assertPmDataAvailable(heldout, frozenCycle.availableCutoffNs);
        await checkExposure(researchRuntime(runtime), ctx, heldout, true);
        const result = await evaluatePmIndependent({ source: object(frozen.source), capital: frozen.capital as unknown as Pm.Money,
          currency: String(object(frozen.capital).currency), catalog: String(heldout.catalog), dataDir: config.dataDir, binary: config.jobBinary,
          cycleId: task.input.cycleId, allocationMethod,
          ownerBoundaries: (object(policy.validDomain).ownerBoundaries ?? []) as unknown as Pm.OwnerBoundary[],
          selectedCapital: String(frozen.selectedCapital), heldoutData: heldout,
          ...(frozen.capture ? { capture: frozen.capture as unknown as CapturedAccount,
            datasetRevision: { id: String(object(object(frozen.configured).revision).id), contentRef: String(object(object(frozen.configured).revision).contentRef) } } : {}) }, ctx, pmScienceCheckpoint(runtime, 'pm-independent', ctx));
        const record = snapshot; const decision = record.decisions[task.input.decisionId]!; const cycle = record.cycles[task.input.cycleId]!;
          const artifactId = identity('independent-evidence', decision.id); const artifact = result as unknown as JsonObject; const atNs = nowNs();
          const selection = result.point.simulationRequest ? object(result.point.simulationRequest.selection) : null;
          const evidence: Pm.EvidenceRef = { id: artifactId, sourceKind: 'NATIVE_RESULT', role: 'INDEPENDENT', runtime: { conversationId: record.conversationId, taskId: runtime.taskId },
            objectKey: artifactId, contentDigest: null, mediaType: 'application/json', sourceRevision: cycle.basisRevision,
            eventInterval: selection ? { startNs: String(selection.event_start_ns), endExclusiveNs: String(selection.event_end_ns) } : null,
            availableAtNs: atNs, clockPrecision: 'NANOSECOND', schemaId: 'qz.pm.independent-portfolio/1', visibility: 'OWNER_ONLY' };
          const project = (simulation: JsonObject | null) => {
            if (!simulation || !evidence.eventInterval) return [];
            const report = frozen.capture ? object(simulation.spot_cash_report, 'native_spot_independent_report') : simulation;
            if (frozen.capture && object(report.daily_returns).report_currency !== decision.capital!.equity.currency) throw new Problem('independent_spot_report_currency_mismatch');
            return projectNativeIndependentMetrics({ portfolio: {
            partition: 'HELDOUT', statistics: report.statistics ?? [], returns_status: frozen.capture ? null : simulation.returns_status ?? null,
            recorded_fee_totals: simulation.recorded_fee_totals ?? null } }, [...pmMetricDefinitions(), ...pmDiagnosticDefinitions()],
            { role: 'INDEPENDENT', interval: evidence.eventInterval, asOfNs: cycle.asOfNs, availableAtNs: atNs, evidenceRefs: [artifactId], methodRef: policy.id,
              currency: decision.capital!.equity.currency, calendarId: 'UTC_COMPLETE_DAYS' });
          };
          const metrics = project(result.point.simulation); const baselineMetrics = project(result.baseline.simulation);
          const baselineSelection = object(result.baseline.simulationRequest.selection);
          if (!selection || baselineSelection.event_start_ns !== selection.event_start_ns || baselineSelection.event_end_ns !== selection.event_end_ns) throw new Problem('independent_baseline_scope_mismatch');
          const qualification = evaluatePmQualification(object(object(policy.validDomain).qualificationProtocol, 'qualification_protocol'), policy.selectionRule, metrics, baselineMetrics, atNs);
          artifact.metrics = json(metrics); artifact.baselineMetrics = json(baselineMetrics); artifact.qualification = json(qualification); evidence.contentDigest = pmArtifactDigest(artifact);
          const complete = result.point.status === 'COMPLETED' && metrics.length > 0 && policy.selectionRule.objectives.every(objective => metrics.filter(metric => metric.definition.id === objective.metricDefinitionId && metric.status === 'AVAILABLE').length === 1);
          const retainedArtifacts = await retainPmArtifacts(runtime, { [artifactId]: artifact }, ctx);
        await runtime.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const decision = record.decisions[task.input.decisionId]!;
          const cycle = record.cycles[task.input.cycleId]!; const trial = record.derivedTrials[trialId]!;
          record.evidence[artifactId] = evidence; record.artifacts[artifactId] = retainedArtifacts[artifactId]!;
          revise(trial); trial.state = result.point.status === 'COMPLETED' ? 'COMPUTED' : 'FAILED'; trial.evidenceRefs = [artifactId]; trial.errorCode = result.point.reason;
          const baselineTrial = record.derivedTrials[baselineTrialId]!; revise(baselineTrial); baselineTrial.state = result.baseline.status === 'COMPLETED' ? 'COMPUTED' : 'FAILED'; baselineTrial.evidenceRefs = [artifactId]; baselineTrial.errorCode = result.baseline.reason;
          const strategy = record.strategies[decision.strategyVersionId]!;
          const assessment = assessStage(strategy, qualification.outcome === 'READY' ? 'SHADOW' : 'INDEPENDENT_EVALUATION', [evidence]);
          assessment.outcome = complete && result.baseline.status === 'COMPLETED' ? qualification.outcome : 'NEEDS_EVIDENCE';
          assessment.requirementsVersion = 'qz.pm.frozen-paired-economic-falsification/1';
          assessment.reasonCodes.push(...qualification.reasonCodes);
          assessment.nextPermittedActions = assessment.outcome === 'READY' ? ['OBSERVE_SHADOW_WITHOUT_LIVE_TARGET_AUTHORIZATION'] : ['COLLECT_NEW_UNEXPOSED_EVIDENCE'];
          assessment.reasonCodes.push('frozen_configuration_evaluated_without_reselection', 'evaluation_is_not_live_execution_qualification');
          const currentBasis = cycle.state !== 'SUPERSEDED' && cycle.basisRevision === record.basisRevision;
          record.assessments[assessment.id] = assessment;
          if (currentBasis) strategy.pipelineAssessmentId = assessment.id;
          if (decision.ensembleRevisionId && complete) record.ensembles[decision.ensembleRevisionId]!.qualification = 'INDEPENDENTLY_EVALUATED';
          for (const missing of Object.values(record.gaps)) if (missing.affectedEntityId === decision.id && missing.requiredCapability === 'independent_portfolio_evaluation' && complete) { revise(missing); missing.state = 'RESOLVED'; missing.resolutionEvidenceRefs = [artifactId]; }
          decision.reasonCodes = decision.reasonCodes.filter(reason => reason !== 'new_portfolio_requires_independent_evaluation');
          decision.reasonCodes.push(result.point.status === 'COMPLETED' ? 'independent_portfolio_evaluated' : 'independent_portfolio_evaluation_failed');
          decision.reasonCodes.push(...qualification.reasonCodes);
          decision.explanationFacts = { ...object(decision.explanationFacts), independentEvidenceRef: artifactId, economicQualification: json(qualification) };
          if (assessment.outcome === 'NOT_READY') {
            gap(record, 'independent_economic_falsification', 'frozen_configuration_not_preferred_to_matched_baseline', 'COLLECT_AUTHORIZED_DATA', decision.id);
            if (currentBasis) { revise(decision); decision.status = 'REVOKED'; if (record.currentDecisionId === decision.id) record.currentDecisionId = null; }
          }
          if (currentBasis) { revise(cycle); cycle.state = complete && assessment.outcome !== 'NEEDS_EVIDENCE' ? 'CONCLUDED' : 'WAITING_EVIDENCE'; }
          record.searchFamily.trialIds = Object.keys(record.derivedTrials); record.searchFamily.countedAttempts = Object.values(record.derivedTrials).filter(item => item.countedAttempt).length.toString();
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, ctx);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        const reason = error instanceof Error && /^[a-zA-Z][a-zA-Z0-9_]+$/.test(error.message) ? error.message : 'independent_portfolio_evaluation_failed';
        await runtime.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const cycle = record.cycles[task.input.cycleId]!;
          const errorId = identity('pm-independent-error', task.input.decisionId);
          const detail = json({ name: error instanceof Error ? error.name : 'Error', message: error instanceof Error ? error.message : String(error), stage: 'PM_INDEPENDENT_EVALUATION' });
          record.artifacts[errorId] = detail;
          record.evidence[errorId] = { id: errorId, sourceKind: 'DERIVED', role: 'INDEPENDENT', runtime: { conversationId: record.conversationId, taskId: runtime.taskId },
            objectKey: errorId, contentDigest: digest(detail), mediaType: 'application/json', sourceRevision: cycle.basisRevision, eventInterval: null,
            availableAtNs: nowNs(), clockPrecision: 'NANOSECOND', schemaId: 'qz.pm.error/1', visibility: 'OWNER_ONLY' };
          for (const id of [trialId, baselineTrialId]) { const trial = record.derivedTrials[id]; if (trial) { revise(trial); trial.state = 'FAILED'; trial.errorCode = reason; } }
          if (cycle.state !== 'SUPERSEDED') { revise(cycle); cycle.state = 'WAITING_EVIDENCE'; cycle.reasonCodes.push(reason); }
          gap(record, 'independent_portfolio_evaluation', reason, 'RECOMPUTE', task.input.decisionId);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: reason } } };
        }, ctx);
      }
    } },
    async abort(task, runtime, ctx) { await runtime.commit(async tx => {
      const record = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId);
      for (const id of [identity('pm-independent-trial', task.input.decisionId), identity('pm-independent-baseline-trial', task.input.decisionId)]) {
        const trial = record.derivedTrials[id]; if (trial) { revise(trial); trial.state = 'FAILED'; trial.errorCode = 'owner_stopped_independent_evaluation'; }
      }
      gap(record, 'independent_portfolio_evaluation', 'owner_stopped_independent_evaluation', 'RECOMPUTE', task.input.decisionId);
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, ctx); },
  });
}

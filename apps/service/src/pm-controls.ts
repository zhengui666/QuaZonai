import { defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ResearchHost } from '../../research/src/host.ts';
import { CampaignResearchDoc } from '../../research/src/pm-campaign.ts';
import { closeResearch } from '../../research/src/closure.ts';
import { researchRuntime } from '../../research/src/research-runtime.ts';
import { canonical } from '../../research/src/documents.ts';
import { PmDocument, ensureRevision, fact, gap, identity, pmRecord, pmState, revise } from './pm-facts.ts';
import { json, object, Problem, text } from './facts.ts';
import { pmCapabilities } from './pm-registry.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { Application } from './facts.ts';
import { pmView } from './pm-view.ts';
import { readPmArtifact } from './pm-evidence.ts';
import { readNativeEvidence } from '../../research/plugins/native-evidence.ts';

type Action = 'pause' | 'resume' | 'close';
type ResourceKind = 'evidence' | 'capabilities' | 'decision' | 'decisions' | 'cycle' | 'capacity' | 'campaign' | 'trials' | 'campaignDecision' | 'strategyVersions' | 'strategyAssessments';
const context = BACKGROUND_CONTEXT;
export function pmControls(ready: Promise<{ host: ResearchHost }>) {
  const ControlCampaign = defineTask<{ mandateId: string; campaignId: string; action: Action; key: string }, { phase: 'apply' }, null>({
    name: 'qz.pm.campaign-command', version: 1, initial: () => ({ phase: 'apply' }),
    phases: { async apply(task, runtime, ctx) {
      const { host } = await ready;
      try {
        const record = pmRecord(pmState(await runtime.snapshot(PmDocument, ctx)), task.input.mandateId); const campaign = record.campaigns[task.input.campaignId]!;
        const operation = record.artifacts[task.input.key]!;
        if (campaign.runtime.conversationId !== record.conversationId && operation.previousCampaignState !== 'CONCLUDED') {
          if (task.input.action === 'pause') await host.stop(campaign.runtime.conversationId);
          else if (task.input.action === 'close') await closeResearch({ ...researchRuntime(runtime), conversationId: campaign.runtime.conversationId as ConversationId }, ctx, 'owner_requested_campaign_comparison');
          else {
            let previousId = record.artifacts[task.input.key]!.previousSubmissionId;
            if (typeof previousId !== 'number') {
              const submission = (await host.state(campaign.runtime.conversationId)).submission;
              if (!submission) throw new Problem('campaign_submission_unavailable'); previousId = submission.id;
              await runtime.commit(async tx => { pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId).artifacts[task.input.key]!.previousSubmissionId = previousId!; return undefined; }, ctx);
            }
            await host.resume(campaign.runtime.conversationId, previousId as number);
          }
        } else if (task.input.action === 'close') throw new Problem('campaign_not_started');
        await runtime.commit(async tx => {
          const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); const campaign = stored.campaigns[task.input.campaignId]!;
          revise(campaign); campaign.state = task.input.action === 'pause' ? 'PAUSED' : task.input.action === 'resume' ? (stored.artifacts[task.input.key]!.previousCampaignState === 'CONCLUDED' ? 'CONCLUDED' : campaign.runtime.conversationId === stored.conversationId ? 'PLANNED' : 'SEARCHING') : 'CONCLUDED';
          stored.artifacts[task.input.key]!.state = 'APPLIED'; stored.artifacts[`campaign-control:${campaign.id}`] = { ...stored.artifacts[task.input.key]! };
          return { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, ctx);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        const reason = error instanceof Error ? error.message : 'campaign_control_failed';
        await runtime.commit(async tx => { const stored = pmRecord(pmState(await tx.doc(PmDocument)), task.input.mandateId); stored.artifacts[task.input.key]!.state = 'FAILED'; stored.artifacts[task.input.key]!.reason = reason;
          stored.artifacts[`campaign-control:${task.input.campaignId}`] = { ...stored.artifacts[task.input.key]! };
          gap(stored, 'campaign_control', reason, 'RECOMPUTE', task.input.campaignId);
          return { status: 'terminal', outcome: { status: 'failed', error: { message: reason } } };
        }, ctx);
      }
    } },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  async function readResource(kind: ResourceKind, id?: string) {
    const { host } = await ready; if (kind === 'capabilities') return pmCapabilities();
    const state = pmState(await host.harness.snapshot(PmDocument, context));
    if (kind === 'decision' || kind === 'decisions') { const record = pmRecord(state, text(id, 'mandate_id')); return kind === 'decision' ? record.decisions[record.currentDecisionId ?? ''] ?? null : Object.values(record.decisions); }
    for (const record of Object.values(state.mandates)) {
      if (kind === 'evidence' && record.evidence[id!]) {
        const reference = record.evidence[id!]!; let payload = record.artifacts[reference.objectKey] ?? null;
        if (payload?.kind === 'PM_ORIGINAL_NATIVE_EVIDENCE_REFERENCE') {
          if (typeof payload.taskId !== 'number' || payload.taskId !== reference.runtime?.taskId || payload.conversationId !== reference.runtime?.conversationId) throw new Problem('native_evidence_reference_mismatch', 409);
          const task = await host.harness.getTask(payload.taskId as TaskId<JsonObject>, context);
          if (!task || task.conversationId !== payload.conversationId || task.state.status !== 'terminal' || task.state.outcome.status !== 'completed') throw new Problem('native_evidence_unavailable', 409);
          payload = await readNativeEvidence(host.harness, task.conversationId, task.id, object(object(task.state.outcome.result).native, 'original_native_evidence'), context);
        }
        if (payload) payload = await readPmArtifact(host.harness, reference, payload, context);
        return { reference, payload };
      }
      if (kind === 'cycle' && record.cycles[id!]) return record.cycles[id!];
      if (kind === 'capacity' && record.capacities[id!]) return record.capacities[id!];
      const campaign = record.campaigns[id!];
      if (campaign && kind === 'campaign') return campaign;
      if (campaign && kind === 'trials') return Object.values(record.derivedTrials).filter(item => item.campaignId === id);
      if (campaign && kind === 'campaignDecision') {
        if (campaign.runtime.conversationId === record.conversationId) return null;
        return (await host.harness.snapshot(CampaignResearchDoc, campaign.runtime.conversationId as ConversationId, context))?.comparisons.at(-1) ?? null;
      }
      const strategy = record.strategies[id!];
      if (strategy || Object.values(record.strategies).some(item => item.definitionId === id)) {
        const versions = Object.values(record.strategies).filter(item => item.definitionId === (strategy?.definitionId ?? id));
        if (kind === 'strategyVersions') return versions;
        if (kind === 'strategyAssessments') return Object.values(record.assessments).filter(item => versions.some(version => version.id === item.strategyVersionId));
      }
    }
    throw new Problem('pm_resource_not_found', 404);
  }
  async function campaignCommand(id: string, action: Action, value: unknown, strategyId?: string) {
    const { host } = await ready; const body = object(value); const requestId = text(body.requestId, 'request_id'); const original = json(value);
    const mandateId = await host.harness.commit(async tx => {
      const state = pmState(await tx.doc(PmDocument)); const record = Object.values(state.mandates).find(item => item.campaigns[id]);
      if (!record) throw new Problem('campaign_not_found', 404);
      const key = `campaign-command:${id}:${strategyId ?? ''}:${action}:${requestId}`; const replay = state.commands[key];
      if (replay) { if (canonical(replay.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return record.mandate.id; }
      assertBusinessAdmission(await tx.doc(Application));
      ensureRevision(record, body.expectedRevision);
      if (Object.values(record.retirements).some(item => item.strategyVersionId === record.latestStrategyId) && action !== 'pause') throw new Problem('strategy_retired', 409);
      const campaign = record.campaigns[id]!; const previousControl = record.artifacts[`campaign-control:${id}`];
      if (previousControl?.state === 'PENDING') throw new Problem('campaign_control_pending', 409);
      if (action === 'resume' && campaign.state !== 'PAUSED') throw new Problem('campaign_not_paused', 409);
      if (action === 'close' && campaign.state === 'PLANNED') throw new Problem('campaign_not_started', 409);
      if (action === 'pause' && campaign.state === 'CONCLUDED' && !strategyId) throw new Problem('campaign_already_concluded', 409);
      record.artifacts[key] = { action, state: 'PENDING', campaignId: id, previousCampaignState: action === 'resume' ? previousControl?.previousCampaignState ?? campaign.state : campaign.state };
      record.artifacts[`campaign-control:${id}`] = { ...record.artifacts[key]! };
      if (strategyId) {
        record.artifacts[`strategy-control:${strategyId}`] = { action, requestId };
        if (action === 'pause') {
          if (record.currentDecisionId) { const decision = record.decisions[record.currentDecisionId]!; revise(decision); decision.status = 'REVOKED'; decision.reasonCodes.push('owner_strategy_paused'); record.currentDecisionId = null; }
          if (record.latestCycleId) { const cycle = record.cycles[record.latestCycleId]!; revise(cycle); cycle.state = 'SUPERSEDED'; cycle.reasonCodes.push('owner_strategy_paused'); }
        }
      }
      if (action === 'pause') campaign.state = 'PAUSED';
      const taskId = await tx.createTask(ControlCampaign, { mandateId: record.mandate.id, campaignId: id, action, key },
        { conversationId: record.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true });
      record.artifacts[key]!.taskId = taskId; state.commands[key] = { original, mandateId: record.mandate.id, resourceId: id }; return record.mandate.id;
    }, context);
    const state = pmState(await host.harness.snapshot(PmDocument, context)); return pmView(pmRecord(state, mandateId), state);
  }
  async function strategyCommand(id: string, action: 'pause' | 'resume', value: unknown) {
    const { host } = await ready; const state = pmState(await host.harness.snapshot(PmDocument, context));
    const record = Object.values(state.mandates).find(item => item.strategies[id]); if (!record) throw new Problem('strategy_not_found', 404);
    if (record.latestStrategyId !== id) throw new Problem('strategy_version_not_current', 409);
    return campaignCommand(Object.values(record.campaigns).at(-1)!.id, action, value, id);
  }
  async function startCampaign(value: unknown) {
    const { host } = await ready; const body = object(value); const id = text(body.mandateId, 'mandate_id'); const requestId = text(body.requestId, 'request_id');
    await host.harness.commit(async tx => {
      const state = pmState(await tx.doc(PmDocument)); const record = pmRecord(state, id); const key = `campaign-start:${id}:${requestId}`; const original = json(value);
      if (state.commands[key]) { if (canonical(state.commands[key]!.original) !== canonical(original)) throw new Problem('request_id_conflict', 409); return; }
      assertBusinessAdmission(await tx.doc(Application));
      ensureRevision(record, body.expectedRevision); const previous = Object.values(record.campaigns).at(-1)!;
      if (!['CONCLUDED', 'PAUSED'].includes(previous.state)) throw new Problem('campaign_already_active', 409);
      if (canonical(json(previous.datasetRevisionIds)) === canonical(json(record.datasets))) throw new Problem('new_campaign_requires_new_data_revision', 409);
      if (Object.values(record.derivedTrials).filter(item => item.countedAttempt).length >= record.budget.maximumTrials) throw new Problem('shared_trial_budget_exhausted', 409);
      const campaignId = identity('campaign', [id, record.datasets]);
      record.campaigns[campaignId] ??= { ...previous, ...fact(campaignId), state: 'PLANNED', runtime: { conversationId: record.conversationId, taskId: record.monitorTaskId },
        datasetRevisionIds: [...record.datasets], hypothesisIds: [], trialIds: [], latestComparisonId: null, selectedStrategyVersionId: null, conclusionReason: null };
      state.commands[key] = { original, mandateId: id, resourceId: campaignId };
    }, context);
    const state = pmState(await host.harness.snapshot(PmDocument, context)); return pmView(pmRecord(state, id), state);
  }
  return { task: ControlCampaign, readResource, campaignCommand, strategyCommand, startCampaign };
}

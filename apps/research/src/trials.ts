import type { Context, JsonValue } from '@earendil-works/chord';
import type { JsonObject, ToolExecutionApi, ToolExecutionResult } from '@earendil-works/pi-durable';
import { canonical, Study } from './documents.ts';
import { modelFeedback } from './feedback.ts';
import { classifyExecutionError, executionDiagnostic } from './execution-errors.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { admitCampaignTrial, settleCampaignTrial } from './pm-ledger.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { closeResearch } from './closure.ts';

// Scientific facts committed on the upstream transaction line. No task scheduler.
export async function evaluateTrial(
  api: ToolExecutionApi, context: Context, proposal: JsonObject,
  compute: () => Promise<JsonObject>,
): Promise<JsonObject> {
  let recovered = false;
  const previous = await api.commit(async (tx) => {
    const study = await tx.doc(Study, api.conversationId);
    const old = study.trials.find((trial) => trial.taskId === api.taskId);
    if (old) {
      if (canonical(old.proposal!) !== canonical(proposal)) throw new Error('trial_identity_conflict');
      // A committed scientific failure is settled even if the native safe tool
      // crashed before recording its terminal outcome. Only STARTED may resume.
      if (old.status === 'FAILED') throw new Error(typeof old.error === 'string' ? old.error : 'factor_failed');
      if (study.final !== null && old.status === 'STARTED') throw new Error('research_already_final');
      recovered = old.status === 'STARTED';
      return old.status === 'REJECTED' ? { rejected: true } : old.result == null ? null : JSON.parse(JSON.stringify(old.result)) as JsonObject;
    }
    if (study.final !== null) throw new Error('research_already_final');
    const campaign = await tx.doc(CampaignResearchDoc, api.conversationId);
    if (campaign.state !== 'SEARCHING') throw new Error('research_search_closed');
    if (campaign.binding?.intakeRequired && (!campaign.intake || campaign.intake.support !== 'CONDITIONAL')) throw new Error('strategy_intake_evidence_required');
    const maximum = study.input?.policy.max_trials;
    const admitted = study.trials.filter((trial) => trial.status !== 'REJECTED').length;
    if (typeof maximum === 'number' && admitted >= maximum) {
      study.trials.push({ taskId: api.taskId, proposal, status: 'REJECTED', error: 'trial_budget_exhausted', result: null });
      await admitCampaignTrial(tx, api, proposal, true);
      return { rejected: true };
    }
    const measured = campaign.budgetMovements.filter(item => item.resource === 'NATIVE_SECONDS' && item.kind === 'CONSUME').reduce((sum, item) => sum + Number(item.amount), 0);
    if (campaign.binding?.budget.nativeComputeSeconds != null && campaign.resourceMeasurements.some(item => item.resource === 'NATIVE_SECONDS' && item.status === 'UNAVAILABLE')) throw new Error('native_compute_budget_unverifiable');
    if (campaign.binding?.budget.nativeComputeSeconds !== null && campaign.binding?.budget.nativeComputeSeconds !== undefined && measured >= campaign.binding.budget.nativeComputeSeconds) throw new Error('native_compute_budget_exhausted');
    await admitCampaignTrial(tx, api, proposal);
    const scope = study.input?.specification?.search;
    if (scope) {
      const members = Array.isArray(proposal.members) ? proposal.members.length : 1;
      if (members > scope.maxMembers || scope.scope === 'single-factor' && (proposal.members !== undefined || typeof proposal.model !== 'string') || scope.scope === 'ensemble' && !Array.isArray(proposal.members)) {
        study.trials.push({ taskId: api.taskId, proposal, status: 'FAILED', result: null, error: 'research_search_scope_exceeded' });
        await settleCampaignTrial(tx, api, null, 'research_search_scope_exceeded', '0');
        return { scopeRejected: true };
      }
    }
    study.trials.push({ taskId: api.taskId, proposal, status: 'STARTED', result: null, error: null });
    return null;
  }, context);
  if (previous && typeof previous === 'object' && !Array.isArray(previous)) {
    if (previous.rejected) throw new Error('trial_budget_exhausted');
    if (previous.scopeRejected) throw new Error('research_search_scope_exceeded');
    return previous;
  }
  const started = process.hrtime.bigint();
  try {
    const result = await compute();
    await api.commit(async (tx) => {
      const trial = (await tx.doc(Study, api.conversationId)).trials.find((item) => item.taskId === api.taskId)!;
      trial.status = 'COMPUTED'; trial.result = result; trial.error = null; trial.errorDetail = null;
      await settleCampaignTrial(tx, api, result, null, recovered ? null : (Number(process.hrtime.bigint() - started) / 1e9).toString());
    }, context);
    return result;
  } catch (error) {
    if (context.abortSignal?.aborted) throw error;
    // If close/abort prevents this commit, STARTED can resume on native replay.
    // Once FAILED commits, replay must disclose the same error without retrying.
    await api.commit(async (tx) => {
      const trial = (await tx.doc(Study, api.conversationId)).trials.find((item) => item.taskId === api.taskId)!;
      trial.status = 'FAILED'; trial.error = executionDiagnostic(error, 'factor_failed');
      trial.errorDetail = { ...classifyExecutionError(error) };
      await settleCampaignTrial(tx, api, null, executionDiagnostic(error, 'factor_failed'), recovered ? null : (Number(process.hrtime.bigint() - started) / 1e9).toString());
    }, context).catch(() => {});
    throw error;
  }
}

export async function studyInput(api: ResearchRuntime, context: Context) {
  const study = await api.snapshot(Study, api.conversationId, context);
  if (!study?.input) throw new Error('research_input_missing');
  return study.input;
}

export function object(value: JsonValue | undefined, name: string): JsonObject {
  if (value === null || !value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name}_required`);
  return value;
}

export async function trialFeedback(api: ToolExecutionApi, context: Context, evaluate: () => Promise<JsonObject>): Promise<ToolExecutionResult> {
  try {
    const result = await evaluate();
    return { content: [{ type: 'text', text: JSON.stringify({ trialId: api.taskId, ...modelFeedback(result) }) }] };
  } catch (error) {
    if (context.abortSignal?.aborted) throw error;
    if (error instanceof Error && error.message === 'research_already_final') return { isError: true, content: [{ type: 'text', text: 'research_already_final: no additional scientific computation was started.' }], control: { terminate: true } };
    if (!(error instanceof Error) || !['trial_budget_exhausted', 'native_compute_budget_exhausted', 'native_compute_budget_unverifiable', 'research_search_closed'].includes(error.message)) throw error;
    await closeResearch(api, context, error.message);
    return { isError: true, content: [{ type: 'text', text: 'Research search closed. Original attempts, failures and deterministic candidate comparison are retained; independent results remain owner-only.' }], control: { terminate: true } };
  }
}

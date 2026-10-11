import type { Context, JsonValue } from '@earendil-works/chord';
import type { JsonObject, ToolExecutionApi, ToolExecutionResult } from '@earendil-works/pi-durable';
import { canonical, Study } from './documents.ts';

// Scientific facts committed on the upstream transaction line. No task scheduler.
export async function evaluateTrial(
  api: ToolExecutionApi, context: Context, proposal: JsonObject,
  compute: () => Promise<JsonObject>,
): Promise<JsonObject> {
  const previous = await api.commit(async (tx) => {
    const study = await tx.doc(Study, api.conversationId);
    const old = study.trials.find((trial) => trial.taskId === api.taskId);
    if (old) {
      if (canonical(old.proposal!) !== canonical(proposal)) throw new Error('trial_identity_conflict');
      return old.status === 'REJECTED' ? { rejected: true } : old.result;
    }
    if (study.final !== null) throw new Error('research_already_final');
    const maximum = study.input?.policy.max_trials;
    const admitted = study.trials.filter((trial) => trial.status !== 'REJECTED').length;
    if (typeof maximum === 'number' && admitted >= maximum) {
      study.trials.push({ taskId: api.taskId, proposal, status: 'REJECTED', error: 'trial_budget_exhausted', result: null });
      return { rejected: true };
    }
    study.trials.push({ taskId: api.taskId, proposal, status: 'STARTED', result: null, error: null });
    return null;
  }, context);
  if (previous && typeof previous === 'object' && !Array.isArray(previous)) {
    if (previous.rejected) throw new Error('trial_budget_exhausted');
    return previous;
  }
  try {
    const result = await compute();
    await api.commit(async (tx) => {
      const trial = (await tx.doc(Study, api.conversationId)).trials.find((item) => item.taskId === api.taskId)!;
      trial.status = 'COMPUTED'; trial.result = result; trial.error = null;
    }, context);
    return result;
  } catch (error) {
    // During close/abort the native task remains authoritative. Its intent is
    // already recorded, and a safe replay resumes the same scientific attempt.
    await api.commit(async (tx) => {
      const trial = (await tx.doc(Study, api.conversationId)).trials.find((item) => item.taskId === api.taskId)!;
      trial.status = 'FAILED'; trial.error = error instanceof Error ? error.message : 'factor_failed';
    }, context).catch(() => {});
    throw error;
  }
}

export async function studyInput(api: ToolExecutionApi, context: Context) {
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
    return { content: [{ type: 'text', text: JSON.stringify({ trialId: api.taskId, ...result }) }] };
  } catch (error) {
    if (!(error instanceof Error) || error.message !== 'trial_budget_exhausted') throw error;
    await api.commit(async (tx) => {
      const study = await tx.doc(Study, api.conversationId);
      study.final ??= { taskId: api.taskId, trialId: null, comparedTrials: study.trials.map((trial) => trial.taskId!), status: 'STOPPED', reason: 'trial_budget_exhausted', result: null };
    }, context);
    return { isError: true, content: [{ type: 'text', text: 'trial_budget_exhausted: original attempts and rejected proposal are recorded; research stopped without a final evidence claim.' }], control: { terminate: true } };
  }
}

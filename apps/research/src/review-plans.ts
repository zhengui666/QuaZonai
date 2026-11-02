import type { Context } from '@earendil-works/chord';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { ResearchInput } from './documents.ts';
import { canonical } from './documents.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import type { ReviewNativePlan } from './review-native-task.ts';
import { readNativeEvidence } from '../plugins/native-evidence.ts';
import { nativeTargetForData } from './native-execution.ts';

const object = (value: unknown): JsonObject | null => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as JsonObject : null;
/** No model-provided returns: load the original owned native child and require VALIDATION provenance. */
export async function originalValidationAccount(api: ResearchRuntime, context: Context, trial: JsonObject) {
  const result = object(trial.result); const portfolio = object(result?.portfolio);
  if (!result || portfolio?.partition !== 'VALIDATION' || typeof result.evidenceTaskId !== 'number') return null;
  const task = await api.getTask(result.evidenceTaskId as TaskId<JsonObject>, context);
  if (task?.conversationId !== api.conversationId || task.state.status !== 'terminal' || task.state.outcome.status !== 'completed') return null;
  const output = object(task.state.outcome.result); const encoded = object(output?.native);
  if (!encoded) return null;
  const original = await readNativeEvidence(api, api.conversationId, task.id, encoded, context);
  const simulation = object(original.simulation);
  if (!simulation || simulation.returns_kind !== 'PORTFOLIO_DAILY' || !Array.isArray(simulation.returns)) return null;
  const rows = simulation.returns.map(object);
  if (!rows.length || rows.some(row => typeof row?.timestamp_ns !== 'string' || typeof row.value !== 'number' || !Number.isFinite(row.value))) return null;
  const request = object(original.simulationRequest) ?? object(original.nativeRequest);
  const selection = object(request?.selection);
  // The raw account itself, not the research summary label, must end at the validation cut.
  const end = object(result.request)?.validation_end_ns ?? object(result.request)?.event_end_ns;
  if (typeof end !== 'string' || !rows.every(row => BigInt(row!.timestamp_ns as string) < BigInt(end))) return null;
  return { source: original, selection, timestamps: rows.map(row => row!.timestamp_ns as string), returns: rows.map(row => row!.value as number) };
}

/** All native plans are frozen before the reviewer sees a tool. Missing comparable paths stay explicit. */
export async function reviewSelectionPlan(api: ResearchRuntime, context: Context, input: ResearchInput,
  selected: JsonObject, trials: readonly JsonObject[], evidenceRef: string): Promise<ReviewNativePlan[]> {
  if (!object(selected.proposal)?.members) return [];
  const proposal = object(selected.proposal)!;
  const cohort = trials.filter(trial => {
    const candidate = object(trial.proposal);
    return Array.isArray(candidate?.members) && canonical((candidate?.request ?? null) as never) === canonical((proposal.request ?? null) as never)
      && canonical((candidate?.strategy ?? null) as never) === canonical((proposal.strategy ?? null) as never);
  });
  const rows = await Promise.all(cohort.filter(trial => trial.status === 'COMPUTED').map(async trial => ({ trial, account: await originalValidationAccount(api, context, trial) })));
  const selectedAccount = rows.find(row => row.trial.taskId === selected.taskId)?.account;
  if (!selectedAccount) return [];
  const aligned = rows.filter(row => row.account && canonical(row.account.timestamps) === canonical(selectedAccount.timestamps));
  if (!aligned.length) return [];
  return [{ kind: 'SELECTION', target: nativeTargetForData(input.data), evidenceRef: `${evidenceRef}#selection`,
    source: { selectedTrialId: selected.taskId!, cohort: cohort.map(trial => ({ taskId: trial.taskId!, status: trial.status!, proposal: trial.proposal!, result: trial.result ?? null })) },
    analysis: { kind: 'SELECTION', total_trials: cohort.length, ledger_complete: rows.length === cohort.length && aligned.length === rows.length,
      selected_trial_id: String(selected.taskId), trials: aligned.map(row => ({ id: String(row.trial.taskId), returns: row.account!.returns })),
      timestamps_ns: selectedAccount.timestamps, frequency: 'PORTFOLIO_DAILY', block_length: 1 } }];
}

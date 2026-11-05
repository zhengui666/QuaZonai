import { createHash } from 'node:crypto';
import { canonical } from './documents.ts';
import type { ResearchInput } from './documents.ts';
import { thresholdPolicy } from './strategy.ts';
import { nativeReviewReturnPanel } from './review-account-science.ts';
import type { ReviewNativePlan } from './review-native-task.ts';
import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { NativeJson, NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { Context } from '@earendil-works/chord';
import type { ResearchRuntime } from './research-runtime.ts';
import type { StrategyPolicy } from './strategy.ts';
import { nativeTargetForData, retainNativeRemoteUncertainty, withNativeExecutionInterceptor, withNativeRemoteTracking } from './native-execution.ts';
import { replayReviewAccount, replayStoredReviewAccount } from './review-account-science.ts';
import { canonicalJsonDigest } from './native-json.ts';
import { NativeEvidencePart, readNativeEvidence, retainNativeEvidence } from '../plugins/native-evidence.ts';
const object = (value: unknown): JsonObject => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as JsonObject : {};
type AccountRecord = {
  digest: string; taskId: TaskId<JsonObject>; status: 'RESERVED' | 'COMPUTED' | 'FAILED';
  maximumCalls?: number; calls: { command: string; digest: string; result: JsonObject | null }[]; result: JsonObject | null;
}
/** Persistent scientific facts: task-scoped documents retire at settlement and are never a cohort archive. */
export const ReviewFactorAccounts = defineDoc<{ records: Record<string, AccountRecord>; cohorts: Record<string, JsonObject> }>({
  kind: 'qz.review-factor-accounts', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', initial: () => ({ records: {}, cohorts: {} }),
});
const detached = <T>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
const sourceKey = (data: JsonObject, model: string, policy: StrategyPolicy, evidenceRef: string) => createHash('sha256').update(canonical(detached({ data, model, policy, evidenceRef }) as unknown as JsonObject)).digest('hex');

/** Journal every fixed diagnostic computation and its original receipt. */
export const ReviewFactorAccountTask = defineTask<{ data: JsonObject; model: string; policy: StrategyPolicy; binary: NativeTarget; evidenceRef: string; maximumCalls?: number; recordKey: string; portfolioSource?: JsonObject }, { phase: 'compute' }, JsonObject>({
  name: 'qz.review.factor-account', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    return withNativeRemoteTracking(task.id, runtime, context, async () => {
      let ordinal = 0;
      try {
        const result = await withNativeExecutionInterceptor(async (command, input, next) => {
          const index = ordinal++; const digest = canonicalJsonDigest(input);
          let stored: JsonObject | null = null; let reserved = false;
          await runtime.commit(async tx => {
            const journal = await tx.doc(ReviewFactorAccounts, runtime.conversationId);
            const state = journal.records[task.input.recordKey]; if (!state || state.taskId !== task.id) throw new Error('review_factor_account_receipt_missing');
            const original = state.calls[index];
            if (original) { if (original.command !== command || original.digest !== digest) throw new Error('review_factor_account_stage_conflict'); stored = detached(original.result); reserved = true; }
            else state.calls.push({ command, digest, result: null });
            return undefined;
          }, context);
          if (stored) return await readNativeEvidence(runtime, runtime.conversationId, task.id, stored, context) as NativeJson;
          // A crash may follow external execution but precede its checkpoint. Never resubmit or erase that attempt.
          if (reserved) throw new Error('review_factor_account_stage_requires_reconciliation');
          const value = object(await next());
          const encoded = await retainNativeEvidence(value, async (part, data) => {
            await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, runtime.conversationId, `${task.id}:${index * 1000000 + part}`, null)).data = data; return undefined; }, context);
          });
          // Segment the compression part indices without changing the original result's data shape.
          if (encoded.format) encoded.partOffset = index * 1000000;
          await runtime.commit(async tx => { (await tx.doc(ReviewFactorAccounts, runtime.conversationId)).records[task.input.recordKey]!.calls[index]!.result = encoded; return undefined; }, context);
          return value;
        }, () => task.input.portfolioSource ? replayStoredReviewAccount({ data: task.input.data, source: task.input.portfolioSource, binary: task.input.binary, evidenceRef: task.input.evidenceRef }, context) : replayReviewAccount({ data: task.input.data, members: [{ model: task.input.model, weight: '1' }], policy: task.input.policy, binary: task.input.binary, evidenceRef: task.input.evidenceRef }, context));
        const full = await retainNativeEvidence(result.evidence, async (part, data) => {
          await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, runtime.conversationId, `${task.id}:${ordinal * 1000000 + part}`, null)).data = data; return undefined; }, context);
        });
        if (full.format) full.partOffset = ordinal * 1000000;
        const evidence: JsonObject = { method: result.evidence.method ?? 'QZ_FACTOR_ACCOUNT_REVIEW_V1', paired: result.evidence.paired ?? null,
          stress: result.evidence.stress ?? null, assumptions: result.evidence.assumptions ?? [], limitations: result.evidence.limitations ?? [], consumedScientificCalls: ordinal };
        await runtime.commit(async tx => {
          const output = { metrics: JSON.parse(JSON.stringify(result.metrics)), evidence, native: full, evidenceTaskId: task.id };
          const record = (await tx.doc(ReviewFactorAccounts, runtime.conversationId)).records[task.input.recordKey]!;
          record.status = 'COMPUTED'; record.result = output;
          return { status: 'terminal', outcome: { status: 'completed', result: output } };
        }, context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, task.id, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(async tx => { const record = (await tx.doc(ReviewFactorAccounts, runtime.conversationId)).records[task.input.recordKey]; if (record) record.status = 'FAILED'; return { status: 'terminal', outcome: { status: 'failed', error: { message: 'review_factor_account_evidence_unavailable' } } }; }, context);
      }
    });
  } },
  async abort(task, runtime, context) { await runtime.commit(async tx => { const record = (await tx.doc(ReviewFactorAccounts, runtime.conversationId)).records[task.input.recordKey]; if (record) record.status = 'FAILED'; return { status: 'terminal', outcome: { status: 'aborted' } }; }, context); },
});

export async function factorAccountEvidence(api: ResearchRuntime, context: Context, data: JsonObject, model: string, policy: StrategyPolicy, evidenceRef: string): Promise<JsonObject | null> {
  const key = sourceKey(data, model, policy, evidenceRef);
  const admission = await api.commit(async tx => {
    const journal = await tx.doc(ReviewFactorAccounts, api.conversationId); const previous = journal.records[key];
    if (previous) return { id: previous.taskId, status: previous.status, result: detached(previous.result) };
    const id = await tx.createTask(ReviewFactorAccountTask, { data, model, policy, evidenceRef, binary: nativeTargetForData(data), recordKey: key }, { ownership: { kind: 'task', taskId: api.taskId } });
    journal.records[key] = { digest: key, taskId: id, status: 'RESERVED', calls: [], result: null };
    return { id, status: 'RESERVED', result: null };
  }, context);
  if (admission.status === 'COMPUTED') return admission.result;
  if (admission.status === 'FAILED') return null;
  const settled = await api.waitForTask(admission.id, context);
  return settled.state.outcome.status === 'completed' ? settled.state.outcome.result : null;
}

/** Every admitted same-request factor attempt is disclosed, including failures and duplicates. */
export function factorReviewCohort(selected: JsonObject, trials: readonly JsonObject[]): JsonObject[] {
  const proposal = object(selected.proposal);
  return trials.filter(trial => {
    const candidate = object(trial.proposal);
    return trial.status !== 'REJECTED' && typeof candidate.model === 'string' && !Array.isArray(candidate.members)
      && canonical(candidate.request ?? null) === canonical(proposal.request ?? null);
  }).map(detached).sort((a, b) => Number(a.taskId) - Number(b.taskId));
}

/** Build DSR input from actual reference-strategy NAV returns, never factor labels. */
export async function factorCohortSelectionEvidence(api: ResearchRuntime, context: Context, input: ResearchInput,
  selected: JsonObject, trials: readonly JsonObject[], evidenceRef: string): Promise<{ plans: ReviewNativePlan[]; selectedAccount: JsonObject | null; cohort: JsonObject }> {
  const policy = thresholdPolicy(object(input.policy.strategy));
  const members = factorReviewCohort(selected, trials);
  const digest = createHash('sha256').update(canonical(detached({ data: input.data, policy, members }) as unknown as JsonObject)).digest('hex');
  await api.commit(async tx => {
    const journal = await tx.doc(ReviewFactorAccounts, api.conversationId);
    journal.cohorts[digest] ??= { method: 'QZ_FACTOR_REFERENCE_COHORT_V1', trialIds: members.map(trial => trial.taskId!), reservations: {}, status: 'STARTED' };
  }, context);
  const rows: { id: string; returns: number[] }[] = []; let timestamps: string[] | null = null;
  let selectedAccount: JsonObject | null = null;
  let complete = members.length > 0 && members.some(trial => trial.taskId === selected.taskId);
  for (const trial of members) {
    if (trial.status !== 'COMPUTED') { complete = false; continue; }
    const reference = `research:${api.conversationId}:trial:${trial.taskId}:validation#reference-strategy`;
    await api.commit(async tx => {
      const cohort = (await tx.doc(ReviewFactorAccounts, api.conversationId)).cohorts[digest]!;
      const reservations = object(cohort.reservations); reservations[String(trial.taskId)] ??= 8; cohort.reservations = reservations;
    }, context);
    const account = await factorAccountEvidence(api, context, input.data, object(trial.proposal).model as string, policy, reference);
    if (trial.taskId === selected.taskId) selectedAccount = account;
    if (!account) { complete = false; continue; }
    try {
      const native = await readNativeEvidence(api, api.conversationId, account.evidenceTaskId as TaskId, object(account.native), context);
      const simulation = object(object(native.candidate).simulation);
      const panel = nativeReviewReturnPanel(simulation);
      if (!timestamps) timestamps = panel.times;
      if (canonical(panel.times) !== canonical(timestamps)) { complete = false; continue; }
      rows.push({ id: String(trial.taskId), returns: panel.returns });
    } catch { complete = false; }
  }
  const disclosure: JsonObject = { id: digest, method: 'QZ_FACTOR_REFERENCE_COHORT_V1', totalTrials: members.length,
    comparableReturnSeries: rows.length, ledgerComplete: complete && rows.length === members.length, trialIds: members.map(trial => trial.taskId!) };
  await api.commit(async tx => {
    const cohort = (await tx.doc(ReviewFactorAccounts, api.conversationId)).cohorts[digest]!;
    cohort.status = 'COMPUTED'; cohort.disclosure = disclosure;
    if (rows.some(row => row.id === String(selected.taskId))) object(cohort.reservations).selection = 1;
  }, context);
  if (!timestamps || !rows.some(row => row.id === String(selected.taskId))) return { plans: [], selectedAccount, cohort: { ...disclosure, reason: 'selected_reference_net_return_path_unavailable' } };
  return { selectedAccount, cohort: disclosure, plans: [{ kind: 'SELECTION', target: nativeTargetForData(input.data), evidenceRef: `${evidenceRef}#selection`,
    source: { cohort: disclosure, timestamps_ns: timestamps, trials: rows },
    analysis: { kind: 'SELECTION', total_trials: members.length, ledger_complete: complete && rows.length === members.length, selected_trial_id: String(selected.taskId),
      trials: rows, timestamps_ns: timestamps, frequency: 'DAY', block_length: 1 } }] };
}

/** Persistent paired-account evidence for a research ensemble, including fitted portfolios. */
export async function portfolioAccountEvidence(api: ResearchRuntime, context: Context, data: JsonObject, original: JsonObject, policy: StrategyPolicy, evidenceRef: string): Promise<JsonObject | null> {
  const key = createHash('sha256').update(canonical(detached({ data, original, policy, evidenceRef }) as unknown as JsonObject)).digest('hex');
  const admission = await api.commit(async tx => {
    const journal = await tx.doc(ReviewFactorAccounts, api.conversationId); const previous = journal.records[key];
    if (previous) return { id: previous.taskId, status: previous.status, result: detached(previous.result) };
    const id = await tx.createTask(ReviewFactorAccountTask, { data, model: '', policy, portfolioSource: original, evidenceRef, binary: nativeTargetForData(data), recordKey: key }, { ownership: { kind: 'task', taskId: api.taskId } });
    journal.records[key] = { digest: key, taskId: id, status: 'RESERVED', calls: [], result: null };
    return { id, status: 'RESERVED', result: null };
  }, context);
  if (admission.status === 'COMPUTED') return admission.result;
  if (admission.status === 'FAILED') return null;
  const settled = await api.waitForTask(admission.id, context);
  return settled.state.outcome.status === 'completed' ? settled.state.outcome.result : null;
}

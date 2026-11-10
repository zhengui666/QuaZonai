import type { Context } from '@earendil-works/chord';
import { defineDocFamily, defineTask, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { ResearchRuntime } from '../../research/src/research-runtime.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { canonical } from '../../research/src/documents.ts';
import { forecastMembers, blendTargets, executeNative } from '../../research/plugins/native-portfolio.ts';
import { accountReviewMetrics } from '../../research/src/review-account-science.ts';
import { runReviewScience } from '../../research/src/review-science-adapter.ts';
import { withNativeRemoteTracking, retainNativeRemoteUncertainty } from '../../research/src/native-execution.ts';
import { computePortfolio, type PortfolioTaskInput } from './native.ts';
import { assessCapacity } from './capacity.ts';
import { json, Problem } from './facts.ts';

type Base = { binary: NativeTarget; dataDir: string };
export type ScientificStagePlan = Base & (
  | { kind: 'native'; command: string; input: JsonObject; args?: string[] }
  | { kind: 'forecast'; catalog: string; members: Parameters<typeof forecastMembers>[2]; request: JsonObject; policy: Parameters<typeof forecastMembers>[4]; currency: string; partition: 'VALIDATION' | 'HELDOUT' }
  | { kind: 'blend'; rows: JsonObject[][]; weights: string[]; force?: boolean }
  | { kind: 'capacity'; catalog: string; simulation: JsonObject; assumptions?: Parameters<typeof assessCapacity>[3] }
  | { kind: 'account-metrics'; candidate: JsonObject; baseline: JsonObject; stress?: JsonObject; evidenceRef: string }
  | { kind: 'review-science'; source: JsonObject; analysis: JsonObject; evidenceRef: string }
  | { kind: 'portfolio'; input: PortfolioTaskInput; partition: 'VALIDATION' | 'HELDOUT'; data?: JsonObject }
);
export type ScientificStageOperation = ScientificStagePlan extends infer Plan ? Plan extends Base ? Omit<Plan, keyof Base> : never : never;
/** Only original child identities and business accounting live here; outcomes belong to official Tasks. */
export const ScientificStages = defineDocFamily<{ children: Record<string, { taskId: number; calls: number }>; reservedCalls: number }, null>({
  kind: 'qz.scientific-stage-references', version: 1, scope: 'session', family: true,
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127,
  initial: () => ({ children: {}, reservedCalls: 0 }),
});
export const ScientificStage = defineTask<ScientificStagePlan, { phase: 'execute' }, JsonObject>({
  name: 'qz.scientific-stage', version: 1, initial: () => ({ phase: 'execute' }),
  phases: { async execute(task, runtime, context) {
    let directory: string | undefined;
    try {
      directory = await mkdtemp(join(task.input.dataDir, 'scientific-stage-'));
      const result = await withNativeRemoteTracking(task.id, runtime, context, async () => {
        const plan = task.input;
        switch (plan.kind) {
          case 'native': return executeNative(plan.binary, plan.command, plan.input, plan.args ?? [], directory!, context);
          case 'forecast': return json(await forecastMembers(plan.binary, plan.catalog, plan.members, plan.request, plan.policy, plan.currency, directory!, context, false, plan.partition));
          case 'blend': return json(await blendTargets(plan.binary, plan.rows, plan.weights, directory!, context, plan.force));
          case 'capacity': return json(await assessCapacity(plan.binary, plan.catalog, plan.simulation, plan.assumptions, directory!, context));
          case 'account-metrics': return json(await accountReviewMetrics({ candidate: plan.candidate, baseline: plan.baseline, ...(plan.stress ? { stress: plan.stress } : {}), binary: plan.binary, evidenceRef: plan.evidenceRef, context }));
          case 'review-science': return json(await runReviewScience({ source: plan.source, analysis: plan.analysis, evidenceRef: plan.evidenceRef, target: plan.binary, context }));
          case 'portfolio': return json(await computePortfolio(plan.input, context, undefined, { partition: plan.partition, ...(plan.data ? { data: plan.data } : {}) }));
        }
      });
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), context);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, task.id, runtime, context);
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'scientific_stage_failed' } } }), context);
    } finally { if (directory) await rm(directory, { recursive: true, force: true }); }
  } },
  async abort(_task, runtime, context) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context); },
});
export async function scientificStageResult(api: ResearchRuntime, context: Context, ownerTaskId: number, key: string): Promise<JsonObject | undefined> {
  const reference = (await api.snapshot(ScientificStages, String(ownerTaskId), context))?.children[key];
  if (!reference) return undefined;
  const task = await api.getTask(reference.taskId as TaskId<JsonObject>, context);
  return task?.state.status === 'terminal' && task.state.outcome.status === 'completed' ? task.state.outcome.result : undefined;
}
export async function runScientificStage(api: ResearchRuntime, context: Context, key: string, calls: number, input: ScientificStagePlan): Promise<JsonObject> {
  if (!key || !Number.isSafeInteger(calls) || calls < 0) throw new Problem('scientific_stage_invalid');
  const plan = json(input) as unknown as ScientificStagePlan;
  const taskId = await api.commit(async tx => {
    const state = await tx.doc(ScientificStages, String(api.taskId), null);
    const existing = state.children[key];
    if (existing) {
      const task = await tx.task(existing.taskId as TaskId);
      if (!task || existing.calls !== calls || canonical(json(task.input)) !== canonical(json(plan))) throw new Problem('scientific_stage_identity_conflict');
      return existing.taskId as TaskId<JsonObject>;
    }
    const taskId = await tx.createTask(ScientificStage, plan, { conversationId: api.conversationId, ownership: { kind: 'task', taskId: api.taskId } });
    state.children[key] = { taskId, calls }; state.reservedCalls += calls;
    return taskId;
  }, context);
  const settled = await api.waitForTask(taskId, context);
  const outcome = settled.state.outcome;
  if (outcome.status !== 'completed') throw new Problem(outcome.status === 'failed' || outcome.status === 'faulted' ? outcome.error.message : 'scientific_stage_aborted');
  return json(outcome.result);
}

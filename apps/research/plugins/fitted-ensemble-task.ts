import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../src/native-execution.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import { createHash } from 'node:crypto';
import { v7 as uuidv7 } from 'uuid';
import type { Context } from '@earendil-works/chord';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { fitNativeEnsemble, replayFittedEnsemble } from './native-fitted-portfolio.ts';
import type { FittedMember } from './native-fitted-portfolio.ts';
import { readNativeEvidence, NativeEvidencePart, retainNativeEvidence } from './native-evidence.ts';
import { object } from '../src/trials.ts';
import type { ThresholdPolicy, StrategyPartition } from '../src/strategy.ts';
import type { ResearchRuntime } from '../src/research-runtime.ts';
import { classifyExecutionError, executionDiagnostic, ExecutionFailure } from '../src/execution-errors.ts';
import { canonical, Study } from '../src/documents.ts';
import { iterationRequest, normalizedRequest } from './native-wasm.ts';

/** This UUID identifies a persisted bridge artifact, never the upstream data
 * producer's ID or a new authorization to access that producer. */
export const ResearchDatasetBridge = defineDoc<{ artifact: JsonObject | null }>({
  kind: 'qz.ensemble.dataset-bridge', version: 1, scope: 'task', initial: () => ({ artifact: null }),
});

type Input = { mode: 'FIT'; data: JsonObject; members: FittedMember[]; binary: NativeTarget }
  | { mode: 'REPLAY'; data: JsonObject; fitTaskId: number; policy: ThresholdPolicy; partition: StrategyPartition; binary: NativeTarget };
export const FittedEnsembleTask = defineTask<Input, { phase: 'compute' }, JsonObject>({
  name: 'qz.ensemble.fit-replay', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    return withNativeRemoteTracking(task.id, runtime, context, async () => {
    try {
      let output: { summary: JsonObject; evidence: JsonObject };
      if (task.input.mode === 'FIT') output = await fitNativeEnsemble({ ...task.input, evidenceIdentity: `native-task:${runtime.conversationId}:${task.id}` }, context);
      else {
        const source = await runtime.getTask(task.input.fitTaskId as TaskId<JsonObject>, context);
        if (source?.kind !== 'qz.ensemble.fit-replay' || source.conversationId !== runtime.conversationId || source.state.status !== 'terminal' || source.state.outcome.status !== 'completed') throw new Error('registered_ensemble_fit_missing');
        const evidence = await readNativeEvidence(runtime, source.conversationId, source.id, object(source.state.outcome.result.native, 'fit_native'), context);
        const members = object(evidence.fit, 'fit').members;
        if (!Array.isArray(members) || !Array.isArray(evidence.members)) throw new Error('registered_ensemble_fit_invalid');
        const original = evidence.members.map(value => { const member = object(value, 'fit_member');
          if (typeof member.factorVersionId !== 'string' || typeof member.model !== 'string' || typeof member.sourceTrialId !== 'number') throw new Error('registered_ensemble_fit_invalid');
          return { factorVersionId: member.factorVersionId, model: member.model, sourceTrialId: member.sourceTrialId }; });
        const replay = task.input;
        const fittedData = object(evidence.data, 'original_fit_data');
        const expectedData = replay.partition === 'VALIDATION' ? fittedData : object(fittedData.sealed, 'original_fit_sealed_data');
        if (canonical(replay.data) !== canonical(expectedData)) throw new Error('ensemble_replay_original_data_mismatch');
        let datasetBridge: JsonObject | undefined;
        if (replay.policy.simulationSettings.multi_currency_spot_cash != null) await runtime.commit(async tx => {
          const originalStudy = await tx.doc(Study, runtime.conversationId);
          const request = replay.partition === 'VALIDATION' ? iterationRequest(replay.data, true) : normalizedRequest(replay.data);
          const binding = { datasetRevisionIds: originalStudy.input?.campaign?.datasetRevisionIds ?? [],
            sourceInputDigest: createHash('sha256').update(canonical(replay.data)).digest('hex'),
            sourceSelection: object(object(request.forecast, 'forecast').selection, 'selection'),
            partition: replay.partition, simulationSettings: replay.policy.simulationSettings,
            runtime: { conversationId: runtime.conversationId, taskId: task.id } };
          const fingerprint = createHash('sha256').update(canonical(binding)).digest('hex');
          const stored = await tx.doc(ResearchDatasetBridge, task.id);
          stored.artifact ??= { schema_version: 1, id: uuidv7(), kind: 'RESEARCH_DATASET_REVISION_BRIDGE', fingerprint, ...binding };
          if (stored.artifact.fingerprint !== fingerprint) throw new Error('research_dataset_bridge_basis_mismatch');
          datasetBridge = JSON.parse(JSON.stringify(stored.artifact)) as JsonObject;
          return undefined;
        }, context);
        output = await replayFittedEnsemble({ ...replay, fit: object(evidence.fit, 'fit'), catalogPrefix: object(evidence.catalogPrefix, 'original_fit_catalog_prefix_evidence'), members: original, datasetBridge }, context);
        output.evidence.fitRequest = evidence.fitRequest!;
        output.evidence.fitTaskId = source.id;
        output.evidence.data = fittedData;
      }
      const native = await retainNativeEvidence(output.evidence, async (index, data) => {
        await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, runtime.conversationId, `${task.id}:${index}`, null)).data = data; return undefined; }, context);
      });
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { summary: { ...output.summary, evidenceTaskId: task.id }, native } } }), context);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, task.id, runtime, context);
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: executionDiagnostic(error, 'ensemble_failed'), detail: { ...classifyExecutionError(error) } } } }), context);
    }
    });
  } },
  async abort(_task, runtime, context) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context); },
});
const Child = defineDoc<{ taskId: TaskId<JsonObject> | null }>({ kind: 'qz.ensemble.child', version: 1, scope: 'task', initial: () => ({ taskId: null }) });
export async function runFittedEnsemble(api: ResearchRuntime, context: Context, input: Input): Promise<JsonObject> {
  const id = await api.commit(async tx => {
    const child = await tx.doc(Child, api.taskId);
    child.taskId ??= await tx.createTask(FittedEnsembleTask, input, { ownership: { kind: 'task', taskId: api.taskId } });
    return child.taskId;
  }, context);
  const settled = await api.waitForTask(id, context); const outcome = settled.state.outcome;
  if (outcome.status !== 'completed') throw new ExecutionFailure('error' in outcome && outcome.error ? outcome.error.message : outcome.status, { operation: 'ensemble-fit-replay' });
  return object(outcome.result.summary, 'ensemble_summary');
}

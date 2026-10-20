import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, ToolExecutionApi } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { evaluateWasm } from './native-wasm.ts';
import { readNativeEvidence } from './native-evidence.ts';
import { object } from '../src/trials.ts';
import { classifyExecutionError, executionDiagnostic, ExecutionFailure } from '../src/execution-errors.ts';

export const FactorTask = defineTask<{ model: string; data: JsonObject }, { phase: 'compute' }, JsonObject>({
  name: 'qz.factor.wasm', version: 1, initial: () => ({ phase: 'compute' }),
  phases: {
    async compute(task, runtime, context) {
      try {
        const reference = task.input.data.catalog_scan_request !== undefined ? object(task.input.data.catalog_scope, 'catalog_scope') : null;
        let scope: JsonObject | undefined;
        if (reference) {
          const original = await runtime.getTask(reference.taskId as TaskId<JsonObject>, context);
          if (original?.state.status !== 'terminal' || original.state.outcome.status !== 'completed') throw new Error('native_catalog_scope_missing');
          scope = await readNativeEvidence(runtime, original.conversationId, original.id, object(original.state.outcome.result.native, 'scope_native'), context);
        }
        const result = await evaluateWasm(task.input.model, task.input.data, context, scope);
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), context);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: {
          message: executionDiagnostic(error, 'factor_failed'), detail: { ...classifyExecutionError(error, 'scientific_execution_failed', { operation: task.input.data.catalog_scan_request !== undefined ? 'scan-factor-catalog' : 'evaluate-factor' }) },
        } } }), context);
      }
    },
  },
  async abort(_task, runtime, context) {
    // The native execution environment's mandatory cleanup joins its processes
    // before the run invocation ends; upstream then schedules this abort phase.
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

const Child = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.factor-child', version: 1, scope: 'task', initial: () => ({ taskId: null }),
});

export async function runFactor(api: ToolExecutionApi, context: Context, model: string, data: JsonObject): Promise<JsonObject> {
  const id = await api.commit(async (tx) => {
    const binding = await tx.doc(Child, api.taskId);
    if (binding.taskId !== null) return binding.taskId;
    const child = await tx.createTask(FactorTask, { model, data }, { ownership: { kind: 'task', taskId: api.taskId } });
    binding.taskId = child;
    return child;
  }, context);
  const settled = await api.waitForTask(id, context);
  if (settled.state.outcome.status !== 'completed') {
    const outcome = settled.state.outcome;
    const detail = 'error' in outcome ? outcome.error?.detail : undefined;
    throw new ExecutionFailure('error' in outcome && outcome.error ? outcome.error.message : outcome.status,
      detail && typeof detail === 'object' && !Array.isArray(detail) ? detail : { operation: data.catalog_scan_request !== undefined ? 'scan-factor-catalog' : 'evaluate-factor' });
  }
  return settled.state.outcome.result;
}

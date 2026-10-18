import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, ToolExecutionApi } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { evaluatePortfolio } from './native-portfolio.ts';
import type { PortfolioEvaluationInput } from './native-portfolio.ts';
import { object } from '../src/trials.ts';
import { NativeEvidencePart, retainNativeEvidence } from './native-evidence.ts';

export const PortfolioEvaluationTask = defineTask<PortfolioEvaluationInput, { phase: 'compute' }, JsonObject>({
  name: 'qz.strategy-portfolio.evaluate', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    try {
      const result = await evaluatePortfolio(task.input, context);
      const native = await retainNativeEvidence(result.evidence, async (index, data) => {
        await runtime.commit(async (tx) => {
          (await tx.doc(NativeEvidencePart, task.id, String(index), null)).data = data;
          return undefined;
        }, context);
      });
      // Complete raw evidence stays in this native child's outcome or task documents, never model-facing Study facts.
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed',
        result: { summary: { ...result.summary, evidenceTaskId: task.id }, native } } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'portfolio_evaluation_failed' } } }), context);
    }
  } },
  async abort(_task, runtime, context) {
    // ExecutionEnv cleanup has joined the local processes before this phase runs.
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

// A complete ensemble has its own child binding; never reuse FactorTask's one-model child.
const Child = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.strategy-portfolio-child', version: 1, scope: 'task', initial: () => ({ taskId: null }),
});

export async function runPortfolio(api: ToolExecutionApi, context: Context, input: PortfolioEvaluationInput): Promise<JsonObject> {
  const id = await api.commit(async (tx) => {
    const binding = await tx.doc(Child, api.taskId);
    if (binding.taskId !== null) return binding.taskId;
    const child = await tx.createTask(PortfolioEvaluationTask, input, { ownership: { kind: 'task', taskId: api.taskId } });
    binding.taskId = child;
    return child;
  }, context);
  const settled = await api.waitForTask(id, context);
  if (settled.state.outcome.status !== 'completed') {
    const outcome = settled.state.outcome;
    throw new Error('error' in outcome && outcome.error ? outcome.error.message : outcome.status);
  }
  return object(settled.state.outcome.result.summary, 'portfolio_summary');
}

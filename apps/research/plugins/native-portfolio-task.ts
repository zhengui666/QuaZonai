import type { ResearchRuntime } from '../src/research-runtime.ts';
import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { evaluatePortfolio } from './native-portfolio.ts';
import type { PortfolioEvaluationInput } from './native-portfolio.ts';
import { object } from '../src/trials.ts';
import { NativeEvidencePart, retainNativeEvidence } from './native-evidence.ts';
import { classifyExecutionError, executionDiagnostic, ExecutionFailure } from '../src/execution-errors.ts';
import { readCatalogScope } from './native-catalog-scope.ts';

export const PortfolioEvaluationTask = defineTask<PortfolioEvaluationInput, { phase: 'compute' }, JsonObject>({
  name: 'qz.strategy-portfolio.evaluate', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    try {
      const scope = task.input.policy.kind === 'catalog-ensemble' ? await readCatalogScope(runtime, object(task.input.data.catalog_scope, 'catalog_scope'), context) : undefined;
      const result = await evaluatePortfolio(task.input, context, scope);
      const native = await retainNativeEvidence(result.evidence, async (index, data) => {
        await runtime.commit(async (tx) => {
          (await tx.doc(NativeEvidencePart, runtime.conversationId, `${task.id}:${index}`, null)).data = data;
          return undefined;
        }, context);
      });
      // Keep full evidence associated with this child in the original store, never model-facing Study facts.
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed',
        result: { summary: { ...result.summary, evidenceTaskId: task.id }, native } } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: {
        message: executionDiagnostic(error, 'portfolio_evaluation_failed'), detail: { ...classifyExecutionError(error, 'scientific_execution_failed', { operation: 'portfolio-evaluation' }) },
      } } }), context);
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

export async function runPortfolio(api: ResearchRuntime, context: Context, input: PortfolioEvaluationInput): Promise<JsonObject> {
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
    const detail = 'error' in outcome ? outcome.error?.detail : undefined;
    throw new ExecutionFailure('error' in outcome && outcome.error ? outcome.error.message : outcome.status,
      detail && typeof detail === 'object' && !Array.isArray(detail) ? detail : { operation: 'portfolio-evaluation' });
  }
  return object(settled.state.outcome.result.summary, 'portfolio_summary');
}

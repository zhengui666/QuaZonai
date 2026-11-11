import type { Context } from '@earendil-works/chord';
import { defineDoc, defineTask, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { isTickResearchInput } from '../../../packages/contracts/src/tick-input.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult, type NativeTickEconomicRequest, type NativeTickEconomicResult } from '../../../packages/contracts/src/economic-tick.ts';
import { canonical, Study } from '../src/documents.ts';
import { executeNativeValue, nativeTargetForData, retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../src/native-execution.ts';
import { classifyExecutionError, executionDiagnostic } from '../src/execution-errors.ts';
import type { ResearchRuntime } from '../src/research-runtime.ts';
import { object } from '../src/trials.ts';
import { compileWasm, iterationRequest } from './native-wasm.ts';
import { checkExposure } from './native-exposure.ts';

/** Explicit retained signals share one account; this does not create a factor trial. */
export async function replayTickEconomic(target: NativeTarget, request: NativeTickEconomicRequest,
  artifacts: { catalog: string; fundingCatalog?: string; model: string | string[]; bookCores?: number }, directory: string, context: Context): Promise<NativeTickEconomicResult> {
  const admitted = validateNativeTickEconomicRequest(request);
  const models = Array.isArray(artifacts.model) ? artifacts.model : [artifacts.model];
  if (!models.length || (admitted.signal_weights?.length ?? 1) !== models.length) throw new Error('tick_economic_signal_weights_model_mismatch');
  const result = validateNativeTickEconomicResult(await executeNativeValue(target, 'replay-tick-economic', admitted,
    ['--catalog', artifacts.catalog, '--funding-catalog', artifacts.fundingCatalog ?? artifacts.catalog, ...models.flatMap(model => ['--model', model]), ...(artifacts.bookCores && artifacts.bookCores > 1 ? ['--book-cores', String(artifacts.bookCores)] : [])], directory, context));
  if (canonical((result.request)) !== canonical((admitted))) throw new Error('tick_economic_result_request_mismatch');
  return result;
}

type EconomicInput = { trialId: number; model: string; catalog: string; target: NativeTarget; request: NativeTickEconomicRequest; bookCores: number };

export const TickEconomicTask = defineTask<EconomicInput, { phase: 'compute' }, JsonObject>({
  name: 'qz.factor.tick-economic', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    let directory: string | undefined;
    try {
      directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-tick-economic-'));
      const model = join(directory, 'model.wasm');
      await writeFile(model, compileWasm(task.input.model, 'retained-factor.wat', 'replay-tick-economic'));
      const result = await withNativeRemoteTracking(task.id, runtime, context, () => replayTickEconomic(task.input.target,
        task.input.request, { catalog: task.input.catalog, model, bookCores: task.input.bookCores }, directory!, context));
      const summary: JsonObject = {
        model: result.model, methods: result.methods, limitations: result.limitations,
        metrics: result.metrics.filter(row => row.partition === 'TRAIN' || row.partition === 'VALIDATION'),
        days: result.days.filter(row => BigInt(row.day_start_ns) >= BigInt(task.input.request.research.input.event_start_ns)
          && BigInt(row.day_start_ns) < BigInt(task.input.request.research.validation_end_ns)),
        native_fills: result.native_fills, native_rejections: result.native_rejections,
        fees_usdc_equivalent: result.fees_usdc_equivalent, fees_by_currency: result.fees_by_currency,
        initial_conversion_cost_usdc: result.initial_conversion_cost_usdc, funding_usdc: result.funding_usdc,
        ...(result.small_capital_diagnostics ? { small_capital_diagnostics: result.small_capital_diagnostics } : {}),
      };
      await runtime.commit(async tx => {
        const trial = (await tx.doc(Study, task.conversationId)).trials.find(row => row.taskId === task.input.trialId)!;
        trial.economic = { ...object(trial.economic, 'tick_economic'), taskId: task.id, status: 'COMPUTED', summary, error: null };
        return { status: 'terminal', outcome: { status: 'completed', result: summary } };
      }, context);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, task.id, runtime, context);
      if (runtime.signal.aborted) throw error;
      const message = executionDiagnostic(error, 'tick_economic_replay_failed');
      await runtime.commit(async tx => {
        const trial = (await tx.doc(Study, task.conversationId)).trials.find(row => row.taskId === task.input.trialId)!;
        trial.economic = { ...object(trial.economic, 'tick_economic'), taskId: task.id, status: 'FAILED', summary: null, error: message };
        return { status: 'terminal', outcome: { status: 'failed', error: { message,
          detail: { ...classifyExecutionError(error, 'scientific_execution_failed', { operation: 'replay-tick-economic' }) } } } };
      }, context);
    } finally { if (directory) await rm(directory, { recursive: true, force: true }); }
  } },
  async abort(task, runtime, context) {
    await runtime.commit(async tx => {
      const trial = (await tx.doc(Study, task.conversationId)).trials.find(row => row.taskId === task.input.trialId)!;
      trial.economic = { ...object(trial.economic, 'tick_economic'), taskId: task.id, status: 'STOPPED', summary: null, error: 'tick_economic_local_stop' };
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, context);
  },
});

export async function tickEconomicFeedback(api: Pick<ResearchRuntime, 'getTask'>, context: Context, economic: JsonObject): Promise<JsonObject> {
  const task = await api.getTask(economic.taskId as TaskId<JsonObject>, context);
  if (!task) throw new Error('tick_economic_task_missing');
  const state = task.state;
  if (state.status !== 'terminal') return { ...economic, taskStatus: state.status };
  const outcome = state.outcome;
  return { ...economic, taskId: task.id, taskStatus: state.status,
    status: outcome.status === 'completed' ? 'COMPUTED' : outcome.status === 'aborted' || outcome.status === 'orphaned' ? 'STOPPED' : 'FAILED',
    summary: outcome.status === 'completed' ? outcome.result : null,
    error: 'error' in outcome ? outcome.error?.message ?? outcome.status : outcome.status === 'completed' ? null : outcome.status };
}

const EconomicChild = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.factor-economic-child', version: 1, scope: 'task', checkpointWhen: () => true, initial: () => ({ taskId: null }),
});

export async function runTickEconomic(api: ResearchRuntime, context: Context, trialId: number, recompute = false): Promise<JsonObject> {
  const study = await api.snapshot(Study, api.conversationId, context);
  if (!study?.input) throw new Error('research_input_missing');
  if (study.final !== null || study.reviewSelection) throw new Error('research_search_closed');
  const trial = study.trials.find(row => row.taskId === trialId);
  if (trial?.status !== 'COMPUTED') throw new Error('tick_economic_computed_factor_required');
  const input = study.input;
  if (!isTickResearchInput(input.data)) throw new Error('tick_economic_tick_input_required');
  const { bookCores = 1, ...policy } = object(input.policy.tickEconomic, 'tick_economic_policy');
  if (!Number.isSafeInteger(bookCores) || (bookCores as number) < 1) throw new Error('tick_book_cores_invalid');
  if ('research' in policy || 'schema_version' in policy) throw new Error('tick_economic_policy_fields_invalid');
  const request = validateNativeTickEconomicRequest({ ...policy, schema_version: 1, research: iterationRequest(input.data, true) });
  const proposal = object(trial.proposal, 'trial_proposal');
  if (typeof proposal.model !== 'string' || canonical(object(trial.result, 'trial_result').request!) !== canonical(iterationRequest(input.data))) throw new Error('tick_economic_original_request_mismatch');
  if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
  await checkExposure(api, context, input.data, false, input.data.sealed ? object(input.data.sealed, 'heldout_data') : undefined);
  const id = await api.commit(async tx => {
    const current = await tx.doc(Study, api.conversationId);
    if (current.final !== null || current.reviewSelection) throw new Error('research_search_closed');
    const binding = await tx.doc(EconomicChild, api.taskId);
    if (binding.taskId !== null) return binding.taskId;
    const source = current.trials.find(row => row.taskId === trialId)!;
    let history: JsonObject[] | undefined;
    if (source.economic) {
      const economic = object(source.economic, 'tick_economic');
      const child = await tx.task(economic.taskId as TaskId<JsonObject>);
      if (!child) throw new Error('tick_economic_task_missing');
      if (!recompute || child.state.status !== 'terminal') {
        binding.taskId = child.id as TaskId<JsonObject>;
        return binding.taskId;
      }
      const { history: previous, ...record } = economic;
      history = [...((previous ?? []) as JsonObject[]), record];
    }
    const taskId = await tx.createTask(TickEconomicTask, { trialId, model: proposal.model as string, catalog: input.data.catalog as string,
      target: nativeTargetForData(input.data), request, bookCores: bookCores as number }, { ownership: { kind: 'task', taskId: api.taskId } });
    source.economic = { taskId, status: 'STARTED', summary: null, error: null, ...(history ? { history } : {}) };
    binding.taskId = taskId;
    return taskId;
  }, context);
  await api.waitForTask(id, context);
  const economic = object((await api.snapshot(Study, api.conversationId, context))!.trials.find(row => row.taskId === trialId)!.economic, 'tick_economic');
  const selected = economic.taskId === id ? economic : object((economic.history as JsonObject[]).find(row => row.taskId === id), 'tick_economic_history');
  return tickEconomicFeedback(api, context, selected);
}

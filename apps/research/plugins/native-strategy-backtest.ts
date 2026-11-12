import { nativeWalkForwardResult } from '../src/native-walk-forward-result.ts';
import { defineDoc, defineExtension, defineTask, defineTool, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { bool, obj, opt } from '@earendil-works/pi-schema';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { executeNativeValue, nativeTargetForData, withNativeRemoteTracking, cancelUnsettledNativeExecutions, retainNativeRemoteUncertainty, nativeUncertainTaskId, reconcileNativeExecution, type NativeScanContext, type NativeScanCheckpoint } from '../src/native-execution.ts';
import { object, studyInput } from '../src/trials.ts';
import { Study, canonical } from '../src/documents.ts';
import { ResearchConfiguration, registeredNativeStrategyRequest } from '../src/research-configuration.ts';
import { executionVersions } from '../src/native-result-version.ts';

export function normalizedNativeStrategyRequest(request: JsonObject): JsonObject {
  const expected = structuredClone(request);
  if (expected.available_start_ns === null) delete expected.available_start_ns;
  const venue = object(expected.venue, 'native_strategy_venue');
  if (venue.base_currency === undefined) venue.base_currency = null;
  const strategy = object(expected.strategy, 'native_strategy_config');
  if (strategy.base === null) delete strategy.base;
  return expected;
}

/** Validate the exact bound request and upstream report, never substitute a score. */
export function nativeStrategyResult(value: Parameters<typeof object>[0], request: JsonObject): JsonObject {
  const result = object(value, 'native_strategy_result');
  const expected = normalizedNativeStrategyRequest(request);
  if (canonical(result.request ?? null) !== canonical(expected)) throw new Error('native_strategy_request_mismatch');
  if (result.schema_version !== 1 || typeof result.native_version !== 'string' || !executionVersions.has(result.native_version)) throw new Error('native_strategy_version_mismatch');
  const report = object(result.canonical_result, 'native_strategy_canonical_result');
  const run = object(report.run, 'native_strategy_run');
  if (report.schema !== 'nautilus-backtest-result/v1' || !['completed', 'stopped'].includes(String(run.outcome))) throw new Error('native_strategy_report_invalid');
  for (const key of ['accounts', 'diagnostics', 'fills', 'orders', 'portfolio_snapshots', 'position_snapshots', 'positions']) if (!Array.isArray(report[key])) throw new Error('native_strategy_report_invalid');
  object(report.statistics, 'native_strategy_statistics');
  object(report.components, 'native_strategy_components');
  const counts = object(result.data_rows_by_type, 'native_strategy_data_rows');
  if (!Object.keys(counts).length || Object.values(counts).some(value => typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > 9223372036854775807n)) throw new Error('native_strategy_data_rows_invalid');
  if (!Array.isArray(result.notes) || result.notes.some(value => typeof value !== 'string')) throw new Error('native_strategy_notes_invalid');
  return result;
}

const NativeStrategyChild = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.native-strategy-child', version: 1, scope: 'task', initial: () => ({ taskId: null }),
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127,
});

export const nativeStrategyBacktestTool = defineTool({
  name: 'native_strategy_backtest',
  description: 'Run the exact historical data.native_strategy_backtest_request on its registered catalog using the compiled upstream Nautilus strategy. EMA_CROSS consumes QuoteTick; ORDER_BOOK_IMBALANCE consumes L2 order book data. Venue fees and balances are explicit simulation assumptions. No inferred data range, custom factor, scoring, optimizer, live deployment or trading permission. This historical result does not imply paper/live deployment of the same strategy. By default retain the historical result and continue the Pi tool loop for requested validation; finishResearch=true explicitly ends an independent one-backtest submission. For a two-lane study, read native_strategy_brief and record logic review with the intended validation method BEFORE this execution.',
  parameters: obj({ finishResearch: opt(bool()) }), replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context);
    const request = registeredNativeStrategyRequest(input);
    if (typeof input.data.catalog !== 'string' || !input.data.catalog) throw new Error('catalog_required');
    const id = await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.trials.length || study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
      const binding = await tx.doc(NativeStrategyChild, api.taskId);
      if (!binding.taskId) binding.taskId = await tx.createTask(NativeStrategyBacktestTask, {
        request, catalog: input.data.catalog as string, target: nativeTargetForData(input.data),
      }, { ownership: { kind: 'task', taskId: api.taskId } });
      const configuration = await tx.doc(ResearchConfiguration, api.conversationId);
      if (configuration.evaluationTaskId !== binding.taskId) {
        if (configuration.evaluationTaskId) configuration.previousTaskIds = [...(configuration.previousTaskIds ?? []), configuration.evaluationTaskId];
        configuration.evaluationTaskId = binding.taskId;
      }
      return binding.taskId;
    }, context);
    const settled = await api.waitForTask(id, context);
    if (settled.state.outcome.status !== 'completed') {
      const outcome = settled.state.outcome;
      throw new Error('error' in outcome && outcome.error ? outcome.error.message : 'native_strategy_backtest_aborted');
    }
    const result = nativeStrategyResult(settled.state.outcome.result, request);
    await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
      study.final = { taskId: api.taskId, trialId: null, comparedTrials: [], status: 'COMPUTED',
        reason: 'explicit_native_strategy_backtest_completed', scientificClaim: 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION', result };
    }, context);
    return { output: [{ type: 'text', text: JSON.stringify(result) }], ...(args.finishResearch === true ? { control: { terminate: true } } : {}) };
  },
});

export const NativeStrategyBacktestTask = defineTask<{ request: JsonObject; catalog: string; target: NativeTarget; walkForward?: boolean; sourceRequest?: JsonObject; scanContext?: NativeScanContext }, NativeScanCheckpoint, JsonObject>({
  name: 'qz.native-strategy.backtest', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-native-strategy-'));
    try {
      const raw = await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(
        task.input.target, task.input.walkForward ? 'native-walk-forward' : 'native-strategy-backtest', task.input.request,
        ['--catalog', task.input.catalog], directory, context), task.input.scanContext);
      const result = task.input.walkForward ? nativeWalkForwardResult(raw, task.input.request) : nativeStrategyResult(raw, task.input.request);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), context);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, task.id, runtime, context);
      if (runtime.signal.aborted) throw error;
      const nativeTaskId = nativeUncertainTaskId(error);
      if (nativeTaskId !== null) {
        await runtime.commit(() => ({ status: 'sleeping', until: runtime.now() + 1000, checkpoint: { phase: 'reconcile', nativeTaskId } }), context);
        return;
      }
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'native_strategy_backtest_failed' } } }), context);
    } finally { await rm(directory, { recursive: true, force: true }); }
  }, async reconcile(task, runtime, context) {
    if (task.state.checkpoint.phase !== 'reconcile') throw new Error('native_reconciliation_checkpoint_invalid');
    const settled = await reconcileNativeExecution(task.id, runtime, task.state.checkpoint.nativeTaskId, context);
    await runtime.commit(() => settled ? { status: 'running', checkpoint: { phase: 'compute' } }
      : { status: 'sleeping', until: runtime.now() + 1000, checkpoint: task.state.checkpoint }, context);
  } },
  async abort(task, runtime, context) {
    await cancelUnsettledNativeExecutions(task.id, runtime, context);
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});


export const nativeStrategyBriefTool = defineTool({
  name: 'native_strategy_brief',
  description: 'Read the unchanged registered upstream strategy request, data binding and original research mandate before any backtest. This reads configuration only, no market data or performance. Use it to plan official parameter candidates, explicit fees, chronological validation and logic review. Source event/availability windows are not proof of complete observed coverage; use actual native data row counts and notes later. No currency conversion or inferred balance.',
  parameters: obj({}), replay: 'safe',
  async execute(_args, api, context) {
    const input = await studyInput(api, context);
    const request = registeredNativeStrategyRequest(input);
    return { output: [{ type: 'text', text: JSON.stringify({ goal: input.goal, specification: input.specification ?? null,
      request, catalog: input.data.catalog ?? null, target: nativeTargetForData(input.data),
      scope: 'REGISTERED_CONFIGURATION_ONLY_NOT_OBSERVED_DATA_COVERAGE',
      limitations: ['BALANCES_AND_FEES_ARE_EXPLICIT_ASSUMPTIONS', 'NO_IMPLICIT_USDT_USDC_CONVERSION', 'NATIVE_STATISTICS_KEEP_THEIR_UPSTREAM_RETURN_BASIS'] }) }] };
  },
});
export default defineExtension({ name: 'qz.native-strategy-backtest', tasks: [NativeStrategyBacktestTask], tools: [nativeStrategyBriefTool, nativeStrategyBacktestTool] });

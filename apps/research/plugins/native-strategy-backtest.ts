import { nativeWalkForwardResult } from '../src/native-walk-forward-result.ts';
import { defineDoc, defineExtension, defineTask, defineTool, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { bool, obj, opt, compile, type Schema } from '@earendil-works/pi-schema';
import { readFile } from 'node:fs/promises';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { executeNativeValue, nativeTargetForData, withNativeRemoteTracking, cancelUnsettledNativeExecutions, retainNativeRemoteUncertainty, nativeUncertainTaskId, reconcileNativeExecution, type NativeScanContext, type NativeScanCheckpoint } from '../src/native-execution.ts';
import { validateNumbers } from '../src/input.ts';
import { object, studyInput } from '../src/trials.ts';
import { Study, canonical } from '../src/documents.ts';
import { ResearchConfiguration, assertResearchRunSettled, registeredNativeStrategyRequest } from '../src/research-configuration.ts';
import { executionVersions } from '../src/native-result-version.ts';


// Consume the generated Rust wire schema; no parallel strategy language or parameter allowlist.
const schemas = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas;
function resolveSchema(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(resolveSchema);
  if (!value || typeof value !== 'object') return value;
  const record = value as Record<string, unknown>;
  if (typeof record.$ref === 'string') return resolveSchema(schemas[record.$ref.split('/').at(-1)!]);
  return Object.fromEntries(Object.entries(record).map(([key, item]) => [key, resolveSchema(item)]));
}
export const nativeStrategyRequestSchema = resolveSchema(schemas.NativeStrategyBacktestRequestV1) as Schema<JsonObject>;
const validateRequest = compile(nativeStrategyRequestSchema);
const nativeCatalogInspectSchema = resolveSchema(schemas.NativeCatalogInspectRequestV1) as Schema<JsonObject>;
const validateCatalogQuery = compile(nativeCatalogInspectSchema);
export const nativeStrategyCatalogTool = defineTool({
  name: 'native_strategy_catalog',
  description: 'Inspect only the current registered Nautilus catalog through its bound executor: actual instrument definitions and versions, native data selectors and inclusive ts_init file intervals. Use instrument_ids/data_types to filter and page_size to set the projection page size. next_cursor non-null means INCOMPLETE: pass cursor unchanged to read the next required page; do not infer absence from the first page. The same selector may span pages, and this is not a catalog snapshot. No execution, orders or performance. Use exact string envelope timestamps, not numeric instrument JSON timestamps. Coverage is not proof of event completeness, received-time accuracy, historical fees or suitability for a strategy. Do not silently convert currencies or use current definitions as historical evidence.',
  parameters: nativeCatalogInspectSchema,
  prepareArguments(args) {
    validateNumbers(args);
    if (!validateCatalogQuery.check(args)) throw new Error('native_catalog_query_schema_invalid');
    return args as JsonObject;
  }, replay: 'safe',
  async execute(args, api, context) {
    const input = await studyInput(api, context);
    if (typeof input.data.catalog !== 'string' || !input.data.catalog) throw new Error('catalog_required');
    const binding = structuredClone(input);
    binding.data.native_strategy_backtest_request = {};
    registeredNativeStrategyRequest(binding);
    const id = await api.commit(async tx => {
      const child = await tx.doc(NativeStrategyChild, api.taskId);
      child.taskId ??= await tx.createTask(NativeStrategyCatalogTask, { catalog: input.data.catalog as string, target: nativeTargetForData(input.data), query: args }, { ownership: { kind: 'task', taskId: api.taskId } });
      return child.taskId;
    }, context);
    const task = await api.waitForTask(id, context);
    if (task.state.outcome.status !== 'completed') throw new Error(task.state.outcome.status === 'failed' ? task.state.outcome.error.message : 'native_catalog_inspection_aborted');
    return { output: [{ type: 'text', text: JSON.stringify(task.state.outcome.result) }] };
  },
});

export const NativeStrategyCatalogTask = defineTask<{ catalog: string; target: NativeTarget; query: JsonObject }, { phase: 'read' }, JsonObject>({
  name: 'qz.native-strategy.catalog', version: 1, initial: () => ({ phase: 'read' }),
  phases: { async read(task, runtime, context) {
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-native-catalog-'));
    try {
      const result = object(await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(task.input.target,
        'catalog-inspect', task.input.query, ['--catalog', task.input.catalog], directory, context)), 'native_catalog_inspection');
      if (result.schema_version !== 1 || result.timestamp_basis !== 'ts_init' || result.interval_bounds !== 'inclusive'
        || result.coverage_basis !== 'native_file_intervals_not_event_completeness' || !Array.isArray(result.instruments) || !Array.isArray(result.selectors)) throw new Error('native_catalog_inspection_invalid');
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'native_catalog_inspection_failed' } } }), context);
    } finally { await rm(directory, { recursive: true, force: true }); }
  } },
  async abort(task, runtime, context) {
    await cancelUnsettledNativeExecutions(task.id, runtime, context);
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

export const nativeStrategyConfigureTool = defineTool({
  name: 'native_strategy_configure',
  description: 'Configure an official compiled Nautilus strategy on the unchanged registered catalog in this research conversation. First inspect native_strategy_catalog for real instrument definitions, selectors and file coverage. Use the generated request schema verbatim. Explicitly choose source event and availability windows, instrument, native strategy, simulation venue balances, currency, leverage and fees from the owner mandate and registered evidence; ask for missing consequential assumptions. Never invent prices, convert USDT/USDC, infer historical fee rates from current instrument definitions or claim file intervals prove completeness. This saves configuration only; the Rust runner validates native semantics when run. Existing official Tasks keep prior results. An unresolved execution must be settled before replacing configuration. No live trading.',
  parameters: obj({ request: nativeStrategyRequestSchema }), replay: 'safe', executionMode: 'sequential',
  prepareArguments(args) {
    // Pi normally coerces tool arguments. Check the original JSON before coercion can round a numeric nanosecond.
    validateNumbers(args);
    if (!args || typeof args !== 'object' || !validateRequest.check((args as Record<string, unknown>).request)) throw new Error('native_strategy_request_schema_invalid');
    return args as { request: JsonObject };
  },
  async execute(args, api, context) {
    validateNumbers(args.request);
    if (!validateRequest.check(args.request)) throw new Error('native_strategy_request_schema_invalid');
    const request = structuredClone(args.request);
    await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (!study.input || typeof study.input.data.catalog !== 'string' || !study.input.data.catalog) throw new Error('catalog_required');
      // Run the existing historical-binding check without changing the admitted catalog or executor.
      registeredNativeStrategyRequest({ ...study.input, data: { ...study.input.data, native_strategy_backtest_request: request } });
      const configuration = await tx.doc(ResearchConfiguration, api.conversationId);
      if (configuration.configuredByTaskId === api.taskId) return;
      await assertResearchRunSettled(tx, api.conversationId);
      configuration.request = request;
      configuration.configuredByTaskId = api.taskId;
      study.input.data.native_strategy_backtest_request = request;
      // The previous result remains on its original Pi Task; it is not the result of this configuration.
      study.final = null;
    }, context);
    return { output: [{ type: 'text', text: JSON.stringify({ request, configurationTaskId: api.taskId,
      scope: 'CONFIGURED_NOT_EXECUTED', limitations: ['NATIVE_RUNNER_SEMANTIC_VALIDATION_PENDING', 'NO_IMPLICIT_CURRENCY_CONVERSION', 'NO_TRADING_PERMISSION'] }) }] };
  },
});

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
    const request = input.data.native_strategy_backtest_request === undefined ? null : registeredNativeStrategyRequest(input);
    return { output: [{ type: 'text', text: JSON.stringify({ goal: input.goal, specification: input.specification ?? null,
      request, requestSchema: nativeStrategyRequestSchema, catalog: input.data.catalog ?? null, target: nativeTargetForData(input.data),
      scope: 'REGISTERED_CONFIGURATION_ONLY_NOT_OBSERVED_DATA_COVERAGE',
      limitations: ['BALANCES_AND_FEES_ARE_EXPLICIT_ASSUMPTIONS', 'NO_IMPLICIT_USDT_USDC_CONVERSION', 'NATIVE_STATISTICS_KEEP_THEIR_UPSTREAM_RETURN_BASIS'] }) }] };
  },
});
export default defineExtension({ name: 'qz.native-strategy-backtest', tasks: [NativeStrategyBacktestTask, NativeStrategyCatalogTask], tools: [nativeStrategyBriefTool, nativeStrategyCatalogTool, nativeStrategyConfigureTool, nativeStrategyBacktestTool] });

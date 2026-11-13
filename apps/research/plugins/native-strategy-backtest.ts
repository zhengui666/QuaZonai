import { isNativeTable } from '../src/native-table.ts';
import { nativeResultView, nativeToolText, nativeToolOutputLimits } from '../src/native-result-view.ts';
import type { JsonValue } from '@earendil-works/chord';
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
import { ResearchConfiguration, assertResearchRunSettled, registeredNativeStrategyRequest, registeredNativeResearchRequest } from '../src/research-configuration.ts';
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
export const nativeConfiguredRequestSchema = resolveSchema(schemas.NativeConfiguredBacktestRequestV1) as Schema<JsonObject>;
const validateConfiguredRequest = compile(nativeConfiguredRequestSchema);
export function nativeConfiguredResult(value: Parameters<typeof object>[0], request: JsonObject): JsonObject {
  const result = object(value, 'native_configured_result');
  if (result.schema_version !== 1 || canonical(result.request!) !== canonical(request) || typeof result.native_version !== 'string' || !Array.isArray(result.native_results)
    || (result.canonical_result === null ? typeof result.canonical_unavailable_reason !== 'string' : !result.canonical_result || typeof result.canonical_result !== 'object' || Array.isArray(result.canonical_result))) throw new Error('native_configured_result_invalid');
  return result;
}

const nativeCatalogInspectSchema = resolveSchema(schemas.NativeCatalogInspectRequestV1) as Schema<JsonObject>;
const validateCatalogQuery = compile(nativeCatalogInspectSchema);
export const nativeStrategyCatalogTool = defineTool({
  name: 'native_strategy_catalog',
  description: 'Inspect only the current registered Nautilus catalog through its bound executor: actual instrument definitions and versions, native data selectors and inclusive ts_init file intervals. Use instrument_ids/data_types to filter and page_size to set the projection page size. next_cursor non-null means INCOMPLETE: pass cursor unchanged to read the next required page; do not infer absence from the first page. The same selector may span pages, and this is not a catalog snapshot. No execution, orders or performance. Use exact string envelope timestamps, not numeric instrument JSON timestamps. Coverage is not proof of event completeness, received-time accuracy, historical fees or suitability for a strategy. Do not silently convert currencies or use current definitions as historical evidence. Explicit STATIC_SCENARIO may reference an exact catalog definition timestamp while assuming its rules unchanged over the selected window; it is hypothetical, never historical-specification verification.',
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
  description: 'Configure an official compiled Nautilus strategy on the unchanged registered catalog in this research conversation. First inspect native_strategy_catalog for real instrument definitions, selectors and file coverage. Use the generated request schema verbatim. Explicitly choose source event and availability windows, instrument, native strategy, simulation venue balances, currency, leverage and fees from the owner mandate and registered evidence; ask for missing consequential assumptions. Never invent prices, convert USDT/USDC, infer historical fee rates from current instrument definitions or claim file intervals prove completeness. MARGIN starting balances are collateral rather than a position-notional cap; OBI max_trade_size is per trade, not a cumulative-position limit. leverage=1 alone does not prove unlevered exposure because native margin requirements also depend on instrument.margin_init. This saves configuration only; the Rust runner validates native semantics when run. Existing official Tasks keep prior results. An unresolved execution must be settled before replacing configuration. No live trading.',
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
      delete study.input.data.native_configured_backtest_request;
      // The previous result remains on its original Pi Task; it is not the result of this configuration.
      study.final = null;
    }, context);
    return { output: [{ type: 'text', text: JSON.stringify({ request, configurationTaskId: api.taskId,
      scope: 'CONFIGURED_NOT_EXECUTED', instrumentDefinition: request.instrument_definition ?? { mode: 'HISTORICAL_AS_OF_START' }, limitations: ['NATIVE_RUNNER_SEMANTIC_VALIDATION_PENDING', 'NO_IMPLICIT_CURRENCY_CONVERSION', 'NO_TRADING_PERMISSION'] }) }] };
  },
});

export const nativeConfiguredConfigureTool = defineTool({
  name: 'native_configured_configure',
  description: 'Configure an installed Rust backtest factory name and opaque config on the unchanged catalog/executor. Retain prior executions on their original Tasks and clear only the current displayed result. Declare all intended factory requests to native_strategy_logic_review before execution, then explicitly configure/run each requested case. This does not compile code, create folds, choose winners or validate factory implementation. No live trading.',
  parameters: obj({ request: nativeConfiguredRequestSchema }), replay: 'safe', executionMode: 'sequential',
  prepareArguments(args) {
    // Pi normally coerces tool arguments. Check the original JSON before coercion can round a numeric nanosecond.
    validateNumbers(args);
    if (!args || typeof args !== 'object' || !validateConfiguredRequest.check((args as Record<string, unknown>).request)) throw new Error('native_configured_request_schema_invalid');
    return args as { request: JsonObject };
  },
  async execute(args, api, context) {
    validateNumbers(args.request);
    if (!validateConfiguredRequest.check(args.request)) throw new Error('native_configured_request_schema_invalid');
    const request = structuredClone(args.request);
    await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (!study.input || typeof study.input.data.catalog !== 'string' || !study.input.data.catalog) throw new Error('catalog_required');
      // Run the existing historical-binding check without changing the admitted catalog or executor.
      registeredNativeResearchRequest({ ...study.input, data: { ...study.input.data, native_configured_backtest_request: request } });
      const configuration = await tx.doc(ResearchConfiguration, api.conversationId);
      if (configuration.configuredByTaskId === api.taskId) return;
      await assertResearchRunSettled(tx, api.conversationId);
      configuration.request = request;
      configuration.configuredByTaskId = api.taskId;
      study.input.data.native_configured_backtest_request = request;
      delete study.input.data.native_strategy_backtest_request;
      // The previous result remains on its original Pi Task; it is not the result of this configuration.
      study.final = null;
    }, context);
    return { output: [{ type: 'text', text: JSON.stringify({ request, configurationTaskId: api.taskId,
      scope: 'CONFIGURED_NOT_EXECUTED', instrumentDefinition: { mode: 'FACTORY_CONFIGURATION_NOT_INDEPENDENTLY_VERIFIED' }, limitations: ['NATIVE_RUNNER_SEMANTIC_VALIDATION_PENDING', 'NO_IMPLICIT_CURRENCY_CONVERSION', 'NO_TRADING_PERMISSION'] }) }] };
  },
});

export function normalizedNativeStrategyRequest(request: JsonObject): JsonObject {
  const expected = structuredClone(request);
  if (expected.available_start_ns === null) delete expected.available_start_ns;
  if (expected.instrument_definition === null) delete expected.instrument_definition;
  const venue = object(expected.venue, 'native_strategy_venue');
  if (venue.base_currency === undefined) venue.base_currency = null;
  if (venue.bar_execution === null) delete venue.bar_execution;
  const strategy = object(expected.strategy, 'native_strategy_config');
  if (strategy.base === null) delete strategy.base;
  if (strategy.kind === 'GRID_MARKET_MAKER') {
    if (strategy.trade_size === undefined) strategy.trade_size = null;
    if (strategy.expire_time_secs === undefined) strategy.expire_time_secs = null;
  }
  return expected;
}

/** Validate the exact bound request and upstream report, never substitute a score. */
/** Validate the native evidence envelope only; all returns and statistics are upstream calculations. */
export function nativeEquityPerformance(value: JsonValue | undefined): JsonObject {
  const report = object(value, 'native_equity_performance');
  if (report.source !== 'NAUTILUS_PORTFOLIO_SNAPSHOTS' || !['AVAILABLE', 'UNAVAILABLE'].includes(String(report.status))
    || report.reason !== null && typeof report.reason !== 'string' || report.currency !== null && typeof report.currency !== 'string'
    || !Array.isArray(report.returns) || !Array.isArray(report.statistics) || !Array.isArray(report.notes) || !report.notes.every(note => typeof note === 'string')) throw new Error('native_equity_performance_invalid');
  const counter = (value: unknown) => typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value);
  for (const key of ['snapshot_count', 'observed_utc_days', 'unpriced_snapshot_count', 'return_days']) if (!counter(report[key])) throw new Error('native_equity_count_invalid');
  for (const key of ['first_snapshot_ns', 'last_snapshot_ns']) if (report[key] !== null && (!counter(report[key]) || BigInt(report[key] as string) > 9223372036854775807n)) throw new Error('native_equity_clock_invalid');
  if (report.first_snapshot_ns !== null && report.last_snapshot_ns !== null && BigInt(report.first_snapshot_ns as string) > BigInt(report.last_snapshot_ns as string)) throw new Error('native_equity_clock_invalid');
  if (BigInt(report.return_days as string) !== BigInt(report.returns.length)) throw new Error('native_equity_count_invalid');
  let last: bigint | undefined;
  for (const value of report.returns) { const row = object(value, 'native_equity_return');
    if (!counter(row.ts_event_ns) || BigInt(row.ts_event_ns as string) > 9223372036854775807n || last !== undefined && BigInt(row.ts_event_ns as string) <= last
      || row.value !== null && (typeof row.value !== 'number' || !Number.isFinite(row.value))) throw new Error('native_equity_return_invalid');
    last = BigInt(row.ts_event_ns as string);
  }
  const keys = new Set<string>();
  for (const value of report.statistics) { const row = object(value, 'native_equity_statistic');
    if (![252, 365].includes(Number(row.annualization_days)) || typeof row.annualization_days !== 'number'
      || ![`CAGR (${row.annualization_days} days)`, `Sharpe Ratio (${row.annualization_days} days)`].includes(String(row.native_key))
      || row.value !== null && (typeof row.value !== 'number' || !Number.isFinite(row.value))) throw new Error('native_equity_statistic_invalid');
    const key = `${row.annualization_days}:${row.native_key}`; if (keys.has(key)) throw new Error('native_equity_statistic_duplicate'); keys.add(key);
  }
  if (keys.size !== 4) throw new Error('native_equity_statistics_incomplete');
  if (report.status === 'AVAILABLE' && (!report.returns.length || report.unpriced_snapshot_count !== '0' || report.currency === null || report.reason !== null)) throw new Error('native_equity_evidence_incomplete');
  return report;
}
export function nativeEquityMetric(value: JsonValue | undefined, objective: JsonObject): number | null {
  const report = nativeEquityPerformance(value);
  if (report.status !== 'AVAILABLE' || objective.currency != null && objective.currency !== report.currency) return null;
  const rows = (report.statistics as JsonObject[]).filter(row => row.native_key === objective.native_key && row.annualization_days === objective.annualization_days);
  return rows.length === 1 && typeof rows[0]!.value === 'number' ? rows[0]!.value : null;
}

export function nativeStrategyResult(value: Parameters<typeof object>[0], request: JsonObject): JsonObject {
  const result = object(value, 'native_strategy_result');
  const expected = normalizedNativeStrategyRequest(request);
  if (canonical(result.request ?? null) !== canonical(expected)) throw new Error('native_strategy_request_mismatch');
  if (result.schema_version !== 1 || typeof result.native_version !== 'string' || !executionVersions.has(result.native_version)) throw new Error('native_strategy_version_mismatch');
  const basis = object(result.instrument_definition, 'native_instrument_definition');
  const assumption = request.instrument_definition && typeof request.instrument_definition === 'object' && !Array.isArray(request.instrument_definition) ? request.instrument_definition : { mode: 'HISTORICAL_AS_OF_START' };
  const scenario = assumption.mode === 'STATIC_SCENARIO';
  for (const key of ['ts_init_ns', 'ts_event_ns']) if (typeof basis[key] !== 'string' || !/^(0|[1-9][0-9]*)$/.test(basis[key] as string) || BigInt(basis[key] as string) > 9223372036854775807n) throw new Error('native_instrument_definition_timestamp_invalid');
  if (!['STATIC_SCENARIO', 'HISTORICAL_AS_OF_START'].includes(String(assumption.mode)) || basis.mode !== assumption.mode
    || basis.scope !== (scenario ? 'HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED' : 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START')) throw new Error('native_instrument_definition_scope_mismatch');
  if (scenario ? basis.ts_init_ns !== assumption.ts_init_ns || basis.ts_event_ns !== assumption.ts_event_ns
    : BigInt(basis.ts_init_ns as string) > BigInt(String(request.available_start_ns ?? request.event_start_ns))) throw new Error('native_instrument_definition_binding_mismatch');
  nativeEquityPerformance(result.equity_performance);
  const report = object(result.canonical_result, 'native_strategy_canonical_result');
  const run = object(report.run, 'native_strategy_run');
  if (report.schema !== 'nautilus-backtest-result/v1' || !['completed', 'stopped'].includes(String(run.outcome))) throw new Error('native_strategy_report_invalid');
  for (const key of ['accounts', 'diagnostics', 'fills', 'orders', 'portfolio_snapshots', 'position_snapshots', 'positions']) if (!Array.isArray(report[key]) && !isNativeTable(report[key])) throw new Error('native_strategy_report_invalid');
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
  name: 'native_strategy_backtest', outputLimits: nativeToolOutputLimits,
  description: 'Run the exact historical data.native_strategy_backtest_request on its registered catalog using the compiled upstream Nautilus strategy. EMA_CROSS and GRID_MARKET_MAKER consume QuoteTick; ORDER_BOOK_IMBALANCE consumes L2 order book data. HURST_VPIN_DIRECTIONAL uses official BAR/QuoteTick/TradeTick subscriptions; non-time external bars require explicit venue.bar_execution=false with quotes/trades for execution. Venue fees and balances are explicit simulation assumptions. No inferred data range, custom factor, scoring, optimizer, live deployment or trading permission. This historical result does not imply paper/live deployment of the same strategy. By default retain the historical result and continue the Pi tool loop for requested validation; finishResearch=true explicitly ends an independent one-backtest submission. For a two-lane study, read native_strategy_brief and record logic review with the intended validation method BEFORE this execution.',
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
    return { output: [{ type: 'text', text: nativeToolText(nativeResultView(result, id), id) }], ...(args.finishResearch === true ? { control: { terminate: true } } : {}) };
  },
});

export const nativeConfiguredBacktestTool = defineTool({
  name: 'native_configured_backtest', outputLimits: nativeToolOutputLimits,
  description: 'Run a user-compiled Rust factory through the registered native runner and catalog. The factory constructs official Nautilus BacktestRunConfig, BacktestNode and Strategies, including multiple instruments. name and config are opaque runner-owned inputs, not a QZ strategy DSL. Use only the owner-approved installed factory and explicit assumptions. Returns original official native_results and optional canonical report; no computed equity, selection, grading or live permission. Unknown factory must fail, never fall back to a template.',
  parameters: obj({ request: opt(nativeConfiguredRequestSchema), finishResearch: opt(bool()) }), replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context);
    const request = (args.request ?? input.data.native_configured_backtest_request) as JsonObject;
    if (input.data.native_configured_backtest_request !== undefined && canonical(request) !== canonical(input.data.native_configured_backtest_request)) throw new Error('native_configured_request_requires_configuration');
    validateNumbers(request);
    if (!validateConfiguredRequest.check(request)) throw new Error('native_configured_request_schema_invalid');
    if (typeof input.data.catalog !== 'string' || !input.data.catalog) throw new Error('catalog_required');
    const id = await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.trials.length || study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
      const binding = await tx.doc(NativeStrategyChild, api.taskId);
      if (!binding.taskId) await assertResearchRunSettled(tx, api.conversationId);
      if (!binding.taskId) binding.taskId = await tx.createTask(NativeStrategyBacktestTask, {
        configured: true, request, catalog: input.data.catalog as string, target: nativeTargetForData(input.data),
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
    const result = nativeConfiguredResult(settled.state.outcome.result, request);
    await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
      study.final = { taskId: api.taskId, trialId: null, comparedTrials: [], status: 'COMPUTED',
        reason: 'explicit_native_configured_backtest_completed', scientificClaim: 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION', result };
    }, context);
    return { output: [{ type: 'text', text: nativeToolText(nativeResultView(result, id), id) }], ...(args.finishResearch === true ? { control: { terminate: true } } : {}) };
  },
});

export const NativeStrategyBacktestTask = defineTask<{ request: JsonObject; catalog: string; target: NativeTarget; configured?: boolean; walkForward?: boolean; sourceRequest?: JsonObject; scanContext?: NativeScanContext }, NativeScanCheckpoint, JsonObject>({
  name: 'qz.native-strategy.backtest', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    if (task.input.walkForward) {
      await cancelUnsettledNativeExecutions(task.id, runtime, context);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: 'qz_automatic_walk_forward_removed' } } }), context);
      return;
    }
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-native-strategy-'));
    try {
      const raw = await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(
        task.input.target, task.input.configured ? 'native-configured-backtest' : 'native-strategy-backtest', task.input.request,
        ['--catalog', task.input.catalog], directory, context), task.input.scanContext);
      const result = task.input.configured ? nativeConfiguredResult(raw, task.input.request) : nativeStrategyResult(raw, task.input.request);
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
      request, requestSchema: nativeStrategyRequestSchema, configuredRequest: input.data.native_configured_backtest_request ?? null, configuredRequestSchema: nativeConfiguredRequestSchema, catalog: input.data.catalog ?? null, target: nativeTargetForData(input.data),
      scope: 'REGISTERED_CONFIGURATION_ONLY_NOT_OBSERVED_DATA_COVERAGE',
      limitations: ['BALANCES_AND_FEES_ARE_EXPLICIT_ASSUMPTIONS', 'NO_IMPLICIT_USDT_USDC_CONVERSION', 'NATIVE_STATISTICS_KEEP_THEIR_UPSTREAM_RETURN_BASIS'] }) }] };
  },
});
export default defineExtension({ name: 'qz.native-strategy-backtest', tasks: [NativeStrategyBacktestTask, NativeStrategyCatalogTask], tools: [nativeStrategyBriefTool, nativeStrategyCatalogTool, nativeStrategyConfigureTool, nativeStrategyBacktestTool, nativeConfiguredConfigureTool, nativeConfiguredBacktestTool] });

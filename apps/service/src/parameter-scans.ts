import { NativeStrategyBacktestTask } from '../../research/plugins/native-strategy-backtest.ts';
import { randomUUID } from 'node:crypto';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineDocFamily, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, TaskRecord, Tx, Harness } from '@earendil-works/pi-durable';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { parameterScanPlan, parameterScanStart, normalizeParameterScan, parameterScanPoint, parameterScanAxisValue, parameterScanObjectives, parameterScanObjective, assertDistinctNativeNumbers, ParameterScanError } from '../../../packages/contracts/src/parameter-scans.ts';
import type { ParameterScanSource, ParameterScanMode, ParameterScanStart, ParameterScanPlan, ParameterScanGrid, ParameterValues, ParameterScanMetric, ParameterScanTrial, ParameterScanView, ParameterScanOptions, ParameterScanPreview, ParameterScanSourceView, ParameterScanSurface, ParameterScanMetricId, ParameterScanCounts, NormalizedParameterAxis } from '../../../packages/contracts/src/parameter-scans.ts';
import type { ListPage, Metric } from '../../../packages/contracts/src/index.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { Study, canonical } from '../../research/src/documents.ts';
import { NativeRemoteUncertainty, remoteNativeCapabilities } from '../../research/src/native-execution.ts';
import { reconcileNativeRemoteTasks } from '../../research/src/native-remote-receipts.ts';
import { classifyExecutionError } from '../../research/src/execution-errors.ts';
import { Application, json, object, Problem, nsTime } from './facts.ts';
import { boundScienceTarget } from './science-binding.ts';
import type { ServiceConfig } from './config.ts';
import { readResearchAdmissions } from './admissions.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { eventStream } from './event-stream.ts';
import { nativeCurve } from './projections.ts';

const context = BACKGROUND_CONTEXT;
type ScanMetric = ParameterScanMetric;
type ScanTrial = ParameterScanTrial;
type RetainedSource = { researchId: number; name: string; data: JsonObject; request: JsonObject; target: NativeTarget; defaults: ParameterValues; view: ParameterScanSourceView };
type ScanIndex = { conversationId: number; taskId: number; requestId: string; createdAt: string };
type ScanFact = { original: ParameterScanStart; sourceView: ParameterScanSourceView; grid: ParameterScanGrid; trials: Record<string, ScanTrial>; createdAt: string; updatedAt: string; error: string | null; stoppedAt: string | null; stopTaskId?: number };
type ScanInput = { id: string; original: ParameterScanStart; grid: ParameterScanGrid; source: RetainedSource; implementationDigest?: string };
type Child = { ordinal: string; taskId: number };
type ScanPhase = 'dispatch' | 'collect';
type ScanCheckpoint = { [P in ScanPhase]: { phase: P; nextOrdinal: string; active: Child[] } }[ScanPhase];

/** Business references only. Official Tasks own execution, joins and recovery. */
export const ParameterScans = defineDoc<{ runs: Record<string, JsonObject> }>({
  kind: 'qz.parameter-scans', version: 1, scope: 'session',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127,
  initial: () => ({ runs: {} }),
});
export const ParameterScanRecords = defineDocFamily<{ value: JsonObject | null }, null>({
  kind: 'qz.parameter-scan-record', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true,
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, initial: () => ({ value: null }),
});
function indexValue(value: JsonObject): ScanIndex { return value as unknown as ScanIndex; }
function factValue(value: JsonObject): ScanFact { return value as unknown as ScanFact; }
function inputPlan(value: unknown): ParameterScanPlan {
  try { return parameterScanPlan(value); } catch (error) { if (error instanceof ParameterScanError) throw new Problem(error.code, 422); throw error; }
}
function inputStart(value: unknown): ParameterScanStart {
  try { return parameterScanStart(value); } catch (error) { if (error instanceof ParameterScanError) throw new Problem(error.code, 422); throw error; }
}
function gridFor(plan: ParameterScanPlan, source: RetainedSource): ParameterScanGrid {
  for (const axis of plan.axes) if (source.defaults[axis.key] === undefined) throw new Problem('parameter_scan_numeric_path_required', 422);
  for (const axis of plan.axes) {
    const values = axis.domain.kind === 'values' ? axis.domain.values : [axis.domain.start, axis.domain.end];
    if (['event_start_ns', 'available_start_ns'].includes(axis.key) && values.some(value => compareDecimal(value, source.defaults[axis.key]!) < 0)
      || ['event_end_ns', 'available_end_ns'].includes(axis.key) && values.some(value => compareDecimal(value, source.defaults[axis.key]!) > 0)) throw new Problem('parameter_scan_outside_registered_data_scope', 422);
  }
  try {
    const grid = normalizeParameterScan(plan, source.defaults);
    for (const axis of grid.axes) {
      let value: unknown = source.request;
      for (const part of axis.key.split('.')) value = (value as Record<string, unknown>)[part];
      if (typeof value === 'number') assertDistinctNativeNumbers(axis);
    }
    return grid;
  }
  catch (error) { if (error instanceof ParameterScanError) throw new Problem(error.code, 422); throw error; }
}

function pointAt(grid: ParameterScanGrid, ordinal: string) {
  try { return parameterScanPoint(grid, ordinal); } catch (error) { if (error instanceof ParameterScanError) throw new Problem(error.code, 422); throw error; }
}

function positiveId(value: unknown, name: string): number {
  const parsed = typeof value === 'string' && /^[1-9]\d*$/.test(value) ? Number(value) : value;
  if (typeof parsed !== 'number' || !Number.isSafeInteger(parsed) || parsed < 1) throw new Problem(`${name}_invalid`);
  return parsed;
}
function reference(source: ParameterScanSource) { return { researchId: positiveId(source.researchId, 'research_id') }; }
/** Existing primitive numerical leaves only; no generated trading configuration. */
function numericLeaves(value: JsonObject | readonly unknown[], prefix = ''): ParameterValues {
  const result: ParameterValues = {};
  for (const [key, item] of Object.entries(value)) {
    if (['__proto__', 'prototype', 'constructor', 'schema_version'].includes(key)) continue;
    const path = prefix ? `${prefix}.${key}` : key;
    if (typeof item === 'number' && Number.isFinite(item) || typeof item === 'string' && /^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(item)) result[path] = String(item);
    else if (item && typeof item === 'object') Object.assign(result, numericLeaves(item as JsonObject, path));
  }
  return result;
}
function sourceFromStudy(source: ParameterScanSource, _mode: ParameterScanMode, study: NonNullable<Awaited<ReturnType<ResearchHost['state']>>['research']>, config: ServiceConfig): RetainedSource {
  const ref = reference(source);
  if (!study.input || study.deletedAt) throw new Problem('parameter_scan_source_not_found', 404);
  const data = json(study.input.data);
  // Never translate legacy RSI/MACD or weight-target signals into a different upstream strategy.
  if (study.input.campaign || data.sealed || data.indicator_strategy_request !== undefined || data.indicator_strategy_batch_request !== undefined) throw new Problem('parameter_scan_explicit_native_configuration_required', 422);
  const request = json(object(data.native_strategy_backtest_request, 'parameter_scan_native_request'));
  if (typeof data.catalog !== 'string') throw new Problem('catalog_required', 422);
  const venue = object(request.venue, 'native_strategy_venue'); object(request.strategy, 'native_strategy');
  const target = boundScienceTarget(config, data);
  const name = study.input.specification?.name ?? study.input.goal;
  const view: ParameterScanSourceView = { source, label: name, researchId: ref.researchId, datasetId: null, datasetName: null,
    modality: 'NATIVE', currency: typeof venue.base_currency === 'string' ? venue.base_currency : null,
    interval: { startNs: String(request.event_start_ns), endExclusiveNs: String(request.event_end_ns) }, assumptions: [...(request.instrument_definition && typeof request.instrument_definition === 'object' && !Array.isArray(request.instrument_definition) && request.instrument_definition.mode === 'STATIC_SCENARIO' ? ['HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED'] : ['CATALOG_AS_OF_NOT_INDEPENDENT_SOURCE_AUDIT']), 'UNCHANGED_UPSTREAM_STRATEGY', 'INDEPENDENT_ACCOUNT_PER_CANDIDATE', 'UNQUALIFIED_EXPLORATORY', 'UPSTREAM_RETURN_BASIS_NOT_ASSUMED_DAILY_EQUITY', 'MAXIMIZED_SHORT_SAMPLE_SHARPE_IS_NOT_ESTABLISHED_ANNUAL_RETURN'] };
  return { researchId: ref.researchId, name, data, request, target, defaults: numericLeaves(request), view };
}
function effectivePoint(source: RetainedSource, parameters: ParameterValues) {
  const request = structuredClone(source.request);
  for (const [path, value] of Object.entries(parameters)) {
    if (source.defaults[path] === undefined || value === undefined) throw new Problem('parameter_scan_numeric_path_required', 422);
    const parts = path.split('.'); let node: Record<string, unknown> = request;
    for (const key of parts.slice(0, -1)) {
      const child = node[key];
      if (!child || typeof child !== 'object') throw new Problem('parameter_scan_path_invalid', 422);
      node = child as Record<string, unknown>;
    }
    const key = parts.at(-1)!; const original = node[key];
    if (typeof original === 'number') {
      const number = Number(value);
      if (!Number.isFinite(number)) throw new Problem('parameter_scan_native_number_not_representable', 422);
      node[key] = number;
    } else if (typeof original === 'string') node[key] = value;
    else throw new Problem('parameter_scan_numeric_path_required', 422);
  }
  return request;
}

function unavailable(reason: string, metricId: ParameterScanMetricId): ScanMetric {
  const definition = parameterScanObjective(metricId);
  return { key: metricId, label: definition.label, partition: 'FULL_SELECTION', value: null, status: 'UNAVAILABLE', reason };
}
function metricValue(value: unknown, metricId: ParameterScanMetricId): ScanMetric {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return unavailable('metric_missing', metricId);
  const row = value as Record<string, unknown>;
  if (row.status !== undefined && row.status !== 'AVAILABLE' || row.reason_code || typeof row.value !== 'number' || !Number.isFinite(row.value)) return unavailable(String(row.reason_code ?? row.status ?? 'metric_unavailable'), metricId);
  return { ...unavailable('', metricId), value: row.value, status: 'AVAILABLE', reason: null };
}
function trialMetrics(value: JsonObject): ScanMetric[] {
  const report = object(value.canonical_result, 'native_strategy_report'); const statistics = object(report.statistics, 'native_statistics');
  const rows: ScanMetric[] = [];
  const append = (metrics: JsonObject, group: string, currency?: string) => {
    for (const [key, encoded] of Object.entries(metrics)) {
      // Decode the upstream canonical IEEE754 representation; do not recompute a statistic.
      const decoded = typeof encoded === 'string' && /^[a-fA-F0-9]{16}$/.test(encoded) ? Buffer.from(encoded, 'hex').readDoubleBE() : null;
      rows.push({ ...metricValue({ value: decoded }, key), nativeGroup: group, ...(currency ? { currency } : {}) });
    }
  };
  for (const group of ['general', 'returns']) if (statistics[group]) append(object(statistics[group]), group);
  if (statistics.pnls) for (const [currency, metrics] of Object.entries(object(statistics.pnls))) append(object(metrics), 'pnls', currency);
  return rows;
}
function selectedMetric(metrics: Metric[], metricId: ParameterScanMetricId): ScanMetric {
  const rows = metrics.filter(row => row.key === metricId);
  return rows.length === 1 ? rows[0] as ScanMetric : unavailable(rows.length ? 'ambiguous_native_metric' : 'metric_missing', metricId);
}
function sampleCounts(value: JsonObject): Record<string, string> {
  const report = object(value.canonical_result, 'native_strategy_report');
  return Object.fromEntries(['orders', 'positions', 'fills'].flatMap(key => Array.isArray(report[key]) ? [[key, String(report[key].length)]] : []));
}

async function scanFact(tx: Tx, conversationId: ConversationId, id: string) {
  const record = await tx.doc(ParameterScanRecords, conversationId, id, null);
  if (!record.value) throw new Problem('parameter_scan_not_found', 404);
  return factValue(record.value);
}
function resultTrial(trial: ScanTrial, child: TaskRecord<unknown, unknown, unknown>, objective: ParameterScanMetricId): ScanTrial {
  if (child.state.status !== 'terminal') return trial;
  const outcome = child.state.outcome;
  if (outcome.status === 'completed') {
    const value = object(outcome.result, 'parameter_scan_result');
    const summary = value;
    const metrics = trialMetrics(summary);
    const report = object(value.canonical_result, 'native_strategy_report');
    const nativeOutcome = object(report.run, 'native_strategy_run').outcome;
    const complete = nativeOutcome === 'completed';
    return { ...trial, status: complete ? 'COMPLETED' : 'STOPPED', metrics,
      metric: complete ? selectedMetric(metrics, objective) : unavailable('native_run_stopped_before_completion', objective),
      sampleCounts: sampleCounts(summary), details: { ...(trial.details ?? {}), instrumentDefinition: value.instrument_definition ?? null,
        nativeRun: report.run!, notes: value.notes ?? null, dataRowsByType: value.data_rows_by_type ?? null },
      error: complete ? null : 'native_run_stopped_before_completion' };
  }
  const detail = 'error' in outcome ? classifyExecutionError(outcome.error?.message, 'scientific_execution_failed', outcome.error?.detail && typeof outcome.error.detail === 'object' && !Array.isArray(outcome.error.detail) ? outcome.error.detail : undefined) : null;
  return { ...trial, status: outcome.status === 'aborted' ? 'STOPPED' : 'FAILED', error: detail?.code ?? (outcome.status === 'aborted' ? null : 'parameter_scan_trial_failed'), ...(detail ? { details: json({ error: detail }) } : {}) };
}

export const ParameterScanTask = defineTask<ScanInput, ScanCheckpoint, null>({
  name: 'qz.parameter-scan', version: 1,
  initial: () => ({ phase: 'dispatch', nextOrdinal: '0', active: [] }),
  phases: {
    async dispatch(task, runtime, ctx) {
      await runtime.commit(async tx => {
        const fact = await scanFact(tx, runtime.conversationId, task.input.id);
        if (fact.stoppedAt) return { status: 'terminal', outcome: { status: 'aborted' } };
        const active: Child[] = []; let next = BigInt(task.state.checkpoint.nextOrdinal); let children = 0;
        while (next < BigInt(task.input.grid.total) && children++ < task.input.original.execution.concurrency) {
          const point = parameterScanPoint(task.input.grid, String(next)); const request = effectivePoint(task.input.source, point.parameters); next++;
          const scanContext = { scanId: task.input.id, ordinal: point.ordinal, ...(task.input.implementationDigest ? { implementationDigest: task.input.implementationDigest } : {}) };
          const child = await tx.createTask(NativeStrategyBacktestTask, { request,
            catalog: String(task.input.source.data.catalog), target: task.input.source.target, scanContext }, { ownership: { kind: 'task', taskId: task.id } });
          active.push({ ordinal: point.ordinal, taskId: child });
          fact.trials[point.ordinal] = { ...point, taskId: child, status: 'PENDING', metric: null, metrics: [], sampleCounts: null, error: null, remoteStatus: null };
        }
        fact.updatedAt = new Date(runtime.now()).toISOString();
        const checkpoint: ScanCheckpoint = { ...task.state.checkpoint, nextOrdinal: String(next), active, phase: active.length ? 'collect' : 'dispatch' };
        if (active.length) return { status: 'waiting', checkpoint, on: [...new Set(active.map(child => child.taskId as TaskId))], policy: 'allSettled' };
        if (next < BigInt(task.input.grid.total)) return { status: 'running', checkpoint };
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, ctx);
    },
    async collect(task, runtime, ctx) {
      const children = await Promise.all(task.state.checkpoint.active.map(child => runtime.getTask(child.taskId as TaskId<JsonObject>, ctx)));
      await runtime.commit(async tx => {
        const fact = await scanFact(tx, runtime.conversationId, task.input.id);
        for (const [index, child] of children.entries()) {
          const ref = task.state.checkpoint.active[index]!;
          if (!child) throw new Error('parameter_scan_child_missing');
          fact.trials[ref.ordinal] = resultTrial(fact.trials[ref.ordinal]!, child, task.input.original.objective.metricId);
        }
        fact.updatedAt = new Date(runtime.now()).toISOString();
        if (children.some(child => child?.state.status === 'terminal' && 'error' in child.state.outcome && child.state.outcome.error?.message === 'nautilus_scan_implementation_changed')) {
          fact.error = 'nautilus_scan_implementation_changed';
          return { status: 'terminal', outcome: { status: 'failed', error: { message: fact.error } } };
        }
        return { status: 'running', checkpoint: { ...task.state.checkpoint, phase: 'dispatch', active: [] } };
      }, ctx);
    },
  },
  async abort(task, runtime, ctx) {
    await runtime.commit(async tx => {
      const fact = await scanFact(tx, runtime.conversationId, task.input.id);
      for (const ref of task.state.checkpoint.active) {
        const child = await tx.task(ref.taskId as TaskId<JsonObject>);
        if (child) fact.trials[ref.ordinal] = resultTrial(fact.trials[ref.ordinal]!, child, task.input.original.objective.metricId);
      }
      fact.stoppedAt = new Date(runtime.now()).toISOString(); fact.updatedAt = fact.stoppedAt;
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, ctx);
  },
});
/** Closing a local Task does not prove that its original remote calls stopped. */
export function createParameterScanExtension(harness: () => Harness) {
  const StopTask = defineTask<{ id: string; parentTaskId: number }, { phase: 'stop' } | { phase: 'reconcile' }, null>({
    name: 'qz.parameter-scan.stop', version: 1, initial: () => ({ phase: 'stop' }),
    phases: {
      async stop(task, runtime, ctx) {
        await harness().abortTask(task.input.parentTaskId as TaskId, ctx);
        await runtime.commit(() => ({ status: 'waiting', checkpoint: { phase: 'reconcile' }, on: [task.input.parentTaskId as TaskId], policy: 'allSettled' }), ctx);
      },
      async reconcile(task, runtime, ctx) {
        const factJson = (await runtime.snapshot(ParameterScanRecords, runtime.conversationId, task.input.id, ctx))?.value;
        if (!factJson) throw new Error('parameter_scan_not_found');
        const fact = factValue(factJson);
        const ids = [...new Set(Object.values(fact.trials).flatMap(row => row.taskId === null ? [] : [row.taskId]))];
        await reconcileNativeRemoteTasks(harness(), ids.map(id => ({ id, conversationId: runtime.conversationId })), ctx);
        await runtime.commit(async tx => {
          const current = await scanFact(tx, runtime.conversationId, task.input.id);
          let unresolved = false;
          for (const id of ids) {
            const remote = await tx.doc(NativeRemoteUncertainty, runtime.conversationId, String(id), null);
            if (remote.requests.some(row => !['COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED'].includes(String(row.status)))) unresolved = true;
          }
          current.updatedAt = new Date(runtime.now()).toISOString();
          return unresolved ? { status: 'sleeping', until: runtime.now() + 1000, checkpoint: { phase: 'reconcile' } } : { status: 'terminal', outcome: { status: 'completed', result: null } };
        }, ctx);
      },
    },
    abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
  });
  const scientificTasks: (typeof NativeStrategyBacktestTask)[] = [];
  return { stopTask: StopTask, scientificTasks, extension: defineExtension({ name: 'qz.parameter-scans', tasks: [ParameterScanTask, StopTask] }) };
}

export function parameterScanWorkbench(host: ResearchHost, config: ServiceConfig, stopTask: ReturnType<typeof createParameterScanExtension>['stopTask']) {
  async function source(input: ParameterScanSource, mode: ParameterScanMode) {
    const ref = reference(input); const state = await host.state(ref.researchId);
    if (!state.research) throw new Problem('parameter_scan_source_not_found', 404);
    const retained = sourceFromStudy(input, mode, state.research, config);
    const admissions = await readResearchAdmissions(host);
    const original = state.originalSubmission?.requestId ? admissions[`request:${state.originalSubmission.requestId}`] : null;
    const dataset = original?.dataset;
    retained.view.datasetId = typeof dataset?.id === 'string' ? dataset.id : null;
    retained.view.datasetName = typeof dataset?.name === 'string' ? dataset.name : null;
    if (typeof dataset?.currency === 'string') retained.view.currency = dataset.currency;
    return retained;
  }
  async function options(input?: ParameterScanSource, mode?: ParameterScanMode): Promise<ParameterScanOptions> {
    if (!input) return { source: null, modes: ['STRATEGY'], parameters: [], objectives: [...parameterScanObjectives], defaults: {}, reason: 'select_configured_native_research' };
    const retained = await source(input, mode ?? 'STRATEGY');
    return { source: retained.view, modes: ['STRATEGY'], defaults: retained.defaults,
      parameters: Object.keys(retained.defaults).map(key => ({ key, label: key, kind: 'decimal' as const, modes: ['STRATEGY' as const] })),
      objectives: [...parameterScanObjectives], reason: null };
  }
  async function preview(value: unknown): Promise<ParameterScanPreview> {
    const input = inputPlan(value); const retained = await source(input.source, input.mode);
    const grid = gridFor(input, retained);
    const examples = Array.from({ length: Number(BigInt(grid.total) < 5n ? BigInt(grid.total) : 5n) }, (_, index) => parameterScanPoint(grid, String(index)));
    return { input, source: retained.view, grid, objective: { ...parameterScanObjective(input.objective.metricId), direction: input.objective.direction }, examples,
      warnings: ['UNQUALIFIED_EXPLORATORY', 'EXPLORATORY_BEST_IS_NOT_INDEPENDENT_OOS_EVIDENCE'] };
  }
  async function read(id: string) {
    const indexJson = (await host.harness.snapshot(ParameterScans, context))?.runs[id];
    if (!indexJson) throw new Problem('parameter_scan_not_found', 404);
    const index = indexValue(indexJson);
    const factJson = (await host.harness.snapshot(ParameterScanRecords, index.conversationId as ConversationId, id, context))?.value;
    if (!factJson) throw new Problem('parameter_scan_not_found', 404);
    const fact = factValue(factJson);
    const task = await host.harness.getTask(index.taskId as TaskId<null>, context);
    if (!task || task.kind !== ParameterScanTask.definition.name) throw new Problem('parameter_scan_task_missing', 409);
    return { index, fact, task };
  }
  async function projectedTrials(id: string) {
    const original = await read(id);
    const references = [...new Set(Object.values(original.fact.trials).flatMap(row => row.taskId === null ? [] : [row.taskId]))].map(id => ({ id, conversationId: original.index.conversationId }));
    await reconcileNativeRemoteTasks(host.harness, references, context);
    const trials = await Promise.all(Object.values(original.fact.trials).map(async stored => {
      // Collected terminal summaries are immutable. Full task results are read
      // again only by the explicit per-point detail/curve endpoint.
      const child = stored.taskId === null || ['COMPLETED', 'FAILED', 'STOPPED'].includes(stored.status) ? undefined : await host.harness.getTask(stored.taskId as TaskId<JsonObject>, context);
      const row = child ? resultTrial(stored, child, original.fact.original.objective.metricId) : stored;
      const remote = stored.taskId === null ? null : await host.harness.snapshot(NativeRemoteUncertainty, original.index.conversationId as ConversationId, String(stored.taskId), context);
      const statuses = remote?.requests.map(item => String(item.status)) ?? [];
      const remoteStatus = statuses.includes('UNKNOWN') ? 'UNKNOWN' : statuses.includes('RUNNING') ? 'RUNNING' : statuses.includes('PENDING') ? 'PENDING' : statuses.includes('CANCELLED') ? 'CANCELLED' : statuses.includes('FAILED') ? 'FAILED' : statuses.length ? 'COMPLETED' : null;
      const state = child?.state;
      const status: ScanTrial['status'] = state && state.status !== 'terminal' ? remoteStatus === 'UNKNOWN' ? 'RECONCILING' : state.status === 'pending' ? 'PENDING' : 'RUNNING' : row.status;
      return { ...row, status, remoteStatus };
    }));
    return { ...original, trials };
  }
  async function get(id: string): Promise<ParameterScanView> {
    const { index, fact, task, trials } = await projectedTrials(id);
    const outcome = task.state.status === 'terminal' ? task.state.outcome : null;
    const counts: ParameterScanCounts = { total: fact.grid.total, eligible: fact.grid.eligible, skipped: fact.grid.skipped, completed: '0', failed: '0', stopped: '0', running: '0', pending: '0', unknown: '0', comparable: '0' };
    for (const row of trials) {
      const key = row.status === 'COMPLETED' ? 'completed' : row.status === 'FAILED' ? 'failed' : row.status === 'STOPPED' ? 'stopped' : row.status === 'RECONCILING' ? 'unknown' : row.status === 'PENDING' ? 'pending' : 'running';
      counts[key] = String(BigInt(counts[key]!) + 1n);
    }
    const unstarted = BigInt(fact.grid.eligible) - BigInt(trials.length);
    if (outcome || fact.stoppedAt || task.abortRequested) counts.stopped = String(BigInt(counts.stopped) + unstarted);
    else counts.pending = String(unstarted + BigInt(counts.pending));
    const scored = trials.filter(row => row.status === 'COMPLETED' && row.metric?.status === 'AVAILABLE' && row.metric.value !== null).sort((a, b) => (fact.original.objective.direction === 'maximize' ? b.metric!.value! - a.metric!.value! : a.metric!.value! - b.metric!.value!) || (BigInt(a.ordinal) < BigInt(b.ordinal) ? -1 : 1));
    counts.comparable = String(scored.length);
    const completed = outcome?.status === 'completed';
    const unsettled = trials.some(row => row.remoteStatus !== null && !['COMPLETED', 'CANCELLED', 'FAILED', 'REJECTED'].includes(row.remoteStatus));
    // Remote uncertainty is an overlapping observation count: a locally stopped
    // point can still have an original call awaiting a terminal receipt.
    counts.unknown = String(trials.filter(row => row.status === 'RECONCILING' || (fact.stoppedAt || task.abortRequested || outcome?.status === 'aborted')
      && row.remoteStatus !== null && !['COMPLETED', 'CANCELLED', 'FAILED', 'REJECTED'].includes(row.remoteStatus)).length);
    const status: ParameterScanView['status'] = outcome?.status === 'aborted' ? unsettled ? 'STOPPING' : 'STOPPED' : outcome ? completed ? 'COMPLETED' : 'FAILED' : task.abortRequested || fact.stoppedAt ? 'STOPPING' : counts.unknown !== '0' ? 'RECONCILING' : task.state.status === 'pending' ? 'QUEUED' : 'RUNNING';
    return { id, requestId: fact.original.requestId, name: fact.original.name, source: fact.original.source, sourceView: fact.sourceView, input: inputPlan(fact.original), mode: fact.original.mode, taskId: index.taskId,
      createdAt: fact.createdAt, updatedAt: fact.updatedAt, status, execution: fact.original.execution, objective: fact.original.objective,
      grid: fact.grid, counts, best: scored.length ? { ordinal: scored[0]!.ordinal, parameters: scored[0]!.parameters, metric: scored[0]!.metric! } : null,
      error: fact.error ?? (outcome && !['completed', 'aborted'].includes(outcome.status) ? 'parameter_scan_failed' : null), scientificQualification: 'UNQUALIFIED_EXPLORATORY' };
  }
  async function start(value: unknown): Promise<ParameterScanView> {
    const input = inputStart(value);
    const old = await host.harness.commit(async tx => {
      const index = Object.entries((await tx.doc(ParameterScans)).runs).find(([, row]) => row.requestId === input.requestId);
      if (!index) return null;
      const fact = await scanFact(tx, indexValue(index[1]).conversationId as ConversationId, index[0]);
      if (canonical(json(fact.original)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409);
      return index[0];
    }, context);
    if (old) return get(old);
    const retained = await source(input.source, input.mode);
    const grid = gridFor(input, retained);
    if (grid.eligible === '0') throw new Problem('parameter_scan_no_eligible_points', 422);
    let implementationDigest: string | undefined;
    if (typeof retained.target !== 'string') {
      const capabilities = await remoteNativeCapabilities(retained.target);
      const required = ['native-strategy-backtest'];
      if (required.some(command => !capabilities.commands.includes(command))) throw new Problem('nautilus_command_unsupported', 422);
      implementationDigest = capabilities.implementationDigest;
    }
    const id = await host.harness.commit(async tx => {
      const index = await tx.doc(ParameterScans);
      const existing = Object.entries(index.runs).find(([, row]) => row.requestId === input.requestId);
      if (existing) {
        if (canonical(json((await scanFact(tx, indexValue(existing[1]).conversationId as ConversationId, existing[0])).original)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409);
        return existing[0];
      }
      assertBusinessAdmission(await tx.doc(Application));
      const current = sourceFromStudy(input.source, input.mode, await tx.doc(Study, retained.researchId as ConversationId), config);
      if (canonical(current.request) !== canonical(retained.request)) throw new Problem('parameter_scan_configuration_changed', 409);
      const id = randomUUID(); const createdAt = new Date().toISOString();
      const taskId = await tx.createTask(ParameterScanTask, { id, original: input, grid, source: retained, ...(implementationDigest ? { implementationDigest } : {}) },
        { conversationId: retained.researchId as ConversationId, ownership: { kind: 'conversation' } });
      index.runs[id] = json({ taskId, conversationId: retained.researchId, requestId: input.requestId, createdAt });
      (await tx.doc(ParameterScanRecords, retained.researchId as ConversationId, id, null)).value = json({ original: input, sourceView: retained.view, grid, createdAt, updatedAt: createdAt, trials: {}, error: null, stoppedAt: null });
      return id;
    }, context);
    return get(id);
  }
  async function stop(id: string): Promise<ParameterScanView> {
    const { index } = await read(id);
    await host.harness.commit(async tx => {
      const fact = await scanFact(tx, index.conversationId as ConversationId, id);
      const parent = await tx.task(index.taskId as TaskId);
      if (parent?.state.status === 'terminal' && parent.state.outcome.status === 'completed') return;
      fact.stoppedAt ??= new Date().toISOString(); fact.updatedAt = fact.stoppedAt;
      fact.stopTaskId ??= await tx.createTask(stopTask, { id, parentTaskId: index.taskId }, { conversationId: index.conversationId as ConversationId, ownership: { kind: 'conversation' } });
    }, context);
    return get(id);
  }
  async function list(query: Record<string, unknown> = {}): Promise<ListPage<ParameterScanView>> {
    const index = (await host.harness.snapshot(ParameterScans, context))?.runs ?? {};
    let rows = await Promise.all(Object.keys(index).map(get));
    const q = typeof query.q === 'string' ? query.q.normalize('NFKC').toLowerCase().trim() : '';
    const facet = (values: (string | null)[]) => [...new Set(values.filter((value): value is string => value !== null))].sort().map(value => ({ value, label: value }));
    const facets = { status: facet(rows.map(row => row.status)), mode: facet(rows.map(row => row.mode)), datasetId: facet(rows.map(row => row.sourceView.datasetId)) };
    if (q) rows = rows.filter(row => `${row.name} ${row.id} ${row.sourceView.label} ${row.sourceView.datasetName ?? ''}`.normalize('NFKC').toLowerCase().includes(q));
    for (const field of ['status', 'mode', 'datasetId'] as const) if (typeof query[field] === 'string' && query[field]) {
      const selected = new Set(query[field].split(','));
      rows = rows.filter(row => selected.has(field === 'datasetId' ? row.sourceView.datasetId ?? 'unknown' : row[field]));
    }
    const sort = query.sort ?? 'createdAt'; const order = query.order ?? 'desc';
    if (!['createdAt', 'name'].includes(String(sort)) || !['asc', 'desc'].includes(String(order))) throw new Problem('parameter_scan_sort_invalid');
    rows.sort((a, b) => (sort === 'name' ? a.name.localeCompare(b.name) : a.createdAt.localeCompare(b.createdAt)) * (order === 'asc' ? 1 : -1) || a.id.localeCompare(b.id));
    return page(rows, query, facets);
  }
  async function trials(id: string, query: Record<string, unknown> = {}): Promise<ListPage<ParameterScanTrial>> {
    const projected = await projectedTrials(id);
    let rows = projected.trials;
    if (typeof query.status === 'string') rows = rows.filter(row => row.status === query.status);
    if (query.sort !== undefined && !['metric', 'ordinal'].includes(String(query.sort))) throw new Problem('parameter_scan_sort_invalid');
    if (query.sort === 'metric') rows.sort((a, b) => {
      const x = a.metric?.status === 'AVAILABLE' ? a.metric.value : null; const y = b.metric?.status === 'AVAILABLE' ? b.metric.value : null;
      return x === null ? y === null ? 0 : 1 : y === null ? -1 : projected.fact.original.objective.direction === 'minimize' ? x - y : y - x;
    });
    else rows.sort((a, b) => BigInt(a.ordinal) < BigInt(b.ordinal) ? -1 : 1);
    return page(rows, query, { status: [...new Set(projected.trials.map(row => row.status))].map(value => ({ value, label: value })) });
  }
  async function trial(id: string, ordinal: string): Promise<ParameterScanTrial> {
    const { fact, task, trials } = await projectedTrials(id);
    const point = pointAt(fact.grid, ordinal);
    const row: ParameterScanTrial = trials.find(row => row.ordinal === ordinal) ?? { ...point, taskId: null, status: point.skipReason ? 'SKIPPED' : task.state.status === 'terminal' ? 'STOPPED' : 'PENDING', metric: null, metrics: [], sampleCounts: null, error: point.skipReason, remoteStatus: null };
    let native: JsonObject | null = null;
    if (row.taskId !== null) {
      const child = await host.harness.getTask(row.taskId as TaskId<JsonObject>, context);
      if (child?.state.status === 'terminal' && child.state.outcome.status === 'completed') {
        const value = object(child.state.outcome.result, 'parameter_scan_result');
        native = value;
      }
    }
    const input = task.input as unknown as ScanInput;
    const effective = effectivePoint(input.source, point.parameters);
    const details = json({ ...row.details, effectiveRequest: effective, ...(row.taskId === null ? {} : { evidenceTaskId: row.taskId }), ...(native ? { nativeResult: native } : {}) });
    const currency = object(effective.venue).base_currency;
    if (native?.canonical_result && typeof currency === 'string') {
      try { return { ...row, curve: nativeCurve(native, currency), details }; }
      catch (error) { if (!(error instanceof Problem)) throw error; details.curveReason = error.message; }
    }
    return { ...row, details };
  }
  async function surface(id: string, query: Record<string, unknown>): Promise<ParameterScanSurface> {
    const { fact, task, trials } = await projectedTrials(id);
    if (typeof query.x !== 'string' || typeof query.y !== 'string' || query.x === query.y) throw new Problem('parameter_scan_surface_axes_invalid');
    const x = fact.grid.axes.find(axis => axis.key === query.x); const y = fact.grid.axes.find(axis => axis.key === query.y);
    if (!x || !y) throw new Problem('parameter_scan_surface_axes_invalid');
    let fixed: ParameterValues = {};
    if (query.fixed !== undefined) {
      if (typeof query.fixed !== 'string') throw new Problem('parameter_scan_surface_fixed_invalid');
      try { fixed = JSON.parse(query.fixed); } catch { throw new Problem('parameter_scan_surface_fixed_invalid'); }
    }
    if (!fixed || typeof fixed !== 'object' || Array.isArray(fixed) || Object.values(fixed).some(value => typeof value !== 'string')) throw new Problem('parameter_scan_surface_fixed_invalid');
    const remaining = fact.grid.axes.filter(axis => axis.key !== x.key && axis.key !== y.key);
    if (remaining.some(axis => fixed[axis.key] === undefined) || Object.keys(fixed).some(key => !remaining.some(axis => axis.key === key))) throw new Problem('parameter_scan_surface_fixed_required');
    const metricId = query.metricId ?? fact.original.objective.metricId;
    if (typeof metricId !== 'string' || !metricId.trim()) throw new Problem('parameter_scan_metric_invalid');
    const chosenMetric = metricId as ParameterScanMetricId;
    const limit = query.limit === undefined ? 50 : positiveId(query.limit, 'parameter_scan_surface_limit');
    if (limit > 100) throw new Problem('parameter_scan_surface_limit_invalid');
    const offset = (value: unknown, maximum: string) => {
      if (value === undefined) return 0n;
      if (typeof value !== 'string' || !/^(0|[1-9]\d*)$/.test(value) || BigInt(value) >= BigInt(maximum)) throw new Problem('parameter_scan_surface_offset_invalid');
      return BigInt(value);
    };
    const xOffset = offset(query.xOffset, x.count); const yOffset = offset(query.yOffset, y.count);
    const indices = new Map<string, bigint>();
    for (const axis of remaining) indices.set(axis.key, axisIndex(axis, fixed[axis.key]!));
    const xCount = Number(BigInt(x.count) - xOffset < BigInt(limit) ? BigInt(x.count) - xOffset : BigInt(limit));
    const yCount = Number(BigInt(y.count) - yOffset < BigInt(limit) ? BigInt(y.count) - yOffset : BigInt(limit));
    const xs = Array.from({ length: xCount }, (_, i) => parameterScanAxisValue(x, xOffset + BigInt(i)));
    const ys = Array.from({ length: yCount }, (_, i) => parameterScanAxisValue(y, yOffset + BigInt(i)));
    const byOrdinal = new Map(trials.map(row => [row.ordinal, row]));
    const cells: ParameterScanSurface['cells'] = [];
    for (const [xi, xv] of xs.entries()) for (const [yi, yv] of ys.entries()) {
      indices.set(x.key, xOffset + BigInt(xi)); indices.set(y.key, yOffset + BigInt(yi));
      const ordinal = fact.grid.axes.reduce((result, axis) => result * BigInt(axis.count) + indices.get(axis.key)!, 0n).toString();
      const point = parameterScanPoint(fact.grid, ordinal); const trial = byOrdinal.get(ordinal);
      cells.push({ ordinal, x: xv, y: yv, status: point.skipReason ? 'SKIPPED' : trial?.status ?? (task.state.status === 'terminal' ? 'STOPPED' : 'PENDING'),
        metric: trial ? trial.status === 'COMPLETED' ? selectedMetric(trial.metrics, chosenMetric) : unavailable(trial.error ?? 'native_run_incomplete', chosenMetric) : null, reason: point.skipReason ?? trial?.error ?? null });
    }
    return { x: { key: x.key, values: xs, offset: xOffset.toString(), total: x.count }, y: { key: y.key, values: ys, offset: yOffset.toString(), total: y.count }, fixed, metricId: chosenMetric, cells };
  }
  return { options, preview, start, get, stop, list, trials, trial, surface };
}

function page<T>(rows: T[], query: Record<string, unknown>, facets: ListPage<T>['facets']): ListPage<T> {
  const page = query.page === undefined ? 1 : positiveId(query.page, 'parameter_scan_page');
  const pageSize = query.pageSize === undefined ? 25 : positiveId(query.pageSize, 'parameter_scan_page_size');
  if (pageSize > 200 || !Number.isSafeInteger((page - 1) * pageSize)) throw new Problem('parameter_scan_page_invalid');
  return { items: rows.slice((page - 1) * pageSize, page * pageSize), total: rows.length, page, pageSize, facets };
}
function axisIndex(axis: NormalizedParameterAxis, value: string): bigint {
  try { compareDecimal(value, '0'); } catch { throw new Problem('parameter_scan_surface_fixed_invalid'); }
  if (axis.domain.kind === 'values') {
    const index = axis.domain.values.findIndex(item => compareDecimal(item, value) === 0);
    if (index < 0) throw new Problem('parameter_scan_surface_fixed_invalid');
    return BigInt(index);
  }
  let lo = 0n, hi = BigInt(axis.count);
  while (lo < hi) {
    const middle = (lo + hi) / 2n;
    if (compareDecimal(parameterScanAxisValue(axis, middle), value) < 0) lo = middle + 1n; else hi = middle;
  }
  if (lo >= BigInt(axis.count) || compareDecimal(parameterScanAxisValue(axis, lo), value) !== 0) throw new Problem('parameter_scan_surface_fixed_invalid');
  return lo;
}
function optionsSource(query: Record<string, unknown>): ParameterScanSource | undefined {
  if (query.sourceKind === undefined && query.researchId === undefined) return undefined;
  if (query.sourceKind === 'research') return { kind: 'research', researchId: positiveId(query.researchId, 'research_id') };
  throw new Problem('parameter_scan_source_invalid');
}
export function registerParameterScanRoutes(app: FastifyInstance, host: ResearchHost, work: ReturnType<typeof parameterScanWorkbench>, streams: Set<() => Promise<void>>) {
  const root = `${API}/parameter-scans`;
  app.get<{ Querystring: Record<string, unknown> }>(`${root}/options`, request => {
    const mode = request.query.mode;
    if (mode !== undefined && mode !== 'STRATEGY') throw new Problem('parameter_scan_mode_invalid');
    return work.options(optionsSource(request.query), mode);
  });
  app.post(`${root}/preview`, request => work.preview(request.body));
  app.post(root, async (request, reply) => reply.code(202).send(await work.start(request.body)));
  app.get<{ Querystring: Record<string, unknown> }>(root, request => work.list(request.query));
  app.get<{ Querystring: Record<string, unknown> }>(`${root}/list`, request => work.list(request.query));
  app.get<{ Params: { id: string } }>(`${root}/:id`, request => work.get(request.params.id));
  app.get<{ Params: { id: string }; Querystring: Record<string, unknown> }>(`${root}/:id/trials`, request => work.trials(request.params.id, request.query));
  app.get<{ Params: { id: string; ordinal: string } }>(`${root}/:id/trials/:ordinal`, request => work.trial(request.params.id, request.params.ordinal));
  app.get<{ Params: { id: string }; Querystring: Record<string, unknown> }>(`${root}/:id/surface`, request => work.surface(request.params.id, request.query));
  app.post<{ Params: { id: string } }>(`${root}/:id/stop`, async (request, reply) => reply.code(202).send(await work.stop(request.params.id)));
  app.get<{ Params: { id: string } }>(`${root}/:id/events`, async (request, reply) => {
    const view = await work.get(request.params.id);
    const [document, graph] = await Promise.all([host.harness.watchDoc(ParameterScanRecords, view.sourceView.researchId as ConversationId, request.params.id, context), host.harness.watchTaskGraph(context)]);
    if (!document) { await graph.stop(); throw new Problem('parameter_scan_not_found', 404); }
    await eventStream(reply, 'parameter-scan', () => work.get(request.params.id), [document, graph], streams);
  });
}

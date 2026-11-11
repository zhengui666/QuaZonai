import { randomUUID } from 'node:crypto';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineDocFamily, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, TaskRecord, Tx, Harness } from '@earendil-works/pi-durable';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import { nativeResearchInput, withNativeResearchRequest } from '../../../packages/contracts/src/tick-input.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { parameterScanPlan, parameterScanStart, normalizeParameterScan, parameterScanPoint, parameterScanAxisValue, parameterScanParameters, parameterScanObjectives, ParameterScanError } from '../../../packages/contracts/src/parameter-scans.ts';
import type { ParameterScanSource, ParameterScanMode, ParameterScanStart, ParameterScanPlan, ParameterScanGrid, ParameterValues, ParameterScanMetric, ParameterScanTrial, ParameterScanView, ParameterScanOptions, ParameterScanPreview, ParameterScanSourceView, ParameterScanSurface, ParameterScanMetricId, ParameterScanCounts, NormalizedParameterAxis } from '../../../packages/contracts/src/parameter-scans.ts';
import type { ListPage, Metric } from '../../../packages/contracts/src/index.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { Study, canonical } from '../../research/src/documents.ts';
import { FactorTask } from '../../research/plugins/native-factor-task.ts';
import { CatalogScopeTask, CatalogScopes } from '../../research/plugins/native-catalog-scope.ts';
import { PortfolioEvaluationTask } from '../../research/plugins/native-portfolio-task.ts';
import { TickEconomicTask } from '../../research/plugins/tick-economic.ts';
import { tickEconomicPolicy } from '../../research/src/input.ts';
import { validateNativeTickEconomicRequest } from '../../../packages/contracts/src/economic-tick.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { resolveStrategyPolicy, strategyMembers, thresholdPolicy, validateNativeSimulationSettings } from '../../research/src/strategy.ts';
import type { StrategyMember, StrategyPolicy } from '../../research/src/strategy.ts';
import { NativeRemoteUncertainty, remoteNativeCapabilities } from '../../research/src/native-execution.ts';
import { reconcileNativeRemoteTasks } from '../../research/src/native-remote-receipts.ts';
import { readNativeEvidence } from '../../research/plugins/native-evidence.ts';
import { classifyExecutionError } from '../../research/src/execution-errors.ts';
import { Application, json, object, Problem, nsTime } from './facts.ts';
import { boundScienceTarget } from './science-binding.ts';
import type { ServiceConfig } from './config.ts';
import { factorReference } from './factors.ts';
import { readResearchAdmissions } from './admissions.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { eventStream } from './event-stream.ts';
import { nativeCurve } from './projections.ts';

const context = BACKGROUND_CONTEXT;
type ScanMetric = ParameterScanMetric;
type ScanTrial = ParameterScanTrial;
type RetainedSource = { researchId: number; trialId: number; memberIndex: number | null; name: string; data: JsonObject; members: StrategyMember[]; policy: StrategyPolicy | null; target: NativeTarget; defaults: ParameterValues; tickEconomic?: ReturnType<typeof tickEconomicPolicy>; view: ParameterScanSourceView };
type ScanIndex = { conversationId: number; taskId: number; requestId: string; createdAt: string };
type ScanFact = { original: ParameterScanStart; sourceView: ParameterScanSourceView; grid: ParameterScanGrid; trials: Record<string, ScanTrial>; createdAt: string; updatedAt: string; error: string | null; stoppedAt: string | null; stopTaskId?: number; scopeTaskId?: number };
type ScanInput = { id: string; original: ParameterScanStart; grid: ParameterScanGrid; source: RetainedSource; implementationDigest?: string };
type Child = { ordinal: string; taskId: number };
type ScanPhase = 'prepare' | 'scope' | 'dispatch' | 'collect';
type ScanCheckpoint = { [P in ScanPhase]: { phase: P; nextOrdinal: string; active: Child[]; scopeTaskId?: number; scope?: JsonObject } }[ScanPhase];

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
  if (plan.axes.some(axis => !parameterScanParameters.some(row => row.key === axis.key && (!row.modalities || row.modalities.includes(source.view.modality))))
    || !parameterScanObjectives.some(row => row.metricId === plan.objective.metricId && (!row.modalities || row.modalities.includes(source.view.modality)))) throw new Problem('parameter_scan_modality_unsupported', 422);
  try {
    const grid = normalizeParameterScan(plan, source.defaults);
    if (source.view.modality === 'TRADE_TICK' && grid.axes.some(axis => ['fast_period', 'slow_period'].includes(axis.key)
      && (axis.domain.kind === 'values' ? axis.domain.values : [axis.domain.end]).some(value => BigInt(value) > 1_000_000n))) throw new Problem('parameter_scan_tick_period_invalid', 422);
    return grid;
  } catch (error) { if (error instanceof ParameterScanError) throw new Problem(error.code, 422); throw error; }
}
function pointAt(grid: ParameterScanGrid, ordinal: string) {
  try { return parameterScanPoint(grid, ordinal); } catch (error) { if (error instanceof ParameterScanError) throw new Problem(error.code, 422); throw error; }
}

function positiveId(value: unknown, name: string): number {
  const parsed = typeof value === 'string' && /^[1-9]\d*$/.test(value) ? Number(value) : value;
  if (typeof parsed !== 'number' || !Number.isSafeInteger(parsed) || parsed < 1) throw new Problem(`${name}_invalid`);
  return parsed;
}
function reference(source: ParameterScanSource) {
  return source.kind === 'factor' ? { ...factorReference(source.factorId), factorId: source.factorId }
    : { researchId: positiveId(source.researchId, 'research_id'), trialId: positiveId(source.trialId, 'trial_id'), memberIndex: null, factorId: null };
}
function sourceFromStudy(source: ParameterScanSource, mode: ParameterScanMode, study: NonNullable<Awaited<ReturnType<ResearchHost['state']>>['research']>, config: ServiceConfig): RetainedSource {
  const ref = reference(source);
  if (!study.input || study.deletedAt || ref.factorId && study.deletedFactors?.[ref.factorId]) throw new Problem('parameter_scan_source_not_found', 404);
  const trial = study.trials.find(row => row.taskId === ref.trialId);
  if (!trial || trial.status !== 'COMPUTED' || !trial.result) throw new Problem('parameter_scan_computed_source_required', 422);
  const native = nativeResearchInput(study.input.data);
  const tick = native?.modality === 'TRADE_TICK';
  const proposal = object(trial.proposal, 'parameter_scan_proposal');
  const allMembers = strategyMembers(proposal);
  const member = ref.memberIndex === null ? null : allMembers[ref.memberIndex];
  if (ref.memberIndex !== null && !member) throw new Problem('factor_not_found', 404);
  const members = member ? [{ model: member.model, weight: '1' }] : allMembers;
  if (mode === 'FACTOR' && members.length !== 1) throw new Problem('parameter_scan_single_factor_required', 422);
  const clipped = iterationRequest(study.input.data, true);
  const data = withNativeResearchRequest(json(study.input.data), clipped);
  // Scan feedback never receives the original independent-data binding.
  delete data.sealed;
  const target = boundScienceTarget(config, data);
  const parameters = tick ? object(clipped.parameters, 'parameters') : data.catalog_scan_request === undefined ? object(object(clipped.forecast, 'forecast').parameters, 'parameters') : object(clipped.parameters, 'parameters');
  const defaults: ParameterValues = {};
  for (const key of ['fast_period', 'slow_period'] as const) {
    const field = tick ? `${key}_cohorts` : key;
    if (typeof parameters[field] === 'number') defaults[key] = String(parameters[field]);
  }
  let policy: StrategyPolicy | null = null;
  let tickEconomic: RetainedSource['tickEconomic'];
  if (mode === 'STRATEGY' && tick) {
    if (members.length !== 1) throw new Problem('parameter_scan_single_factor_required', 422);
    tickEconomic = tickEconomicPolicy(data, study.input.policy);
    defaults.investedWeight = tickEconomic.request.scenario.gross_fraction;
    defaults.maximum_asset_weight = tickEconomic.request.scenario.maximum_asset_weight;
    defaults.participation_fraction = tickEconomic.request.scenario.participation_fraction;
  } else if (mode === 'STRATEGY') {
    if (data.catalog_scan_request !== undefined) throw new Problem('catalog_factor_only_account_binding_required', 422);
    policy = resolveStrategyPolicy(proposal, object(study.input.policy.strategy, 'strategy_policy'));
    validateNativeSimulationSettings(policy.simulationSettings, policy.kind === 'cross-market-threshold' ? policy.allocations!.map(row => row.instrumentId) : [policy.instrumentId!]);
    defaults.threshold = String(policy.threshold); defaults.investedWeight = policy.investedWeight; defaults.targetTtlNs = policy.targetTtlNs;
  }
  if (!defaults.fast_period || !defaults.slow_period) throw new Problem('parameter_scan_native_parameters_required', 422);
  const name = study.input.specification?.name ?? study.input.goal;
  const view: ParameterScanSourceView = { source, label: name, researchId: ref.researchId, trialId: ref.trialId,
    memberIndexes: ref.memberIndex === null ? allMembers.map((_, index) => index) : [ref.memberIndex], datasetId: null, datasetName: null,
    modality: tick ? 'TRADE_TICK' : 'BAR', currency: tickEconomic ? 'USDT' : typeof policy?.simulationSettings.base_currency === 'string' ? policy.simulationSettings.base_currency : null,
    interval: { startNs: String(clipped.train_end_ns), endExclusiveNs: String(clipped.validation_end_ns) }, assumptions: ['VALIDATION_ONLY', 'UNQUALIFIED_EXPLORATORY'] };
  return { researchId: ref.researchId, trialId: ref.trialId, memberIndex: ref.memberIndex, name, data, members, policy, target, defaults, ...(tickEconomic ? { tickEconomic } : {}), view };
}

function effectivePoint(source: RetainedSource, parameters: ParameterValues) {
  const data = json(source.data); const request = iterationRequest(data, true);
  const tick = source.view.modality === 'TRADE_TICK';
  const native = tick ? object(request.parameters, 'parameters') : data.catalog_scan_request === undefined ? object(object(request.forecast, 'forecast').parameters, 'parameters') : object(request.parameters, 'parameters');
  native[tick ? 'fast_period_cohorts' : 'fast_period'] = Number(parameters.fast_period);
  native[tick ? 'slow_period_cohorts' : 'slow_period'] = Number(parameters.slow_period);
  const policy = source.policy ? json(source.policy) as unknown as StrategyPolicy : null;
  if (policy) {
    policy.threshold = Number(parameters.threshold); policy.investedWeight = parameters.investedWeight!; policy.targetTtlNs = parameters.targetTtlNs!;
    thresholdPolicy(json(policy));
  }
  const effectiveData = withNativeResearchRequest(data, request);
  const tickEconomic = source.tickEconomic ? { ...source.tickEconomic, request: validateNativeTickEconomicRequest({ ...source.tickEconomic.request, research: request,
    scenario: { ...source.tickEconomic.request.scenario, gross_fraction: parameters.investedWeight!, maximum_asset_weight: parameters.maximum_asset_weight!, participation_fraction: parameters.participation_fraction! } }) } : undefined;
  return { data: effectiveData, policy, tickEconomic };
}

function unavailable(reason: string, metricId: ParameterScanMetricId): ScanMetric {
  const definition = parameterScanObjectives.find(row => row.metricId === metricId)!;
  return { key: metricId, label: definition.label, partition: 'VALIDATION', value: null, status: 'UNAVAILABLE', reason };
}
function metricValue(value: unknown, metricId: ParameterScanMetricId): ScanMetric {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return unavailable('metric_missing', metricId);
  const row = value as Record<string, unknown>;
  if (row.status !== undefined && row.status !== 'AVAILABLE' || row.reason_code || typeof row.value !== 'number' || !Number.isFinite(row.value)) return unavailable(String(row.reason_code ?? row.status ?? 'metric_unavailable'), metricId);
  return { ...unavailable('', metricId), value: row.value, status: 'AVAILABLE', reason: null };
}
function trialMetrics(summary: JsonObject): ScanMetric[] {
  const partitions = Array.isArray(summary.partitions) ? summary.partitions.filter(value => value && typeof value === 'object' && !Array.isArray(value) && value.partition === 'VALIDATION') : [];
  const partition = partitions.length === 1 ? object(partitions[0]) : null;
  const factor = partition ? object(partition.metric, 'factor_metric') : null;
  const factorMetric = metricValue(factor?.pearson_ic, 'factor.pearson_ic');
  const component = summary.partitions_component && typeof summary.partitions_component === 'object' && !Array.isArray(summary.partitions_component) ? summary.partitions_component.index : null;
  if (typeof component === 'number') { factorMetric.component = component; factorMetric.label = `component ${component}: ${factorMetric.label}`; }
  const result: ScanMetric[] = [factorMetric];
  if (Array.isArray(summary.partition_metrics)) {
    const rows = summary.partition_metrics.filter(value => value && typeof value === 'object' && !Array.isArray(value) && value.partition === 'VALIDATION');
    const metrics = rows.length === 1 ? object(rows[0]) : {};
    for (const [key, field] of [['account.sharpe365', 'sharpe_365'], ['account.cagr365', 'annualized_return_365'], ['account.maxDrawdown', 'max_drawdown']] as const) {
      result.push(metricValue({ value: metrics[field] }, key));
    }
    return result;
  }
  const portfolio = summary.portfolio && typeof summary.portfolio === 'object' && !Array.isArray(summary.portfolio) ? summary.portfolio : null;
  const rows = portfolio?.partition === 'VALIDATION' && Array.isArray(portfolio.statistics) ? portfolio.statistics.map(value => object(value, 'native_statistic')) : [];
  for (const definition of parameterScanObjectives.filter(row => row.mode === 'STRATEGY' && (!row.modalities || row.modalities.includes('BAR')))) {
    const selected = rows.filter(row => row.native_key === definition.nativeKey);
    result.push(selected.length === 1 ? metricValue(selected[0], definition.metricId) : unavailable('native_metric_unavailable', definition.metricId));
  }
  return result;
}
function selectedMetric(metrics: Metric[], metricId: ParameterScanMetricId): ScanMetric { return metrics.find(row => row.key === metricId) as ScanMetric | undefined ?? unavailable('metric_missing', metricId); }
function sampleCounts(summary: JsonObject): Record<string, string> | null {
  const partitions = Array.isArray(summary.partitions) ? summary.partitions.filter(value => value && typeof value === 'object' && !Array.isArray(value) && value.partition === 'VALIDATION') : [];
  if (partitions.length !== 1) {
    const economic = Array.isArray(summary.partition_metrics) ? summary.partition_metrics.find(value => value && typeof value === 'object' && !Array.isArray(value) && value.partition === 'VALIDATION') : null;
    if (!economic) return null;
    const row = object(economic);
    return Object.fromEntries(['days', 'valid_return_days'].flatMap(key => typeof row[key] === 'string' ? [[key, row[key] as string]] : []));
  }
  const metric = object(object(partitions[0]).metric, 'factor_metric');
  const counts = metric.counts;
  if (!counts || typeof counts !== 'object' || Array.isArray(counts)) return null;
  return Object.fromEntries(Object.entries(counts).filter((row): row is [string, string] => typeof row[1] === 'string' && /^\d+$/.test(row[1])));
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
    const summary = value.summary === undefined ? value : object(value.summary, 'parameter_scan_summary');
    const metrics = trialMetrics(summary);
    return { ...trial, status: 'COMPLETED', metrics, metric: selectedMetric(metrics, objective), sampleCounts: sampleCounts(summary), error: null };
  }
  const detail = 'error' in outcome ? classifyExecutionError(outcome.error?.message, 'scientific_execution_failed', outcome.error?.detail && typeof outcome.error.detail === 'object' && !Array.isArray(outcome.error.detail) ? outcome.error.detail : undefined) : null;
  return { ...trial, status: outcome.status === 'aborted' ? 'STOPPED' : 'FAILED', error: detail?.code ?? (outcome.status === 'aborted' ? null : 'parameter_scan_trial_failed'), ...(detail ? { details: json({ error: detail }) } : {}) };
}

export const ParameterScanTask = defineTask<ScanInput, ScanCheckpoint, null>({
  name: 'qz.parameter-scan', version: 1,
  initial: () => ({ phase: 'prepare', nextOrdinal: '0', active: [] }),
  phases: {
    async prepare(task, runtime, ctx) {
      await runtime.commit(async tx => {
        const fact = await scanFact(tx, runtime.conversationId, task.input.id);
        if (fact.stoppedAt) return { status: 'terminal', outcome: { status: 'aborted' } };
        if (task.input.source.data.catalog_scan_request !== undefined && !task.input.source.data.catalog_scope) {
          const scanContext = { scanId: task.input.id, ordinal: 'scope', ...(task.input.implementationDigest ? { implementationDigest: task.input.implementationDigest } : {}) };
          const scopeTaskId = await tx.createTask(CatalogScopeTask, { catalog: String(task.input.source.data.catalog), binary: task.input.source.target, ...{ scanContext } }, { ownership: { kind: 'task', taskId: task.id } });
          fact.scopeTaskId = scopeTaskId;
          return { status: 'waiting', checkpoint: { ...task.state.checkpoint, phase: 'scope', scopeTaskId }, on: [scopeTaskId], policy: 'allSettled' };
        }
        return { status: 'running', checkpoint: { ...task.state.checkpoint, phase: 'dispatch' } };
      }, ctx);
    },
    async scope(task, runtime, ctx) {
      const child = await runtime.getTask(task.state.checkpoint.scopeTaskId as TaskId<JsonObject>, ctx);
      await runtime.commit(async tx => {
        const fact = await scanFact(tx, runtime.conversationId, task.input.id);
        if (child?.state.status !== 'terminal' || child.state.outcome.status !== 'completed') {
          fact.error = 'parameter_scan_catalog_scope_failed'; fact.updatedAt = new Date(runtime.now()).toISOString();
          return { status: 'terminal', outcome: { status: 'failed', error: { message: fact.error } } };
        }
        const value = object(child.state.outcome.result, 'catalog_scope');
        return { status: 'running', checkpoint: { ...task.state.checkpoint, phase: 'dispatch', scope: { conversationId: value.conversationId!, taskId: value.taskId!, count: value.count! } } };
      }, ctx);
    },
    async dispatch(task, runtime, ctx) {
      await runtime.commit(async tx => {
        const fact = await scanFact(tx, runtime.conversationId, task.input.id);
        if (fact.stoppedAt) return { status: 'terminal', outcome: { status: 'aborted' } };
        const active: Child[] = []; let next = BigInt(task.state.checkpoint.nextOrdinal); let visited = 0;
        // A bounded phase yields to official scheduling even across a long run of invalid points.
        while (next < BigInt(task.input.grid.total) && active.length < task.input.original.execution.concurrency && visited++ < 1000) {
          const point = parameterScanPoint(task.input.grid, String(next++));
          if (point.skipReason) continue;
          const effective = effectivePoint(task.input.source, point.parameters);
          if (task.state.checkpoint.scope) effective.data.catalog_scope = task.state.checkpoint.scope;
          const scanContext = { scanId: task.input.id, ordinal: point.ordinal, ...(task.input.implementationDigest ? { implementationDigest: task.input.implementationDigest } : {}) };
          const ownership = { ownership: { kind: 'task' as const, taskId: task.id } };
          const child = effective.tickEconomic
            ? await tx.createTask(TickEconomicTask, { trialId: task.input.source.trialId, model: task.input.source.members[0]!.model, catalog: String(effective.data.catalog), target: task.input.source.target, ...effective.tickEconomic, scanContext }, ownership)
            : task.input.original.mode === 'FACTOR'
            ? await tx.createTask(FactorTask, { model: task.input.source.members[0]!.model, data: effective.data, binary: task.input.source.target, scanContext }, ownership)
            : await tx.createTask(PortfolioEvaluationTask, { data: effective.data, members: task.input.source.members, policy: effective.policy!, partition: 'VALIDATION', binary: task.input.source.target, scanContext }, ownership);
          active.push({ ordinal: point.ordinal, taskId: child });
          fact.trials[point.ordinal] = { ordinal: point.ordinal, parameters: point.parameters, skipReason: null, taskId: child, status: 'PENDING', metric: null, metrics: [], sampleCounts: null, error: null, remoteStatus: null };
        }
        fact.updatedAt = new Date(runtime.now()).toISOString();
        const checkpoint: ScanCheckpoint = { ...task.state.checkpoint, nextOrdinal: String(next), active, phase: active.length ? 'collect' : 'dispatch' };
        if (active.length) return { status: 'waiting', checkpoint, on: active.map(child => child.taskId as TaskId), policy: 'allSettled' };
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
        const ids = [...Object.values(fact.trials).flatMap(row => row.taskId === null ? [] : [row.taskId]), ...(fact.scopeTaskId === undefined ? [] : [fact.scopeTaskId])];
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
  const scientificTasks = [FactorTask, PortfolioEvaluationTask, CatalogScopeTask, TickEconomicTask] as const;
  return { stopTask: StopTask, scientificTasks, extension: defineExtension({ name: 'qz.parameter-scans', tasks: [ParameterScanTask, StopTask, ...scientificTasks] }) };
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
    if (typeof dataset?.currency === 'string' && !retained.tickEconomic) retained.view.currency = dataset.currency;
    if (retained.data.catalog_scan_request !== undefined && retained.data.catalog_scope === undefined) {
      const cached = (await host.harness.snapshot(CatalogScopes, context))?.roots.find(row => row.catalog === retained.data.catalog
        && (typeof retained.target === 'string' ? row.nautilusTarget === undefined : row.nautilusTarget !== undefined && canonical(row.nautilusTarget) === canonical(json(retained.target))));
      if (cached) retained.data.catalog_scope = cached.reference!;
    }
    return retained;
  }
  async function options(input?: ParameterScanSource, mode?: ParameterScanMode): Promise<ParameterScanOptions> {
    if (!input) return { source: null, modes: ['FACTOR', 'STRATEGY'], parameters: [...parameterScanParameters], objectives: [...parameterScanObjectives], defaults: {}, reason: null };
    let retained: RetainedSource;
    try { retained = await source(input, mode ?? 'FACTOR'); }
    catch (error) {
      if (mode || !(error instanceof Error) || error.message !== 'parameter_scan_single_factor_required') throw error;
      retained = await source(input, 'STRATEGY');
    }
    const modes: ParameterScanMode[] = [];
    if (retained.members.length === 1) modes.push('FACTOR');
    try { await source(input, 'STRATEGY'); modes.push('STRATEGY'); } catch (error) {
      if (mode === 'STRATEGY') throw error;
    }
    const selected = mode ?? (retained.policy || retained.tickEconomic ? 'STRATEGY' : 'FACTOR');
    return { source: retained.view, modes, defaults: retained.defaults,
      parameters: parameterScanParameters.filter(row => row.modes.includes(selected) && (!row.modalities || row.modalities.includes(retained.view.modality))).map(row => retained.view.modality === 'TRADE_TICK' && ['fast_period', 'slow_period'].includes(row.key) ? { ...row, label: row.label + '（cohort）' } : row),
      objectives: parameterScanObjectives.filter(row => row.mode === selected && (!row.modalities || row.modalities.includes(retained.view.modality))), reason: null };
  }
  async function preview(value: unknown): Promise<ParameterScanPreview> {
    const input = inputPlan(value); const retained = await source(input.source, input.mode);
    const grid = gridFor(input, retained);
    const examples = Array.from({ length: Number(BigInt(grid.total) < 5n ? BigInt(grid.total) : 5n) }, (_, index) => parameterScanPoint(grid, String(index)));
    return { input, source: retained.view, grid, objective: parameterScanObjectives.find(row => row.metricId === input.objective.metricId)!, examples,
      warnings: ['UNQUALIFIED_EXPLORATORY', ...(BigInt(grid.skipped) ? ['INVALID_PERIOD_PAIRS_SKIPPED'] : [])] };
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
    const references = [...Object.values(original.fact.trials).filter(row => row.taskId !== null).map(row => ({ id: row.taskId!, conversationId: original.index.conversationId })),
      ...(original.fact.scopeTaskId === undefined ? [] : [{ id: original.fact.scopeTaskId, conversationId: original.index.conversationId }])];
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
    const scopeRemote = original.fact.scopeTaskId === undefined ? null : await host.harness.snapshot(NativeRemoteUncertainty, original.index.conversationId as ConversationId, String(original.fact.scopeTaskId), context);
    const scopeUnsettled = scopeRemote?.requests.some(row => !['COMPLETED', 'FAILED', 'CANCELLED', 'REJECTED'].includes(String(row.status))) ?? false;
    return { ...original, trials, scopeUnsettled };
  }
  async function get(id: string): Promise<ParameterScanView> {
    const { index, fact, task, trials, scopeUnsettled } = await projectedTrials(id);
    const outcome = task.state.status === 'terminal' ? task.state.outcome : null;
    const counts: ParameterScanCounts = { total: fact.grid.total, eligible: fact.grid.eligible, skipped: fact.grid.skipped, completed: '0', failed: '0', stopped: '0', running: '0', pending: '0', unknown: '0', comparable: '0' };
    for (const row of trials) {
      const key = row.status === 'COMPLETED' ? 'completed' : row.status === 'FAILED' ? 'failed' : row.status === 'STOPPED' ? 'stopped' : row.status === 'RECONCILING' ? 'unknown' : row.status === 'PENDING' ? 'pending' : 'running';
      counts[key] = String(BigInt(counts[key]!) + 1n);
    }
    const unstarted = BigInt(fact.grid.eligible) - BigInt(trials.length);
    if (outcome || fact.stoppedAt || task.abortRequested) counts.stopped = String(BigInt(counts.stopped) + unstarted);
    else counts.pending = String(unstarted + BigInt(counts.pending));
    const scored = trials.filter(row => row.status === 'COMPLETED' && row.metric?.status === 'AVAILABLE' && row.metric.value !== null).sort((a, b) => b.metric!.value! - a.metric!.value! || (BigInt(a.ordinal) < BigInt(b.ordinal) ? -1 : 1));
    counts.comparable = String(scored.length);
    const completed = outcome?.status === 'completed';
    const unsettled = scopeUnsettled || trials.some(row => row.remoteStatus !== null && !['COMPLETED', 'CANCELLED', 'FAILED', 'REJECTED'].includes(row.remoteStatus));
    // Remote uncertainty is an overlapping observation count: a locally stopped
    // point can still have an original call awaiting a terminal receipt.
    counts.unknown = String(trials.filter(row => row.status === 'RECONCILING' || (fact.stoppedAt || task.abortRequested || outcome?.status === 'aborted')
      && row.remoteStatus !== null && !['COMPLETED', 'CANCELLED', 'FAILED', 'REJECTED'].includes(row.remoteStatus)).length + (scopeUnsettled ? 1 : 0));
    const status: ParameterScanView['status'] = outcome?.status === 'aborted' ? unsettled ? 'STOPPING' : 'STOPPED' : outcome ? completed ? 'COMPLETED' : scopeUnsettled ? 'RECONCILING' : 'FAILED' : task.abortRequested || fact.stoppedAt ? 'STOPPING' : counts.unknown !== '0' || scopeUnsettled ? 'RECONCILING' : task.state.status === 'pending' ? 'QUEUED' : 'RUNNING';
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
      const required = retained.view.modality === 'TRADE_TICK' ? [input.mode === 'FACTOR' ? 'evaluate-tick-factor' : 'replay-tick-economic'] : input.mode === 'FACTOR' ? [retained.data.catalog_scan_request === undefined ? 'evaluate-factor' : 'scan-factor-catalog']
        : ['evaluate-factor', retained.policy?.forecastMode === 'price-volume' ? 'forecast-factor' : 'forecast', 'blend-targets', 'simulate'];
      if (retained.data.catalog_scan_request !== undefined) required.push('catalog-factor-scope');
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
      sourceFromStudy(input.source, input.mode, await tx.doc(Study, retained.researchId as ConversationId), config);
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
      return x === null ? y === null ? 0 : 1 : y === null ? -1 : y - x;
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
        native = value.native ? await readNativeEvidence(host.harness, child.conversationId, child.id, object(value.native, 'parameter_scan_native'), context) : value;
      }
    }
    const input = task.input as unknown as ScanInput;
    const effective = effectivePoint(input.source, point.parameters);
    const details = json({ ...row.details, effectiveRequest: iterationRequest(effective.data, true), effectivePolicy: effective.tickEconomic?.request ?? effective.policy,
      ...(row.taskId === null ? {} : { evidenceTaskId: row.taskId }) });
    if (input.source.view.modality === 'TRADE_TICK' && Array.isArray(native?.days)) {
      return { ...row, curve: native.days.map(value => object(value)).filter(day => typeof day.day_start_ns === 'string'
        && BigInt(day.day_start_ns) >= BigInt(input.source.view.interval.startNs) && BigInt(day.day_start_ns) < BigInt(input.source.view.interval.endExclusiveNs))
        .map(day => ({ at: nsTime(day.day_start_ns as string)!, value: typeof day.equity_usdt === 'string' ? day.equity_usdt : null })), details };
    }
    if (native?.simulation && typeof native.simulation === 'object' && !Array.isArray(native.simulation) && effective.policy && typeof effective.policy.simulationSettings.base_currency === 'string') {
      try { return { ...row, curve: nativeCurve(native.simulation, effective.policy.simulationSettings.base_currency), details }; }
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
    if (!parameterScanObjectives.some(row => row.metricId === metricId && row.mode === fact.original.mode && (!row.modalities || row.modalities.includes(fact.sourceView.modality)))) throw new Problem('parameter_scan_metric_invalid');
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
        metric: trial ? selectedMetric(trial.metrics, chosenMetric) : null, reason: point.skipReason ?? trial?.error ?? null });
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
  if (query.sourceKind === undefined && query.sourceId === undefined && query.researchId === undefined && query.trialId === undefined) return undefined;
  if (query.sourceKind === 'factor' && typeof query.sourceId === 'string') return { kind: 'factor', factorId: query.sourceId };
  if (query.sourceKind === 'research-trial') return { kind: 'research-trial', researchId: positiveId(query.researchId, 'research_id'), trialId: positiveId(query.trialId, 'trial_id') };
  throw new Problem('parameter_scan_source_invalid');
}
export function registerParameterScanRoutes(app: FastifyInstance, host: ResearchHost, work: ReturnType<typeof parameterScanWorkbench>, streams: Set<() => Promise<void>>) {
  const root = `${API}/parameter-scans`;
  app.get<{ Querystring: Record<string, unknown> }>(`${root}/options`, request => {
    const mode = request.query.mode;
    if (mode !== undefined && mode !== 'FACTOR' && mode !== 'STRATEGY') throw new Problem('parameter_scan_mode_invalid');
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

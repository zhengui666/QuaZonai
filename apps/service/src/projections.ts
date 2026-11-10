import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId, Cursor, TaskRecord } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import type { EquityPoint, Json, JsonObject, Metric, ObservationView, PortfolioView, RecordedDataset, ResearchDetail, ResearchInput, TaskView } from '../../../packages/contracts/src/index.ts';
import { readResearchAdmissions } from './admissions.ts';
import { Application, counter, decimal, nowNs, nsTime, object, Problem } from './facts.ts';
import { submissionView } from '../../research/src/recovery.ts';
import { researchCriterion } from '../../../packages/contracts/src/research.ts';
import type { ObservationFact, PortfolioFact, ResearchAdmissions } from './facts.ts';
import { classifyExecutionError } from '../../research/src/execution-errors.ts';
import { portfolioMetrics, portfolioPerformance } from './portfolio-performance.ts';

function publicExecutionError(value: unknown, fallback: 'scientific_execution_failed' | 'backtest_failed', operation: string, stored?: unknown) {
  const context = stored && typeof stored === 'object' && !Array.isArray(stored) ? stored as Record<string, unknown> : {};
  const detail = classifyExecutionError(value, fallback, { operation: context.operation ?? operation, stage: context.stage });
  return { error: detail?.code ?? null, errorDetail: detail };
}

// Observation errors retain their existing code-only boundary.
function publicObservationError(value: unknown, fallback: string): string | null {
  if (typeof value !== 'string' || !value) return null;
  return /^[a-z][a-z0-9_]{0,127}$/.test(value) ? value : fallback;
}

function tickInputDetails(value: Json | undefined): { inputCoverage?: JsonObject } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
  const coverage = value.coverage;
  if (!coverage || typeof coverage !== 'object' || Array.isArray(coverage) || coverage.modality !== 'TRADE_TICK') return {};
  const request = value.request && typeof value.request === 'object' && !Array.isArray(value.request) ? value.request : undefined;
  const input = request?.input && typeof request.input === 'object' && !Array.isArray(request.input) ? request.input : undefined;
  return { inputCoverage: { ...coverage, ...(typeof input?.representation === 'string' ? { representation: input.representation } : {}),
    ...(typeof input?.event_start_ns === 'string' && typeof input?.event_end_ns === 'string' ? { event_start_ns: input.event_start_ns, event_end_ns: input.event_end_ns } : {}) } };
}

/** Retain native scalar values; missing metrics are not converted into zero. */
export function metrics(value: Json | undefined, prefix = ''): Metric[] {
  if (!value || typeof value !== 'object') return [];
  if (!Array.isArray(value) && Array.isArray(value.partitions)) {
    const component = value.partitions_component ? object(value.partitions_component, 'partitions_component').index : undefined;
    const factor = value.partitions.flatMap((item) => {
      const partition = object(item, 'partition');
      return metrics(partition.metric, String(partition.partition)).map((metric) => ({ ...metric, partition: String(partition.partition),
        ...(typeof component === 'number' ? { component, label: `component ${component}: ${metric.label}` } : {}) }));
    });
    if (!value.portfolio) return factor;
    const portfolio = object(value.portfolio, 'portfolio_summary');
    const account: Metric[] = Array.isArray(portfolio.statistics) ? portfolio.statistics.map((item) => {
      const statistic = object(item, 'native_statistic'); const key = `${portfolio.partition}.portfolio.${statistic.native_key}`;
      return { key, label: key, partition: String(portfolio.partition), value: typeof statistic.value === 'number' ? statistic.value : null };
    }) : [];
    return [...factor, ...account];
  }
  return Object.entries(value).flatMap(([key, item]) => {
    if (['schema_version', 'request', 'native_versions', 'methods', 'points'].includes(key)) return [];
    const path = prefix ? `${prefix}.${key}` : key;
    if (item && typeof item === 'object' && !Array.isArray(item) && 'value' in item) {
      const measured = item.value;
      return [{ key: path, label: path, value: typeof measured === 'number' || typeof measured === 'string' ? measured : null,
        ...(typeof item.status === 'string' ? { status: item.status } : {}) }];
    }
    if (typeof item === 'number' || typeof item === 'string' && /^-?\d+(?:\.\d+)?$/.test(item)) return [{ key: path, label: path, value: item }];
    if (item && typeof item === 'object') return metrics(item, path);
    return [];
  });
}
function taskView(record: TaskRecord<Json, Json, Json>): TaskView {
  return { id: record.id, name: record.kind, status: record.state.status,
    ...(record.state.status === 'terminal' ? { outcome: record.state.outcome.status } : {}),
    error: record.state.status === 'terminal' && ['failed', 'faulted'].includes(record.state.outcome.status) ? 'task_failed' : null };
}

export function researchTaskViews(host: ResearchHost) {
  // Only immutable terminal metadata is cached. Live state comes from the official graph.
  const history = new Map<number, TaskView[]>(); const loading = new Map<number, Promise<TaskView[]>>();
  host.harness.observeCommits({ advance({ changes }) {
    for (const change of changes) if (change.type === 'task' && change.value.state.status === 'terminal') {
      const record = change.value; const rows = history.get(record.conversationId);
      if (rows) history.set(record.conversationId, [taskView(record), ...rows.filter(row => row.id !== record.id)].sort((a, b) => b.id - a.id).slice(0, 100));
    }
  }, end() { history.clear(); loading.clear(); } });
  async function terminalViews(id: number) {
    if (history.has(id)) return history.get(id)!;
    let pending = loading.get(id);
    if (!pending) {
      pending = host.harness.commit(async tx => {
        const rows: TaskView[] = []; let cursor: Cursor | undefined;
        do {
          const page = await tx.scanTasks({ conversationId: id as ConversationId, status: 'terminal', order: 'descending' }, 10, cursor);
          rows.push(...page.items.map(taskView)); cursor = page.next;
        } while (cursor && rows.length < 100);
        history.set(id, rows); return rows;
      }, BACKGROUND_CONTEXT);
      loading.set(id, pending);
    }
    try { return await pending; } finally { if (loading.get(id) === pending) loading.delete(id); }
  }
  return async (id: number): Promise<TaskView[]> => {
    const graph = await host.harness.watchTaskGraph(BACKGROUND_CONTEXT);
    try {
      const live = Object.values(graph.value.tasks).filter(task => task.conversationId === id).map(task => ({
        id: task.id, name: task.kind, status: task.state.status, error: null,
      }));
      return [...new Map([...live, ...await terminalViews(id)].map(row => [row.id, row])).values()].sort((a, b) => b.id - a.id).slice(0, 100);
    } finally { await graph.stop(); }
  };
}

export function researchSummary(id: number, view: Awaited<ReturnType<ResearchHost['state']>>, admissions: ResearchAdmissions | undefined, running: boolean) {
  const study = view.research;
  if (!study?.input || study.deletedAt) throw new Problem('research_not_found', 404);
  const input = study.input; const final = study.final;
  const admission = view.originalSubmission?.requestId ? admissions?.[`request:${view.originalSubmission.requestId}`] : undefined;
  const original = admission?.original && typeof admission.original === 'object' && !Array.isArray(admission.original) ? admission.original : null;
  const dataset = admission?.dataset ? admission.dataset as unknown as RecordedDataset : null;
  return {
    id, title: input.specification?.name ?? input.goal, status: final ? String(final.status) : running || ['queued', 'placed'].includes(view.submission?.status ?? '') ? 'RUNNING' : 'IDLE',
    trials: study.trials.length, model: `${input.model.provider}/${input.model.modelId}`,
    createdAt: null, updatedAt: null, dataset,
    datasetId: dataset?.id ?? (original && !('input' in original) && typeof original.datasetId === 'string' ? original.datasetId : null),
    originalSubmission: submissionView(view.originalSubmission),
  };
}

export async function researchDetail(host: ResearchHost, id: number, readTasks: ReturnType<typeof researchTaskViews>): Promise<ResearchDetail> {
  const view = await host.state(id); const study = view.research;
  if (!study?.input || study.deletedAt) throw new Problem('research_not_found', 404);
  const tasks = await readTasks(id); const final = study.final;
  const admissions = await readResearchAdmissions(host);
  return {
    ...researchSummary(id, view, admissions, tasks.some(task => task.status !== 'terminal')),
    reviews: Object.values(view.reviews?.records ?? {}).flatMap(record => record.report && typeof record.report === 'object' && !Array.isArray(record.report)
      ? [record.report as unknown as import('../../../packages/contracts/src/review.ts').ReviewReport] : []),
    input: study.input as ResearchInput, submission: submissionView(view.submission),
    ...(view.literature ? { literature: view.literature } : {}),
    ...(study.previousStops ? { previousStops: study.previousStops } : {}),
    ...(study.input.specification ? { criterion: researchCriterion(study.input.specification, String(final?.status ?? 'PENDING'), metrics(final?.result)) } : {}),
    trialRecords: study.trials.map((trial) => ({
      id: Number(trial.taskId), hypothesis: String((trial.proposal as JsonObject)?.hypothesis ?? ''),
      status: String(trial.status), metrics: metrics(trial.result), ...tickInputDetails(trial.result),
      ...publicExecutionError(trial.error, 'scientific_execution_failed', Array.isArray((trial.proposal as JsonObject)?.members) ? 'portfolio-evaluation' : 'evaluate-factor', trial.errorDetail),
    })), tasks,
    final: final ? { status: String(final.status), trialId: typeof final.trialId === 'number' ? final.trialId : null,
      reason: typeof final.reason === 'string' ? final.reason : null,
      ...publicExecutionError(final.error, 'scientific_execution_failed',
        Array.isArray((study.trials.find(trial => trial.taskId === final.trialId)?.proposal as JsonObject)?.members) ? 'portfolio-evaluation' : 'evaluate-factor', final.errorDetail),
      metrics: metrics(final.result), ...tickInputDetails(final.result) } : null,
  };
}

export function portfolioView(fact: PortfolioFact): PortfolioView {
  return { ...fact.view, metrics: portfolioMetrics(fact), performance: portfolioPerformance(fact), ...publicExecutionError(fact.view.error, 'backtest_failed', 'portfolio-backtest', fact.view.errorDetail), ...(fact.weights && !fact.input.factors ? { weights: fact.view.researchIds.map((researchId, index) => ({ researchId, weight: fact.weights![index]! })) } : {}),
    targetExpiresAt: typeof fact.target?.valid_until_ns === 'string' ? nsTime(fact.target.valid_until_ns) : null,
    ...(typeof fact.target?.valid_until_ns === 'string' ? { targetExpired: BigInt(counter(fact.target.valid_until_ns, 'expiry')) <= BigInt(nowNs()) } : {}) };
}
export function observationView(fact: ObservationFact): ObservationView {
  const snapshot = fact.snapshot;
  return { ...fact.view, error: publicObservationError(fact.view.error, 'observation_operation_failed'), connectionId: fact.connection.id, connectionName: fact.connection.name,
    provenance: snapshot ? { clientId: String(snapshot.clientId), accountId: String(snapshot.accountId),
      sequence: counter(snapshot.sequence, 'account_sequence'), nativeClockNs: counter(snapshot.nativeClockNs, 'account_clock') } : null };
}

function nativeMoney(value: Json | undefined, currency: string): string | null {
  if (value == null) return null;
  // Official Nautilus canonical Money is "<exact amount> <currency>".
  if (typeof value !== 'string') throw new Problem('native_money_unsupported');
  const match = /^(-?\d+(?:\.\d+)?) ([A-Za-z0-9]+)$/.exec(value);
  if (!match || match[2] !== currency) throw new Problem('native_equity_currency_mismatch');
  return decimal(match[1], 'equity');
}
export function nativeCurve(simulation: JsonObject, currency: string): EquityPoint[] {
  const canonical = object(simulation.canonical_result, 'canonical_result');
  if (!Array.isArray(canonical.portfolio_snapshots) || !canonical.portfolio_snapshots.length) throw new Problem('native_shared_account_equity_missing');
  const points = canonical.portfolio_snapshots.map((value) => {
    const snapshot = object(value, 'native_snapshot');
    const equities = snapshot.total_equity;
    if (!Array.isArray(equities) || equities.length !== 1) throw new Problem('native_single_currency_equity_required');
    const clock = counter(snapshot.ts_event, 'native_clock');
    return { clock: BigInt(clock), at: nsTime(clock), value: nativeMoney(equities[0], currency) };
  });
  // Native snapshot arrival order is not necessarily event-time order.
  return points.sort((a, b) => a.clock < b.clock ? -1 : a.clock > b.clock ? 1 : 0).map(({ at, value }) => ({ at, value }));
}

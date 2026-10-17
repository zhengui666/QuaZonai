import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ResearchHost } from '../../research/src/host.ts';
import type { EquityPoint, Json, JsonObject, Metric, ObservationView, PortfolioView, RecordedDataset, ResearchDetail, ResearchInput, TaskView } from '../../../packages/contracts/src/index.ts';
import { Application, counter, decimal, nowNs, nsTime, object, Problem } from './facts.ts';
import { submissionView } from '../../research/src/recovery.ts';
import type { ObservationFact, PortfolioFact, ResearchAdmissions } from './facts.ts';

// Opaque public-code convention, as on the HTTP error boundary; raw diagnostics stay in Durable.
function publicExecutionError(value: unknown, fallback: string): string | null {
  if (typeof value !== 'string' || !value) return null;
  return /^[a-z][a-z0-9_]{0,127}$/.test(value) ? value : fallback;
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
export async function researchDetail(host: ResearchHost, id: number, admitted?: ResearchAdmissions): Promise<ResearchDetail> {
  const view = await host.state(id);
  const study = view.research;
  if (!study?.input) throw new Problem('research_not_found', 404);
  const input = study.input as ResearchInput;
  const records = await host.harness.commit((tx) => tx.scanTasks({ conversationId: id as import('@earendil-works/pi-durable').ConversationId, order: 'descending' }, 100), BACKGROUND_CONTEXT);
  const tasks: TaskView[] = records.items.map((record) => ({
    id: record.id, name: record.kind, status: record.state.status,
    ...(record.state.status === 'terminal' ? { outcome: record.state.outcome.status } : {}),
    error: record.state.status === 'terminal' && ['failed', 'faulted'].includes(record.state.outcome.status) ? 'task_failed' : null,
  }));
  const final = study.final;
  const admissions = admitted ?? (await host.harness.snapshot(Application, BACKGROUND_CONTEXT))?.researchRequests;
  const admission = view.originalSubmission?.requestId ? admissions?.[`request:${view.originalSubmission.requestId}`] : undefined;
  const original = admission?.original && typeof admission.original === 'object' && !Array.isArray(admission.original) ? admission.original : null;
  const dataset = admission?.dataset ? admission.dataset as unknown as RecordedDataset : null;
  const submission = view.submission;
  return {
    id, title: input.goal, status: final ? String(final.status) : records.items.some((task) => task.state.status !== 'terminal') || ['queued', 'placed'].includes(submission?.status ?? '') ? 'RUNNING' : 'IDLE',
    trials: study.trials.length, model: `${input.model.provider}/${input.model.modelId}`,
    createdAt: null, updatedAt: null, input, dataset,
    datasetId: dataset?.id ?? (original && !('input' in original) && typeof original.datasetId === 'string' ? original.datasetId : null),
    submission: submissionView(submission), originalSubmission: submissionView(view.originalSubmission),
    ...(study.previousStops ? { previousStops: study.previousStops } : {}),
    trialRecords: study.trials.map((trial) => ({
      id: Number(trial.taskId), hypothesis: String((trial.proposal as JsonObject)?.hypothesis ?? ''),
      status: String(trial.status), metrics: metrics(trial.result), error: publicExecutionError(trial.error, 'scientific_execution_failed'),
    })), tasks,
    final: final ? { status: String(final.status), trialId: typeof final.trialId === 'number' ? final.trialId : null,
      reason: typeof final.reason === 'string' ? final.reason : null,
      error: publicExecutionError(final.error, 'scientific_execution_failed'), metrics: metrics(final.result) } : null,
  };
}

export function portfolioView(fact: PortfolioFact): PortfolioView {
  return { ...fact.view, error: publicExecutionError(fact.view.error, 'backtest_failed'), ...(fact.weights ? { weights: fact.view.researchIds.map((researchId, index) => ({ researchId, weight: fact.weights![index]! })) } : {}),
    targetExpiresAt: typeof fact.target?.valid_until_ns === 'string' ? nsTime(fact.target.valid_until_ns) : null,
    ...(typeof fact.target?.valid_until_ns === 'string' ? { targetExpired: BigInt(counter(fact.target.valid_until_ns, 'expiry')) <= BigInt(nowNs()) } : {}) };
}
export function observationView(fact: ObservationFact): ObservationView {
  const snapshot = fact.snapshot;
  return { ...fact.view, error: publicExecutionError(fact.view.error, 'observation_operation_failed'), connectionId: fact.connection.id, connectionName: fact.connection.name,
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
    return { at: nsTime(counter(snapshot.ts_event, 'native_clock')), value: nativeMoney(equities[0], currency) };
  });
  return points;
}

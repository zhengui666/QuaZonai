import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { Cursor, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { FastifyInstance } from 'fastify';
import type { ResearchHost } from '../../research/src/host.ts';
import { Requests } from '../../research/src/documents.ts';
import { latestTrialReview } from '../../research/src/review-documents.ts';
import { researchCriterion } from '../../../packages/contracts/src/research.ts';
import type { ResearchSpecification, Metric, RecordedDataset } from '../../../packages/contracts/src/index.ts';
import type { AllocationListItem, FactorListItem, ListCriterion, ListPage, PortfolioListItem, ResearchListItem, TickReplayListItem } from '../../../packages/contracts/src/lists.ts';
import type { TickEconomicReplayStart, TickEconomicReplayView } from '../../../packages/contracts/src/economic-tick.ts';
import type { ReviewAllocationView } from '../../../packages/contracts/src/review.ts';
import { executionStatus, portfolioRuntime } from './runtime-views.ts';
import { readResearchAdmissions } from './admissions.ts';
import { Application, decimalUnits, portfolio, Problem } from './facts.ts';
import type { ResearchAdmissions } from './facts.ts';
import { factorReference, factorViews } from './factors.ts';
import { metrics } from './projections.ts';
import { portfolioPerformance } from './portfolio-performance.ts';
import { replayMetadata, TickEconomicReplays } from './economic-tick.ts';
import { ReviewAllocations } from './review-allocation.ts';
import type { ServiceConfig } from './config.ts';

const context = BACKGROUND_CONTEXT;
type Collection = 'research' | 'factors' | 'portfolios' | 'tick-economic-replays' | 'review-allocations';
type Item = ResearchListItem | FactorListItem | PortfolioListItem | TickReplayListItem | AllocationListItem;
type Scalar = string | number | bigint | null;
type Indexed<T extends Item = Item> = { item: T; search: string; filters: Record<string, string[]>; sort: Record<string, Scalar>; labels?: Record<string, Record<string, string>> };
type Query = Record<string, unknown>;
const filterKeys: Record<Collection, string[]> = {
  research: ['status', 'scope', 'datasetId', 'criterionStatus', 'metric', 'modality', 'model'],
  factors: ['status', 'datasetId', 'evidence', 'combinable', 'modality', 'research', 'reviewDecision'],
  portfolios: ['status', 'datasetId', 'reviewDecision', 'currency', 'memberType', 'source'],
  'tick-economic-replays': ['status', 'datasetId', 'memberType', 'source'],
  'review-allocations': ['status', 'reviewDecision', 'source'],
};
const sortKeys: Record<Collection, string[]> = {
  research: ['newest', 'title', 'datasetName', 'status', 'trials', 'criterionValue'],
  factors: ['newest', 'name', 'researchName', 'datasetName', 'status', 'independentIc'],
  portfolios: ['name', 'datasetName', 'memberCount', 'capital', 'sharpe', 'annualizedReturn', 'maxDrawdown', 'createdAt'],
  'tick-economic-replays': ['name', 'datasetName', 'memberCount', 'createdAt'],
  'review-allocations': ['name', 'memberCount', 'createdAt'],
};
const labels: Record<string, string> = {
  RUNNING: '进行中', STARTED: '计算中', QUEUED: '待执行', COMPUTED: '已计算', FAILED: '失败', STOPPED: '已停止', IDLE: '当前无执行',
  'single-factor': '单成员', ensemble: '多成员', BAR: 'K线', TRADE_TICK: '逐笔',
  independent: '独立评估', historical: '历史重评', component_only: '组合成员', validation_only: '仅验证集', unavailable: '无证据',
  met: '达到标准', not_met: '未达到标准', computed: '已计算（未设阈值）', insufficient: '证据不足', failed: '计算失败',
  PASS: '通过', FAIL: '未通过', BLOCK: '受阻', REVIEW: '待复核', NEEDS_EVIDENCE: '需要补证据', REJECT: '不予放行', INCONCLUSIVE: '证据不足', REVIEW_BLOCKED: '评审未放行',
  true: '可组合', false: '不可组合', pearson_ic: '信息系数（IC）',
};
const value = (input: string | number | boolean | null | undefined) => input == null || input === '' ? 'unknown' : String(input);
const one = (input: string | number | boolean | null | undefined) => [value(input)];
const normalized = (input: string) => input.normalize('NFKC').toLocaleLowerCase().trim();
const searchText = (...parts: unknown[]) => normalized(parts.filter(part => part != null).join(' '));
const shortCriterion = (criterion: ReturnType<typeof researchCriterion>): ListCriterion | null => criterion ? {
  status: criterion.status, metric: criterion.metric, value: criterion.value, target: criterion.target, partition: criterion.partition,
} : null;
function modality(data: JsonObject): ResearchListItem['modality'] {
  const native = data.nativeInput;
  if (native && typeof native === 'object' && !Array.isArray(native) && (native.modality === 'BAR' || native.modality === 'TRADE_TICK')) return native.modality;
  return data.request || data.catalog_scan_request ? 'BAR' : null;
}
function independentIc(evidence: FactorListItem['evidence'], measured: Metric[]): number | null {
  if (evidence !== 'independent') return null;
  const values = measured.filter(metric => metric.key === 'HELDOUT.pearson_ic' && (metric.partition === undefined || metric.partition === 'HELDOUT') && metric.component === undefined);
  const metric = values.length === 1 ? values[0] : undefined;
  return metric && (metric.status === undefined || metric.status === 'AVAILABLE') && typeof metric.value === 'number' && Number.isFinite(metric.value) ? metric.value : null;
}

/** A rebuildable read index over public Durable snapshots, never a second source of business facts. */
export function listIndex(host: ResearchHost, config: Pick<ServiceConfig, 'datasets'>) {
  const research = new Map<number, Indexed<ResearchListItem>>();
  const factors = new Map<number, Indexed<FactorListItem>[]>();
  const portfolios = new Map<string, Indexed<PortfolioListItem>>();
  const ticks = new Map<string, Indexed<TickReplayListItem>>();
  const allocations = new Map<string, Indexed<AllocationListItem>>();
  const known = new Set<number>(); const dirty = new Set<number>(); const dirtyTasks = new Set<number>();
  const researchNames = new Map<number, string>();
  let admissions: ResearchAdmissions = {}; let admissionStamp = '';
  let originals = new Map<number, string>(); let originalStamp = '';
  let appDirty = true; let requestsDirty = true; let ticksDirty = true; let allocationsDirty = true;
  let initialized = false; let refreshing: Promise<void> | undefined;
  const tickStamps = new Map<string, string>();
  const datasetName = (id: string, sourceResearchIds: number[]) => {
    // A configured ID may have been renamed since admission. Only this object's
    // original sources can supply its frozen name; another study is not provenance.
    const recorded = [...new Set(sourceResearchIds)].flatMap(researchId => {
      const admission = admissions[`request:${originals.get(researchId) ?? ''}`];
      const original = admission?.original as { datasetId?: string } | undefined;
      if ((admission?.dataset?.id ?? original?.datasetId) !== id) return [];
      return [typeof admission?.dataset?.name === 'string' ? admission.dataset.name : null];
    });
    if (recorded.length) return recorded.every(name => name === recorded[0]) ? recorded[0]! : null;
    return config.datasets.find(dataset => dataset.id === id)?.name ?? null;
  };
  const commits = host.harness.observeCommits({ advance({ changes }) {
    // Publication listeners must not call Session operations or perform blocking work.
    for (const change of changes) {
      if (change.type === 'conversation') { known.add(change.value.id); dirty.add(change.value.id); }
      else if (change.type === 'task') {
        dirty.add(change.value.conversationId);
        if (change.value.kind === 'qz.portfolio') appDirty = true;
        if (change.value.kind === 'qz.review-allocation') allocationsDirty = true;
        if (change.value.kind === 'qz.tick-economic-replay') { ticksDirty = true; dirtyTasks.add(change.value.id); }
      } else if (change.type === 'submission' && change.value.type === 'input') dirty.add(change.value.conversationId);
      else if (change.type === 'document' || change.type === 'document.copy') {
        const kind = change.record.kind;
        if (kind === 'qz.application') appDirty = true;
        else if (kind === 'qz.research-requests') { requestsDirty = true; appDirty = true; }
        else if (kind === 'qz.tick-economic-replays') ticksDirty = true;
        else if (kind === 'qz.review-allocations') allocationsDirty = true;
        else if (change.conversationId !== undefined && ['qz.study', 'qz.review-ledger'].includes(kind)) dirty.add(change.conversationId);
      }
    }
  }, end() {} });
  const unsubscribe = () => commits.detach();

  async function refreshApplication() {
    appDirty = false;
    const app = await host.harness.snapshot(Application, context);
    const next: ResearchAdmissions = {};
    for (const [key, admission] of Object.entries(await readResearchAdmissions(host))) {
      const original = admission.original && typeof admission.original === 'object' && !Array.isArray(admission.original) ? admission.original : {};
      const dataset = admission.dataset;
      next[key] = { original: { ...(typeof original.datasetId === 'string' ? { datasetId: original.datasetId } : {}) }, input: {},
        ...(dataset ? { dataset: Object.fromEntries(['id', 'name', 'currency', 'source', 'nativeModality'].filter(key => dataset[key] !== undefined).map(key => [key, dataset[key]!])) } : {}) };
    }
    const stamp = JSON.stringify(next);
    if (stamp !== admissionStamp) { admissionStamp = stamp; admissions = next; for (const id of known) dirty.add(id); ticksDirty = true; }
    portfolios.clear();
    for (const raw of Object.values(app?.portfolios ?? {})) {
      const fact = await portfolioRuntime(host, portfolio(raw)); if (fact.deletedAt) continue;
      const view = fact.view; const memberCount = view.factors?.length ?? view.researchIds.length;
      const evidenceResearchIds = fact.evidence.flatMap(row => typeof row.researchId === 'number' ? [row.researchId] : []);
      const item: PortfolioListItem = { id: view.id, name: view.name, status: view.status, datasetId: view.datasetId,
        datasetName: datasetName(view.datasetId, evidenceResearchIds.length ? evidenceResearchIds : view.researchIds), memberCount, memberType: memberCount > 1 ? 'ensemble' : 'single-factor',
        capital: view.capital, currency: view.currency, createdAt: view.createdAt, reviewDecision: view.review?.decision ?? null,
        progress: view.progress ? { phase: view.progress.phase, updatedAt: view.progress.updatedAt, ...(view.progress.completed !== undefined ? { completed: view.progress.completed } : {}), ...(view.progress.total !== undefined ? { total: view.progress.total } : {}) } : null,
        performance: portfolioPerformance(fact) };
      const sources = [...view.researchIds.map(String), ...(view.factors ?? []).map(member => member.factorId)];
      const sourceNames = sources.map(id => {
        const evidence = fact.evidence.find(row => String(row.researchId) === id || row.factorId === id);
        const input = evidence?.input as { goal?: string; specification?: { name?: string } } | undefined;
        return input?.specification?.name ?? input?.goal ?? researchNames.get(Number(id)) ?? '来源名称未记录';
      });
      portfolios.set(item.id, { item, search: searchText(item.name, item.id, item.datasetName, ...sources, ...sourceNames),
        filters: { status: one(item.status), datasetId: one(item.datasetId), reviewDecision: one(item.reviewDecision), currency: one(item.currency), memberType: one(item.memberType), source: sources },
        sort: { name: item.name, datasetName: item.datasetName, memberCount, capital: decimalUnits(item.capital), createdAt: item.createdAt,
          sharpe: item.performance.sharpe, annualizedReturn: item.performance.annualizedReturn, maxDrawdown: item.performance.maxDrawdown },
        labels: { datasetId: { [item.datasetId]: item.datasetName ?? '数据集名称未记录' }, source: Object.fromEntries(sources.map((id, i) => [id, sourceNames[i]!])) } });
    }
  }
  async function refreshRequests() {
    requestsDirty = false;
    const requests = await host.harness.snapshot(Requests, context);
    const entries = Object.entries(requests?.requests ?? {}).filter(([, request]) => !request.recovery && request.conversationId !== null).map(([key, request]) => [request.conversationId!, key.slice('request:'.length)] as const);
    const stamp = JSON.stringify(entries);
    if (stamp !== originalStamp) { originals = new Map(entries); originalStamp = stamp; for (const id of known) dirty.add(id); appDirty = true; ticksDirty = true; }
  }
  async function refreshResearch(ids: number[]) {
    const active = new Set((await host.harness.inspect(context)).tasks.map(task => task.record.conversationId as number));
    for (const id of ids) {
      const view = await host.listState(id); const study = view.research;
      const name = study?.input?.specification?.name ?? study?.input?.goal;
      if (name !== researchNames.get(id)) {
        if (name) researchNames.set(id, name); else researchNames.delete(id);
        appDirty = true; ticksDirty = true;
      }
      if (!study?.input || study.deletedAt) { research.delete(id); factors.delete(id); continue; }
      known.add(id);
      const input = study.input; const final = study.final;
      const admission = admissions[`request:${originals.get(id) ?? ''}`];
      const dataset = admission?.dataset as unknown as RecordedDataset | undefined;
      const original = admission?.original as { datasetId?: string } | undefined;
      const spec = input.specification as ResearchSpecification | undefined;
      const criterion = final && final.status !== 'STARTED' ? shortCriterion(researchCriterion(spec, String(final.status), metrics(final.result))) : null;
      const item: ResearchListItem = { id, title: spec?.name ?? input.goal, status: final ? String(final.status) : active.has(id) || ['queued', 'placed'].includes(view.submission?.status ?? '') ? 'RUNNING' : 'IDLE',
        trials: study.trials.length, model: `${input.model.provider}/${input.model.modelId}`, datasetId: dataset?.id ?? original?.datasetId ?? null,
        datasetName: dataset?.name ?? null, scope: spec?.search.scope ?? null, modality: modality(input.data), criterion };
      research.set(id, { item, search: searchText(item.id, item.title, input.goal, spec?.question, spec?.rationale, item.datasetName, item.model),
        filters: { status: one(item.status), scope: one(item.scope), datasetId: one(item.datasetId), criterionStatus: one(criterion?.status), metric: one(spec?.evaluation.metric), modality: one(item.modality), model: one(item.model) },
        sort: { newest: id, title: item.title, datasetName: item.datasetName, status: item.status, trials: item.trials, criterionValue: criterion?.value ?? null },
        labels: { datasetId: { [value(item.datasetId)]: item.datasetName ?? '数据集名称未记录' } } });
      factors.set(id, factorViews(id, study, admissions, originals.get(id)).map(factor => {
        const review = latestTrialReview(view.reviews, factor.trialId);
        const item: FactorListItem = { id: factor.id, name: factor.name, researchId: id, researchName: spec?.name ?? input.goal, trialId: factor.trialId, memberIndex: factor.memberIndex,
          datasetId: factor.datasetId, datasetName: dataset?.name ?? null, status: factor.status, evidence: factor.evidence,
          combinable: factor.combinable && (!review || review.decision === 'PASS'), nativeModality: modality(input.data),
          reviewDecision: review?.decision ?? null, independentIc: independentIc(factor.evidence, factor.metrics) };
        return { item, search: searchText(item.id, item.name, factor.hypothesis, item.researchName, id, item.datasetName),
          filters: { status: one(item.status), datasetId: one(item.datasetId), evidence: one(item.evidence), combinable: one(item.combinable), modality: one(item.nativeModality), research: one(id), reviewDecision: one(item.reviewDecision) },
          sort: { newest: id, name: item.name, researchName: item.researchName, datasetName: item.datasetName, status: item.status, independentIc: item.independentIc },
          labels: { datasetId: { [value(item.datasetId)]: item.datasetName ?? '数据集名称未记录' }, research: { [id]: item.researchName } } };
      }));
    }
  }
  async function refreshTicks() {
    ticksDirty = false; const taskIds = new Set(dirtyTasks); dirtyTasks.clear();
    const runs = (await host.harness.snapshot(TickEconomicReplays, context))?.runs ?? {};
    const kept = new Set<string>();
    for (const raw of Object.values(runs)) {
      const fact = raw as unknown as { original: TickEconomicReplayStart; view: TickEconomicReplayView };
      const view = fact.view; kept.add(view.id);
      const members = view.members ?? (view.factorId ? [{ factorId: view.factorId, weight: '1' }] : []);
      const sourceIds = [...new Set(members.map(member => String(factorReference(member.factorId).researchId)))];
      const sourceNames = sourceIds.map(id => researchNames.get(Number(id)) ?? '研究名称未记录');
      const recordedName = datasetName(view.datasetId, sourceIds.map(Number));
      const stamp = JSON.stringify({ id: view.id, datasetId: view.datasetId, status: view.status, members, sourceNames, datasetName: recordedName });
      if (ticks.has(view.id) && tickStamps.get(view.id) === stamp && !taskIds.has(view.taskId)) continue;
      const task = await host.harness.getTask(view.taskId as TaskId, context);
      const metadata = replayMetadata(fact, task);
      const item: TickReplayListItem = { id: view.id, name: sourceNames.length === 1 ? sourceNames[0]! : members.length > 1 ? '多因子逐笔回放' : '单因子逐笔回放',
        datasetId: view.datasetId, datasetName: recordedName, status: executionStatus(task, view.result ? 'COMPUTED' : view.status, 'QUEUED') as TickReplayListItem['status'],
        memberCount: members.length, memberType: members.length > 1 ? 'ensemble' : 'single-factor', scientificQualification: view.scientificQualification,
        createdAt: metadata.createdAt, progress: metadata.progress };
      ticks.set(item.id, { item, search: searchText(item.name, item.id, item.datasetName, ...sourceIds, ...sourceNames, ...members.map(member => member.factorId)),
        filters: { status: one(item.status), datasetId: one(item.datasetId), memberType: one(item.memberType), source: sourceIds },
        sort: { name: item.name, datasetName: item.datasetName, memberCount: item.memberCount, createdAt: item.createdAt },
        labels: { datasetId: { [item.datasetId]: item.datasetName ?? '数据集名称未记录' }, source: Object.fromEntries(sourceIds.map((id, i) => [id, sourceNames[i]!])) } });
      tickStamps.set(item.id, stamp);
    }
    for (const id of ticks.keys()) if (!kept.has(id)) { ticks.delete(id); tickStamps.delete(id); }
  }
  async function refreshAllocations() {
    allocationsDirty = false;
    const records = (await host.harness.snapshot(ReviewAllocations, context))?.records ?? {};
    allocations.clear();
    for (const raw of Object.values(records)) {
      const view = raw.view as unknown as ReviewAllocationView;
      const sources = (raw.sources ?? []) as unknown as { view: { id: string; name: string } }[];
      const sourceNames = view.portfolioIds.map(id => sources.find(source => source.view.id === id)?.view.name ?? portfolios.get(id)?.item.name ?? id);
      const task = await host.harness.getTask(view.taskId as TaskId, context);
      if (!task) throw new Problem('review_allocation_task_missing', 500);
      const item: AllocationListItem = { id: view.id, name: view.name, status: executionStatus(task, 'COMPUTED', 'QUEUED') as AllocationListItem['status'], memberCount: view.portfolioIds.length, reviewDecision: view.review?.decision ?? null, createdAt: view.createdAt };
      allocations.set(item.id, { item, search: searchText(item.name, item.id, ...view.portfolioIds, ...sourceNames), filters: { status: one(item.status), reviewDecision: one(item.reviewDecision), source: view.portfolioIds },
        sort: { name: item.name, memberCount: item.memberCount, createdAt: item.createdAt }, labels: { source: Object.fromEntries(view.portfolioIds.map((id, i) => [id, sourceNames[i]!])) } });
    }
  }
  async function refresh() {
    if (!initialized) {
      let cursor: Cursor | undefined;
      do { const page = await host.list(cursor); for (const row of page.items) { known.add(row.id); dirty.add(row.id); } cursor = page.next; } while (cursor);
      initialized = true;
    }
    do {
      if (appDirty) await refreshApplication();
      if (requestsDirty) await refreshRequests();
      if (dirty.size) { const ids = [...dirty]; dirty.clear(); await refreshResearch(ids); }
      if (ticksDirty) await refreshTicks();
      if (allocationsDirty) await refreshAllocations();
    } while (appDirty || requestsDirty || dirty.size || ticksDirty || allocationsDirty);
  }
  return {
    close: unsubscribe,
    async list(collection: Collection, query: Query): Promise<ListPage<Item>> {
      const parsed = parseQuery(collection, query);
      refreshing ??= refresh().catch(error => {
        // A failed read must never make a partial rebuild look authoritative on retry.
        initialized = false; appDirty = true; requestsDirty = true; ticksDirty = true; allocationsDirty = true;
        tickStamps.clear();
        for (const id of known) dirty.add(id);
        throw error;
      }).finally(() => { refreshing = undefined; }); await refreshing;
      const rows: Indexed[] = collection === 'research' ? [...research.values()] : collection === 'factors' ? [...factors.values()].flat() : collection === 'portfolios' ? [...portfolios.values()] : collection === 'tick-economic-replays' ? [...ticks.values()] : [...allocations.values()];
      return selectPage(collection, rows, parsed);
    },
  };
}

function parseQuery(collection: Collection, query: Query) {
  const allowed = new Set(['q', 'page', 'pageSize', 'sort', 'order', ...filterKeys[collection],
    ...(['portfolios', 'tick-economic-replays', 'review-allocations'].includes(collection) ? ['createdFrom', 'createdTo'] : []),
    ...(collection === 'portfolios' ? ['capitalMin', 'capitalMax'] : []), ...(collection === 'factors' ? ['icMin', 'icMax'] : [])]);
  if (Object.entries(query).some(([key, input]) => !allowed.has(key) || typeof input !== 'string')) throw new Problem('list_query_invalid');
  const strings = query as Record<string, string>;
  const integer = (name: string, fallback: number) => { const input = strings[name]; if (input === undefined) return fallback; if (!/^\d+$/.test(input) || !Number.isSafeInteger(Number(input)) || Number(input) < 1) throw new Problem('list_pagination_invalid'); return Number(input); };
  const page = integer('page', 1); const pageSize = integer('pageSize', 20);
  if (![20, 50, 100].includes(pageSize)) throw new Problem('list_page_size_invalid');
  const sort = strings.sort ?? (collection === 'research' || collection === 'factors' ? 'newest' : 'createdAt');
  const order = strings.order ?? 'desc';
  if (!sortKeys[collection].includes(sort) || !['asc', 'desc'].includes(order)) throw new Problem('list_sort_invalid');
  const filters = Object.fromEntries(filterKeys[collection].filter(key => strings[key]).map(key => [key, strings[key]!.split(',').filter(Boolean)]));
  if (sort === 'criterionValue' && (filters.metric?.length !== 1 || filters.metric[0] === 'unknown')) throw new Problem('list_metric_scope_required');
  if ((sort === 'capital' || strings.capitalMin || strings.capitalMax) && (filters.currency?.length !== 1 || filters.currency[0] === 'unknown')) throw new Problem('list_currency_scope_required');
  const date = (name: string, end: boolean) => {
    const input = strings[name]; if (!input) return undefined;
    const time = Date.parse(input);
    if (!Number.isFinite(time) || !/^\d{4}-\d{2}-\d{2}(?:T.*Z)?$/.test(input) || /^\d{4}-\d{2}-\d{2}$/.test(input) && new Date(time).toISOString().slice(0, 10) !== input) throw new Problem('list_date_invalid');
    return time + (end && input.length === 10 ? 86_400_000 - 1 : 0);
  };
  const decimal = (name: string) => { const input = strings[name]; if (!input) return undefined; if (!/^\d+(?:\.\d{1,18})?$/.test(input)) throw new Problem('list_capital_invalid'); return decimalUnits(input); };
  const number = (name: string) => { const input = strings[name]; if (!input) return undefined; if (!/^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(input) || !Number.isFinite(Number(input))) throw new Problem('list_ic_invalid'); return Number(input); };
  const createdFrom = date('createdFrom', false), createdTo = date('createdTo', true), capitalMin = decimal('capitalMin'), capitalMax = decimal('capitalMax'), icMin = number('icMin'), icMax = number('icMax');
  if (createdFrom !== undefined && createdTo !== undefined && createdFrom > createdTo || capitalMin !== undefined && capitalMax !== undefined && capitalMin > capitalMax || icMin !== undefined && icMax !== undefined && icMin > icMax) throw new Problem('list_range_invalid');
  return { page, pageSize, sort, order, filters, terms: normalized(strings.q ?? '').split(/\s+/u).filter(Boolean), createdFrom, createdTo, capitalMin, capitalMax, icMin, icMax };
}
function selectPage(collection: Collection, rows: Indexed[], query: ReturnType<typeof parseQuery>): ListPage<Item> {
  const facets: ListPage<Item>['facets'] = {};
  for (const key of filterKeys[collection]) {
    const options = new Map<string, string>();
    for (const row of rows) for (const input of row.filters[key] ?? []) options.set(input, row.labels?.[key]?.[input] ?? (input === 'unknown' ? key === 'criterionStatus' ? '未评价' : key === 'reviewDecision' ? '未审阅' : '未记录' : key === 'scope' ? input === 'single-factor' ? '单因子研究' : '组合研究' : labels[input] ?? input));
    facets[key] = [...options].map(([value, label]) => ({ value, label })).sort((a, b) => a.label.localeCompare(b.label, 'zh-CN', { numeric: true }));
  }
  const filtered = rows.filter(row => {
    if (!query.terms.every(term => row.search.includes(term)) || !Object.entries(query.filters).every(([key, values]) => values.some(input => row.filters[key]?.includes(input)))) return false;
    if (query.createdFrom !== undefined || query.createdTo !== undefined) { const created = typeof row.sort.createdAt === 'string' ? Date.parse(row.sort.createdAt) : NaN; if (!Number.isFinite(created) || query.createdFrom !== undefined && created < query.createdFrom || query.createdTo !== undefined && created > query.createdTo) return false; }
    if (query.capitalMin !== undefined || query.capitalMax !== undefined) { const capital = row.sort.capital; if (typeof capital !== 'bigint' || query.capitalMin !== undefined && capital < query.capitalMin || query.capitalMax !== undefined && capital > query.capitalMax) return false; }
    if (query.icMin !== undefined || query.icMax !== undefined) { const ic = row.sort.independentIc; if (typeof ic !== 'number' || query.icMin !== undefined && ic < query.icMin || query.icMax !== undefined && ic > query.icMax) return false; }
    return true;
  });
  const direction = query.order === 'asc' ? 1 : -1;
  filtered.sort((a, b) => {
    const left = a.sort[query.sort], right = b.sort[query.sort];
    if (left == null || right == null) { if (left != null) return -1; if (right != null) return 1; }
    else { const compared = typeof left === 'string' && typeof right === 'string' ? left.localeCompare(right, 'zh-CN', { numeric: true }) : left < right ? -1 : left > right ? 1 : 0; if (compared) return compared * direction; }
    return String(a.item.id).localeCompare(String(b.item.id), 'en', { numeric: true }) * direction;
  });
  const page = Math.min(query.page, Math.max(1, Math.ceil(filtered.length / query.pageSize)));
  return { items: filtered.slice((page - 1) * query.pageSize, page * query.pageSize).map(row => row.item), total: filtered.length, page, pageSize: query.pageSize, facets };
}

export function registerListRoutes(app: FastifyInstance, host: ResearchHost, config: Pick<ServiceConfig, 'datasets'>) {
  const index = listIndex(host, config);
  app.addHook('onClose', async () => { index.close(); });
  for (const collection of Object.keys(filterKeys) as Collection[]) app.get<{ Querystring: Query }>(`/api/v3/${collection}/list`, request => index.list(collection, request.query));
}

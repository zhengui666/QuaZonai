import { isNativeTable, nativeRowCount } from '../../research/src/native-table.ts';
import { NativeStrategyBacktestTask } from '../../research/plugins/native-strategy-backtest.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineDocFamily, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, Tx, Harness } from '@earendil-works/pi-durable';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { ParameterScanStart, ParameterScanGrid, ParameterScanTrial, ParameterScanView, ParameterScanSourceView, ParameterScanCounts } from '../../../packages/contracts/src/parameter-scans.ts';
import type { ListPage } from '../../../packages/contracts/src/index.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { NativeRemoteUncertainty, cancelUnsettledNativeExecutions } from '../../research/src/native-execution.ts';
import { reconcileNativeRemoteTasks } from '../../research/src/native-remote-receipts.ts';
import { object, Problem } from './facts.ts';
import type { ServiceConfig } from './config.ts';
import { eventStream } from './event-stream.ts';
const context = BACKGROUND_CONTEXT;
type ScanTrial = ParameterScanTrial;
type ScanIndex = { conversationId: number; taskId: number; requestId: string; createdAt: string };
type ScanFact = { original: ParameterScanStart; sourceView: ParameterScanSourceView; grid: ParameterScanGrid; trials: Record<string, ScanTrial>; createdAt: string; updatedAt: string; error: string | null; stoppedAt: string | null; stopTaskId?: number };
const retired = (): never => { throw new Problem('parameter_scan_retired_use_explicit_native_strategy', 410); };
function positiveId(value: unknown, name: string): number {
 const parsed = typeof value === 'string' && /^[1-9]\d*$/.test(value) ? Number(value) : value;
 if (typeof parsed !== 'number' || !Number.isSafeInteger(parsed) || parsed < 1) throw new Problem(`${name}_invalid`);
 return parsed;
}
export function sampleCounts(value: JsonObject): Record<string, string> {
 const report = object(value.canonical_result, 'native_strategy_report');
 return Object.fromEntries(['orders','positions','fills'].flatMap(key => Array.isArray(report[key]) || isNativeTable(report[key]) ? [[key,String(nativeRowCount(report[key]))]] : []));
}
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

async function scanFact(tx: Tx, conversationId: ConversationId, id: string) {
 const record = await tx.doc(ParameterScanRecords, conversationId, id, null);
 if (!record.value) throw new Problem('parameter_scan_not_found', 404);
 return factValue(record.value);
}
// Keep only the original task identity and abort hook. Recovery never dispatches
// another point, recreates a grid, scores a result, or chooses a winner.
export const ParameterScanTask = defineTask<{id:string}, {phase:'dispatch'|'collect'}, null>({
 name:'qz.parameter-scan', version:1, initial:()=>({phase:'dispatch'}),
 phases:{dispatch:retired,collect:retired},
 abort: (_task,runtime,ctx)=>runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx),
});
export function createParameterScanExtension(harness: () => Harness) {
  const StopTask = defineTask<{ id: string; parentTaskId: number }, { phase: 'stop' } | { phase: 'reconcile' }, null>({
    name: 'qz.parameter-scan.stop', version: 1, initial: () => ({ phase: 'stop' }),
    phases: {
      async stop(task, runtime, ctx) {
        const factJson=(await runtime.snapshot(ParameterScanRecords,runtime.conversationId,task.input.id,ctx))?.value;
        if(!factJson)throw new Error('parameter_scan_not_found');
        const ids=[...new Set([task.input.parentTaskId,...Object.values(factValue(factJson).trials).flatMap(row=>row.taskId===null?[]:[row.taskId])])];
        // A terminal parent is a no-op to official abortTask: explicitly mark retained children too.
        for(const id of ids){
          await harness().abortTask(id as TaskId,ctx);
          const child=await harness().getTask(id as TaskId,ctx);
          if(child?.state.status==='terminal')await cancelUnsettledNativeExecutions(id as TaskId,runtime,ctx);
        }
        await runtime.commit(() => ({ status: 'waiting', checkpoint: { phase: 'reconcile' }, on: ids as TaskId[], policy: 'allSettled' }), ctx);
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

export function parameterScanWorkbench(host: ResearchHost, _config: ServiceConfig, stopTask: ReturnType<typeof createParameterScanExtension>['stopTask']) {
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
 // Read retained remote facts only. Historical GETs never poll, adopt, or cancel.
 async function retainedTrials(id:string){
  const original=await read(id);
  const rows=await Promise.all(Object.values(original.fact.trials).map(async row=>{
   const remote=row.taskId===null?null:await host.harness.snapshot(NativeRemoteUncertainty,original.index.conversationId as ConversationId,String(row.taskId),context);
   const statuses=remote?.requests.map(item=>String(item.status))??[];
   const uncertain=statuses.find(status=>['UNKNOWN','RUNNING','PENDING'].includes(status));
   const child=row.taskId===null?null:await host.harness.getTask(row.taskId as TaskId,context);
   const outcome=child?.state.status==='terminal'?child.state.outcome:null;
   // Stored terminal business outcomes are original facts, not scheduler success.
   // An unresolved remote operation still takes precedence over any local terminal state.
   const retainedTerminal = ['COMPLETED','FAILED','STOPPED','SKIPPED'].includes(row.status);
   const result = outcome?.status === 'completed' ? outcome.result as JsonObject | null : null;
   const canonical = result?.canonical_result as JsonObject | undefined;
   const nativeRun = canonical?.run as JsonObject | undefined;
   const status:ScanTrial['status']=uncertain?'RECONCILING':retainedTerminal?row.status:nativeRun?.outcome==='stopped'?'STOPPED':outcome?.status==='completed'?'COMPLETED':outcome?.status==='aborted'?'STOPPED':outcome?'FAILED':child?.state.status==='pending'?'PENDING':child?'RUNNING':row.status;
   return {...row,status,remoteStatus:statuses.length ? uncertain ?? (statuses.every(status=>status===statuses[0]) ? statuses[0]! : null) : row.remoteStatus};
  }));
  return {...original,rows};
 }
 async function get(id:string):Promise<ParameterScanView>{
 const {index,fact,task,rows}=await retainedTrials(id);
 const counts:ParameterScanCounts={total:fact.grid.total,eligible:fact.grid.eligible,skipped:fact.grid.skipped,completed:'0',failed:'0',stopped:'0',running:'0',pending:'0',unknown:'0',comparable:'0'};
 // Administrative row counts only; no parameter expansion or metric ranking.
 for(const row of rows){
  if(row.status==='SKIPPED')continue; // The retained grid already carries skipped count.
  const key=row.status==='COMPLETED'?'completed':row.status==='FAILED'?'failed':row.status==='STOPPED'?'stopped':row.status==='RECONCILING'?'unknown':row.status==='PENDING'?'pending':'running';
  counts[key]=String(BigInt(counts[key])+1n);
 }
 const unsettled=rows.some(row=>row.remoteStatus!==null&&['UNKNOWN','PENDING','RUNNING'].includes(row.remoteStatus));
 counts.unknown=String(rows.filter(row=>row.remoteStatus!==null&&['UNKNOWN','PENDING','RUNNING'].includes(row.remoteStatus)).length);
 const outcome=task.state.status==='terminal'?task.state.outcome:null;
 const activeChild=rows.some(row=>['PENDING','RUNNING','RECONCILING'].includes(row.status));
 const status:ParameterScanView['status']=unsettled||activeChild?(fact.stoppedAt||task.abortRequested?'STOPPING':'RECONCILING'):outcome?.status==='completed'?'COMPLETED':outcome?.status==='aborted'?(unsettled?'STOPPING':'STOPPED'):outcome?'FAILED':task.abortRequested?'STOPPING':'RECONCILING';
 return {id,requestId:fact.original.requestId,name:fact.original.name,source:fact.original.source,sourceView:fact.sourceView,input:fact.original,mode:fact.original.mode,taskId:index.taskId,createdAt:fact.createdAt,updatedAt:fact.updatedAt,status,execution:fact.original.execution,objective:fact.original.objective,grid:fact.grid,counts,best:null,error:fact.error,scientificQualification:'UNQUALIFIED_EXPLORATORY'};
 }
 async function trials(id:string,query:Record<string,unknown>={}):Promise<ListPage<ParameterScanTrial>>{
 if(query.sort==='metric')return retired();
 let {rows}=await retainedTrials(id);
 if(typeof query.status==='string')rows=rows.filter(row=>row.status===query.status);
 return page(rows,query,{});
 }
 async function trial(id:string,ordinal:string):Promise<ParameterScanTrial>{
 const {rows}=await retainedTrials(id);const row=rows.find(row=>row.ordinal===ordinal);if(!row)throw new Problem('parameter_scan_trial_not_found',404);
 if(row.taskId===null)return row;
 const child=await host.harness.getTask(row.taskId as TaskId<JsonObject>,context);
 return child?.state.status==='terminal'&&child.state.outcome.status==='completed'?{...row,details:{...row.details,evidenceTaskId:row.taskId,nativeResult:child.state.outcome.result}}:row;
 }
  async function stop(id: string): Promise<ParameterScanView> {
    const { index } = await read(id);
    await host.harness.commit(async tx => {
      const fact = await scanFact(tx, index.conversationId as ConversationId, id);

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
 return {get,stop,list,trials,trial};
}
function page<T>(rows: T[], query: Record<string, unknown>, facets: ListPage<T>['facets']): ListPage<T> {
  const page = query.page === undefined ? 1 : positiveId(query.page, 'parameter_scan_page');
  const pageSize = query.pageSize === undefined ? 25 : positiveId(query.pageSize, 'parameter_scan_page_size');
  if (pageSize > 200 || !Number.isSafeInteger((page - 1) * pageSize)) throw new Problem('parameter_scan_page_invalid');
  return { items: rows.slice((page - 1) * pageSize, page * pageSize), total: rows.length, page, pageSize, facets };
}
export function registerParameterScanRoutes(app: FastifyInstance, host: ResearchHost, work: ReturnType<typeof parameterScanWorkbench>, streams: Set<() => Promise<void>>) {
  const root = `${API}/parameter-scans`;
  app.get<{ Querystring: Record<string, unknown> }>(root, request => work.list(request.query));
  app.get<{ Querystring: Record<string, unknown> }>(`${root}/list`, request => work.list(request.query));
  app.get<{ Params: { id: string } }>(`${root}/:id`, request => work.get(request.params.id));
  app.get<{ Params: { id: string }; Querystring: Record<string, unknown> }>(`${root}/:id/trials`, request => work.trials(request.params.id, request.query));
  app.get<{ Params: { id: string; ordinal: string } }>(`${root}/:id/trials/:ordinal`, request => work.trial(request.params.id, request.params.ordinal));
  app.post<{ Params: { id: string } }>(`${root}/:id/stop`, async (request, reply) => reply.code(202).send(await work.stop(request.params.id)));
  app.get<{ Params: { id: string } }>(`${root}/:id/events`, async (request, reply) => {
    const view = await work.get(request.params.id);
    const [document, graph] = await Promise.all([host.harness.watchDoc(ParameterScanRecords, view.sourceView.researchId as ConversationId, request.params.id, context), host.harness.watchTaskGraph(context)]);
    if (!document) { await graph.stop(); throw new Problem('parameter_scan_not_found', 404); }
    await eventStream(reply, 'parameter-scan', () => work.get(request.params.id), [document, graph], streams);
  });
}

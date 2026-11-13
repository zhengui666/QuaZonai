import assert from 'node:assert/strict';
import test from 'node:test';
import * as contracts from '../../../packages/contracts/src/parameter-scans.ts';
import { registerParameterScanRoutes, parameterScanWorkbench, ParameterScanTask, ParameterScans, ParameterScanRecords, createParameterScanExtension } from '../src/parameter-scans.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import type { ServiceConfig } from '../src/config.ts';

test('retired grid helpers have no executable exports', () => {
 for(const key of ['normalizeParameterScan','parameterScanPoint','parameterScanAxisValue','assertDistinctNativeNumbers','parameterScanPlan','parameterScanStart'])assert.equal(key in contracts,false);
});
test('new scan/preview/surface routes and workbench entry points are absent',async()=>{
 const host={harness:new Proxy({}, {get(){throw Error('unexpected_storage_or_dispatch');}})} as unknown as ResearchHost;
 const work=parameterScanWorkbench(host,{} as ServiceConfig,createParameterScanExtension(()=>host.harness).stopTask);
 for(const key of ['start','preview','surface','options'])assert.equal(key in work,false);
 const routes:string[]=[];
 registerParameterScanRoutes({get:(path:string)=>routes.push(`GET ${path}`),post:(path:string)=>routes.push(`POST ${path}`)} as never,host,work,new Set());
 assert.equal(routes.includes('POST /api/v3/parameter-scans'),false);
 assert.equal(routes.some(path=>/preview|surface|options/.test(path)),false);
 await assert.rejects(()=>work.trials('old',{sort:'metric'}),/parameter_scan_retired/);
});
test('original scan phases are retired without creating any candidate',async()=>{
 for(const phase of ['dispatch','collect'] as const)assert.throws(()=>ParameterScanTask.definition.phases[phase]({} as never,{} as never,{} as never),/parameter_scan_retired/);
});
test('historical point reads return stored parameters and original native evidence only',async()=>{
 const row={ordinal:'0',parameters:{'strategy.period':'7'},taskId:12,status:'COMPLETED',metric:null,metrics:[],sampleCounts:null,error:null,remoteStatus:null};
 const fact={original:{name:'old'},grid:{axes:[]},trials:{'0':row}};
 let writes=0;
 const native={canonical_result:{orders:[{original:true}]}};
 const host={harness:{
  snapshot:async(doc:unknown)=>doc===ParameterScans?{runs:{old:{conversationId:9,taskId:10}}}:doc===ParameterScanRecords?{value:fact}:undefined,
  getTask:async(id:number)=>id===10?{kind:'qz.parameter-scan',state:{status:'terminal',outcome:{status:'completed'}}}:{state:{status:'terminal',outcome:{status:'completed',result:native}}},
  commit:async()=>{writes++;throw Error('no_history_writes');}
 }} as unknown as ResearchHost;
 const work=parameterScanWorkbench(host,{} as ServiceConfig,createParameterScanExtension(()=>host.harness).stopTask);
 const result=await work.trial('old','0');assert.deepEqual(result.parameters,row.parameters);assert.deepEqual(result.details?.nativeResult,native);assert.equal(writes,0);
 await assert.rejects(()=>work.trial('old','999'),/parameter_scan_trial_not_found/);
});

test('history projects stored child and remote terminal facts without writes or polling',async()=>{
 const row={ordinal:'0',parameters:{},taskId:12,status:'PENDING',remoteStatus:'UNKNOWN',metrics:[]};
 const fact={original:{name:'old',requestId:'original'},grid:{total:'1',eligible:'1',skipped:'0'},trials:{'0':row}};
 let remoteStatus='UNKNOWN'; let outcome='completed';
 const host={harness:{
  snapshot:async(doc:unknown)=>doc===ParameterScans?{runs:{old:{conversationId:9,taskId:10}}}:doc===ParameterScanRecords?{value:fact}:{requests:[{status:remoteStatus}]},
  getTask:async(id:number)=>id===10?{kind:'qz.parameter-scan',state:{status:'terminal',outcome:{status:'aborted'}}}:{state:{status:'terminal',outcome:{status:outcome,result:{}}}},
  commit:async()=>{throw Error('no_history_writes');}
 }} as unknown as ResearchHost;
 const work=parameterScanWorkbench(host,{} as ServiceConfig,createParameterScanExtension(()=>host.harness).stopTask);
 assert.equal((await work.trial('old','0')).status,'RECONCILING');
 assert.equal((await work.get('old')).status,'RECONCILING');
 remoteStatus='COMPLETED';
 assert.equal((await work.trial('old','0')).status,'COMPLETED');
 assert.equal((await work.trial('old','0')).remoteStatus,'COMPLETED');
 assert.equal((await work.get('old')).counts.completed,'1');
 assert.equal((await work.get('old')).counts.unknown,'0');
 assert.equal((await work.get('old')).status,'STOPPED');
 remoteStatus='CANCELLED';outcome='aborted';
 assert.equal((await work.trial('old','0')).status,'STOPPED');
 assert.equal((await work.trial('old','0')).remoteStatus,'CANCELLED');
 assert.equal(row.status,'PENDING');assert.equal(row.remoteStatus,'UNKNOWN');
});

test('stop explicitly aborts retained children even when original parent is terminal',async()=>{
 const marked:number[]=[];let waiting:unknown;
 const extension=createParameterScanExtension(()=>({abortTask:async(id:number)=>{marked.push(id);},getTask:async()=>null}) as never);
 await extension.stopTask.definition.phases.stop({input:{id:'old',parentTaskId:10}} as never,{
  conversationId:9,snapshot:async()=>({value:{trials:{a:{taskId:12},b:{taskId:13},c:{taskId:12}}}}),
  commit:async(fn:()=>unknown)=>{waiting=fn();}
 } as never,{} as never);
 assert.deepEqual(marked,[10,12,13]);
 assert.deepEqual(waiting,{status:'waiting',checkpoint:{phase:'reconcile'},on:[10,12,13],policy:'allSettled'});
});

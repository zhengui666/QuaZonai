import assert from 'node:assert/strict';
import test from 'node:test';
import { defineTask, defineExtension, type JsonObject } from '@earendil-works/pi-durable';
import { ParameterScans, ParameterScanRecords, ParameterScanTask } from '../src/parameter-scans.ts';
import { NativeRemoteUncertainty } from '../../research/src/native-execution.ts';
import { setup, context } from './support.ts';

// Official Harness/SQLite persistence. These retained protocol facts are not market evidence.
const Done = defineTask<JsonObject, {phase:'done'}, JsonObject>({
 name:'test.scan.original-result', version:1, initial:()=>({phase:'done'}),
 phases:{done:(task,runtime,ctx)=>runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result:task.input}}),ctx)},
 abort:(_task,runtime,ctx)=>runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx),
});

test('new scan creation, preview and options remain unavailable', async()=>{
 const state=await setup();
 try{
  for(const [method,url] of [['POST','/api/v3/parameter-scans'],['POST','/api/v3/parameter-scans/preview'],['GET','/api/v3/parameter-scans/options']] as const){
   const response=await state.service.app.inject({method,url,headers:state.headers,...(method==='POST'?{payload:{}}:{})});
   assert.equal(response.statusCode,404,response.body);
  }
 }finally{await state.cleanup();}
});

test('historical scan retains terminal business outcomes, raw metrics and UNKNOWN without ranking or re-execution',async()=>{
 const state=await setup();
 state.options.extensions.push(defineExtension({name:'test.scan.history',tasks:[Done]}));
 await state.restart();
 try{
  const harness=state.service.research.harness;const owner=await harness.root(context);
  const raw:JsonObject={canonical_result:{run:{outcome:'stopped'},statistics:{returns:{'Sharpe Ratio (252 days)':'4014000000000000'}}},equity_performance:{status:'UNAVAILABLE',returns:[]}};
  const child=await owner.commit(tx=>tx.createTask(Done,raw,{ownership:{kind:'conversation'}}),context);
  await harness.waitForTask(child,context);
  const parent=await owner.commit(tx=>tx.createTask(ParameterScanTask,{id:'history'},{ownership:{kind:'conversation'}}),context);
  await harness.waitForTask(parent,context);
  const metric={key:'Sharpe Ratio (252 days)',value:null,reason:'native_run_stopped_before_completion'};
  const scientific={metric,metrics:[{key:'Sharpe Ratio (252 days)',value:5}],curve:[{time:'9007199254740993',value:'5000.000000000000000001'}],sampleCounts:{orders:'20'},details:{nativeRun:{outcome:'stopped'},dataRowsByType:{QuoteTick:'20'}}};
  const trials:JsonObject={};
  for(const [ordinal,status] of [['0','STOPPED'],['1','RUNNING'],['2','FAILED'],['3','SKIPPED']])trials[ordinal!]={ordinal:ordinal!,parameters:{fast_period:'5'},taskId:child,status:status!,remoteStatus:'COMPLETED',error:null,...scientific};
  await owner.commit(async tx=>{
   (await tx.doc(ParameterScans)).runs.history={conversationId:owner.id,taskId:parent,requestId:'original',createdAt:'2026-01-01T00:00:00Z'};
   (await tx.doc(ParameterScanRecords,owner.id,'history',null)).value={original:{requestId:'original',name:'Historical',source:{kind:'RESEARCH',researchId:owner.id},mode:'GRID',execution:{concurrency:1},objective:{metricId:'Sharpe Ratio (252 days)',direction:'maximize'}},sourceView:{researchId:owner.id,label:'Original',datasetId:null,datasetName:null},grid:{total:'4',eligible:'4',skipped:'0'},trials,createdAt:'2026-01-01T00:00:00Z',updatedAt:'2026-01-01T00:00:00Z',error:null,stoppedAt:null};
  },context);
  const taskIds = (await harness.commit(tx=>tx.scanTasks({},100),context)).items.map(task=>task.id);
  for(const reopen of [false,true]){
   if(reopen)await state.restart();
   const get=async(path:string)=>state.service.app.inject({method:'GET',url:'/api/v3/parameter-scans/history'+path,headers:state.headers});
   const view=await get('');assert.equal(view.statusCode,200,view.body);assert.equal(view.json().best,null);assert.equal(view.json().counts.stopped,'2');assert.equal(view.json().counts.running,'0');
   const list=await get('/trials');assert.deepEqual(list.json().items.map((row:any)=>row.status),['STOPPED','STOPPED','FAILED','SKIPPED']);
   assert.equal((await get('/trials?sort=metric')).statusCode,410);
   const detail=await get('/trials/0');assert.equal(detail.statusCode,200,detail.body);
   const row=detail.json();assert.deepEqual(row.metric,metric);assert.deepEqual(row.metrics,scientific.metrics);assert.deepEqual(row.curve,scientific.curve);
   assert.deepEqual(row.details.nativeResult,raw);assert.deepEqual(row.details.nativeRun,{outcome:'stopped'});
   const stored=await state.service.research.harness.snapshot(ParameterScanRecords,owner.id,'history',context);
   assert.deepEqual(stored?.value?.trials,trials,'GET and restart do not overwrite original facts');
   assert.deepEqual((await state.service.research.harness.commit(tx=>tx.scanTasks({},100),context)).items.map(task=>task.id),taskIds,'history reads never create replacement work');
  }
  await state.service.research.harness.commit(async tx=>{
   (await tx.doc(NativeRemoteUncertainty,owner.id,String(child),null)).requests=[{taskId:child,requestId:'retained-request',target:{id:'old',url:'http://127.0.0.1:1'},status:'UNKNOWN'}];
  },context);
  const pending=await state.service.app.inject({method:'GET',url:'/api/v3/parameter-scans/history/trials',headers:state.headers});
  assert.equal(pending.statusCode,200,pending.body);
  assert.ok(pending.json().items.every((row:any)=>row.status==='RECONCILING'&&row.remoteStatus==='UNKNOWN'));
  assert.equal((await state.service.research.harness.snapshot(NativeRemoteUncertainty,owner.id,String(child),context))?.requests[0]?.status,'UNKNOWN');
 }finally{await state.cleanup();}
});

import assert from 'node:assert/strict';
import {test} from 'node:test';
import {prepareNativeMarketImport,prepareNativeFillObservationImport} from '../plugins/native-market-import.ts';

test('import tool consumes generated original contract without coercing timestamps or allowing bar/resume options',()=>{
  const request={schema_version:1,manifest_path:'/source/manifest.json',instruments_path:'/source/instruments.json',output_path:'/new/catalog-output',start_seconds:'1778284800',end_seconds:'1778288400',received_time:true,maximum_receive_delay_ns:'18446744073709551615',path_prefixes:['training/']};
  assert.deepEqual(prepareNativeMarketImport(request),request);
  assert.throws(()=>prepareNativeMarketImport({...request,maximum_receive_delay_ns:null}),/RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY/);
  assert.throws(()=>prepareNativeMarketImport({...request,received_time:false}),/RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY/);
  for(const extra of [{path_prefixes:['BTC_trades.parquet']},{path_prefixes:['../outside/']},{path_prefixes:['/absolute/']},{bar_seconds:1},{resume:true},{start_seconds:1778284800},{maximum_receive_delay_ns:18446744073709551615},{schema_version:2}]) {
    assert.throws(()=>prepareNativeMarketImport({...request,...extra}));
  }
});

import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {exposureReadTool} from '../plugins/research-assessment.ts';
import {nativeToolText} from '../src/native-result-view.ts';

test('oversized preparation metadata has a working same-conversation summary reader, not a backtest table reader',async()=>{
  // Transport-only metadata, not synthetic market observations or a scientific import claim.
  const result={schema_version:1,research_qualified:false,report:{native_files:Array.from({length:1000},(_,i)=>({path:`/declared/output/${i}/${'p'.repeat(80)}`}))}};
  const hint=JSON.parse(nativeToolText(result,7));assert.equal(hint.reader,'native_assessment_read_exposure');
  let conversationId=1;
  const api={conversationId:1,agent:async()=>({model:{provider:'test',modelId:'metadata'}}),models:{getModel:()=>({contextWindow:200000})},
    commit:async(fn:any)=>fn({conversation:async()=>({owner:null}),task:async()=>({conversationId,kind:'qz.native-market-import',state:{status:'terminal',outcome:{status:'completed',result}}}),doc:async()=>({pages:[],runs:[]})})};
  const read=await exposureReadTool.execute({taskId:7,path:'$.summary/report/native_files',offset:0,pageSize:3},api as any,context);
  const page=JSON.parse((read.output![0] as any).text);assert.equal(page.rows.length,3);assert.equal(page.nextOffset,3);
  assert.equal(page.rows[0].summaryPath,'$.summary/report/native_files/0');
  await assert.rejects(()=>exposureReadTool.execute({taskId:7,path:'$.canonical_result.orders',offset:0,pageSize:3},api as any,context),/assessment_evidence_task_unavailable/);
  conversationId=2;
  await assert.rejects(()=>exposureReadTool.execute({taskId:7,path:'$.summary',offset:0,pageSize:3},api as any,context),/assessment_evidence_task_unavailable/);
});

import {createModels,fauxProvider,fauxAssistantMessage,fauxToolCall} from '@earendil-works/pi-ai';
import {openResearch} from '../src/host.ts';
import {MemoryStorage} from '@earendil-works/pi-durable';

test('ordinary intake exposes the import tool and preserves native capability rejection',async()=>{
  const models=createModels(),faux=fauxProvider();models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_market_import',{schema_version:1,manifest_path:'/source/manifest.json',instruments_path:'/source/instruments.json',output_path:'/new/catalog-output'}),{stopReason:'toolUse'}),fauxAssistantMessage('Capability is not available; no import was started.')]);
  const host=await openResearch({database:'/tmp/import-capability.sqlite',storage:new MemoryStorage(),models});
  try {
    const admission=await host.start('import-tool-intake',{goal:'Check explicit local preparation capability only.',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:'/source/catalog',nautilusTarget:{kind:'RUST_PROCESS',id:'no-import',binary:'/not-started',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'capability-only',precision:'high'},research:{singleStrategyRegistration:true,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}}},policy:{}});
    await (await host.harness.submission(admission.submissionId,context))!.wait(context);
    const tasks=await host.harness.commit(tx=>tx.scanTasks({conversationId:admission.conversationId},100),context);
    const entries=await host.harness.commit(tx=>tx.scanEntries({conversationId:admission.conversationId},100),context);
    const encoded=JSON.stringify(entries);
    assert.match(encoded,/native_market_import_not_available/);
    assert.doesNotMatch(encoded,/unknown tool|tool_not_found/i);
    assert.equal(tasks.items.filter(t=>t.kind==='qz.native.execution').length,0);
  }finally{await host.close();}
});

test('fill observation request preserves explicit provenance and timezone choice, without research or trade synthesis flags',()=>{
  const request={schema_version:1,source_path:'/source/training.parquet',source_revision:'declared-source-revision',output_path:'/output/new',assume_source_local_time_utc:false};
  assert.deepEqual(prepareNativeFillObservationImport(request),request);
  for(const extra of [{bar_seconds:1},{deduplicate:true},{instruments_path:'/source/current.json'},{assume_source_local_time_utc:'true'}])assert.throws(()=>prepareNativeFillObservationImport({...request,...extra}));
  const {source_revision,...missing}=request;assert.throws(()=>prepareNativeFillObservationImport(missing));
});

import {nativeRunnerMarketImportTool} from '../plugins/native-market-import.ts';
test('preparation import reuses original child after target revision changes and never changes study binding',async()=>{
  let resolutions=0,created=0;
  const target={kind:'RUST_PROCESS',id:'prepared',binary:'/not-executed',capabilities:{preparation:{marketImport:true}}} as any;
  const tool=nativeRunnerMarketImportTool(async(id,revision)=>{resolutions++;assert.equal(id,'built');assert.equal(revision,2);if(resolutions>1)throw new Error('revision_changed');return target;});
  const request={schema_version:1,manifest_path:'/source/manifest.json',instruments_path:'/source/instruments.json',output_path:'/new/catalog'};
  const args={runnerBuildRequestId:'built',expectedRunnerRevision:2,request};
  const child={taskId:null as number|null};const original={data:{catalog:'/old/catalog'},mode:undefined as string|undefined};
  const api={taskId:5,conversationId:1,snapshot:async()=>({input:original}),commit:async(fn:any)=>fn({doc:async()=>child,createTask:async(def:any,input:any)=>{created++;assert.equal(def.definition.name,'qz.native-market-import');assert.deepEqual(input,{target,request,command:'native-market-import'});return 6;}}),waitForTask:async()=>({state:{outcome:{status:'failed',error:{message:'original_import_failure'}}}})};
  await assert.rejects(tool.execute(args,api as any,context),/original_import_failure/);
  await assert.rejects(tool.execute(args,api as any,context),/original_import_failure/);
  assert.equal(resolutions,1);assert.equal(created,1);assert.deepEqual(original.data,{catalog:'/old/catalog'});
  original.mode='LITERATURE_ONLY';await assert.rejects(tool.execute(args,api as any,context),/literature_only_native_execution_forbidden/);
});
test('preparation target rejection creates no import and keeps generated request validation',async()=>{
  let created=0;
  const tool=nativeRunnerMarketImportTool(async()=>{throw new Error('native_build_not_completed');});
  const api={taskId:1,conversationId:1,snapshot:async()=>({input:{data:{}}}),commit:async(fn:any)=>fn({doc:async()=>({taskId:null}),createTask:async()=>{created++;}})};
  const args={runnerBuildRequestId:'failed',expectedRunnerRevision:2,request:{schema_version:1,manifest_path:'/source/manifest.json',instruments_path:'/source/instruments.json',output_path:'/new/catalog'}};
  await assert.rejects(tool.execute(args,api as any,context),/native_build_not_completed/);assert.equal(created,0);
  assert.throws(()=>tool.prepareArguments!({...args,request:{...args.request,received_time:true}}),/RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY/);
});

import {researchTaskEvidence} from '../../service/src/research-assessments.ts';
test('owner import evidence stays research-scoped and bounds large completed reports',async()=>{
 const task={kind:'qz.native-market-import',conversationId:1,id:7,input:{request:{schema_version:1}},state:{status:'sleeping',checkpoint:{phase:'reconcile',nativeTaskId:8}}} as any;
 const host={harness:{commit:async(fn:any)=>fn({task:async()=>task,doc:async()=>({input:{},deletedAt:null})})}} as any;
 assert.deepEqual((await researchTaskEvidence(host,1,7)).state,task.state);
 await assert.rejects(researchTaskEvidence(host,2,7),/research_evidence_not_found/);
 task.state={status:'terminal',outcome:{status:'completed',result:{report:{native_files:Array.from({length:1000},()=>({path:'p'.repeat(100)}))}}}};
 const bounded=await researchTaskEvidence(host,1,7);assert.equal(bounded.state.outcome.result.reader,'native_assessment_read_exposure');
 assert.ok(JSON.stringify(bounded).length<10000);
 task.kind='pi.tool';await assert.rejects(researchTaskEvidence(host,1,7),/research_evidence_not_found/);
});

import assert from 'node:assert/strict';
import {test} from 'node:test';
import {prepareNativeMarketImport} from '../plugins/native-market-import.ts';

test('import tool consumes generated original contract without coercing timestamps or allowing bar/resume options',()=>{
  const request={schema_version:1,manifest_path:'/source/manifest.json',instruments_path:'/source/instruments.json',output_path:'/new/catalog-output',start_seconds:'1778284800',end_seconds:'1778288400',received_time:true,maximum_receive_delay_ns:'18446744073709551615',path_prefixes:['training/']};
  assert.deepEqual(prepareNativeMarketImport(request),request);
  assert.throws(()=>prepareNativeMarketImport({...request,maximum_receive_delay_ns:null}),/RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY/);
  assert.throws(()=>prepareNativeMarketImport({...request,received_time:false}),/RECEIVED_TIME_REQUIRES_MAXIMUM_DELAY/);
  for(const extra of [{bar_seconds:1},{resume:true},{start_seconds:1778284800},{maximum_receive_delay_ns:18446744073709551615},{schema_version:2}]) {
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

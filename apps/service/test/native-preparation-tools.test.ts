import {AssistantEntry, ToolTask, configure} from '@earendil-works/pi-durable';
import {createService} from '../src/service.ts';
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider,fauxAssistantMessage,fauxToolCall} from '@earendil-works/pi-ai';
import {openResearch} from '../../research/src/host.ts';
import {Study,Requests} from '../../research/src/documents.ts';
import {configDefaults} from '../src/config.ts';
import {instanceSettings} from '../src/instance-settings.ts';
import {nativeEnvironmentBuild} from '../src/native-environment-build.ts';
import {nativePreparationTools} from '../src/native-preparation-tools.ts';
import {workbench} from '../src/workbench.ts';
import {Application} from '../src/facts.ts';

// Real Harness tasks/admission/SQLite with a controlled compiler, not Rust execution.
test('Pi preparation preserves original build and explicitly admits a new bound study without rebinding old studies',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-pi-prepare-'));
 const config=configDefaults({dataDir:dir,datasets:[{id:'fixture',name:'Protocol only',assetClass:'TEST',currency:'USD',source:'controlled',start:null,end:null,ready:true,data:{catalog:dir},policy:{}}]});
 let count=0;const builder=nativeEnvironmentBuild(config,async options=>{assert.equal(options.optimizationLevel,2);return {executable:join(dir,`runner-${++count}`),manifestPath:join(dir,'Cargo.toml'),qualification:'COMPILED_NOT_CONNECTED',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'standard'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};});
 const models=createModels(),faux=fauxProvider({tokensPerSecond:100000});models.setProvider(faux.provider);
 let operations!:Parameters<typeof nativePreparationTools>[0];
 const preparation=nativePreparationTools({configuration:()=>operations.configuration(),build:v=>operations.build(v),read:v=>operations.read(v),select:v=>operations.select(v),startResearch:v=>operations.startResearch(v)});
 const options={database:join(dir,'research.sqlite'),models,extensions:[builder.extension,preparation]};
 let host=await openResearch(options);let settings=await instanceSettings(config);
 const connect=()=>{const builds=builder.workbench(host,settings);const work=workbench(host,config,builds.researchTarget);operations={configuration:builds.configuration,build:builds.start,read:builds.read,select:builds.select,startResearch:work.startResearch};return{builds,work};};
 let {builds,work}=connect();
 const model={provider:faux.provider.id,modelId:faux.getModel().id};
 try{
  await host.harness.commit(async tx=>{(await tx.doc(Application)).settings={defaultModel:null,thinkingLevel:null,serviceTier:null};},context);
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/controlled/cargo',manifestPath:join(dir,'Cargo.toml'),packageName:'owner',backtestFactory:'factory'}});
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_runner_preparation_status',{}),{stopReason:'toolUse'}),fauxAssistantMessage(fauxToolCall('native_runner_build',{requestId:'original-build',expectedRevision:1,optimizationLevel:2}),{stopReason:'toolUse'}),fauxAssistantMessage('Build task accepted, not yet claimed complete.')]);
  const old=await host.start('preparation-parent',{goal:'Explicit preparation only',model,data:{catalog:dir},policy:{}});
  await (await host.harness.submission(old.submissionId,context))!.wait(context);
  const built=await builds.read('original-build');await host.harness.waitForTask(built.taskId,context);
  assert.equal(count,1);
  const settingsBefore=settings.view(), datasetsBefore=structuredClone(config.datasets);
  const scopedBody={requestId:'scoped-without-global-selection',runnerBuildRequestId:'original-build',expectedRunnerRevision:1,goal:'New scoped discussion only',datasetId:'fixture',model};
  faux.setResponses([
   fauxAssistantMessage(fauxToolCall('native_strategy_brief',{}),{stopReason:'toolUse'}),
   transcript=>{const message=transcript.messages.find(m=>m.role==='toolResult'&&m.toolName==='native_strategy_brief');assert.ok(message?.role==='toolResult');assert.equal(message.isError,false);const text=message.content.filter(p=>p.type==='text').map(p=>p.text).join('');assert.equal(JSON.parse(text).datasetId,'fixture');return fauxAssistantMessage('Scoped study, no native execution.');}
  ]);
  const scoped=await work.startResearch(scopedBody);await (await host.harness.submission(scoped.submissionId,context))!.wait(context);
  const scopedInput=(await host.harness.snapshot(Study,scoped.conversationId,context))!.input!;
  assert.equal((scopedInput.data.nautilusTarget as any).binary,join(dir,'runner-1'));
  assert.deepEqual(settings.view(),settingsBefore);assert.deepEqual(config.datasets,datasetsBefore);assert.equal(config.nativeProcess,undefined);
  assert.equal((await host.harness.snapshot(Study,old.conversationId,context))!.input!.data.nautilusTarget,undefined);
  await assert.rejects(work.startResearch({...scopedBody,requestId:'wrong-scoped-revision',expectedRunnerRevision:0}),/revision_conflict/);
  const conversation=await host.harness.conversation(old.conversationId,context);assert.ok(conversation);
  faux.setResponses([
   fauxAssistantMessage(fauxToolCall('native_runner_build_status',{requestId:'original-build'}),{stopReason:'toolUse'}),
   fauxAssistantMessage(fauxToolCall('native_runner_build',{requestId:'original-build',expectedRevision:1,optimizationLevel:2}),{stopReason:'toolUse'}),
   fauxAssistantMessage(fauxToolCall('native_runner_select',{requestId:'original-build',expectedRevision:1}),{stopReason:'toolUse'}),
   fauxAssistantMessage('Explicit runner selected.')]);
  await (await conversation.submit({type:'input',content:'Read original build, replay same request, then explicitly select it.',requestId:'select-original'},context)).wait(context);
  assert.equal(count,1);assert.equal(settings.view().revision,2);
  assert.equal((await host.harness.snapshot(Study,old.conversationId,context))!.input!.data.nautilusTarget,undefined);
  const body={requestId:'bound-original',runnerBuildRequestId:'original-build',expectedRunnerRevision:2,goal:'New study using explicit runner',datasetId:'fixture',model};
  faux.setResponses([fauxAssistantMessage('New bound study; no backtest executed.')]);
  const next=await work.startResearch(body);await (await host.harness.submission(next.submissionId,context))!.wait(context);
  assert.notEqual(next.conversationId,old.conversationId);
  faux.setResponses([
   fauxAssistantMessage(fauxToolCall('native_research_start_bound',{...body,requestId:'bound-via-tool'}),{stopReason:'toolUse'}),
   fauxAssistantMessage('Original child reference only; no completed backtest claimed.'),
   fauxAssistantMessage('Bound child discussion only.')]);
  await (await conversation.submit({type:'input',content:'Explicitly start another new bound research with the same runner.',requestId:'start-bound-tool'},context)).wait(context);
  const viaTool=(await host.harness.snapshot(Requests,context))!.requests['request:bound-via-tool']!;
  assert.ok(viaTool.conversationId);
  assert.deepEqual((await host.harness.snapshot(Study,viaTool.conversationId!,context))!.input!.data.nautilusTarget,config.nativeProcess);
  const saved=await host.harness.snapshot(Study,next.conversationId,context);
  assert.deepEqual(saved!.input!.data.nautilusTarget,config.nativeProcess);
  await settings.saveEnvironment({expectedRevision:2,environment:null});
  await assert.rejects(work.startResearch({...body,requestId:'stale-new'}),/revision_conflict/);
  await assert.rejects(work.startResearch({...scopedBody,requestId:'changed-environment',expectedRunnerRevision:3}),/native_build_environment_changed/);
  assert.deepEqual(await work.startResearch(scopedBody),scoped);
  assert.deepEqual((await host.harness.snapshot(Study,scoped.conversationId,context))!.input,scopedInput);
  assert.deepEqual(await work.startResearch(body),next,'original request replays its original frozen target after settings change');
  assert.deepEqual((await host.harness.snapshot(Study,next.conversationId,context))!.input,saved!.input);
  await settings.close();await host.close();settings=await instanceSettings(config);host=await openResearch(options);({builds,work}=connect());
  assert.deepEqual(await work.startResearch(scopedBody),scoped);
  assert.deepEqual(await work.startResearch(body),next);assert.equal((await builds.read('original-build')).taskId,built.taskId);assert.equal(count,1);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

// Seed a genuinely pending official ToolTask, close SQLite, then let the actual
// production Service startup resume it. No manual connect callback on reopen.
test('Service initializes preparation callbacks before resuming a persisted tool', async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-pi-prepare-reopen-'));
 const config=configDefaults({dataDir:dir});
 const models=createModels();models.setProvider(fauxProvider().provider);
 const unexpected=async()=>{throw new Error('seed_host_must_not_execute');};
 const preparation=nativePreparationTools({configuration:()=>{throw new Error('seed_host_must_not_execute');},build:unexpected,read:unexpected,select:unexpected,startResearch:unexpected});
 const seed=await openResearch({database:config.research.database,models,extensions:[preparation],deferResume:true});
 let service:Awaited<ReturnType<typeof createService>>|undefined;
 try{
  const taskId=await seed.harness.commit(async tx=>{
   const conversation=await tx.createConversation({ownership:{kind:'ownerless'}});
   await configure(tx,conversation.id,{extensions:[preparation]});
   const call=fauxToolCall('native_runner_preparation_status',{});
   const entry=await tx.appendEntry(AssistantEntry,conversation.id,{model:[fauxAssistantMessage(call,{stopReason:'toolUse'})]});
   return tx.createTask(ToolTask,{kind:'model',assistant:entry.id,callId:call.id},{conversationId:conversation.id,ownership:{kind:'conversation'}});
  },context);
  assert.notEqual((await seed.harness.getTask(taskId,context))!.state.status,'terminal');
  await seed.close();
  service=await createService({config,models});
  const original=await service.research.harness.waitForTask(taskId,context);
  assert.equal(original.id,taskId);assert.equal(original.state.outcome.status,'completed');
  if(original.state.outcome.status!=='completed'||original.state.outcome.result.kind!=='model')assert.fail('original tool did not complete');
  const result=original.state.outcome.result;
  const entry=await service.research.harness.commit(tx=>tx.entry(result.entryId),context);
  const wire=JSON.stringify(entry);assert.ok(wire.includes('revision'));assert.ok(wire.includes('selectedRunner'));assert.equal(wire.includes('TypeError'),false);assert.equal(wire.includes('\"isError\":true'),false);
 }finally{await service?.close();await seed.close();await rm(dir,{recursive:true,force:true});}
});

test('production preparation wiring registers an explicit descriptor and rejects unavailable build before import',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-pi-registration-'));
 const config=configDefaults({dataDir:dir});config.research.codingWorkspace=dir;
 const models=createModels(),faux=fauxProvider({tokensPerSecond:100000});models.setProvider(faux.provider);
 const service=await createService({config,models});
 try{
  const path=join(dir,'descriptor.json');await writeFile(path,JSON.stringify({id:'prepared',name:'Protocol metadata only',assetClass:'TEST',currency:'USD',source:'controlled',start:null,end:null,ready:false,data:{catalog:join(dir,'not-executed')},policy:{}}));
  faux.setResponses([
   fauxAssistantMessage(fauxToolCall('native_dataset_registration_status',{}),{stopReason:'toolUse'}),
   fauxAssistantMessage(fauxToolCall('native_dataset_register',{requestId:'registered',expectedRevision:1,path}),{stopReason:'toolUse'}),
   fauxAssistantMessage(fauxToolCall('native_runner_market_import',{runnerBuildRequestId:'missing',expectedRunnerRevision:0,request:{schema_version:1,manifest_path:'/not-read',instruments_path:'/not-read',output_path:'/not-written'}}),{stopReason:'toolUse'}),
   fauxAssistantMessage('Descriptor registered; missing Build did not import market data.')]);
  const result=await service.research.start('prepare-registration',{goal:'Explicit preparation protocol only',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:dir},policy:{}});
  await (await service.research.harness.submission(result.submissionId,context))!.wait(context);
  const entries=await service.research.harness.commit(tx=>tx.scanEntries({conversationId:result.conversationId},100),context);
  const encoded=JSON.stringify(entries);assert.match(encoded,/native_build_request_not_found/);assert.match(encoded,/datasetIds/);assert.doesNotMatch(encoded,/tool_not_found|unknown tool|TypeError/i);
  const tasks=await service.research.harness.commit(tx=>tx.scanTasks({conversationId:result.conversationId},100),context);
  assert.equal(tasks.items.filter(t=>t.kind==='qz.native-market-import').length,0);
 }finally{await service.close();await rm(dir,{recursive:true,force:true});}
});

import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider,fauxAssistantMessage,fauxToolCall} from '@earendil-works/pi-ai';
import {LiveDoc,AssistantEntry,ToolTask} from '@earendil-works/pi-durable';
import {runNativeProcess,NativeProcessFailure} from '../src/native-process.ts';
import {JsonReader} from '../src/native-json.ts';
import {openResearch} from '../src/host.ts';
import {NativeRemoteUncertainty} from '../src/native-execution.ts';
import {ResearchConfiguration} from '../src/research-configuration.ts';
import {NativeEvidencePart} from '../plugins/native-evidence.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';

test('real nonzero process exit and missing-launch evidence remain separate from unknown business outcome',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-exit-facts-'));
 try{
  const script=join(dir,'failed.mjs');await writeFile(script,"for await(const c of process.stdin){};process.stderr.write('PRIVATE_DIAGNOSTIC');process.exit(7);");
  await assert.rejects(runNativeProcess('nonzero',process.execPath,script,[],{},new JsonReader()),error=>{
   assert.ok(error instanceof NativeProcessFailure);assert.deepEqual(error.processFacts,{errorCategory:'native_process_failed',exitObserved:true,exitCode:7,signal:null,terminationRequested:false,cleanupSignalRequested:true});
   assert.equal(JSON.stringify(error).includes('PRIVATE_DIAGNOSTIC'),false);return true;
  });
  const killed=join(dir,'killed.mjs');await writeFile(killed,"for await(const c of process.stdin){};process.kill(process.pid,'SIGKILL');");
  await assert.rejects(runNativeProcess('signal',process.execPath,killed,[],{},new JsonReader()),error=>{
   assert.ok(error instanceof NativeProcessFailure);assert.deepEqual(error.processFacts,{errorCategory:'native_process_exit_unknown',exitObserved:true,exitCode:null,signal:'SIGKILL',terminationRequested:false,cleanupSignalRequested:true});return true;
  });
  const streaming=join(dir,'streaming.mjs');await writeFile(streaming,"process.stdout.write('chunk');setInterval(()=>{},1000);");
  await assert.rejects(runNativeProcess('parser-failure',process.execPath,streaming,[],{},{write(){throw new Error('private parser failure');},finish(){return null;}}),error=>{
   assert.ok(error instanceof NativeProcessFailure);assert.equal(error.processFacts.errorCategory,'native_process_output_invalid');
   assert.equal(error.processFacts.terminationRequested,true);assert.equal(error.processFacts.cleanupSignalRequested,true);return true;
  });
  await assert.rejects(runNativeProcess('no-exit',join(dir,'missing-binary'),'capabilities',[],{},new JsonReader()),error=>{
   assert.ok(error instanceof NativeProcessFailure);assert.deepEqual(error.processFacts,{errorCategory:'native_process_launch_failed',exitObserved:false,exitCode:null,signal:null,terminationRequested:false,cleanupSignalRequested:true});return true;
  });
 }finally{await rm(dir,{recursive:true,force:true});}
});

for(const toolName of ['native_configured_backtest','native_strategy_backtest']) for(const missing of [false,true]) test(`original pending research displays UNKNOWN facts and refuses replacement (tool=${toolName}, missing=${missing})`,{timeout:15000},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-exit-durable-')),binary=join(dir,'runner'),marker=join(dir,'launches');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';appendFileSync(${JSON.stringify(marker)},'start\\n');for await(const c of process.stdin){};process.stderr.write('PRIVATE_OWNER_DIAGNOSTIC');process.exit(9);`,{mode:0o755});
 const target:ProcessNativeTarget={kind:'RUST_PROCESS',id:'controlled',binary:missing?join(dir,'not-installed'):binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'standard'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const models=createModels(),faux=fauxProvider({tokensPerSecond:100000});models.setProvider(faux.provider);
 faux.setResponses([fauxAssistantMessage(fauxToolCall(toolName,{}),{stopReason:'toolUse'})]);
 const options={database:join(dir,'research.sqlite'),models};let host=await openResearch(options);
 try{
  const receipt=await host.start('original-failure',{goal:'Observe controlled process failure only',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:dir,nautilusTarget:target,[toolName==='native_configured_backtest'?'native_configured_backtest_request':'native_strategy_backtest_request']:{schema_version:1,name:'owner',config:{}}},policy:{}});
  let live='';
  for(let i=0;i<150;i++){live=JSON.stringify(await host.harness.snapshot(LiveDoc,receipt.conversationId,context));if(live.includes('EXECUTION_RESULT_UNKNOWN'))break;await new Promise(r=>setTimeout(r,20));}
  assert.ok(live.includes('EXECUTION_RESULT_UNKNOWN'),live);assert.ok(live.includes('exitCode'));assert.ok(live.includes('No automatic resubmission'));assert.equal(live.includes('PRIVATE_OWNER_DIAGNOSTIC'),false);
  const tasks=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items;assert.equal(tasks.length,1);
  const task=tasks[0]!;assert.equal(task.state.outcome?.status,'failed');
  assert.equal((task.state.outcome as any).error.message,'nautilus_execution_unknown');
  assert.equal((task.state.outcome as any).error.detail.process.exitCode,missing?null:9);
  assert.equal((task.state.outcome as any).error.detail.process.exitObserved,!missing);
  assert.equal((task.state.outcome as any).error.detail.process.cleanupSignalRequested,true);
  assert.equal((task.state.outcome as any).error.detail.process.terminationRequested,false);
  const owner=(task.input as any).ownerTaskId;
  const uncertainty=await host.harness.snapshot(NativeRemoteUncertainty,task.conversationId,String(owner),context);
  assert.equal(uncertainty!.requests[0]!.status,'UNKNOWN');assert.equal((uncertainty!.requests[0]!.process as any).exitCode,missing?null:9);
  const beforeConfiguration=await host.harness.snapshot(ResearchConfiguration,receipt.conversationId,context);
  // A second official tool task must not replace the unresolved original binding.
  const retry=await host.harness.commit(async tx=>{
   const call=fauxToolCall(toolName,{});
   const entry=await tx.appendEntry(AssistantEntry,receipt.conversationId,{model:[fauxAssistantMessage(call,{stopReason:'toolUse'})]});
   return tx.createTask(ToolTask,{kind:'model',assistant:entry.id,callId:call.id},{conversationId:receipt.conversationId,ownership:{kind:'conversation'}});
  },context);
  for(let i=0;i<50;i++){if((await host.harness.getTask(retry,context))!.state.status==='terminal')break;await new Promise(r=>setTimeout(r,20));}
  const retryTask=(await host.harness.getTask(retry,context))!;
  assert.equal(retryTask.state.status,'terminal','new tool must reject instead of starting another unresolved backtest');
  if(retryTask.state.status!=='terminal')assert.fail('replacement did not settle');
  assert.equal(retryTask.state.outcome.status,'failed');
  if(retryTask.state.outcome.status!=='failed')assert.fail('new tool did not reject');
  assert.equal(retryTask.state.outcome.error.message,`Tool ${toolName} threw`);
  assert.equal((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native-strategy.backtest'},10),context)).items.length,1);
  assert.deepEqual(await host.harness.snapshot(ResearchConfiguration,receipt.conversationId,context),beforeConfiguration);
  const privatePart=await host.harness.snapshot(NativeEvidencePart,task.conversationId,`${task.id}:private-native-stderr`,context);
  if(!missing)assert.equal(privatePart!.data,'PRIVATE_OWNER_DIAGNOSTIC');else assert.ok(!privatePart);
  await host.close();host=await openResearch(options);
  for(let i=0;i<40;i++){live=JSON.stringify(await host.harness.snapshot(LiveDoc,receipt.conversationId,context));if(live.includes('EXECUTION_RESULT_UNKNOWN'))break;await new Promise(r=>setTimeout(r,20));}
  assert.ok(live.includes('EXECUTION_RESULT_UNKNOWN'));if(missing)await assert.rejects(readFile(marker),{code:'ENOENT'});else assert.equal((await readFile(marker,'utf8')).trim(),'start');
  assert.deepEqual((await host.harness.getTask(task.id,context))!.state.outcome,task.state.outcome);
  assert.equal((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items.length,1);
 }finally{await host.close();await rm(dir,{recursive:true,force:true});}
});

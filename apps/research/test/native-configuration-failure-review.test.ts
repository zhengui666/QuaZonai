import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider,fauxAssistantMessage,fauxToolCall} from '@earendil-works/pi-ai';
import {LiveDoc} from '@earendil-works/pi-durable';
import {openResearch} from '../src/host.ts';
import {NativeRemoteUncertainty,reconcileConfiguredFailure,reconcileNativeExecution} from '../src/native-execution.ts';
import {NativeEvidencePart} from '../plugins/native-evidence.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';

// Real process and official Durable lifecycle, controlled native boundary only.
for (const variant of ['empty-venues','missing-fee-model','fee-without-node-error','fee-wrong-terminal-error','fee-wrong-classification','different-error','different-exit','signal','other-command']) test(`source-reviewed original failure: ${variant}`,{timeout:20000},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-config-review-')),binary=join(dir,'runner.mjs'),marker=join(dir,'launches');
 const feeDiagnostic="\u001b[1m2026-11-16T21:53:05Z\u001b[0m \u001b[1;31m[ERROR] TRADER-001.nautilus_backtest::node: Error building backtest 'run-id': BacktestVenueConfig for 'IDEAS_SIM' requires an explicit fee_model, including an explicit zero-fee model\u001b[0m\nQZ_NATIVE_JOB_FAILED\nNATIVE_CONFIGURED_ENGINE_UNAVAILABLE\n";
 const feeReview=variant==='missing-fee-model'||variant.startsWith('fee-');
 const diagnostic=variant==='fee-without-node-error'?'QZ_NATIVE_JOB_FAILED\nNATIVE_CONFIGURED_ENGINE_UNAVAILABLE\n':variant==='fee-wrong-terminal-error'?feeDiagnostic.replace('NATIVE_CONFIGURED_ENGINE_UNAVAILABLE','another error'):feeReview?feeDiagnostic:variant==='different-error'?'QZ_NATIVE_JOB_FAILED\nother error\n':'QZ_NATIVE_JOB_FAILED\nvenues must not be empty\n';
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';appendFileSync(${JSON.stringify(marker)},'start\\n');for await(const c of process.stdin){};process.stderr.write(${JSON.stringify(diagnostic)});${variant==='signal'?"process.kill(process.pid,'SIGKILL');":`process.exit(${variant==='different-exit'?2:1});`}`,{mode:0o755});
 const target:ProcessNativeTarget={kind:'RUST_PROCESS',id:'controlled',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'standard'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const models=createModels(),faux=fauxProvider({tokensPerSecond:100000});models.setProvider(faux.provider);
 const tool=variant==='other-command'?'native_strategy_backtest':'native_configured_backtest';
 let resumedContext='';
 faux.setResponses([fauxAssistantMessage(fauxToolCall(tool,{}),{stopReason:'toolUse'}),(transcript)=>{resumedContext=JSON.stringify(transcript);return fauxAssistantMessage('The original historical execution failed. No replacement is requested.');}]);
 const options={database:join(dir,'research.sqlite'),models};let host=await openResearch(options);
 try{
  const receipt=await host.start('original',{goal:'Controlled failure',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:dir,nautilusTarget:target,[`${tool}_request`]:{schema_version:1,name:'owner',config:{}}},policy:{}});
  let task:any;
  for(let i=0;i<200;i++){
   task=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items[0];
   if(task?.state.status==='terminal')break;
   await new Promise(r=>setTimeout(r,20));
  }
  assert.equal(task.state.outcome.error.message,'nautilus_execution_unknown');
  const original=structuredClone(task.state),owner=task.input.ownerTaskId;
  const review={nativeTaskId:task.id,ownerTaskId:owner,requestId:task.input.requestId,classification:feeReview&&variant!=='fee-wrong-classification'?'MISSING_FEE_MODEL_REJECTED_BEFORE_INSTALL':'EMPTY_VENUES_REJECTED_BEFORE_ENGINE_BUILD',sourceReview:feeReview?'Reviewed controlled native boundary: missing fee model rejects engine construction before install or replay. Not a claim of arbitrary user-code rollback.':'Reviewed controlled native boundary: known configuration rejection before engine build, install, or replay. Not a claim of arbitrary user-code rollback.',sources:['controlled runner source']};
  const before=await host.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(owner),context);
  // Explicit evidence mismatch never mutates receipt or launches anything.
  await assert.rejects(reconcileConfiguredFailure(host.harness,receipt.conversationId,{...review,requestId:'0'.repeat(64)},context),/evidence_conflict/);
  await assert.rejects(reconcileConfiguredFailure(host.harness,receipt.conversationId,{...review,sourceReview:''},context),/review_invalid/);
  await assert.rejects(reconcileConfiguredFailure(host.harness,receipt.conversationId,{...review,classification:'NONZERO_EXIT'},context),/review_invalid/);
  assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(owner),context),before);
  if(!['empty-venues','missing-fee-model'].includes(variant)){
   await assert.rejects(reconcileConfiguredFailure(host.harness,receipt.conversationId,review,context),/evidence_conflict/);
   assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(owner),context),before);
  }else{
   for(let i=0;i<200;i++){const current=await host.harness.getTask(owner,context);if((current!.state as any).checkpoint?.phase==='reconcile')break;await new Promise(r=>setTimeout(r,10));}
   for(let i=0;i<100;i++){if(JSON.stringify(await host.harness.snapshot(LiveDoc,receipt.conversationId,context)).includes('EXECUTION_RESULT_UNKNOWN'))break;await new Promise(r=>setTimeout(r,10));}
   assert.match(JSON.stringify(await host.harness.snapshot(LiveDoc,receipt.conversationId,context)),/EXECUTION_RESULT_UNKNOWN/);
   await host.close();host=await openResearch(options);
   let reached!:()=>void,release!:()=>void;
   const atCommit=new Promise<void>(r=>{reached=r;}),hold=new Promise<void>(r=>{release=r;});
   const harness=host.harness;
   const runtime={conversationId:receipt.conversationId,signal:new AbortController().signal,
    getTask:harness.getTask.bind(harness),snapshot:harness.snapshot.bind(harness),
    async commit(change:any,ctx:any){reached();await hold;return harness.commit(change,ctx);}
   } as Parameters<typeof reconcileNativeExecution>[1];
   // Hold an already-read UNKNOWN response until the explicit review commits.
   const stale=reconcileNativeExecution(owner,runtime,task.id,context);await atCommit;
   const resolved=await reconcileConfiguredFailure(harness,receipt.conversationId,review,context);
   release();assert.equal(await stale,true,'stale UNKNOWN cannot overwrite adopted failure');
   assert.equal(resolved.status,'FAILED');
   // Pi's original sleeping owner consumes adoptedReceipt and reaches its normal failure path.
   const settled=await host.harness.waitForTask(owner,context);
   assert.equal(settled.state.outcome.status,'failed');
   assert.match(JSON.stringify(settled.state.outcome),/native_configured_configuration_rejected/);
   await (await host.harness.submission(receipt.submissionId,context))!.wait(context);
   assert.match(resumedContext,/EXECUTION_FAILED_AFTER_SOURCE_REVIEW/);
   assert.match(resumedContext,/now definitively FAILED/);
   assert.match(resumedContext,/Earlier UNKNOWN output/);
   assert.match(resumedContext,/native_configured_configuration_rejected/);
   assert.match(resumedContext,/No automatic resubmission was performed/);
   const toolTasks=(await host.harness.commit(tx=>tx.scanTasks({kind:'pi.tool'},10),context)).items;
   assert.equal(toolTasks.length,1);
   const entries=(await host.harness.commit(tx=>tx.scanEntries({conversationId:receipt.conversationId},100),context)).items;
   assert.match(JSON.stringify(entries.find(entry=>entry.kind==='pi.tool-result')),/EXECUTION_FAILED_AFTER_SOURCE_REVIEW/,'final tool result carries correction even if stream delivery races settlement');
   const facts=await host.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(owner),context);
   const fact=facts!.requests[0]!;assert.equal(fact.status,'FAILED');
   assert.equal((fact.configurationFailureReview as any).previousStatus,'UNKNOWN');
   assert.equal((fact.configurationFailureReview as any).previousError,'nautilus_execution_unknown');
   assert.deepEqual((fact.configurationFailureReview as any).review,review);
   // Same request is idempotent even after task-scoped call documents retire.
   assert.deepEqual(await reconcileConfiguredFailure(host.harness,receipt.conversationId,review,context),resolved);
   await assert.rejects(reconcileConfiguredFailure(host.harness,receipt.conversationId,{...review,sourceReview:'different'},context),/evidence_conflict/);
   await host.close();host=await openResearch(options);
   assert.deepEqual(await reconcileConfiguredFailure(host.harness,receipt.conversationId,review,context),resolved);
  }
  assert.deepEqual((await host.harness.getTask(task.id,context))!.state,original,'original UNKNOWN task is immutable');
  assert.equal((await host.harness.snapshot(NativeEvidencePart,receipt.conversationId,`${task.id}:private-native-stderr`,context))!.data,diagnostic);
  assert.equal((await readFile(marker,'utf8')).trim(),'start');
  assert.equal((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items.length,1);
 }finally{await host.close();await rm(dir,{recursive:true,force:true});}
});

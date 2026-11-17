import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider,fauxAssistantMessage,fauxToolCall} from '@earendil-works/pi-ai';
import {openNodeSqliteStorage} from '@earendil-works/pi-durable/storage/sqlite/node';
import {defineTask,defineExtension} from '@earendil-works/pi-durable';
import {openResearch} from '../src/host.ts';
import {createMaintenance,maintenanceExtension} from '../../service/src/pm-maintenance.ts';
import {researchDetail,researchTaskViews} from '../../service/src/projections.ts';
import {researchTaskEvidence} from '../../service/src/research-assessments.ts';
import {NativeRemoteUncertainty,reconcileConfiguredFailure} from '../src/native-execution.ts';
import {NativeEvidencePart} from '../plugins/native-evidence.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';
// Deployment's existing JS entry points run against the same official storage.
// @ts-ignore JavaScript deployment module.
import {configure} from '../../../deploy/docker/configure.mjs';
// @ts-ignore JavaScript deployment module.
import {requireIdle,requireExitedPreflight} from '../../../deploy/docker/maintenance.mjs';

for(const variant of ['market-import','market-import-wrong-diagnostic','backtest-import-diagnostic','original','missing-fee-model','fee-wrong-terminal-error','different-error','different-exit','queued-submission','unrelated-task']) test(`stopped original native upgrade: ${variant}`, {timeout:30000}, async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-failure-upgrade-')),binary=join(dir,'runner.mjs'),marker=join(dir,'launches');
 const marketImport=variant.startsWith('market-import');
 const importDiagnostic='QZ_NATIVE_JOB_FAILED\nSOURCE_PATH_PREFIX_INVALID\n';
 const feeDiagnostic="\u001b[1m2026-11-16T21:53:05Z\u001b[0m \u001b[1;31m[ERROR] TRADER-001.nautilus_backtest::node: Error building backtest 'run-id': BacktestVenueConfig for 'IDEAS_SIM' requires an explicit fee_model, including an explicit zero-fee model\u001b[0m\nQZ_NATIVE_JOB_FAILED\nNATIVE_CONFIGURED_ENGINE_UNAVAILABLE\n";
 const diagnostic=variant==='market-import'||variant==='backtest-import-diagnostic'?importDiagnostic:variant==='missing-fee-model'?feeDiagnostic:variant==='fee-wrong-terminal-error'?feeDiagnostic.replace('NATIVE_CONFIGURED_ENGINE_UNAVAILABLE','another error'):variant==='different-error'?'QZ_NATIVE_JOB_FAILED\nother failure\n':'QZ_NATIVE_JOB_FAILED\nvenues must not be empty\n';
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';appendFileSync(${JSON.stringify(marker)},'start\\n');for await(const c of process.stdin){};process.stderr.write(${JSON.stringify(diagnostic)});process.exit(${variant==='different-exit'?2:1});`,{mode:0o755});
 const target:ProcessNativeTarget={kind:'RUST_PROCESS',id:'controlled',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'standard'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false,preparation:{marketImport:true,fillObservationImport:false}}};
 const configPath=configure(dir,'http://localhost:8081');
 const config=JSON.parse(await readFile(configPath,'utf8'));
 const models=createModels(),faux=fauxProvider({tokensPerSecond:100000});models.setProvider(faux.provider);
 faux.setResponses([fauxAssistantMessage(fauxToolCall(marketImport?'native_market_import':'native_configured_backtest',marketImport?{schema_version:1,manifest_path:join(dir,'manifest.json'),instruments_path:join(dir,'instruments.json'),output_path:join(dir,'output')}:{} ),{stopReason:'toolUse'}),fauxAssistantMessage('The original historical execution failed. No replacement is requested.')]);
 const waiting=defineTask<null,{phase:'hold'},null>({name:'test.unrelated-upgrade',version:1,initial:()=>({phase:'hold'}),phases:{async hold(_task,runtime,ctx){await runtime.commit(()=>({status:'sleeping',until:Date.now()+60000,checkpoint:{phase:'hold'}}),ctx);}},abort:(_task,runtime,ctx)=>runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx)});
 const options={database:config.research.database,models,extensions:[maintenanceExtension,defineExtension({name:'test.upgrade',tasks:[waiting]})]};let host=await openResearch(options);
 try{
  const receipt=await host.start('original',{goal:'Controlled failed configuration',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:dir,nautilusTarget:target,native_configured_backtest_request:{schema_version:1,name:'owner',config:{}}},policy:{}});
  let native:any,owner:any;
  for(let i=0;i<300;i++){
   native=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items[0];
   owner=native && await host.harness.getTask(native.input.ownerTaskId,context);
   if(native?.state.status==='terminal' && owner?.state.checkpoint?.phase==='reconcile')break;
   await new Promise(r=>setTimeout(r,20));
  }
  assert.equal(native.state.outcome.error.message,'nautilus_execution_unknown');
  assert.equal(owner.state.checkpoint.phase,'reconcile');
  const originalNative=structuredClone(native);
  const beforeFacts=await host.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(owner.id),context);
  if(variant==='queued-submission'){
   const handle=await host.harness.conversation(receipt.conversationId,context);
   await handle!.submit({type:'input',content:'A separate pending owner instruction.',whenBusy:'followUp',requestId:'another-submission'},context);
  }
  if(variant==='unrelated-task')await (await host.harness.root(context)).commit(tx=>tx.createTask(waiting,null,{ownership:{kind:'conversation'}}),context);
  const maintenance=await createMaintenance(host).enter({requestId:'upgrade-original',reason:'Review already exited original configured backtest'});
  assert.equal(maintenance.status.state,'DRAINING');
  const epoch=maintenance.receipt.epoch;
  const selection={conversationId:receipt.conversationId,ownerTaskId:owner.id,nativeTaskId:native.id,submissionId:receipt.submissionId};
  const detail=await researchDetail(host,receipt.conversationId,researchTaskViews(host));
  const ownerEvidence=marketImport?{taskId:owner.id,kind:owner.kind,state:owner.state}:await researchTaskEvidence(host,receipt.conversationId,owner.id);
  if(['queued-submission','unrelated-task'].includes(variant)){
   assert.throws(()=>requireExitedPreflight(selection,maintenance.status,detail,ownerEvidence),/app was not stopped/);
  }else{
   assert.deepEqual(requireExitedPreflight(selection,maintenance.status,detail,ownerEvidence),{nativeFailure:selection,maintenanceEpoch:epoch});
  }
  await host.close();
  const storage=await openNodeSqliteStorage(options.database);
  let beforeTasks:readonly any[],beforeSubmission:any;
  try{
   beforeTasks=(await storage.scanTasks({conversationId:receipt.conversationId},100,undefined,context)).items;
   beforeSubmission=await storage.submission(receipt.submissionId,context);
  }finally{await storage.close(context);}
  assert.equal(beforeSubmission.status,'placed');
  const scope={maintenanceEpoch:epoch,nativeFailure:{conversationId:receipt.conversationId,ownerTaskId:owner.id,nativeTaskId:native.id,submissionId:receipt.submissionId}};
  await assert.rejects(requireIdle(configPath),/not formally QUIESCED/);
  for(const field of ['conversationId','ownerTaskId','nativeTaskId','submissionId'] as const){
   await assert.rejects(requireIdle(configPath,{maintenanceEpoch:epoch,nativeFailure:{...scope.nativeFailure,[field]:999999}}),/evidence conflict/);
  }
  if(!['original','missing-fee-model','market-import'].includes(variant)){
   await assert.rejects(requireIdle(configPath,scope),/evidence conflict|Unresolved durable task|Unresolved research submission/);
   assert.equal((await readFile(marker,'utf8')).trim(),'start');
   return;
  }
  await assert.rejects(requireIdle(configPath,{...scope,maintenanceEpoch:'wrong-epoch'}));
  await requireIdle(configPath,scope);
  const after=await openNodeSqliteStorage(options.database);
  try{
   assert.deepEqual((await after.scanTasks({conversationId:receipt.conversationId},100,undefined,context)).items,beforeTasks);
   assert.deepEqual(await after.submission(receipt.submissionId,context),beforeSubmission);
  }finally{await after.close(context);}
  assert.equal((await readFile(marker,'utf8')).trim(),'start');
  host=await openResearch(options);
  assert.equal((await createMaintenance(host).status()).state,'DRAINING');
  await createMaintenance(host).resume({requestId:'upgrade-resume-original',epoch});
  assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(owner.id),context),beforeFacts);
  if(marketImport){
   assert.deepEqual(await host.harness.getTask(native.id,context),originalNative);
   assert.equal((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},100),context)).items.length,1);
   assert.equal((await readFile(marker,'utf8')).trim(),'start');
   return; // Upgrade only: source-reviewed reconciliation remains a separate explicit operation.
  }
  const review={nativeTaskId:native.id,ownerTaskId:owner.id,requestId:native.input.requestId,classification:variant==='missing-fee-model'?'MISSING_FEE_MODEL_REJECTED_BEFORE_INSTALL':'EMPTY_VENUES_REJECTED_BEFORE_ENGINE_BUILD',sourceReview:'Controlled child source only writes its invocation marker, consumes stdin, emits the exact configuration rejection, and exits 1. The controlled source does not start a replay or install callback.',sources:['controlled runner source']};
  assert.equal((await reconcileConfiguredFailure(host.harness,receipt.conversationId,review,context)).status,'FAILED');
  const settled=await host.harness.waitForTask(owner.id,context);
  assert.equal(settled.state.outcome.status,'failed');
  assert.match(JSON.stringify(settled.state.outcome),/native_configured_configuration_rejected/);
  assert.equal((await (await host.harness.submission(receipt.submissionId,context))!.wait(context)).status,'done');
  assert.deepEqual(await host.harness.getTask(native.id,context),originalNative);
  assert.equal((await host.harness.snapshot(NativeEvidencePart,receipt.conversationId,`${native.id}:private-native-stderr`,context))!.data,diagnostic);
  assert.equal((await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},100),context)).items.length,1);
  assert.equal((await readFile(marker,'utf8')).trim(),'start');
 }finally{await host.close();await rm(dir,{recursive:true,force:true});}
});

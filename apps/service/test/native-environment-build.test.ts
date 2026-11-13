import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm,mkdir,rename} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../../research/src/host.ts';
import {configDefaults} from '../src/config.ts';
import {instanceSettings} from '../src/instance-settings.ts';
import {nativeEnvironmentBuild} from '../src/native-environment-build.ts';

// Controlled builder boundary only; actual Cargo compilation is covered separately.
test('explicit build reuses original Pi ID; reads/save never build, artifact selection is separate and revision-fenced',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-build-task-'));const config=configDefaults({dataDir:dir});await mkdir(dir,{recursive:true});
 let builds=0;const caps={schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'test-protocol',nautilusSource:'test-only',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false};
 const builder=nativeEnvironmentBuild(config,async options=>{builds++;assert.equal(options.source.packageName,'owner');return {executable:join(dir,'fixed-test-artifact'),manifestPath:join(dir,'generated/Cargo.toml'),capabilities:caps,qualification:'COMPILED_NOT_CONNECTED'};});
 const models=createModels();models.setProvider(fauxProvider().provider);const opts={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};
 let host=await openResearch(opts),settings=await instanceSettings(config);let work=builder.workbench(host,settings);
 try{
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner',backtestFactory:'owner::factory'}});
  settings.view();assert.equal(builds,0);assert.equal(config.nativeProcess,undefined);
  const body={requestId:'build-original',expectedRevision:1};const first=await work.start(body);await host.harness.waitForTask(first.taskId,context);
  assert.equal(builds,1);assert.equal(config.nativeProcess,undefined,'Compilation never connects or selects an artifact.');
  assert.equal((await work.start(body)).taskId,first.taskId);assert.equal((await work.read(body.requestId)).taskId,first.taskId);assert.equal(builds,1);
  await work.select(body);assert.equal(settings.view().revision,2);await work.select(body);assert.equal(settings.view().revision,2);assert.equal(config.nativeProcess!.capabilities.account.compiled,false);assert.equal(config.nativeProcess!.binary,join(dir,'fixed-test-artifact'));
  await settings.close();await host.close();settings=await instanceSettings(config);host=await openResearch(opts);work=builder.workbench(host,settings);
  assert.equal((await work.start(body)).taskId,first.taskId);assert.equal(builds,1,'Restart/read never reruns the old construction.');
  await settings.saveEnvironment({expectedRevision:2,environment:null});assert.equal(config.nativeProcess,undefined);
  await assert.rejects(work.select({...body,expectedRevision:3}),/native_build_environment_changed/);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

test('interrupted construction resumes original Pi task as unknown and never automatically invokes builder again',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-build-unknown-'));const config=configDefaults({dataDir:dir});let count=0;let entered!:()=>void;const started=new Promise<void>(resolve=>{entered=resolve;});
 const builder=nativeEnvironmentBuild(config,async options=>{count++;entered();await new Promise<never>((_,reject)=>options.signal!.addEventListener('abort',()=>reject(new Error('controlled_abort')),{once:true}));throw new Error('unreachable');});
 const models=createModels();models.setProvider(fauxProvider().provider);const opts={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};let host=await openResearch(opts);const settings=await instanceSettings(config);
 try{
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner'}});
  const original={requestId:'unknown-original',expectedRevision:1};const initial=await builder.workbench(host,settings).start(original);await started;await host.close();host=await openResearch(opts);
  await host.harness.waitForTask(initial.taskId,context);const work=builder.workbench(host,settings);const result=await work.start(original);
  assert.equal(result.taskId,initial.taskId);assert.equal(count,1);assert.equal(result.state.status,'terminal');if(result.state.status==='terminal'){assert.equal(result.state.outcome.status,'failed');if(result.state.outcome.status==='failed')assert.equal(result.state.outcome.error.message,'native_build_original_outcome_unknown_no_automatic_retry');}
  await assert.rejects(work.select(original),/native_build_not_completed/);assert.equal(config.nativeProcess,undefined);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

test('competing artifacts have one revision winner; original selection retry and reopen are idempotent',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-build-select-cas-'));const config=configDefaults({dataDir:dir});let count=0;
 const builder=nativeEnvironmentBuild(config,async()=>({executable:join(dir,`artifact-${++count}`),manifestPath:join(dir,'Cargo.toml'),capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false},qualification:'COMPILED_NOT_CONNECTED'}));
 const models=createModels();models.setProvider(fauxProvider().provider);const opts={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};let host=await openResearch(opts),settings=await instanceSettings(config),work=builder.workbench(host,settings);
 try{
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner'}});
  const requests=[{requestId:'artifact-A',expectedRevision:1},{requestId:'artifact-B',expectedRevision:1}];
  for(const request of requests){const task=await work.start(request);await host.harness.waitForTask(task.taskId,context);}
  const settingsPath=join(dir,'instances.json'),held=join(dir,'original-instances.json');await rename(settingsPath,held);await mkdir(settingsPath);
  await assert.rejects(work.select(requests[0]!),/instance_settings_save_failed/);assert.equal(settings.view().revision,1);assert.equal(config.nativeProcess,undefined);
  await rm(settingsPath,{recursive:true});await rename(held,settingsPath);
  const outcomes=await Promise.allSettled(requests.map(request=>work.select(request)));
  assert.equal(outcomes.filter(v=>v.status==='fulfilled').length,1);assert.equal(outcomes.filter(v=>v.status==='rejected').length,1);
  const winner=requests[outcomes.findIndex(v=>v.status==='fulfilled')]!,loser=requests[outcomes.findIndex(v=>v.status==='rejected')]!;
  const frozen=settings.view();assert.equal(frozen.revision,2);await work.select(winner);assert.deepEqual(settings.view(),frozen);await assert.rejects(work.select(loser),/revision_conflict/);
  await settings.close();await host.close();settings=await instanceSettings(config);host=await openResearch(opts);work=builder.workbench(host,settings);
  await work.select(winner);assert.deepEqual(settings.view(),frozen);await assert.rejects(work.select(loser),/revision_conflict/);assert.equal(count,2);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

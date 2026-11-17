import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm,mkdir,rename,writeFile,readFile,symlink} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../../research/src/host.ts';
import {configDefaults} from '../src/config.ts';
import {instanceSettings} from '../src/instance-settings.ts';
import {nativeEnvironmentBuild,readPreparationDiagnostics} from '../src/native-environment-build.ts';

// Controlled builder boundary only; actual Cargo compilation is covered separately.
test('explicit build reuses original Pi ID; reads/save never build, artifact selection is separate and revision-fenced',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-build-task-'));const config=configDefaults({dataDir:dir});await mkdir(dir,{recursive:true});
 let builds=0;const caps={schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'test-protocol',nautilusSource:'test-only',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false};
 const builder=nativeEnvironmentBuild(config,async options=>{builds++;assert.equal(options.source.packageName,'owner');assert.equal(options.targetDirectory,join(dir,'native-build-cache'));assert.equal(options.factories.chainticksFillObservations,true);assert.equal(options.factories.marketImport,true);assert.equal(options.factories.accountFactory,undefined);return {executable:join(dir,'fixed-test-artifact'),manifestPath:join(dir,'generated/Cargo.toml'),capabilities:caps,qualification:'COMPILED_NOT_CONNECTED'};});
 const models=createModels();models.setProvider(fauxProvider().provider);const opts={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};
 let host=await openResearch(opts),settings=await instanceSettings(config);let work=builder.workbench(host,settings);
 try{
  assert.throws(()=>settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner',marketImport:'true'}}),/rust_market_import_invalid/);
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner',backtestFactory:'owner::factory',chainticksFillObservations:true,marketImport:true}});
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

test('real import failures expose bounded categories and only existing logs; original failures never rerun after reopen',async()=>{
 const {writeFile,readdir}=await import('node:fs/promises');
 const dir=await mkdtemp(join(tmpdir(),'qz-import-diagnostics-'));const config=configDefaults({dataDir:dir});
 const builder=nativeEnvironmentBuild(config);const models=createModels();models.setProvider(fauxProvider().provider);
 const opts={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};let host=await openResearch(opts);const settings=await instanceSettings(config);
 const caps={schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'0.69.0',nautilusSource:'test-only',jobVersion:'test',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false};
 const source=join(dir,'identity-artifact');await writeFile(source,'#!/bin/sh\nprintf \'%s\\n\' \''+JSON.stringify(caps)+'\'\n',{mode:0o700});
 const bad=join(dir,'loader-failure');await writeFile(bad,'#!/missing-private-review-loader\nPRIVATE_COMPILER_DIAGNOSTIC\n',{mode:0o700});
 const cases=[{name:'missing',source:join(dir,'absent'),reason:'native_preparation_required_file_unavailable',logs:[]},
 {name:'loader',source:bad,reason:'native_preparation_required_file_unavailable',logs:['capabilities.stdout','capabilities.stderr']},
 {name:'identity',source,reason:'native_preparation_identity_mismatch',logs:['capabilities.stdout','capabilities.stderr'],expectedIdentity:{nautilusSource:'another-source',nautilusVersion:'0.69.0',precision:'high'}}];
 try{
  for(const item of cases){
   const work=builder.workbench(host,settings);const request={requestId:item.name,expectedRevision:0,environmentId:'review',sourceExecutable:item.source,...(item.expectedIdentity?{expectedIdentity:item.expectedIdentity}:{})};
   const original=await work.startImport(request);await host.harness.waitForTask(original.taskId,context);const result=await work.read(item.name);
   assert.equal(result.state.status,'terminal');if(result.state.status!=='terminal'||result.state.outcome.status!=='failed')throw Error('expected failure');
   assert.equal(result.state.outcome.error.message,item.reason);assert.deepEqual(result.state.outcome.error.detail,{diagnosticFiles:item.logs});
   assert.equal(JSON.stringify(result.state).includes(dir),false);assert.equal(JSON.stringify(result.state).includes('PRIVATE_COMPILER_DIAGNOSTIC'),false);
   if(item.name==='missing')assert.deepEqual(await readdir(join(dir,'native-builds')),[]);
   await assert.rejects(work.select({requestId:item.name,expectedRevision:0}),/native_build_not_completed/);assert.equal(config.nativeProcess,undefined);
   await host.close();host=await openResearch(opts);const repeated=await builder.workbench(host,settings).startImport(request);assert.equal(repeated.taskId,original.taskId);assert.deepEqual(repeated.state,result.state);
  }
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

// Original filesystem logs and real Durable lifecycle; the compiler boundary is controlled.
test('build status returns bounded diagnostic evidence for running and failed original tasks',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-build-readable-'));const config=configDefaults({dataDir:dir});
 let release!:()=>void,entered!:()=>void;const started=new Promise<void>(r=>{entered=r;});const wait=new Promise<void>(r=>{release=r;});let builds=0;
 const original='x'.repeat(40000)+'\nerror[E0432]: unresolved import nautilus::Example\napi_key=private-test-key\nAuthorization: Bearer private-test-bearer\n-----BEGIN PRIVATE KEY-----\nprivate-key-body\n-----END PRIVATE KEY-----\n';
 let log='';
 const builder=nativeEnvironmentBuild(config,async options=>{builds++;await mkdir(options.directory,{recursive:true});log=join(options.directory,'build.log');await writeFile(log,original);entered();await wait;throw new Error('compile_failed');});
 const models=createModels();models.setProvider(fauxProvider().provider);const host=await openResearch({database:join(dir,'research.sqlite'),models,extensions:[builder.extension]});const settings=await instanceSettings(config);
 try{
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner'}});
  const work=builder.workbench(host,settings),request={requestId:'compile-errors',expectedRevision:1};const task=await work.start(request);await started;
  const running=await work.read(request.requestId);assert.equal(running.state.status,'running');
  const diagnostic=running.diagnostics.files.find(f=>f.name==='build.log')!;
  assert.equal(diagnostic.status,'available');assert.equal(diagnostic.truncated,true);assert.equal(diagnostic.redacted,true);assert.match(diagnostic.text!,/error\[E0432\]/);
  assert.equal(diagnostic.totalBytes,Buffer.byteLength(original));assert.ok(Buffer.byteLength(diagnostic.text!)<=32768);
  assert.doesNotMatch(JSON.stringify(running.diagnostics),/private-test|private-key-body/);
  assert.equal(running.diagnostics.files.find(f=>f.name==='capabilities.stderr')!.status,'missing');
  release();await host.harness.waitForTask(task.taskId,context);const failed=await work.read(request.requestId);assert.equal(failed.state.status,'terminal');if(failed.state.status==='terminal')assert.equal(failed.state.outcome.status,'failed');
  assert.deepEqual(failed.diagnostics,running.diagnostics);assert.equal((await work.start(request)).taskId,task.taskId);assert.equal(builds,1);assert.equal(await readFile(log,'utf8'),original);
  await symlink(log,join(dir,'capabilities.stdout'));const unavailable=await readPreparationDiagnostics(dir);assert.equal(unavailable.files.find(f=>f.name==='capabilities.stdout')!.status,'unavailable');
 }finally{release();await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});


test('diagnostic redaction precedes truncation and withholds oversized originals',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-diagnostic-boundaries-'));
 const secrets=[
  'password="first'+String.fromCharCode(92,10)+'private-escaped-newline"',
  'password="first'+String.fromCharCode(10)+'private-trailing-escape'+String.fromCharCode(92),
  '-----BEGIN PRIVATE KEY-----\n'+'private-body\n'.repeat(4000)+'-----END PRIVATE KEY-----',
  'api_key='+ 'long-secret'.repeat(4000),
  'HYPERLIQUID_PK=private-hyperliquid',
  'QZ_HYPERLIQUID_TESTNET_PK=private-qz',
  'password="first secret word\nsecond secret word"',
  '"api_key": "json secret words"',
  "password='unterminated\nprivate remaining value",
 ];
 try{
  for(const original of secrets){await writeFile(join(dir,'build.log'),original);const result=await readPreparationDiagnostics(dir);const log=result.files[0]!;assert.equal(log.status,'available');assert.equal(log.redacted,true);assert.match(log.text!,/REDACTED/);assert.doesNotMatch(log.text!,/private-escaped-newline|private-trailing-escape|private-body|long-secret|private-hyperliquid|private-qz|secret word|json secret|unterminated|remaining value/);assert.equal(await readFile(join(dir,'build.log'),'utf8'),original);}
  await writeFile(join(dir,'build.log'),'password="'+'private oversized '.repeat(70000));const result=await readPreparationDiagnostics(dir);assert.equal(result.files[0]!.status,'source_too_large');assert.equal(result.files[0]!.text,undefined);
 }finally{await rm(dir,{recursive:true,force:true});}
});

test('explicit offline owner preparation is scope-gated, original-ID idempotent and exposes preflight stderr',async()=>{
 const {RustPreparationError}=await import('../../nautilus/src/rust-launcher-build.ts');
 const dir=await mkdtemp(join(tmpdir(),'qz-preflight-task-'));const config=configDefaults({dataDir:dir});
 let calls=0;const builder=nativeEnvironmentBuild(config,async options=>{calls++;assert.equal(options.prepareOwnerLockIn,dir);throw new RustPreparationError('owner_lock_preparation',{stderr:'error: no matching package named missing_crate found\napi_key=do-not-show'});});
 const models=createModels();models.setProvider(fauxProvider().provider);const host=await openResearch({database:join(dir,'research.sqlite'),models,extensions:[builder.extension]});const settings=await instanceSettings(config);
 try {
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner'}});
  const work=builder.workbench(host,settings),request={requestId:'prepare-owner',expectedRevision:1,prepareOwnerLock:true};
  await assert.rejects(work.start(request),/research_coding_workspace_required/);assert.equal(calls,0);
  config.research.codingWorkspace=dir;
  const task=await work.start(request);await host.harness.waitForTask(task.taskId,context);
  const result=await work.read(request.requestId);assert.equal(result.state.status,'terminal');
  if(result.state.status==='terminal'&&result.state.outcome.status==='failed')assert.equal(result.state.outcome.error.message,'rust_preflight_failed:owner_lock_preparation');else assert.fail('Expected explicit failure');
  const log=result.diagnostics.files.find(file=>file.name==='preflight.log')!;assert.match(log.text!,/missing_crate/);assert.doesNotMatch(log.text!,/do-not-show/);
  assert.equal((await work.start(request)).taskId,task.taskId);assert.equal(calls,1);
  await assert.rejects(work.start({...request,prepareOwnerLock:false}),/request_id_conflict/);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});


test('owner resolution rejection retains its exact cause in original build-status diagnostics',async()=>{
 const {RustPreparationError}=await import('../../nautilus/src/rust-launcher-build.ts');
 const dir=await mkdtemp(join(tmpdir(),'qz-resolution-diagnostic-'));const config=configDefaults({dataDir:dir});let calls=0;
 const builder=nativeEnvironmentBuild(config,async()=>{calls++;throw new RustPreparationError('owner_native_resolution',new Error('rust_build_owner_dependency_changed:nautilus-data'));});
 const models=createModels();models.setProvider(fauxProvider().provider);const host=await openResearch({database:join(dir,'research.sqlite'),models,extensions:[builder.extension]});const settings=await instanceSettings(config);
 try{
  await settings.saveEnvironment({expectedRevision:0,environment:{kind:'RUST_PROCESS',id:'owner',cargoBinary:'/owner/cargo',manifestPath:'/owner/Cargo.toml',packageName:'owner'}});
  const work=builder.workbench(host,settings),request={requestId:'resolution-failed',expectedRevision:1};const original=await work.start(request);await host.harness.waitForTask(original.taskId,context);
  const result=await work.read(request.requestId);assert.equal(result.state.status,'terminal');
  if(result.state.status==='terminal'&&result.state.outcome.status==='failed')assert.equal(result.state.outcome.error.message,'rust_preflight_failed:owner_native_resolution');else assert.fail('Expected resolution failure');
  assert.match(result.diagnostics.files.find(file=>file.name==='preflight.log')!.text!,/rust_build_owner_dependency_changed:nautilus-data/);
  assert.equal(result.diagnostics.files.find(file=>file.name==='build.log')!.status,'missing');
  assert.equal((await work.start(request)).taskId,original.taskId);assert.equal(calls,1);
  assert.equal(config.nativeProcess,undefined);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

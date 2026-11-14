import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm,writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {openResearch} from '../../research/src/host.ts';
import {configDefaults} from '../src/config.ts';
import {instanceSettings} from '../src/instance-settings.ts';
import {nativeEnvironmentBuild} from '../src/native-environment-build.ts';

test('explicit import uses the same Pi identity and selection CAS without Cargo; reads and restart never execute again',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-native-import-task-')),source=join(dir,'source.mjs'),marker=join(dir,'calls');
 const identity={nautilusSource:'git+controlled-protocol-only',nautilusVersion:'test',nautilusRevision:'test-revision',precision:'standard',jobVersion:'test'};
 const caps={schemaVersion:1,protocol:'quazonai-native-process-v1',identity,research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:true,registration:true,command:'account-runtime',requiresExplicitStart:true,compatibilityDeclaration:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1',compatibilityVerified:false},running:false};
 await writeFile(source,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';if(process.argv[2]!=='capabilities')throw Error('No BOOT or other command permitted');appendFileSync(${JSON.stringify(marker)},'capabilities\\n');console.log(${JSON.stringify(JSON.stringify(caps))});`,{mode:0o700});
 const config=configDefaults({dataDir:dir});const builder=nativeEnvironmentBuild(config);const models=createModels();models.setProvider(fauxProvider().provider);const options={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};
 let settings=await instanceSettings(config),host=await openResearch(options),work=builder.workbench(host,settings);
 const request={requestId:'original-import',expectedRevision:0,environmentId:'owner',sourceExecutable:source,expectedIdentity:{nautilusSource:identity.nautilusSource,nautilusVersion:identity.nautilusVersion,precision:'standard'}};
 try{
 settings.view();await assert.rejects(work.startImport({...request,requestId:'invalid-precision',expectedIdentity:{...request.expectedIdentity,precision:['standard']}}),/native_import_expected_identity_invalid/);await assert.rejects(work.read('invalid-precision'),/native_build_request_not_found/);await assert.rejects(readFile(marker),/ENOENT/);assert.equal(settings.view().environment,undefined);
 const first=await work.startImport(request);await host.harness.waitForTask(first.taskId,context);const done=await work.read(request.requestId);assert.equal(done.state.outcome?.status,'completed');assert.equal(config.nativeProcess,undefined);
 await assert.rejects(work.startImport({...request,sourceExecutable:source+'-changed'}),/request_id_conflict/);await assert.rejects(work.start({requestId:request.requestId,expectedRevision:0}),/request_id_conflict/);
 const otherRequest={...request,requestId:'competing-import'};const second=await work.startImport(otherRequest);await host.harness.waitForTask(second.taskId,context);
 const selected=await Promise.allSettled([work.select({requestId:request.requestId,expectedRevision:0}),work.select({requestId:otherRequest.requestId,expectedRevision:0})]);assert.equal(selected.filter(v=>v.status==='fulfilled').length,1);assert.equal(selected.filter(v=>v.status==='rejected').length,1);
 const winner=selected[0]!.status==='fulfilled'?request:otherRequest;assert.deepEqual(config.nativeProcess!.capabilities.account,caps.account);const originalBinary=config.nativeProcess!.binary;assert.notEqual(originalBinary,source);const copied=await readFile(originalBinary,'utf8');
 await work.select({requestId:winner.requestId,expectedRevision:0});assert.equal(settings.view().revision,1);
 await writeFile(source,'changed source must never overwrite the selected copied artifact');assert.equal(await readFile(originalBinary,'utf8'),copied);
 const calls=await readFile(marker,'utf8');assert.equal(calls,'capabilities\ncapabilities\n');
 await settings.close();await host.close();settings=await instanceSettings(config);host=await openResearch(options);work=builder.workbench(host,settings);
 assert.equal((await work.startImport(request)).taskId,first.taskId);await work.read(request.requestId);assert.equal(await readFile(marker,'utf8'),calls);assert.equal(config.nativeProcess!.binary,originalBinary);assert.deepEqual(config.nativeProcess!.capabilities.account,caps.account);assert.equal(settings.view().environment,undefined);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

test('interrupted import retains its original Pi ID and never executes or selects again on reopen',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-import-unknown-'));const config=configDefaults({dataDir:dir});let executions=0;let entered!:()=>void;const ready=new Promise<void>(resolve=>{entered=resolve;});
 const builder=nativeEnvironmentBuild(config,undefined,async options=>{executions++;entered();await new Promise<never>((_resolve,reject)=>options.signal!.addEventListener('abort',()=>reject(new Error('controlled_import_interrupted')),{once:true}));throw new Error('unreachable');});
 const models=createModels();models.setProvider(fauxProvider().provider);const opts={database:join(dir,'research.sqlite'),models,extensions:[builder.extension]};let host=await openResearch(opts);const settings=await instanceSettings(config);
 const request={requestId:'uncertain-import',expectedRevision:0,environmentId:'owner',sourceExecutable:'/original/trusted-runner'};
 try{
 const first=await builder.workbench(host,settings).startImport(request);await ready;await host.close();host=await openResearch(opts);await host.harness.waitForTask(first.taskId,context);
 const work=builder.workbench(host,settings),result=await work.startImport(request);assert.equal(result.taskId,first.taskId);assert.equal(executions,1);assert.equal(result.state.outcome?.status,'failed');if(result.state.outcome?.status==='failed')assert.match(result.state.outcome.error.message,/outcome_unknown_no_automatic_retry/);
 await assert.rejects(work.select({requestId:request.requestId,expectedRevision:0}),/native_build_not_completed/);assert.equal(config.nativeProcess,undefined);
 await assert.rejects(work.startImport({...request,expectedIdentity:{nautilusSource:'other',nautilusVersion:'other',precision:'high'}}),/request_id_conflict/);
 }finally{await settings.close();await host.close();await rm(dir,{recursive:true,force:true});}
});

test('process declaration remains an explicit unverified pair, never synthesized for old targets',async()=>{
 const {validateNativeTarget}=await import('../../research/src/native-execution.ts');
 const account={compiled:true,registration:true,command:'account-runtime',requiresExplicitStart:true};
 const target={kind:'RUST_PROCESS',id:'owner',binary:'/not-executed',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account,running:false}};
 assert.deepEqual((validateNativeTarget(target) as any).capabilities.account,account);
 for(const fields of [{compatibilityDeclaration:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1',compatibilityVerified:false},{compatibilityDeclaration:null,compatibilityVerified:false}])assert.deepEqual((validateNativeTarget({...target,capabilities:{...target.capabilities,account:{...account,...fields}}}) as any).capabilities.account,{...account,...fields});
 for(const fields of [{compatibilityDeclaration:'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'},{compatibilityVerified:false},{compatibilityDeclaration:null,compatibilityVerified:true},{compatibilityDeclaration:['TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1'],compatibilityVerified:false}])assert.throws(()=>validateNativeTarget({...target,capabilities:{...target.capabilities,account:{...account,...fields}}}),/native_process_target_invalid/);
});

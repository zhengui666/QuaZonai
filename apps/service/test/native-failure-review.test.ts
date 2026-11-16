import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider,fauxAssistantMessage,fauxToolCall} from '@earendil-works/pi-ai';
import {createService} from '../src/service.ts';
import {configDefaults} from '../src/config.ts';
import {NativeRemoteUncertainty} from '../../research/src/native-execution.ts';

test('authenticated explicit original failure review closes only the retained invocation',{timeout:20000},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-failure-api-')),binary=join(dir,'runner.mjs'),marker=join(dir,'starts');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';appendFileSync(${JSON.stringify(marker)},'start\\n');for await(const c of process.stdin){};process.stderr.write('QZ_NATIVE_JOB_FAILED\\nvenues must not be empty\\n');process.exit(1);`,{mode:0o755});
 const models=createModels(),faux=fauxProvider({tokensPerSecond:100000});models.setProvider(faux.provider);
 faux.setResponses([fauxAssistantMessage(fauxToolCall('native_configured_backtest',{}),{stopReason:'toolUse'}),fauxAssistantMessage('Original configuration failed; no replacement.')]);
 const service=await createService({config:configDefaults({dataDir:dir}),models});
 try{
  const target={kind:'RUST_PROCESS',id:'controlled',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled',precision:'standard'},research:{singleStrategyRegistration:false,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
  const receipt=await service.research.start('api-original',{goal:'Controlled failure',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:dir,nautilusTarget:target,native_configured_backtest_request:{schema_version:1,name:'owner',config:{}}},policy:{}});
  let task:any;
  for(let i=0;i<200;i++){task=(await service.research.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items[0];if(task?.state.status==='terminal')break;await new Promise(r=>setTimeout(r,20));}
  for(let i=0;i<200;i++){const current=await service.research.harness.getTask(task.input.ownerTaskId,context);if((current!.state as any).checkpoint?.phase==='reconcile')break;await new Promise(r=>setTimeout(r,10));}
  const body={nativeTaskId:task.id,ownerTaskId:task.input.ownerTaskId,requestId:task.input.requestId,classification:'EMPTY_VENUES_REJECTED_BEFORE_ENGINE_BUILD',sourceReview:'Reviewed controlled runner source rejects config before engine.',sources:['controlled runner']};
  const url=`/api/v3/research/${receipt.conversationId}/native-failure-review`;
  assert.equal((await service.app.inject({method:'POST',url,payload:body})).statusCode,401);
  const login=await service.app.inject({method:'POST',url:'/api/v3/session/setup',payload:{password:'local-test-owner-only'}});assert.equal(login.statusCode,200);
  const cookies={qz_session:login.cookies[0]!.value};
  assert.equal((await service.app.inject({method:'POST',url,cookies,payload:{...body,classification:'NONZERO'}})).statusCode,422);
  assert.equal((await service.app.inject({method:'POST',url,cookies,payload:{...body,requestId:'0'.repeat(64)}})).statusCode,409);
  const result=await service.app.inject({method:'POST',url,cookies,payload:body});assert.equal(result.statusCode,200,result.body);assert.equal(result.json().status,'FAILED');
  const original=await service.research.harness.waitForTask(task.input.ownerTaskId,context);assert.equal(original.state.outcome.status,'failed');
  assert.equal((await service.research.harness.getTask(task.id,context))!.state.outcome?.status,'failed');
  const facts=await service.research.harness.snapshot(NativeRemoteUncertainty,receipt.conversationId,String(task.input.ownerTaskId),context);assert.equal(facts!.requests[0]!.status,'FAILED');
  assert.equal((await service.app.inject({method:'POST',url,cookies,payload:body})).statusCode,200);
  assert.equal((await readFile(marker,'utf8')).trim(),'start');
 }finally{await service.close();await rm(dir,{recursive:true,force:true});}
});

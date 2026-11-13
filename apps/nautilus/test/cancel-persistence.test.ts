import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,mkdir,readFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createCapitalAccount} from '../src/capital-account.ts';
import type {AccountOptions,NativeAccountRuntime} from '../src/account-runtime.ts';
import type {NativeSessionBinding} from '../../../packages/contracts/src/native-strategy-control.ts';

for(const failure of ['before-send','after-send'] as const)test(`CANCEL ${failure} receipt failure retains UNKNOWN without retry`,async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-cancel-write-'));
 const options:AccountOptions={id:'paper',clientId:'client',accountId:'PAPER-001',mode:'paper',startingBalance:'1000',paperFeeRate:'0.001',instruments:['TEST.SIM'],nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
 let binding:NativeSessionBinding|null=null,attempts=0,starts=0;
 const runtime:NativeAccountRuntime={
  async request(){throw Error('unexpected unbound request');},async close(){},
  paperCancellationBinding(){return binding;},
  bindPaperCancellationSession(session){assert.equal(session,binding?.nativeSessionId);return {async request(operation,payload){
   assert.equal(operation,'ORDER_CANCEL_ATTEMPT');attempts++;
   if(failure==='after-send')await mkdir(join(directory,'capital-receipts.json.next'));
   return {command:payload.command,status:'UNKNOWN',attemptId:'controlled-dispatch-only'};
  }};},
  async start(){starts++;},
 };
 let account=await createCapitalAccount(options,directory,'/isolated-test-runner',runtime);
 const call=async(method:'GET'|'POST',path:string,body:Record<string,unknown>={})=>(await account.request(path,method,options.clientId,body)).value as any;
 try{
  const config=await call('GET','/strategy/config');
  binding={sourceId:config.accountSourceId,buildId:'/isolated-test-runner',nativeSessionId:'original-session',environmentId:'Sandbox'};
  const command={schemaVersion:1,requestId:'original-cancel',action:'CANCEL',strategyId:'OWNER-001',binding,order:{traderId:'TRADER-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original-order'}};
  const body={...command,clientId:options.clientId,accountId:options.accountId};
  const before=await readFile(join(directory,'capital-receipts.json'),'utf8');
  if(failure==='before-send')await mkdir(join(directory,'capital-receipts.json.next'));
  const first=await call('POST','/strategy/commands',body);
  assert.equal(first.status,'UNKNOWN');assert.equal(first.persisted,false);assert.equal(first.error,'account_receipts_write_failed');
  assert.deepEqual(first.command,command);assert.equal(attempts,failure==='before-send'?0:1);
  assert.deepEqual(await call('POST','/strategy/commands',body),first);
  const disk=await readFile(join(directory,'capital-receipts.json'),'utf8');
  if(failure==='before-send')assert.equal(disk,before,'No durable intent and no send');
  else {const original=JSON.parse(disk).strategyCommands['original-cancel'];assert.deepEqual(original.original,body);assert.equal(original.receipt.status,'UNKNOWN');assert.equal(original.receipt.attemptId,undefined);}
  await assert.rejects(account.close(),/account_receipts_write_failed/);binding=null;
  await rm(join(directory,'capital-receipts.json.next'),{recursive:true});
  account=await createCapitalAccount(options,directory,'/isolated-test-runner',runtime);
  if(failure==='after-send'){
   const repeated=await call('POST','/strategy/commands',body);assert.equal(repeated.status,'UNKNOWN');assert.deepEqual(repeated.command,command);
  }else {
   await assert.rejects(account.request('/strategy/commands','POST',options.clientId,body),/native_cancel_original_session_unavailable/);
  }
  assert.equal(attempts,failure==='before-send'?0:1);assert.equal(starts,0);
 }finally{await account.close().catch(()=>{});await rm(directory,{recursive:true,force:true});}
});

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

test('CANCEL observation write failure preserves durable UNKNOWN; GET alone can retain the original event',async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-cancel-observe-write-'));
 const options:AccountOptions={id:'paper',clientId:'client',accountId:'PAPER-001',mode:'paper',startingBalance:'1000',paperFeeRate:'0.001',instruments:['TEST.SIM'],nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
 let binding:NativeSessionBinding|null=null,attempts=0,observations=0,starts=0;
 // Controlled protocol fixture, not an official execution or an attribution claim.
 const orderObservation={status:'CANCELED',filledQuantity:'0.25',lastEvent:{Canceled:{trader_id:'TRADER-001',strategy_id:'OWNER-001',instrument_id:'TEST.SIM',client_order_id:'original-order',account_id:'PAPER-001',event_id:'original-event',ts_event:'1700000000000000001'}}};
 const runtime:NativeAccountRuntime={async request(){throw Error('unexpected unbound request');},async close(){},async start(){starts++;},paperCancellationBinding(){return binding;},bindPaperCancellationSession(session){assert.equal(session,binding?.nativeSessionId);return {async request(operation,payload){
  if(operation==='ORDER_CANCEL_ATTEMPT'){attempts++;return {command:payload.command,status:'UNKNOWN',attemptId:'controlled-original-attempt'};}
  assert.equal(operation,'ORDER_OBSERVE');observations++;return {command:payload.command,status:'UNKNOWN',orderObservation:structuredClone(orderObservation)};
 }};}};
 let account=await createCapitalAccount(options,directory,'/isolated-test-runner',runtime);
 const call=async(method:'GET'|'POST',path:string,body:Record<string,unknown>={})=>(await account.request(path,method,options.clientId,body)).value as any;
 const path='/strategy/commands/original-cancel',file=join(directory,'capital-receipts.json'),obstacle=file+'.next';
 try{
  const config=await call('GET','/strategy/config');binding={sourceId:config.accountSourceId,buildId:'/isolated-test-runner',nativeSessionId:'original-session',environmentId:'Sandbox'};
  const command={schemaVersion:1,requestId:'original-cancel',action:'CANCEL',strategyId:'OWNER-001',binding,order:{traderId:'TRADER-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original-order'}};
  const body={...command,clientId:options.clientId,accountId:options.accountId};
  const sent=await call('POST','/strategy/commands',body);assert.equal(sent.status,'UNKNOWN');assert.equal(sent.persisted,true);assert.equal(attempts,1);
  const before=await readFile(file,'utf8');assert.equal(JSON.parse(before).strategyCommands['original-cancel'].receipt.orderObservation,undefined);
  await mkdir(obstacle);
  const failed=await call('GET',path);assert.equal(failed.status,'UNKNOWN');assert.equal(failed.persisted,false);assert.match(failed.error,/^EISDIR:/);assert.deepEqual(failed.command,command);assert.deepEqual(failed.orderObservation,orderObservation);
  assert.equal(await readFile(file,'utf8'),before);assert.equal(attempts,1);assert.equal(observations,1);assert.equal(starts,0);
  assert.deepEqual(await call('POST','/strategy/commands',body),failed);assert.equal(attempts,1);assert.equal(await readFile(file,'utf8'),before);
  await rm(obstacle,{recursive:true});
  const stillFailed=await call('GET',path);assert.equal(stillFailed.persisted,false);assert.equal(stillFailed.error,'account_receipts_write_failed');assert.equal(await readFile(file,'utf8'),before);
  await assert.rejects(account.close(),/account_receipts_write_failed/);
  // Reopen only this account object; the controlled runtime retains its original
  // binding. This is NOT a process restart or authorization to replace a session.
  account=await createCapitalAccount(options,directory,'/isolated-test-runner',runtime);
  const retained=await call('GET',path);assert.equal(retained.status,'UNKNOWN');assert.equal(retained.persisted,true);assert.equal(retained.error,undefined);assert.deepEqual(retained.command,command);assert.deepEqual(retained.orderObservation,orderObservation);
  const disk=JSON.parse(await readFile(file,'utf8')).strategyCommands['original-cancel'];assert.deepEqual(disk.original,body);assert.deepEqual(disk.receipt,retained);assert.equal(attempts,1);assert.equal(observations,3);assert.equal(starts,0);
  await account.close();binding=null;account=await createCapitalAccount(options,directory,'/isolated-test-runner',runtime);
  assert.deepEqual(await call('POST','/strategy/commands',body),retained);
  const reopened=await call('GET',path);assert.equal(reopened.status,'UNKNOWN');assert.equal(reopened.error,'native_cancel_original_session_unavailable');assert.deepEqual(reopened.command,command);assert.deepEqual(reopened.orderObservation,orderObservation);
  assert.equal(attempts,1);assert.equal(observations,3);assert.equal(starts,0);
 }finally{await account.close().catch(()=>{});await rm(directory,{recursive:true,force:true});}
});

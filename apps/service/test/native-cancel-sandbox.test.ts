/** Opt-in integration fixture, not a production transport or account endpoint.
 * The supplied libtest binary must be built from this isolated source tree.
 * It starts one known offline Sandbox node, never a user factory or live client. */
import assert from 'node:assert/strict';
import test from 'node:test';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, writeFile, rename, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineTask, defineExtension } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { Application, json } from '../src/facts.ts';
import { createAccountStrategyCommand } from '../src/account-strategy.ts';
import { nativeStrategyControlPhases, admitNativeControlTask, type NativeControlInput } from '../src/native-strategy-control-phases.ts';
import { nativeStrategyControl, type NativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';

async function readJson(path: string): Promise<any> {
 for(let i=0;i<1500;i++){const value=await readFile(path,'utf8').catch(()=>null);if(value!==null)return JSON.parse(value);await delay(20);}
 throw new Error('sandbox_fixture_response_timeout');
}
async function request(directory:string,sequence:number,op:string,command:NativeStrategyControl){
 const path=join(directory,`request-${sequence}.json`);
 await writeFile(`${path}.pending`,JSON.stringify({op,command}),{flag:'wx',mode:0o600});await rename(`${path}.pending`,path);
 return readJson(join(directory,`response-${sequence}.json`));
}
async function open(directory:string, original:NativeStrategyControl, worker:boolean){
 const models=createModels();models.setProvider(fauxProvider().provider);
 let sends=0, reads=0;
 const Task=defineTask<NativeControlInput,{phase:'send'|'read'},null>({name:'qz.account-strategy-command',version:1,initial:()=>({phase:'send'}),
  phases:nativeStrategyControlPhases({
   async send(command){
    sends++;assert.equal(worker,true,'reopened task must never send');assert.deepEqual(command,original);
    const record=(await host.harness.snapshot(Application,context))!.strategyCommands!.original!;
    assert.equal(record.status,'UNCERTAIN');
    const tasks=await host.harness.commit(tx=>tx.scanTasks({kind:'qz.account-strategy-command'},10),context);
    assert.equal((tasks.items[0]!.state as any).checkpoint.phase,'read');
    const attempt=await request(directory,2,'CANCEL',command);assert.equal(attempt.status,'UNKNOWN');
    // Deliberately lose the actual native queue reply to Pi. Parent kills this
    // owner process only after independently observing official cancellation.
    await new Promise(()=>{});throw new Error('unreachable');
   },
   async read(command){
    reads++;assert.deepEqual(command,original);
    const sequence=Number(await readFile(join(directory,'next-read-sequence'),'utf8'));
    const observation=await request(directory,sequence,'READ',command);
    assert.deepEqual(observation.command,original);
    assert.equal(observation.orderObservation.status,'CANCELED');
    // Existing original receipt retains the native observation, not settlement.
    return json({...observation,status:'UNKNOWN'});
   },
  }),abort:(_task,runtime,ctx)=>runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx),
 });
 const host=await openResearch({database:join(directory,'original.sqlite'),models,deferResume:true,extensions:[defineExtension({name:'test-real-sandbox-cancel',tasks:[Task]})]});
 return {host,Task,counts:()=>({sends,reads})};
}
if(process.env.QZ_CANCEL_PI_FIXTURE){
 const directory=process.env.QZ_CANCEL_PI_FIXTURE;
 const ready=await readJson(join(directory,'ready.json'));const original=nativeStrategyControl(ready.command);
 const {host,Task}=await open(directory,original,true);const root=await host.harness.root(context);
 await root.commit(tx=>admitNativeControlTask(tx,Task,{key:'original',original},{conversationId:root.id,ownership:{kind:'conversation'}}),context);
 host.harness.resume();setInterval(()=>{},1000);
}else test('real Sandbox original cancel survives Pi owner SIGKILL as UNKNOWN with read-only recovery', {skip:!process.env.QZ_CANCEL_SANDBOX_TEST_BINARY,timeout:90000},async()=>{
 const binary=await realpath(process.env.QZ_CANCEL_SANDBOX_TEST_BINARY!);
 const directory=await mkdtemp(join(tmpdir(),'qz-native-pi-cancel-'));
 const native=spawn(binary,['account_runtime::tests::pi_cancel_sandbox_transport_fixture','--exact','--ignored','--nocapture'],{env:{PATH:process.env.PATH!,TMPDIR:directory,QZ_CANCEL_SANDBOX_FIXTURE:directory},stdio:['ignore','pipe','pipe']});
 let diagnostics='';native.stdout.on('data',v=>{diagnostics=(diagnostics+v).slice(-65536);});native.stderr.on('data',v=>{diagnostics=(diagnostics+v).slice(-65536);});
 const nativeExit=new Promise(resolve=>native.once('exit',(code,signal)=>resolve({code,signal})));
 let worker:ReturnType<typeof spawn>|undefined;let workerExit:Promise<any>|undefined;
 try{
  const ready=await readJson(join(directory,'ready.json'));const original=nativeStrategyControl(ready.command);
  assert.equal(original.binding.buildId,binary);assert.equal(original.binding.environmentId,'Sandbox');
  assert.equal(original.binding.sourceId,'test-source');
  const wrong={...original,binding:{...original.binding,nativeSessionId:'wrong-session'}};
  assert.equal((await request(directory,0,'CANCEL',wrong)).error,'test_original_binding_mismatch');
  const prior=await request(directory,1,'READ',original);assert.equal(prior.commandCount,ready.commandCount);assert.equal(prior.orderObservation.status,'PARTIALLY_FILLED');
  worker=spawn(process.execPath,['--experimental-strip-types',fileURLToPath(import.meta.url)],{env:{PATH:process.env.PATH!,TMPDIR:directory,QZ_CANCEL_PI_FIXTURE:directory},stdio:['ignore','pipe','pipe']});
  worker.stdout!.on('data',v=>{diagnostics=(diagnostics+v).slice(-65536);});worker.stderr!.on('data',v=>{diagnostics=(diagnostics+v).slice(-65536);});
  workerExit=new Promise(resolve=>worker!.once('exit',(code,signal)=>resolve({code,signal})));
  await readJson(join(directory,'response-2.json'));
  let sequence=3;let observed:any;
  for(let attempt=0;attempt<100;attempt++){
   observed=await request(directory,sequence++,'READ',original);
   if(observed.orderObservation.status==='CANCELED')break;
   await delay(20);
  }
  await writeFile(join(directory,'next-read-sequence'),String(sequence++),{flag:'wx',mode:0o600});
  assert.equal(observed.orderObservation.status,'CANCELED');assert.equal(observed.orderObservation.filledQuantity,ready.filledQuantity);
  assert.equal(observed.commandCount,ready.commandCount+1);
  assert.equal(worker.kill('SIGKILL'),true);assert.equal((await workerExit).signal,'SIGKILL');
  const reopened=await open(directory,original,false);
  try{
   const tasks=await reopened.host.harness.commit(tx=>tx.scanTasks({kind:'qz.account-strategy-command'},10),context);assert.equal(tasks.items.length,1);
   assert.equal((tasks.items[0]!.state as any).checkpoint.phase,'read');
   reopened.host.harness.resume();await reopened.host.harness.waitForTask(tasks.items[0]!.id,context);
   assert.deepEqual(reopened.counts(),{sends:0,reads:1});
   const record=(await reopened.host.harness.snapshot(Application,context))!.strategyCommands!.original!;
   assert.equal(record.status,'UNCERTAIN');assert.deepEqual(record.original,original);
   assert.equal((record.receipt as any).orderObservation.status,'CANCELED');assert.equal((record.receipt as any).commandCount,observed.commandCount);
   assert.deepEqual((record.receipt as any).orderObservation,observed.orderObservation,'official original order status, fill and last event survive SQLite recovery unchanged');
   assert.equal((record.receipt as any).settled,undefined);
  }finally{await reopened.host.close();}
  await request(directory,sequence,'STOP_FIXTURE',original);assert.deepEqual(await nativeExit,{code:0,signal:null},diagnostics);
 }finally{
  if(worker&&worker.exitCode===null&&worker.signalCode===null)worker.kill('SIGKILL');if(workerExit)await workerExit;
  if(native.exitCode===null&&native.signalCode===null)native.kill('SIGKILL');await nativeExit;await rm(directory,{recursive:true,force:true});
 }
});

// Actual existing AccountRuntime NDJSON + official controller inbox. Fixture-only
// setup explicitly creates one simulated resting order; not a production BOOT test.
test('existing NDJSON and Pi retain original UNKNOWN after official queued cancel and lost reply', {skip:!process.env.QZ_CANCEL_PROCESS_TEST_BINARY,timeout:90000}, async()=>{
 const {AccountRuntime}=await import('../../nautilus/src/account-runtime.ts');
 const binary=await realpath(process.env.QZ_CANCEL_PROCESS_TEST_BINARY!);
 const directory=await mkdtemp(join(tmpdir(),'qz-native-process-cancel-'));
 const wrapper=join(directory,'fixture');const quote=(s:string)=>"'"+s.replace(/'/g,"'\\''")+"'";
 await writeFile(wrapper,`#!/bin/sh\nQZ_CANCEL_SANDBOX_FIXTURE=${quote(directory)} QZ_CANCEL_NDJSON_FIXTURE=1 exec ${quote(binary)} account_runtime::tests::pi_cancel_sandbox_transport_fixture --exact --ignored --nocapture\n`,{mode:0o700});
 const runtime=new AccountRuntime(wrapper,{id:'paper',clientId:'fixture',accountId:'HYPERLIQUID-001',mode:'paper',nativeStrategy:{instrument_id:'BTC-USDC-SPOT.HYPERLIQUID',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}},instruments:['BTC-USDC-SPOT.HYPERLIQUID']},'test-source',directory);
 let sends=0,reads=0;let host:Awaited<ReturnType<typeof openResearch>>|undefined;
 try{
  await runtime.start();
  const ready=await readJson(join(directory,'ready.json'));const original=nativeStrategyControl(ready.command);
  assert.equal(original.binding.buildId,binary);assert.equal(original.binding.environmentId,'Sandbox');
  const bound=runtime.bindPaperCancellationSession(original.binding.nativeSessionId);
  const before=await bound.request('ORDER_OBSERVE',{command:original});
  assert.equal((before.orderObservation as any).status,'PARTIALLY_FILLED');
  const wrong={...original,binding:{...original.binding,nativeSessionId:'wrong'}};
  await assert.rejects(bound.request('ORDER_CANCEL_ATTEMPT',{command:wrong}),/binding_mismatch/);
  const transport = {
   async send(command:NativeStrategyControl){
    sends++;assert.deepEqual(command,original);
    const record=(await host!.harness.snapshot(Application,context))!.strategyCommands!.original!;
    assert.equal(record.status,'UNCERTAIN');
    const tasks=await host!.harness.commit(tx=>tx.scanTasks({kind:'qz.account-strategy-command'},10),context);
    assert.equal((tasks.items[0]!.state as any).checkpoint.phase,'read');
    const reply=await bound.request('ORDER_CANCEL_ATTEMPT',{command});
    assert.equal(reply.status,'UNKNOWN');
    throw new Error('test_reply_lost_after_official_dispatch');
   },
   async read(command:NativeStrategyControl){
    reads++;assert.deepEqual(command,original);
    let result:Record<string,unknown>={};
    for(let i=0;i<100;i++){
     result=await bound.request('ORDER_OBSERVE',{command});
     if((result.orderObservation as any).status==='CANCELED')break;
     await delay(20);
    }
    assert.equal((result.orderObservation as any).status,'CANCELED');
    assert.equal((result.orderObservation as any).filledQuantity,ready.filledQuantity);
    return json(result);
   },
  };
  const Task=createAccountStrategyCommand(transport);
  const openHost=async()=>{const models=createModels();models.setProvider(fauxProvider().provider);return openResearch({database:join(directory,'original.sqlite'),models,deferResume:true,extensions:[defineExtension({name:'test-existing-ndjson-cancel',tasks:[Task]})]});};
  host=await openHost();const root=await host.harness.root(context);
  const id=await root.commit(tx=>admitNativeControlTask(tx,Task,{key:'original',original},{conversationId:root.id,ownership:{kind:'conversation'}}),context);
  host.harness.resume();await host.harness.waitForTask(id,context);
  const record=(await host.harness.snapshot(Application,context))!.strategyCommands!.original!;
  assert.equal(record.status,'UNCERTAIN');assert.equal(sends,1);assert.equal(reads,1);
  assert.equal((record.receipt as any).settled,undefined);
  await host.close();host=await openHost();
  const rootAgain=await host.harness.root(context);
  assert.equal(await rootAgain.commit(tx=>admitNativeControlTask(tx,Task,{key:'original',original},{conversationId:rootAgain.id,ownership:{kind:'conversation'}}),context),id);
  host.harness.resume();await host.harness.waitForTask(id,context);
  assert.equal(sends,1);assert.equal(reads,1);
  assert.deepEqual((await host.harness.snapshot(Application,context))!.strategyCommands!.original,record);
  await runtime.close();
  await assert.rejects(bound.request('ORDER_OBSERVE',{command:original}),/session_unavailable/);
  assert.equal((await host.harness.snapshot(Application,context))!.strategyCommands!.original!.status,'UNCERTAIN');
 }finally{await host?.close();await runtime.close();await rm(directory,{recursive:true,force:true});}
});

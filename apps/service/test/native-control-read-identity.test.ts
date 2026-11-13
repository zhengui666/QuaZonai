/** Controlled transport plus real Pi/SQLite. No native node or order execution. */
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {defineTask,defineExtension} from '@earendil-works/pi-durable';
import {openResearch} from '../../research/src/host.ts';
import {Application,json} from '../src/facts.ts';
import {nativeStrategyControlPhases,admitNativeControlTask,type NativeControlInput} from '../src/native-strategy-control-phases.ts';
import {nativeStrategyControl} from '../../../packages/contracts/src/native-strategy-control.ts';

for(const failure of ['wrong-session','wrong-order','false-completed','read-unavailable'] as const)test(`original Pi CANCEL retains uncertainty after ${failure} read and SQLite reopen`,async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-control-read-identity-'));let sends=0,reads=0;
 const original=nativeStrategyControl({schemaVersion:1,requestId:'original-cancel',action:'CANCEL',strategyId:'ORIGINAL-001',binding:{sourceId:'source',environmentId:'Sandbox',buildId:'build',nativeSessionId:'original-session'},order:{traderId:'TRADER-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original-order'}});
 const models=createModels();models.setProvider(fauxProvider().provider);
 const Task=defineTask<NativeControlInput,{phase:'send'|'read'},null>({name:'qz.account-strategy-command',version:1,initial:()=>({phase:'send'}),phases:nativeStrategyControlPhases({
  async send(command){assert.deepEqual(command,original);sends++;throw Error('controlled_reply_unavailable');},
  async read(command){assert.deepEqual(command,original);reads++;if(failure==='read-unavailable')throw Error('controlled_read_unavailable');
   const reply=structuredClone(command);if(failure==='wrong-session')reply.binding.nativeSessionId='different-session';if(failure==='wrong-order')reply.order!.clientOrderId='different-order';
   return json({command:reply,status:failure==='false-completed'?'COMPLETED':'UNKNOWN',persisted:true,result:{strategyId:original.strategyId,success:true},orderObservation:{status:'CANCELED',filledQuantity:'0.25',lastEvent:{kind:'controlled-untrusted-reply'}}});
  },
 }),abort:(_task,runtime,ctx)=>runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx)});
 const options={database:join(directory,'original.sqlite'),models,deferResume:true,extensions:[defineExtension({name:'controlled-cancel-read',tasks:[Task]})]};
 let host=await openResearch(options);
 try{
  const root=await host.harness.root(context);const input={key:'original',original};const admission={conversationId:root.id,ownership:{kind:'conversation' as const}};
  const taskId=await root.commit(tx=>admitNativeControlTask(tx,Task,input,admission),context);
  host.harness.resume();await host.harness.waitForTask(taskId,context);
  const before=(await host.harness.snapshot(Application,context))!.strategyCommands!.original!;
  assert.equal(before.status,'UNCERTAIN');assert.deepEqual(before.original,original);assert.equal(before.receipt,null);assert.equal(sends,1);assert.equal(reads,1);
  const task=await host.harness.commit(tx=>tx.task(taskId),context);assert.equal(task!.state.status,'terminal');assert.equal((task!.state as any).outcome.status,'failed');assert.equal((task!.state as any).outcome.error.message,failure==='false-completed'?'native_cancel_observation_unavailable':failure==='read-unavailable'?'controlled_read_unavailable':'native_strategy_control_identity_mismatch');
  await host.close();host=await openResearch(options);
  assert.deepEqual((await host.harness.snapshot(Application,context))!.strategyCommands!.original,before);
  const retainedRoot=await host.harness.root(context);assert.equal(retainedRoot.id,root.id);
  assert.equal(await retainedRoot.commit(tx=>admitNativeControlTask(tx,Task,input,admission),context),taskId);
  host.harness.resume();await host.harness.waitForTask(taskId,context);
  assert.equal(sends,1);assert.equal(reads,1);assert.deepEqual((await host.harness.snapshot(Application,context))!.strategyCommands!.original,before);
 }finally{await host.close();await rm(directory,{recursive:true,force:true});}
});

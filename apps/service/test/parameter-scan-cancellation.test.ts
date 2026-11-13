import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:http';
import { BACKGROUND_CONTEXT as ctx } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { Harness, MemoryStorage, createRegistry, defineTask, defineExtension } from '@earendil-works/pi-durable';
import { ParameterScanRecords, createParameterScanExtension } from '../src/parameter-scans.ts';
import { NativeRemoteUncertainty } from '../../research/src/native-execution.ts';

// Real official scheduler/storage; only the remote HTTP peer is controlled.
for(const confirmed of [true,false]) test(`terminal parent and UNKNOWN child: cancellation confirmed=${confirmed}`,  {timeout:15000},async()=>{
 const calls:string[]=[];const requestId='a'.repeat(64);
 const server=createServer((req,res)=>{calls.push(`${req.method} ${req.url}`);res.setHeader('content-type','application/json');res.end(JSON.stringify({schemaVersion:1,requestId,status:confirmed?'CANCELLED':'UNKNOWN',...(!confirmed?{error:'still_unknown'}:{})}));});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
 const address=server.address();assert.ok(address&&typeof address!=='string');
 let harness:Harness;let aborts=0;
 const Done=defineTask<null,{phase:'done'},null>({name:'test.scan.done',version:1,initial:()=>({phase:'done'}),phases:{done:(_t,r,c)=>r.commit(()=>({status:'terminal',outcome:{status:'completed',result:null}}),c)},abort:(_t,r,c)=>r.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),c)});
 const Hold=defineTask<null,{phase:'hold'},null>({name:'test.scan.hold',version:1,initial:()=>({phase:'hold'}),phases:{hold:(_t,r,c)=>r.commit(()=>({status:'sleeping',until:r.now()+60000,checkpoint:{phase:'hold'}}),c)},abort:(_t,r,c)=>{aborts++;return r.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),c);}});
 const scans=createParameterScanExtension(()=>harness);
 const registry=createRegistry();registry.install(scans.extension);registry.install(defineExtension({name:'test.scan.lifecycle',tasks:[Done,Hold]}));
 harness=await Harness.open(new MemoryStorage(),{registry,models:createModels()},ctx);
 try{
  const owner=await harness.root(ctx);
  const parent=await owner.commit(tx=>tx.createTask(Done,null,{ownership:{kind:'conversation'}}),ctx);
  await harness.waitForTask(parent,ctx);
  const unknown=await owner.commit(tx=>tx.createTask(Done,null,{ownership:{kind:'conversation'}}),ctx);
  await harness.waitForTask(unknown,ctx);
  const active=await owner.commit(tx=>tx.createTask(Hold,null,{ownership:{kind:'conversation'}}),ctx);
  const stop=await owner.commit(async tx=>{
   (await tx.doc(ParameterScanRecords,owner.id,'old',null)).value={trials:{a:{taskId:active},b:{taskId:unknown}}};
   (await tx.doc(NativeRemoteUncertainty,owner.id,String(unknown),null)).requests=[{taskId:unknown,requestId,target:{id:'test',url:`http://127.0.0.1:${address.port}`},status:'UNKNOWN'}];
   return tx.createTask(scans.stopTask,{id:'old',parentTaskId:parent},{ownership:{kind:'conversation'}});
  },ctx);
  if(confirmed){const finished=await harness.waitForTask(stop,ctx);assert.equal(finished.state.outcome.status,'completed');}
  else {
   harness.resume();
   const deadline=Date.now()+5000;
   while(true){const task=await harness.getTask(stop,ctx);if(task?.state.status==='sleeping')break;if(Date.now()>deadline)throw Error('stop_not_reconciling');await new Promise(resolve=>setTimeout(resolve,5));}
  }
  assert.equal((await harness.getTask(active,ctx))?.state.status,'terminal');assert.equal(aborts,1);
  assert.equal((await harness.getTask(parent,ctx))?.state.status,'terminal');
  const retained=await harness.snapshot(NativeRemoteUncertainty,owner.id,String(unknown),ctx);
  assert.equal(retained?.requests[0]?.status,confirmed?'CANCELLED':'UNKNOWN');assert.equal(retained?.requests[0]?.requestId,requestId);
  assert.equal(calls.filter(call=>call.startsWith('DELETE')).length,1);assert.equal(calls[0],`DELETE /v1/executions/${requestId}`);assert.equal(calls.some(call=>call.startsWith('POST')),false);
 }finally{await harness.close(ctx);server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));}
});

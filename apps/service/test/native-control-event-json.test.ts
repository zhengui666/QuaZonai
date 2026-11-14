/** Controlled NDJSON child + actual AccountRuntime/Pi/SQLite; no native orders.
 * Rust's separate official-event test checks the source serialization. */
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {defineTask,defineExtension} from '@earendil-works/pi-durable';
import {openResearch} from '../../research/src/host.ts';
import {AccountRuntime} from '../../nautilus/src/account-runtime.ts';
import {Application,json} from '../src/facts.ts';
import {nativeStrategyControlPhases,admitNativeControlTask,type NativeControlInput} from '../src/native-strategy-control-phases.ts';
import {nativeStrategyControl} from '../../../packages/contracts/src/native-strategy-control.ts';

for(const precise of [true,false])test(`original event JSON survives NDJSON and SQLite, precise field present=${precise}`,async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-event-json-'));let sends=0,reads=0;
 const raw='{"type":"OrderCanceled","ts_event":1794603862176207674,"ts_init":1794603862176207675}';
 const binary=join(directory,'peer.mjs');
 await writeFile(binary,`#!${process.execPath}\nimport{createInterface}from'node:readline';createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);if(v.operation==='BOOT'){console.log(JSON.stringify({requestId:v.requestId,result:{ready:true}}));return;}if(v.operation==='SHUTDOWN'){console.log(JSON.stringify({requestId:v.requestId,result:{}}));process.exit(0);}const observation='{"status":"CANCELED","filledQuantity":"0", "lastEvent":'+${JSON.stringify(raw)}+${JSON.stringify(precise?',"lastEventJson":'+JSON.stringify(raw):'')}+'}';console.log('{"requestId":'+JSON.stringify(v.requestId)+',"result":{"command":'+JSON.stringify(v.command)+',"status":"UNKNOWN","persisted":true,"orderObservation":'+observation+'}}');});`,{mode:0o700});
 const runtime=new AccountRuntime(binary,{id:'paper',clientId:'client',accountId:'PAPER-001',mode:'paper',instruments:[],nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}},'original-source',directory);
 const original=nativeStrategyControl({schemaVersion:1,requestId:'original-cancel',action:'CANCEL',strategyId:'S-001',binding:{sourceId:'source',buildId:'build',environmentId:'Sandbox',nativeSessionId:'session'},order:{traderId:'T-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original'}});
 const models=createModels();models.setProvider(fauxProvider().provider);
 const Task=defineTask<NativeControlInput,{phase:'send'|'read'},null>({name:'qz.account-strategy-command',version:1,initial:()=>({phase:'send'}),phases:nativeStrategyControlPhases({
  async send(){sends++;throw Error('controlled_unavailable_reply');},
  async read(command){reads++;return json(await runtime.request('ORDER_OBSERVE',{command}));},
 }),abort:(_t,r,c)=>r.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),c)});
 const options={database:join(directory,'original.sqlite'),models,deferResume:true,extensions:[defineExtension({name:'event-json',tasks:[Task]})]};let host=await openResearch(options);
 try{
  await runtime.start();const root=await host.harness.root(context);const input={key:'original',original};const admission={conversationId:root.id,ownership:{kind:'conversation' as const}};
  const id=await root.commit(tx=>admitNativeControlTask(tx,Task,input,admission),context);host.harness.resume();await host.harness.waitForTask(id,context);
  const before=(await host.harness.snapshot(Application,context))!.strategyCommands!.original!;const event=(before.receipt as any).orderObservation;
  assert.equal(before.status,'UNCERTAIN');assert.deepEqual(before.original,original);
  // The old object is not repaired or relabelled exact: adjacent ns already collapse.
  assert.equal(event.lastEvent.ts_event,event.lastEvent.ts_init);assert.equal(Number.isSafeInteger(event.lastEvent.ts_event),false);
  if(precise){assert.equal(event.lastEventJson,raw);assert.ok(event.lastEventJson.includes('1794603862176207675'));}else assert.equal(Object.hasOwn(event,'lastEventJson'),false);
  await host.close();await runtime.close();host=await openResearch(options);
  const again=await host.harness.root(context);assert.equal(await again.commit(tx=>admitNativeControlTask(tx,Task,input,admission),context),id);
  host.harness.resume();await host.harness.waitForTask(id,context);assert.deepEqual((await host.harness.snapshot(Application,context))!.strategyCommands!.original,before);assert.equal(sends,1);assert.equal(reads,1);
 }finally{await host.close();await runtime.close();await rm(directory,{recursive:true,force:true});}
});

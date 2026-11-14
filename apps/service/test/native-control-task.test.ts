import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {BACKGROUND_CONTEXT as context} from '@earendil-works/chord/context';
import {createModels,fauxProvider} from '@earendil-works/pi-ai';
import {defineExtension} from '@earendil-works/pi-durable';
import {openResearch} from '../../research/src/host.ts';
import {Application} from '../src/facts.ts';
import {AccountStrategyCommand} from '../src/account-strategy.ts';
import {admitNativeControlTask} from '../src/native-strategy-control-phases.ts';
import {nativeStrategyControl} from '../../../packages/contracts/src/native-strategy-control.ts';

test('original Task without installed native transport preserves intent and never falls back to legacy send', async()=>{
 const directory=await mkdtemp(join(tmpdir(),'qz-native-uninstalled-'));
 const models=createModels();models.setProvider(fauxProvider().provider);
 const original=nativeStrategyControl({schemaVersion:1,requestId:'original-cancel',action:'CANCEL',strategyId:'S-001',order:{traderId:'T-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original-order'},binding:{sourceId:'original-source',buildId:'original-build',environmentId:'Sandbox',nativeSessionId:'original-session'}});
 const open=()=>openResearch({database:join(directory,'original.sqlite'),models,deferResume:true,extensions:[defineExtension({name:'test-original-account-task',tasks:[AccountStrategyCommand]})]});
 let host=await open();
 try{
  const root=await host.harness.root(context);
  const id=await root.commit(tx=>admitNativeControlTask(tx,AccountStrategyCommand,{key:'original',original},{conversationId:root.id,ownership:{kind:'conversation'}}),context);
  host.harness.resume();await host.harness.waitForTask(id,context);
  const task=await host.harness.commit(tx=>tx.task(id),context);
  assert.equal((task!.state as any).outcome.error.message,'native_control_transport_unsupported');
  const before=(await host.harness.snapshot(Application,context))!.strategyCommands!.original;
  assert.equal(before!.status,'PENDING','No transport dispatch or uncertainty checkpoint was attempted');
  assert.deepEqual(before!.original,original);
  await host.close();host=await open();
  const again=await host.harness.root(context);
  assert.equal(await again.commit(tx=>admitNativeControlTask(tx,AccountStrategyCommand,{key:'original',original},{conversationId:again.id,ownership:{kind:'conversation'}}),context),id);
  host.harness.resume();await host.harness.waitForTask(id,context);
  assert.deepEqual((await host.harness.snapshot(Application,context))!.strategyCommands!.original,before);
 }finally{await host.close();await rm(directory,{recursive:true,force:true});}
});

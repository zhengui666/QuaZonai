/** Opt-in real production-entry RECOVER, public market data + Sandbox execution.
 * No START, no user factory, signing key, synthetic data client or test Rust build.
 * Keep this newly created private directory for original receipt/diagnostic review. */
import assert from 'node:assert/strict';
import test from 'node:test';
import {execFileSync} from 'node:child_process';
import {readdir,realpath,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {setup,context} from './support.ts';
import {Application} from '../src/facts.ts';

test('production main explicit paper RECOVER exposes scoped session without a strategy or order', {skip:!process.env.QZ_OWNED_PAPER_RUNNER,timeout:100000},async()=>{
 const binary=await realpath(process.env.QZ_OWNED_PAPER_RUNNER!);
 const capabilities=JSON.parse(execFileSync(binary,['capabilities'],{env:{PATH:process.env.PATH},encoding:'utf8',maxBuffer:32768}));
 assert.equal(capabilities.account.compiled,true);assert.equal(capabilities.account.registration,false);assert.equal(capabilities.running,false);
 const state=await setup();console.log('isolated_paper_directory='+state.directory);
 state.options.config.nativeProcess={kind:'RUST_PROCESS',id:'owned-paper-acceptance',binary,capabilities};
 const configuration={id:'owned-paper',clientId:'isolated-client',accountId:'HYPERLIQUID-001',mode:'paper',instruments:['HYPE-USDC-SPOT.HYPERLIQUID'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'HYPE-USDC-SPOT.HYPERLIQUID',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
 const url='/api/v3/accounts/configurations',strategy=url+'/owned-paper/strategy';
 try{
  await state.restart();
  const created=await state.service.app.inject({method:'POST',url,headers:state.headers,payload:{requestId:'create-owned-paper',configuration}});
  assert.equal(created.statusCode,200,created.body);
  const fence=(await state.service.app.inject({url:strategy+'/fence',headers:state.headers})).json();
  assert.equal(fence.ordinaryVenueCancel,null,'Saving did not launch a node');
  const command={requestId:'recover-owned-paper-once',action:'RECOVER',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision};
  assert.equal((await state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:command})).statusCode,202);
  const original=Object.values((await state.service.research.harness.snapshot(Application,context))!.strategyCommands!)[0]!;
  await state.service.research.harness.waitForTask(original.taskId as never,context);
  const receipt=(await state.service.app.inject({url:strategy+'/commands/recover-owned-paper-once',headers:state.headers})).json();
  assert.equal(receipt.status,'COMPLETED',JSON.stringify(receipt));
  assert.equal(receipt.receipt.result.recovery_only,true);
  const status=await state.service.app.inject({url:strategy,headers:state.headers});
  assert.equal(status.statusCode,200,status.body);assert.equal(status.json().strategy.strategy_id,null);assert.equal(status.json().strategy.state,'UNREGISTERED');
  const active=(await state.service.app.inject({url:strategy+'/fence',headers:state.headers})).json();
  assert.equal(active.ordinaryVenueCancel.sourceId,fence.accountSourceId);
  assert.equal(active.ordinaryVenueCancel.buildId,binary);
  assert.equal(active.ordinaryVenueCancel.nativeSessionId,receipt.receipt.result.native_session_id);
  assert.equal(active.ordinaryVenueCancel.environmentId,'Sandbox');
  const records=Object.values((await state.service.research.harness.snapshot(Application,context))!.strategyCommands!);
  assert.equal(records.length,1);assert.equal((records[0]!.original as any).action,'RECOVER');
 }finally{
  await state.service.close();
 }
 const cache=join(state.directory,'accounts','owned-paper','native-cache');
 assert.deepEqual(await readdir(join(cache,'orders')).catch((error:NodeJS.ErrnoException)=>{if(error.code==='ENOENT')return [];throw error;}),[],'No order records in official persisted cache');
 const metadata=JSON.parse(await readFile(join(cache,'metadata.json'),'utf8'));assert.ok(metadata);
 console.log('production_boot_zero_orders_confirmed');
});

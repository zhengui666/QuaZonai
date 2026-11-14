/** Opt-in component joint experiment, NOT production main launch verification.
 * Only libtest argv is adapted. Same real ELF/child/session/production transport.
 * Fixture BOOT has zero orders; explicit test marker asks official test strategy
 * to create its known limit/partial order. No external data client or credentials. */
import assert from 'node:assert/strict';
import {test,mock} from 'node:test';
import childProcess from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {readFile,writeFile,mkdtemp,realpath,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import {setup,context} from './support.ts';
import {Application} from '../src/facts.ts';
async function readJson(path:string){for(let i=0;i<500;i++){try{return JSON.parse(await readFile(path,'utf8'));}catch(e){if((e as NodeJS.ErrnoException).code!=='ENOENT')throw e;}await delay(20);}throw Error('fixture_read_timeout');}
test('Service original CANCEL reaches official Sandbox and preserves UNKNOWN across reopen', {skip:!process.env.QZ_CANCEL_SERVICE_TEST_BINARY,timeout:90000},async()=>{
 const binary=await realpath(process.env.QZ_CANCEL_SERVICE_TEST_BINARY!);
 const fixture=await mkdtemp(join(tmpdir(),'qz-service-official-cancel-'));
 const state=await setup();let launches=0,cancels=0;
 const spawn=childProcess.spawn;
 const mocked=mock.method(childProcess,'spawn',((file:any,args:any,options:any)=>{
  if(file!==binary)return spawn(file,args,options);
  assert.deepEqual(args,['account-runtime']);launches++;
  const child=spawn(file,['account_runtime::tests::pi_cancel_sandbox_transport_fixture','--exact','--ignored','--nocapture'],{...options,env:{PATH:process.env.PATH,TMPDIR:fixture,QZ_CANCEL_SANDBOX_FIXTURE:fixture,QZ_CANCEL_NDJSON_FIXTURE:'1',QZ_CANCEL_SERVICE_FIXTURE:'1'}});
  // Observe original writes without changing bytes or the real child object.
  const write=child.stdin!.write.bind(child.stdin);
  child.stdin!.write=((chunk:any,...rest:any[])=>{if(String(chunk).includes('"operation":"ORDER_CANCEL_ATTEMPT"'))cancels++;return (write as any)(chunk,...rest);}) as any;
  return child;
 }) as typeof spawn);syncBuiltinESMExports();
 state.options.config.nativeProcess={kind:'RUST_PROCESS',id:'isolated-official-fixture',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const url='/api/v3/accounts/configurations',strategy=url+'/paper/strategy';
 const post=(body:Record<string,unknown>)=>state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:body});
 try{
  await state.restart();
  const saved=await state.service.app.inject({method:'POST',url,headers:state.headers,payload:{requestId:'create-paper',configuration:{id:'paper',clientId:'client',accountId:'HYPERLIQUID-001',mode:'paper',instruments:['BTC-USDC-SPOT.HYPERLIQUID'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'BTC-USDC-SPOT.HYPERLIQUID',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}}}});
  assert.equal(saved.statusCode,200,saved.body);assert.equal(launches,0);
  const fence=(await state.service.app.inject({url:strategy+'/fence',headers:state.headers})).json();
  const recover=await post({requestId:'recover-once',action:'RECOVER',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision});assert.equal(recover.statusCode,202,recover.body);
  let app=(await state.service.research.harness.snapshot(Application,context))!;
  await state.service.research.harness.waitForTask(Object.values(app.strategyCommands!)[0]!.taskId as never,context);
  const recovery=(await state.service.app.inject({url:strategy+'/commands/recover-once',headers:state.headers})).json();assert.equal(recovery.status,'COMPLETED',JSON.stringify(recovery));
  assert.deepEqual(await readJson(join(fixture,'boot-empty.json')),{orders:0});
  await writeFile(join(fixture,'explicit-test-start'),'explicit isolated simulation only',{flag:'wx',mode:0o600});
  const ready=await readJson(join(fixture,'ready.json'));const original=ready.command;
  assert.equal(original.binding.buildId,binary);assert.equal(original.binding.sourceId,fence.accountSourceId);assert.equal(original.binding.nativeSessionId,recovery.receipt.result.native_session_id);
  for(const key of ['buildId','sourceId','nativeSessionId'])assert.equal((await post({...original,requestId:'wrong-'+key,binding:{...original.binding,[key]:'wrong'}})).statusCode,409);
  assert.equal(cancels,0);
  const admitted=await post(original);assert.equal(admitted.statusCode,202,admitted.body);
  app=(await state.service.research.harness.snapshot(Application,context))!;
  const record=Object.values(app.strategyCommands!).find(v=>(v.original as any).requestId===original.requestId)!;
  await state.service.research.harness.waitForTask(record.taskId as never,context);
  let observed:any;
  for(let i=0;i<100;i++){const response=await state.service.app.inject({url:strategy+'/commands/'+original.requestId,headers:state.headers});assert.equal(response.statusCode,200,response.body);observed=response.json();if(observed.receipt.orderObservation?.status==='CANCELED')break;await delay(20);}
  assert.equal(observed.status,'UNCERTAIN');assert.equal(observed.receipt.orderObservation.status,'CANCELED');assert.equal(observed.receipt.orderObservation.filledQuantity,ready.filledQuantity);assert.ok(observed.receipt.orderObservation.lastEvent);assert.equal(observed.receipt.settled,undefined);assert.equal(cancels,1);
  const official=structuredClone(observed.receipt.orderObservation);
  assert.equal((await post(original)).statusCode,202);assert.equal(cancels,1);
  await state.restart();assert.equal((await post(original)).statusCode,202);
  const retained=(await state.service.app.inject({url:strategy+'/commands/'+original.requestId,headers:state.headers})).json();
  assert.equal(retained.status,'UNCERTAIN');assert.deepEqual(retained.receipt.orderObservation,official);assert.equal(retained.receipt.error,'native_cancel_original_session_unavailable');assert.equal(cancels,1);assert.equal(launches,1);
  console.log('official_partial_cancel_service_unknown_once_reopen_confirmed');
 }finally{try{await state.cleanup();}finally{mocked.mock.restore();syncBuiltinESMExports();await rm(fixture,{recursive:true,force:true});}}
});

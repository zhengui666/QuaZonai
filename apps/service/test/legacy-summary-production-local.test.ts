/** Opt-in real generated owner launcher, real import and production main.
 * Dedicated isolated paper only; no launch mocking, signing key or test Rust.
 * The synthetic factory is never invoked: only RECOVER and read-only ORDER_PAGE are requested.
 */
import assert from 'node:assert/strict';
import {test,mock} from 'node:test';
import {readFile,readdir,writeFile,realpath} from 'node:fs/promises';
import {join} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import {setup,context} from './support.ts';
import {Application} from '../src/facts.ts';
import {downstreamRequest} from '../src/downstream.ts';
import {AccountRuntime} from '../../nautilus/src/account-runtime.ts';

test('legacy bound launcher RECOVER and one explicit HTTP full snapshot preserve original session and read values', {skip:!process.env.QZ_DECLARED_OWNER_BUILD_RESULT,timeout:120000},async()=>{
 const allowed=new Set(['PATH','TMPDIR','PI_SCRATCH_DIR','QZ_DECLARED_OWNER_BUILD_RESULT','NODE_TEST_CONTEXT','NODE_TEST_WORKER_ID']);
 for(const key of Object.keys(process.env))assert(allowed.has(key),'unexpected inherited environment key: '+key);
 if(process.env.NODE_TEST_CONTEXT!==undefined)assert.equal(process.env.NODE_TEST_CONTEXT,'child-v8');
 if(process.env.NODE_TEST_WORKER_ID!==undefined)assert.equal(process.env.NODE_TEST_WORKER_ID,'1');
 const built=JSON.parse(await readFile(process.env.QZ_DECLARED_OWNER_BUILD_RESULT!,'utf8'));
 assert.equal(built.capabilities.account.compatibilityDeclaration,'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1');assert.equal(built.capabilities.account.compatibilityVerified,false);
 const state=await setup();const evidence:Record<string,any>={sourceCommit:'8f5a46edf89412ea9bf0fd1ea6284240999cd402',directory:state.directory,builtExecutable:built.executable};
 let actualBoot:any=null, actualRuntime:AccountRuntime|null=null; const operations:string[]=[];
 // RECOVER receipt is a separate status projection, not the complete BOOT response.
 // Observe the original protocol response without changing its payload or child.
 const protocol=AccountRuntime.prototype as any,originalSend=protocol.send;
 const observeBoot=mock.method(protocol,'send',async function(this:AccountRuntime,op:string,...args:any[]){operations.push(op); const result=await originalSend.call(this,op,...args);if(op==='BOOT'){actualBoot=structuredClone(result);actualRuntime=this;}return result;});
 let observed:any=null,cancelSends=0;const originalRequest=AccountRuntime.prototype.request;
 const observe=mock.method(AccountRuntime.prototype,'request',async function(this:AccountRuntime,op:string,payload:Record<string,unknown>={}){const result=await originalRequest.call(this,op,payload);if(op==='SNAPSHOT')observed=structuredClone(result);return result;});
 const originalBind=AccountRuntime.prototype.bindPaperCancellationSession;
 const observeCancel=mock.method(AccountRuntime.prototype,'bindPaperCancellationSession',function(this:AccountRuntime,session:string){const bound=originalBind.call(this,session);return {request:(op:'ORDER_CANCEL_ATTEMPT'|'ORDER_OBSERVE',payload:Record<string,unknown>)=>{if(op==='ORDER_CANCEL_ATTEMPT')cancelSends++;return bound.request(op,payload);}};});
 const api='/api/v3/settings/instances',url='/api/v3/accounts/configurations',base=url+'/paper/strategy';
 const get=async(path:string)=>{const r=await state.service.app.inject({url:path,headers:state.headers});assert.equal(r.statusCode,200,r.body);return r.json();};
 const post=async(path:string,payload:Record<string,unknown>)=>state.service.app.inject({method:'POST',url:path,headers:state.headers,payload});
 const command=async(payload:Record<string,unknown>)=>{const r=await post(base+'/commands',payload);assert.equal(r.statusCode,202,r.body);const app=(await state.service.research.harness.snapshot(Application,context))!;const row=Object.values(app.strategyCommands!).find(v=>(v.original as any).requestId===payload.requestId)!;assert(row);await state.service.research.harness.waitForTask(row.taskId as never,context);return get(base+'/commands/'+payload.requestId);};
 try{
  const initial=await get(api);
  const imported=await post(api+'/import',{requestId:'import-original',expectedRevision:initial.revision,environmentId:'declared-synthetic-owner',sourceExecutable:await realpath(built.executable),expectedIdentity:{nautilusSource:built.capabilities.identity.nautilusSource,nautilusVersion:built.capabilities.identity.nautilusVersion,precision:built.capabilities.identity.precision}});
  assert.equal(imported.statusCode,200,imported.body);await state.service.research.harness.waitForTask(imported.json().taskId as never,context);
  const selected=await post(api+'/select-runner',{requestId:'import-original',expectedRevision:initial.revision});assert.equal(selected.statusCode,200,selected.body);
  evidence.runner=state.service.config.nativeProcess;assert.notEqual(evidence.runner.binary,built.executable);assert.equal(evidence.runner.capabilities.account.compatibilityVerified,false);
  const configuration={id:'paper',clientId:'isolated-client',accountId:'HYPERLIQUID-001',mode:'paper',instruments:['HYPE-USDC-SPOT.HYPERLIQUID'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'HYPE-USDC-SPOT.HYPERLIQUID',strategy:{kind:'REGISTERED',name:'owner.account-observer',config:{instrument_id:'HYPE-USDC-SPOT.HYPERLIQUID',trade_size:'20',marker:'declared-isolated-limit-only'}}}};
  const saved=await post(url,{requestId:'create-once',configuration});assert.equal(saved.statusCode,200,saved.body);
  const before=await get(base+'/config');assert.equal(before.lifecycleCapability.supported,null);evidence.beforeLifecycle=before.lifecycleCapability;
  let fence=await get(base+'/fence');assert.equal(fence.ordinaryVenueCancel,null);
  evidence.recovery=await command({requestId:'recover-once',action:'RECOVER',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision});assert.equal(evidence.recovery.status,'COMPLETED');assert.equal(evidence.recovery.receipt.result.recovery_only,true);
  const connection=state.service.config.connections.find(v=>v.id==='paper')!;
  assert(actualRuntime); assert.equal(actualBoot.account_summary_protocol,undefined);
  assert.equal(evidence.recovery.receipt.result.accountObservation.supported,false);
  assert.equal(evidence.recovery.receipt.result.accountObservation.reason,'account_bound_runner_summary_upgrade_required');
  // One deliberate HTTP read, never a periodic fallback or an inferred upgrade.
  assert.equal(operations.includes('SNAPSHOT'),false);
  const body={mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:actualBoot.native_session_id,acknowledgeFullHistoryCost:true};
  const response=await post(base+'/legacy-snapshot',body);assert.equal(response.statusCode,200,response.body);
  const legacy=observed;assert(legacy);const projected=response.json();
  assert.deepEqual(legacy.orders,[]);assert.deepEqual(legacy.fills,[]);assert.deepEqual(legacy.positions,[]);
  assert.equal(projected.nativeSessionId,actualBoot.native_session_id);assert.equal(projected.mode,'paper');
  assert.equal(projected.observationMode,'LEGACY_FULL_SNAPSHOT');assert.equal(projected.historyRead,'FULL_UNBOUNDED');assert.equal(projected.historyIncluded,false);
  assert.equal(projected.automaticRefresh,false);assert.equal(projected.tradePermission,false);
  assert.deepEqual(projected.observation,{cash:legacy.cash??null,equity:legacy.equity??null,nativeVersion:legacy.native_version??null});
  assert.equal(projected.observedAtNs,legacy.observed_at_ns);assert.equal('orders' in projected,false);assert.equal('fills' in projected,false);
  evidence.explicitLegacySnapshot=legacy;evidence.httpObservation=projected;
  for(let i=0;i<2;i++){await assert.rejects(downstreamRequest(connection,'account','GET'),/account_bound_runner_summary_upgrade_required/);await delay(1100);}
  assert.equal(operations.filter(op=>op==='SNAPSHOT').length,1);
  assert.equal(operations.includes('ACCOUNT_SUMMARY'),false); assert.equal(operations.includes('ORDER_STATUS'),false);
  assert.equal(operations.some(op=>['STRATEGY_START','STRATEGY_STOP','CONTROL','ORDER_CANCEL_ATTEMPT','SLICE','TARGET'].includes(op)),false);
  const data=join(state.directory,'accounts','paper');
  const diagnostics=async()=>{const names=(await readdir(data)).filter(n=>n.startsWith('native-stderr-'));return (await Promise.all(names.map(n=>readFile(join(data,n),'utf8')))).join('\n');};
  assert.doesNotMatch(await diagnostics(),/isolated-owner-factory-called|isolated-paper-start/);
  const caps=await downstreamRequest(connection,'capabilities','GET');assert.equal((caps.accountObservation as any).supported,false);evidence.accountObservation=caps.accountObservation;evidence.cancellationCapability=caps.ordinaryVenueCancel;assert.equal((caps.ordinaryVenueCancel as any).scope,'DECLARED_TRUSTED_STRATEGY_ONLY_PAPER_ORDINARY_VENUE');assert.equal((caps.ordinaryVenueCancel as any).compatibilityVerified,false);
  const lifecycle=(await get(base+'/config')).lifecycleCapability;
  assert.deepEqual(lifecycle,{configuredKind:'REGISTERED',supported:true,reason:null,scope:'INSTALLED_DISPATCH_ONLY',strategyVerified:false,nativeSessionId:evidence.recovery.receipt.result.native_session_id});
  assert(actualBoot);assert.equal(actualBoot.native_session_id,evidence.recovery.receipt.result.native_session_id);
  assert.deepEqual(lifecycle,{...actualBoot.strategy_lifecycle,nativeSessionId:actualBoot.native_session_id});evidence.bootLifecycle=actualBoot.strategy_lifecycle;
  for(const [key,value]of Object.entries(lifecycle))assert.deepEqual((caps.nativeStrategy as any)[key],value);evidence.lifecycle=lifecycle;
  fence=await get(base+'/fence');
  const page=await get(base+'/orders?offset=0');
  assert.equal(page.available,true);assert.deepEqual(page.items,[]);assert.equal(page.nextOffset,null);assert.equal(page.total,null);
  assert.deepEqual(page.binding,fence.ordinaryVenueCancel);assert.equal(page.binding.nativeSessionId,evidence.recovery.receipt.result.native_session_id);
  assert.equal(page.binding.buildId,evidence.runner.binary);assert.equal(page.capability.compatibilityVerified,false);
  evidence.orderPage=page;assert.equal(cancelSends,0);
  const app=(await state.service.research.harness.snapshot(Application,context))!;
  const commands=Object.values(app.strategyCommands!);assert.equal(commands.length,1);assert.equal((commands[0]!.original as any).action,'RECOVER');
  assert.doesNotMatch(await diagnostics(),/isolated-owner-factory-called|isolated-paper-start/);
  await state.restart();const offline=await get(base+'/orders?offset=0');assert.equal(offline.available,false);assert.deepEqual(offline.items,[]);
  assert.equal(cancelSends,0);evidence.reopenedOrderPage=offline;
  const reopened=(await get(base+'/config')).lifecycleCapability;assert.equal(reopened.supported,null);assert.equal(reopened.nativeSessionId,null);evidence.reopenedLifecycle=reopened;
  const afterReopen=[...operations];
  const unavailable=await post(base+'/legacy-snapshot',body);assert.notEqual(unavailable.statusCode,200);
  assert.deepEqual(operations,afterReopen,'explicit old-session read after reopen cannot BOOT or SNAPSHOT');
  evidence.operations=operations;assert.deepEqual(operations,['BOOT','RECOVERY_STATUS','SNAPSHOT','ORDER_PAGE','SHUTDOWN']);console.log('legacy_summary_http_recover_only_confirmed');
 }catch(error){evidence.error=String(error);throw error;}
 finally{try{await state.service.close();}finally{observe.mock.restore();observeCancel.mock.restore();observeBoot.mock.restore();await writeFile(join(state.directory,'legacy-summary-evidence.json'),JSON.stringify(evidence,null,2));console.log('preserved_evidence_directory='+state.directory);}}
});

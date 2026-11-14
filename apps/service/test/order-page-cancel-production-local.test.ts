/** Opt-in real generated owner launcher, real import and production main.
 * Dedicated isolated paper only; no launch mocking, signing key or test Rust.
 * The specified synthetic owner factory emits one resting QZ-PAPER limit order.
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

test('production ORDER_PAGE cancellation preserves official event JSON through SQLite reopen', {skip:!process.env.QZ_DECLARED_OWNER_BUILD_RESULT,timeout:120000},async()=>{
 const allowed=new Set(['PATH','TMPDIR','PI_SCRATCH_DIR','QZ_DECLARED_OWNER_BUILD_RESULT','NODE_TEST_CONTEXT','NODE_TEST_WORKER_ID']);
 for(const key of Object.keys(process.env))assert(allowed.has(key),'unexpected inherited environment key: '+key);
 if(process.env.NODE_TEST_CONTEXT!==undefined)assert.equal(process.env.NODE_TEST_CONTEXT,'child-v8');
 if(process.env.NODE_TEST_WORKER_ID!==undefined)assert.equal(process.env.NODE_TEST_WORKER_ID,'1');
 const built=JSON.parse(await readFile(process.env.QZ_DECLARED_OWNER_BUILD_RESULT!,'utf8'));
 assert.equal(built.capabilities.account.compatibilityDeclaration,'TRUSTED_STRATEGY_ONLY_SINGLE_NODE_V1');assert.equal(built.capabilities.account.compatibilityVerified,false);
 const state=await setup();const evidence:Record<string,any>={directory:state.directory,builtExecutable:built.executable};
 let observed:any=null,cancelSends=0;const originalRequest=AccountRuntime.prototype.request;
 const observe=mock.method(AccountRuntime.prototype,'request',async function(this:AccountRuntime,op:string,payload:Record<string,unknown>={}){const result=await originalRequest.call(this,op,payload);if(op==='ACCOUNT_SUMMARY')observed=structuredClone(result);return result;});
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
  let fence=await get(base+'/fence');assert.equal(fence.ordinaryVenueCancel,null);
  evidence.recovery=await command({requestId:'recover-once',action:'RECOVER',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision});assert.equal(evidence.recovery.status,'COMPLETED');assert.equal(evidence.recovery.receipt.result.recovery_only,true);
  const connection=state.service.config.connections.find(v=>v.id==='paper')!;
  let ready=false;
  for(let i=0;i<30;i++){const previous=observed;try{await downstreamRequest(connection,'account','GET');ready=observed!==previous&&observed?.ready===true&&observed.recovery_required===false;}catch{ready=false;}if(ready)break;await delay(1000);}
  assert.equal(ready,true,'No START without successful current account read and fresh official observation');assert.equal(observed.recovery_required,false);assert.deepEqual(observed.official_node_order_activity,{open:false,inflight:false,active_local:false,emulated:false});assert.equal('orders' in observed,false);assert.equal('fills' in observed,false);assert.deepEqual(observed.positions,[]);
  const data=join(state.directory,'accounts','paper');
  const diagnostics=async()=>{const names=(await readdir(data)).filter(n=>n.startsWith('native-stderr-'));return (await Promise.all(names.map(n=>readFile(join(data,n),'utf8')))).join('\n');};
  assert.doesNotMatch(await diagnostics(),/isolated-owner-factory-called|isolated-paper-start/);
  const caps=await downstreamRequest(connection,'capabilities','GET');evidence.cancellationCapability=caps.ordinaryVenueCancel;assert.equal((caps.ordinaryVenueCancel as any).scope,'DECLARED_TRUSTED_STRATEGY_ONLY_PAPER_ORDINARY_VENUE');assert.equal((caps.ordinaryVenueCancel as any).compatibilityVerified,false);
  fence=await get(base+'/fence');evidence.start=await command({requestId:'start-once',action:'START',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision});assert.equal(evidence.start.status,'COMPLETED');
  let page:any=null;
  for(let i=0;i<30;i++){const current=await get(base+'/orders?offset=0');page=current;if(current.items.some((v:any)=>v.client_order_id==='owner-resting-original'&&v.status==='ACCEPTED'))break;await delay(500);}
  assert.equal(page.available,true);assert.equal(page.mode,'paper');assert.equal(page.capability.supported,true);assert.equal(page.capability.compatibilityVerified,false);
  assert.equal(page.items.length,1);assert.equal(page.nextOffset,null);assert.equal(page.total,null);
  const order=page.items.find((v:any)=>v.client_order_id==='owner-resting-original');assert.equal(order?.status,'ACCEPTED');assert.ok(order.venue_order_id);
  assert.equal(order.is_active_local,false);assert.equal(order.emulation_trigger,null);assert.equal(order.exec_algorithm_id,null);
  assert.equal(order.account_id,'HYPERLIQUID-001');assert.equal(order.strategy_id,'OWNER-001');assert.equal(order.instrument_id,'HYPE-USDC-SPOT.HYPERLIQUID');
  evidence.orderPage=page;evidence.originalOrder=order;
  const original={schemaVersion:1,requestId:'cancel-once',action:'CANCEL',strategyId:order.strategy_id,order:{traderId:order.trader_id,accountId:order.account_id,instrumentId:order.instrument_id,clientOrderId:order.client_order_id},binding:page.binding};
  assert.equal(original.binding.buildId,evidence.runner.binary);assert.equal(original.binding.nativeSessionId,evidence.recovery.receipt.result.native_session_id);evidence.original=original;
  for(const key of ['sourceId','buildId','nativeSessionId'])assert.equal((await post(base+'/commands',{...original,requestId:'wrong-'+key,binding:{...original.binding,[key]:'wrong'}})).statusCode,409);assert.equal(cancelSends,0);
  let receipt=await command(original);for(let i=0;i<100;i++){if(receipt.receipt.orderObservation?.status==='CANCELED')break;await delay(20);receipt=await get(base+'/commands/cancel-once');}
  const rawEvent=receipt.receipt.orderObservation?.lastEventJson;
  assert.equal(typeof rawEvent,'string','Updated formal runner must preserve official serde text');
  const serialized=JSON.parse(rawEvent);assert.deepEqual(Object.keys(serialized),['Canceled']);const event=serialized.Canceled;
  assert.equal(event.client_order_id,original.order.clientOrderId);assert.equal(event.strategy_id,original.strategyId);
  assert.equal(event.trader_id,original.order.traderId);assert.equal(event.instrument_id,original.order.instrumentId);
  // Never convert these original decimal tokens to JavaScript Number.
  for(const field of ['ts_event','ts_init']){const token=rawEvent.match(new RegExp('"'+field+'"\\s*:\\s*(\\d+)'));assert(token,field);assert.ok(token[1]!.length>15);}
  evidence.preciseOfficialSerdeText=rawEvent;
  evidence.receipt=receipt;evidence.cancelDispatchCalls=cancelSends;assert.equal(receipt.status,'UNCERTAIN');assert.equal(receipt.receipt.orderObservation.status,'CANCELED');assert.equal(receipt.receipt.orderObservation.filledQuantity,order.filled_quantity);assert.equal(cancelSends,1);evidence.receipt=receipt;
  await state.restart();assert.equal((await post(base+'/commands',original)).statusCode,202);const reopened=await get(base+'/commands/cancel-once');assert.equal(reopened.status,'UNCERTAIN');assert.deepEqual(reopened.receipt.orderObservation,receipt.receipt.orderObservation);assert.equal(reopened.receipt.orderObservation.lastEventJson,rawEvent);assert.equal(reopened.receipt.error,'native_cancel_original_session_unavailable');assert.equal(cancelSends,1);evidence.reopened=reopened;const history=await get(base+'/commands');assert.equal(history.items.length,1);assert.equal(history.items[0].requestId,original.requestId);assert.equal(history.items[0].status,'UNCERTAIN');assert.deepEqual(history.items[0].order,original.order);assert.equal((await get(base+'/orders?offset=0')).available,false);evidence.reopenedHistory=history;
  console.log('production_order_page_exact_event_cancel_once_unknown_confirmed');
 }catch(error){evidence.error=String(error);throw error;}
 finally{try{await state.service.close();}finally{observe.mock.restore();observeCancel.mock.restore();await writeFile(join(state.directory,'order-page-cancel-evidence.json'),JSON.stringify(evidence,null,2));console.log('preserved_evidence_directory='+state.directory);}}
});

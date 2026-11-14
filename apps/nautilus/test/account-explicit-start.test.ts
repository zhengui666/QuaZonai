import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, writeFile, readFile, rm, mkdir, open } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { IncomingMessage } from 'node:http';
import { createCapitalAccount } from '../src/capital-account.ts';
import type { AccountOptions } from '../src/account-runtime.ts';
const options:AccountOptions={id:'explicit',clientId:'client',accountId:'account',mode:'paper',startingBalance:'1000',paperFeeRate:'0.001',instruments:['TEST.SIM'],nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
const quote=(s:string)=>"'"+s.replaceAll("'","'\\''")+"'";
const sleep=(ms:number)=>new Promise(resolve=>setTimeout(resolve,ms));

// Real process transport and persisted receipt ordering; no venue or market claims.
test('direct account calls never spawn on queries/reopen; explicit START uses the single runner account-runtime mode',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-explicit-account-'));const marker=join(dir,'starts'),script=join(dir,'peer.mjs'),binary=join(dir,'peer');
 await writeFile(script,`import fs from 'node:fs';import readline from 'node:readline';
const marker=${JSON.stringify(marker)},statePath=${JSON.stringify(join(dir,'capital-receipts.json'))};
if(process.argv[2]!=='account-runtime')throw Error('single_runner_account_mode_required');
fs.appendFileSync(marker,'spawn\\n');
readline.createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);let result={};
if(v.operation==='BOOT'){const state=JSON.parse(fs.readFileSync(statePath));if(!Object.values(state.strategyCommands??{}).some(x=>x.original.action==='START'&&x.receipt.status==='UNCERTAIN'))throw Error('start_intent_not_persisted');result={ready:true,native_session_id:'original',account_summary_protocol:1};}
else if(v.operation==='ACCOUNT_SUMMARY')result={native_session_id:'original',native_account_id:'account',official_node_order_activity:{open:false,inflight:false,active_local:false,emulated:false},ready:true,recovery_required:false,sequence:'1',observation_id:'original-observation',observed_at_ns:'1',native_version:'protocol-only',orders:[],positions:[],instruments:[],fills:[],cash:'1000',equity:'1000'};
else if(v.operation==='CONTROL')result={applied:true};
else if(v.operation==='STRATEGY_START'||v.operation==='STRATEGY_STATUS')result={strategy_id:'original-strategy',state:'Running',native_version:'protocol-only'};
console.log(JSON.stringify({requestId:v.requestId,result}));if(v.operation==='SHUTDOWN')process.exit(0);
});`);
 await writeFile(binary,`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)} "$@"\n`,{mode:0o700});
 let account=await createCapitalAccount(options,dir,binary);
 const call=(path:string,body?:Record<string,unknown>)=>account.request(path,body?'POST':'GET','client',body);
 try{
  await assert.rejects(account.request('/strategy/fence','GET','wrong-client'),/account_client_identity_mismatch/);
  let readUnauthorizedBody=false;await assert.rejects(account.handle('/strategy/commands',{method:'POST',headers:{'x-qz-client-id':'wrong-client'}} as unknown as IncomingMessage,async()=>{readUnauthorizedBody=true;return {};}),/account_client_identity_mismatch/);assert.equal(readUnauthorizedBody,false);
  for(const path of ['/capabilities','/strategy/fence','/capital-exits'])assert.equal((await call(path)).status,200);
  await assert.rejects(call('/account'),/account_runtime_not_started/);
  await assert.rejects(call('/strategy'),/account_runtime_not_started/);
  await sleep(1100);await assert.rejects(readFile(marker),/ENOENT/);
  const fence=(await call('/strategy/fence')).value as any;
  const start={requestId:'start-original',action:'START',clientId:'client',accountId:'account',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision};
  const stale=await call('/strategy/commands',{...start,requestId:'stale',expectedStrategyRevision:'999'});assert.equal((stale.value as any).status,'REJECTED');
  await assert.rejects(readFile(marker),/ENOENT/);
  const started=await call('/strategy/commands',start);assert.equal((started.value as any).status,'COMPLETED',JSON.stringify(started));
  assert.equal(await readFile(marker,'utf8'),'spawn\n');
  const liveConfig=(await call('/strategy/config')).value as any;
  await assert.rejects(call('/strategy/config',{requestId:'cannot-edit-connected',clientId:'client',accountId:'account',expectedAccountSourceId:liveConfig.accountSourceId,expectedStrategyRevision:liveConfig.strategyRevision,configuration:liveConfig.configuration}),/requires_disconnected_runtime/);
  await call('/strategy');await call('/strategy/commands',start);assert.equal(await readFile(marker,'utf8'),'spawn\n');
  await account.close();account=await createCapitalAccount(options,dir,binary);
  await call('/capital-exits');await call('/strategy/commands/start-original');await call('/strategy/commands',start);await sleep(30);
  assert.equal(await readFile(marker,'utf8'),'spawn\n','reopen and original receipt replay do not restart');
  const stopped=await call('/strategy/commands',{requestId:'stop-unstarted',action:'STOP',clientId:'client',accountId:'account'});
  assert.equal((stopped.value as any).status,'UNCERTAIN');assert.match(String((stopped.value as any).error),/not_started/);
  assert.equal(await readFile(marker,'utf8'),'spawn\n');
 }finally{await account.close();await rm(dir,{recursive:true,force:true});}
});

test('unstarted historical UNKNOWN orders stay readable and cancellation cannot fake settlement or spawn',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-explicit-history-'));const marker=join(dir,'spawned'),binary=join(dir,'must-not-launch');
 await writeFile(binary,`#!/bin/sh\nprintf spawned > ${quote(marker)}\nexit 1\n`,{mode:0o700});
 const funds={requested_amount:{amount:'123',currency:'USDC'},reserved_amount:{amount:'123',currency:'USDC'},actual_fees:{amount:'0.25',currency:'USDC'}};
 const original={requestId:'original',clientId:'client',accountId:'account',command:{preview_id:'original-preview'}};
 const saved={schema_version:2,sourceId:'source',identity:{mode:'paper',network:'mainnet',accountId:'account',accountAddress:null,currency:'USDC'},epoch:'7',revision:'7',strategyRevision:'1',previews:{},targets:{'original-target':{original:{target:{valid_until_ns:'1'}},receipt:{status:'PENDING',requestId:'target-original'},epoch:'7'}},ledgerClaims:{},reverseTransferPending:false,commands:{original:{original,state:'UNCERTAIN',error:'original_unknown',exitId:'exit-original'}},exits:{'exit-original':{view:{id:'exit-original',revision:'1',state:'REDUCING',policy:{kind:'BOUNDED_LIMIT',pacing:{minimum_interval_seconds:5},deadline:'2030-01-01T00:00:00Z'},funds,events:[],progress:[],created_at:'2026-01-01T00:00:00Z'},childIds:['order-original'],pendingSlice:{id:'order-original'},fillCosts:{original:{fee:'0.25'}},sliceBasis:{original:{reference_bid:'100'}},executionCost:'0.25'}}};
 await writeFile(join(dir,'capital-receipts.json'),JSON.stringify(saved));
 let account=await createCapitalAccount(options,dir,binary);
 const call=(path:string,body?:Record<string,unknown>)=>account.handle(path,{method:body?'POST':'GET',headers:{'x-qz-client-id':'client'}} as unknown as IncomingMessage,async()=>body!);
 try{
  for(const reopen of [false,true]){
   if(reopen){await account.close();account=await createCapitalAccount(options,dir,binary);}
   assert.equal((await call('/capital-exits/commands/original')).status,200);
   const exit=(await call('/capital-exits/exit-original')).value as any;assert.deepEqual(exit.exit.funds,funds);
   const target=await call('/targets/original-target');assert.equal((target.value as any).status,'PENDING');
   await sleep(30);await assert.rejects(readFile(marker),/ENOENT/);
  }
  const stop=await call('/strategy/commands',{requestId:'stop',action:'STOP',clientId:'client',accountId:'account'});
  assert.equal((stop.value as any).status,'UNCERTAIN');
  await assert.rejects(call('/targets/original-target/cancel-protection',{clientId:'client',accountId:'account'}),/account_runtime_not_started/);
  const fence=(await call('/strategy/fence')).value as any;
  await assert.rejects(call('/strategy/commands',{requestId:'blocked-start',action:'START',clientId:'client',accountId:'account',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision}),/capital_original_command_unsettled/);
  await assert.rejects(readFile(marker),/ENOENT/);
  const retained=JSON.parse(await readFile(join(dir,'capital-receipts.json'),'utf8'));
  assert.equal(retained.commands.original.state,'UNCERTAIN');assert.equal(retained.targets['original-target'].receipt.status,'PENDING');
  assert.deepEqual(retained.exits['exit-original'].view.funds,funds);assert.deepEqual(retained.exits['exit-original'].childIds,['order-original']);assert.deepEqual(retained.exits['exit-original'].pendingSlice,{id:'order-original'});
 }finally{await account.close();await rm(dir,{recursive:true,force:true});}
});

for (const summaryProtocol of [true, false]) for (const recoveryRequired of [true, false]) test(`explicit RECOVER connects once without automatic writes (summary=${summaryProtocol}, recovery_required=${recoveryRequired}); UNKNOWN survives reopen`,async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-explicit-recover-'));const marker=join(dir,'operations'),script=join(dir,'peer.mjs'),binary=join(dir,'peer');
 await writeFile(script,`import fs from 'node:fs';import readline from 'node:readline';
const marker=${JSON.stringify(marker)},statePath=${JSON.stringify(join(dir,'capital-receipts.json'))};fs.appendFileSync(marker,'spawn\\n');
readline.createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);fs.appendFileSync(marker,v.operation+'\\n');let result={};
if(v.operation==='BOOT'){const s=JSON.parse(fs.readFileSync(statePath));if(!Object.values(s.strategyCommands??{}).some(x=>x.original.action==='RECOVER'&&x.receipt.status==='UNCERTAIN'))throw Error('intent_not_persisted');result={ready:true,recovery_only:true,native_session_id:'original',account_summary_protocol:${summaryProtocol ? 1 : 'undefined'}};}
else if(v.operation==='ACCOUNT_SUMMARY')result={native_session_id:'original',native_account_id:'account',official_node_order_activity:{open:false,inflight:false,active_local:false,emulated:false},ready:true,recovery_required:${JSON.stringify(recoveryRequired)},sequence:'original',observation_id:'original',observed_at_ns:'1',native_version:'protocol-only',orders:[{client_order_id:'original-order',status:'UNKNOWN'}],positions:[],instruments:[],fills:[],cash:null,equity:null};
else if(v.operation==='SNAPSHOT')result={native_session_id:'original',native_account_id:'account',orders:[{client_order_id:'old',events:['large-history-not-forwarded']}],fills:[],observed_at_ns:'1794605191057890704',cash:'0.000000000000000001',equity:null,native_version:'old'};
else if(v.operation==='RECOVERY_STATUS')result={ready:true,recovery_only:true,native_version:'protocol-only',native_session_id:'recovery-session'};
else if(v.operation==='TARGET_STATUS')result={status:'UNKNOWN'};
else if(v.operation==='CANCEL_PROTECTION')result={settled:false};
else if(v.operation!=='SHUTDOWN')throw Error('unexpected_write:'+v.operation);
console.log(JSON.stringify({requestId:v.requestId,result}));if(v.operation==='SHUTDOWN')process.exit(0);});`);
 await writeFile(binary,`#!/bin/sh\nexec ${quote(process.execPath)} ${quote(script)}\n`,{mode:0o700});
 let account=await createCapitalAccount(options,dir,binary);await account.close();
 const path=join(dir,'capital-receipts.json'),saved=JSON.parse(await readFile(path,'utf8'));
 saved.commands.original={original:{requestId:'original',command:{action:'CANCEL'}},state:'UNCERTAIN',error:'UNKNOWN',exitId:'original-exit'};
 saved.targets.original={original:{target:{valid_until_ns:'1'}},receipt:{status:'PENDING',requestId:'original-target'},epoch:'1'};
 await writeFile(path,JSON.stringify(saved));account=await createCapitalAccount(options,dir,binary);
 const call=(path:string,body?:Record<string,unknown>)=>account.handle(path,{method:body?'POST':'GET',headers:{'x-qz-client-id':'client'}} as unknown as IncomingMessage,async()=>body!);
 try{
  await sleep(30);await assert.rejects(readFile(marker),/ENOENT/);
  const fence=(await call('/strategy/fence')).value as any;
  const recover={requestId:'recover-original',action:'RECOVER',clientId:'client',accountId:'account',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision};
  const result=await call('/strategy/commands',recover);assert.equal((result.value as any).status,'COMPLETED',JSON.stringify(result));
  await call('/strategy/commands',recover);await sleep(1100);
  let ops=(await readFile(marker,'utf8')).trim().split('\n');assert.equal(ops.filter(x=>x==='spawn').length,1);
  assert.ok(ops.every(x=>['spawn','BOOT','ACCOUNT_SUMMARY','RECOVERY_STATUS','TARGET_STATUS'].includes(x)),ops.join(','));
  if (!summaryProtocol) {
    assert.equal(ops.includes('ACCOUNT_SUMMARY'),false); assert.equal(ops.includes('SNAPSHOT'),false);
    assert.equal((result.value as any).result.accountObservation.supported,false);
    assert.equal((result.value as any).result.accountObservation.reason,'account_bound_runner_summary_upgrade_required');
    const caps=(await call('/capabilities')).value as any; assert.equal(caps.accountObservation.supported,false);
    const configuration=(await call('/strategy/config')).value as any; assert.deepEqual(configuration.accountObservation,caps.accountObservation);
    await assert.rejects(call('/account'),/account_bound_runner_summary_upgrade_required/);
    const explicit={clientId:'client',accountId:'account',mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:'original',acknowledgeFullHistoryCost:true};
    await assert.rejects(call('/strategy/legacy-snapshot',{...explicit,acknowledgeFullHistoryCost:false}),/explicit_ack_required/);
    await assert.rejects(call('/strategy/legacy-snapshot',{...explicit,expectedNativeSessionId:'wrong'}),/original_session_required/);
    const one=(await call('/strategy/legacy-snapshot',explicit)).value as any;
    assert.equal(one.observation.cash,'0.000000000000000001');assert.equal(one.observation.equity,null);
    assert.equal(one.observedAtNs,'1794605191057890704');assert.equal(one.historyRead,'FULL_UNBOUNDED');assert.equal(one.historyIncluded,false);
    assert.equal(one.tradePermission,false);assert.equal('orders' in one,false);assert.equal(JSON.stringify(one).includes('large-history'),false);
    await sleep(1100);await assert.rejects(call('/account'),/summary_upgrade_required/);
    assert.equal((await readFile(marker,'utf8')).split('\n').filter(x=>x==='SNAPSHOT').length,1,'only explicit read, not periodic fallback');

  }
  const after=JSON.parse(await readFile(path,'utf8'));assert.deepEqual(after.commands.original,saved.commands.original);assert.equal(after.targets.original.receipt.status,'PENDING');
  const current=(await call('/strategy/fence')).value as any;
  await assert.rejects(call('/strategy/commands',{...recover,requestId:'blocked',action:'START',expectedStrategyRevision:current.strategyRevision}),/capital_original_command_unsettled/);
  await call('/targets/original/cancel-protection',{clientId:'client',accountId:'account'});
  ops=(await readFile(marker,'utf8')).trim().split('\n');assert.equal(ops.filter(x=>x==='CANCEL_PROTECTION').length,1,'cancellation only follows explicit request');
  await account.close();account=await createCapitalAccount(options,dir,binary);
  await call('/strategy/commands',recover);await call('/capital-exits/commands/original');await sleep(30);
  assert.equal((await readFile(marker,'utf8')).split('\n').filter(x=>x==='spawn').length,1,'replay does not reconnect');
 }finally{await account.close();await rm(dir,{recursive:true,force:true});}
});

test('saving registered configuration preserves arbitrary native JSON and never launches an account',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-account-config-'));const marker=join(dir,'spawn'),binary=join(dir,'never-start');
 await writeFile(binary,`#!/bin/sh\nprintf started > ${quote(marker)}\nexit 1\n`,{mode:0o700});
 let account=await createCapitalAccount({...options},dir,binary);
 const call=(path:string,body?:Record<string,unknown>)=>account.handle(path,{method:body?'POST':'GET',headers:{'x-qz-client-id':'client'}} as unknown as IncomingMessage,async()=>body!);
 try{
  const current=(await call('/strategy/config')).value as any;
  const configuration={instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'user::strategy',config:['0.000000000000000001',{custom:true},null]}};
  const command={requestId:'config-original',clientId:'client',accountId:'account',expectedAccountSourceId:current.accountSourceId,expectedStrategyRevision:current.strategyRevision,configuration};
  const saved=(await call('/strategy/config',command)).value as any;assert.equal(saved.status,'COMPLETED');assert.equal(saved.runnerValidated,false);
  assert.deepEqual(saved.configuration,configuration);assert.deepEqual((await call('/strategy/config',command)).value,saved);
  await assert.rejects(call('/strategy/config',{...command,configuration:{...configuration,instrument_id:'OTHER.SIM'}}),/strategy_request_id_conflict/);
  await assert.rejects(call('/strategy/config',{...command,requestId:'stale'}),/strategy_start_fence_changed/);
  await account.close();account=await createCapitalAccount({...options},dir,binary);
  const reopened=(await call('/strategy/config')).value as any;assert.deepEqual(reopened.configuration,configuration);assert.equal(reopened.connected,false);
  await sleep(30);await assert.rejects(readFile(marker),/ENOENT/);
  assert.deepEqual((await call('/strategy/config',command)).value,saved,'original config acknowledgement does not execute it');
 }finally{await account.close();await rm(dir,{recursive:true,force:true});}
});


test('failed configuration persistence never becomes a completed replay or replaces the original config',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-config-write-failure-'));
 const binary=join(dir,'missing-binary');
 let account=await createCapitalAccount({...options},dir,binary);
 const call=(path:string,body?:Record<string,unknown>)=>account.handle(path,{method:body?'POST':'GET',headers:{'x-qz-client-id':'client'}} as unknown as IncomingMessage,async()=>body!);
 try {
  const original=(await call('/strategy/config')).value as any;
  const originalBytes=await readFile(join(dir,'capital-receipts.json'),'utf8');
  await mkdir(join(dir,'capital-receipts.json.next'));
  const command={requestId:'failed-save',clientId:'client',accountId:'account',expectedAccountSourceId:original.accountSourceId,expectedStrategyRevision:original.strategyRevision,configuration:{instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'different',config:null}}};
  await assert.rejects(call('/strategy/config',command));
  await assert.rejects(call('/strategy/config',command),/account_receipts_write_failed/);
  await assert.rejects(call('/strategy/commands/failed-save'),/strategy_command_not_found/);
  const after=(await call('/strategy/config')).value as any;
  assert.deepEqual(after.configuration,original.configuration);assert.equal(after.strategyRevision,original.strategyRevision);
  assert.equal(await readFile(join(dir,'capital-receipts.json'),'utf8'),originalBytes);
  await assert.rejects(account.close(),/account_receipts_write_failed/);
  await rm(join(dir,'capital-receipts.json.next'),{recursive:true});
  account=await createCapitalAccount({...options},dir,binary);
  const reopened=(await call('/strategy/config')).value as any;
  assert.deepEqual(reopened.configuration,original.configuration);assert.equal(reopened.strategyRevision,original.strategyRevision);
  await assert.rejects(call('/strategy/commands/failed-save'),/strategy_command_not_found/);
 } finally { await account.close().catch(()=>{});await rm(dir,{recursive:true,force:true}); }
});


for (const failAt of [2, 3]) test(`directory sync failure after rename retains the original configuration request (save ${failAt})`, async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-config-rename-failure-'));let syncs=0, failAtSync=failAt;
 const syncDirectory=async(path:string)=>{ const handle=await open(path,'r');try {if(++syncs===failAtSync)throw new Error('injected_directory_sync_failure');await handle.sync();}finally{await handle.close();} };
 let account=await createCapitalAccount({...options},dir,join(dir,'missing-binary'),undefined,undefined,syncDirectory);
 const call=(path:string,body?:Record<string,unknown>)=>account.handle(path,{method:body?'POST':'GET',headers:{'x-qz-client-id':'client'}} as unknown as IncomingMessage,async()=>body!);
 try {
  const original=(await call('/strategy/config')).value as any;
  const configuration={instrument_id:'TEST.SIM',strategy:{kind:'REGISTERED',name:'new-native',config:null}};
  const command={requestId:'partial-save',clientId:'client',accountId:'account',expectedAccountSourceId:original.accountSourceId,expectedStrategyRevision:original.strategyRevision,configuration};
  const result=(await call('/strategy/config',command)).value as any;
  assert.equal(result.status,'UNCERTAIN');assert.equal(result.persisted,false);
  assert.deepEqual((await call('/strategy/config',command)).value,result);
  assert.deepEqual((await call('/strategy/commands/partial-save')).value,result);
  const after=(await call('/strategy/config')).value as any;assert.deepEqual(after.configuration,configuration);
  const disk=JSON.parse(await readFile(join(dir,'capital-receipts.json'),'utf8'));
  assert.deepEqual(disk.strategyConfiguration,configuration);assert.equal(disk.strategyCommands['partial-save'].receipt.status,failAt===2?'UNCERTAIN':'COMPLETED');
  await assert.rejects(call('/strategy/commands',{requestId:'must-not-trade',action:'START',clientId:'client',accountId:'account',expectedAccountSourceId:after.accountSourceId,expectedStrategyRevision:after.strategyRevision}),/configuration_save_unsettled/);
  await account.close().catch(()=>{});account=await createCapitalAccount({...options},dir,join(dir,'missing-binary'),undefined,undefined,syncDirectory);
  const reopened=(await call('/strategy/config')).value as any;assert.deepEqual(reopened.configuration,configuration);
  const receipt=(await call('/strategy/commands/partial-save')).value as any;
  assert.equal(receipt.status,failAt===2?'UNCERTAIN':'COMPLETED','Completion is only written after the configuration itself was durably synced.');
  if (failAt === 2) {
    const path=join(dir,'capital-receipts.json'),bytes=await readFile(path,'utf8'),changed=JSON.parse(bytes);
    changed.strategyConfiguration.strategy.name='not-original';await writeFile(path,JSON.stringify(changed));
    await assert.rejects(call('/strategy/config/commands/partial-save/verify',{clientId:'client',accountId:'account'}),/original_persistence_mismatch/);
    assert.equal(((await call('/strategy/commands/partial-save')).value as any).status,'UNCERTAIN');
    await writeFile(path,bytes);
  }
  if (failAt === 2) {
    failAtSync=syncs+1;
    await assert.rejects(call('/strategy/config/commands/partial-save/verify',{clientId:'client',accountId:'account'}),/injected_directory_sync_failure/);
    assert.equal(((await call('/strategy/commands/partial-save')).value as any).status,'UNCERTAIN');
    failAtSync=syncs+2;
    const stillUnknown=(await call('/strategy/config/commands/partial-save/verify',{clientId:'client',accountId:'account'})).value as any;
    assert.equal(stillUnknown.status,'UNCERTAIN');assert.equal(stillUnknown.persisted,false);
  }
  const verified=(await call('/strategy/config/commands/partial-save/verify',{clientId:'client',accountId:'account'})).value as any;
  assert.equal(verified.status,'COMPLETED');assert.deepEqual(verified.configuration,configuration);
  assert.deepEqual((await call('/strategy/config/commands/partial-save/verify',{clientId:'client',accountId:'account'})).value,verified);
  assert.deepEqual(((await call('/strategy/config')).value as any).pendingConfigurationRequests,[]);
  const finalDisk=JSON.parse(await readFile(join(dir,'capital-receipts.json'),'utf8'));
  assert.equal(finalDisk.strategyCommands['partial-save'].receipt.status,'COMPLETED');

 }finally{await account.close().catch(()=>{});await rm(dir,{recursive:true,force:true});}
});

test('explicit legacy observation fails closed on changed identity and does not queue STOP or repeat a failed read',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-legacy-observation-'));let session='original',release:((v:Record<string,unknown>)=>void)|undefined;
 const calls:string[]=[];const runtime={accountObservationCapability:()=>({supported:false,nativeSessionId:session,reason:'account_bound_runner_summary_upgrade_required'}),
  request:async(operation:string)=>{calls.push(operation);if(operation==='SNAPSHOT')return new Promise<Record<string,unknown>>(resolve=>{release=resolve;});if(operation==='STRATEGY_STOP')return {strategy_id:'original-strategy',state:'STOPPED',native_version:'controlled'};throw Error('unexpected:'+operation);},close:async()=>{}};
 const account=await createCapitalAccount(options,dir,'unused',runtime);
 const payload={clientId:'client',accountId:'account',mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:'original',acknowledgeFullHistoryCost:true};
 const call=()=>account.request('/strategy/legacy-snapshot','POST','client',payload);
 const frame={native_session_id:'original',native_account_id:'account',orders:[],fills:[],observed_at_ns:'1794605191057890704',cash:'1',equity:null};
 try{
  const pending=call();await assert.rejects(call(),/read_in_progress/);assert.deepEqual(calls,['SNAPSHOT']);
  const stopped=await account.request('/strategy/commands','POST','client',{requestId:'stop-original',action:'STOP',clientId:'client',accountId:'account'});
  assert.equal((stopped.value as any).status,'COMPLETED','STOP dispatch not queued behind legacy read (runtime itself is controlled here)');
  session='changed';release!(frame);await assert.rejects(pending,/binding_changed/);session='original';
  for(const invalid of [{...frame,native_session_id:'wrong'},{...frame,native_account_id:'wrong'},{...frame,fills:null}]) {
   const read=call();release!(invalid);await assert.rejects(read,/binding_changed|invalid/);
  }
  const before=await readFile(join(dir,'capital-receipts.json'),'utf8');
  const read=call();release!(frame);assert.equal(((await read).value as any).observation.cash,'1');
  assert.equal(await readFile(join(dir,'capital-receipts.json'),'utf8'),before,'observation never rewrites receipts/history');
  await sleep(1100);assert.equal(calls.filter(x=>x==='SNAPSHOT').length,5,'background does not resend legacy reads');
 }finally{await account.close();await rm(dir,{recursive:true,force:true});}
});

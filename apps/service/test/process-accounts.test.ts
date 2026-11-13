import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm,mkdir,rename,open} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {processAccounts} from '../src/process-accounts.ts';
import {configDefaults} from '../src/config.ts';
import {downstreamRequest} from '../src/downstream.ts';

test('direct process account configuration preserves identity, never launches on save/read/reopen and RECOVER is explicit',{timeout:15000},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-direct-account-binding-')),binary=join(dir,'peer.mjs'),marker=join(dir,'calls');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';import{createInterface}from'node:readline';if(process.argv[2]!=='account-runtime')throw Error('wrong-mode');appendFileSync(${JSON.stringify(marker)},'spawn\\n');createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);appendFileSync(${JSON.stringify(marker)},v.operation+'\\n');const result=v.operation==='SNAPSHOT'?{ready:true,recovery_required:true,sequence:'1',observation_id:'original',observed_at_ns:'1',native_version:'controlled-peer',orders:[],positions:[],instruments:[],fills:[],cash:null,equity:null}:v.operation==='RECOVERY_STATUS'?{ready:true,recovery_required:true,recovery_only:true,native_session_id:'original',native_version:'controlled-peer'}:{ready:true};console.log(JSON.stringify({requestId:v.requestId,result}));if(v.operation==='SHUTDOWN')process.exit(0);});`,{mode:0o755});
 const config=configDefaults({dataDir:dir,nativeProcess:{kind:'RUST_PROCESS',id:'owner',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'controlled-peer',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}}});
 const creation={requestId:'create-original',configuration:{id:'paper-one',clientId:'client',accountId:'native-paper',mode:'paper' as const,instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS' as const,trade_size:'1',fast_period:2,slow_period:3}}}};
 let manager=await processAccounts(config);
 try{
  const created=await manager.create(creation);assert.equal(created.accountSourceId!==undefined,true);
  await manager.list();await manager.create(creation);await assert.rejects(manager.create({...creation,configuration:{...creation.configuration,startingBalance:'999'}}),/configuration_conflict/);await assert.rejects(readFile(marker),/ENOENT/);
  await assert.rejects(manager.create({...creation,requestId:'duplicate-physical',configuration:{...creation.configuration,id:'paper-alias'}}),/physical_account_already/);
  const binding=config.connections.find(c=>c.kind==='RUST_PROCESS')!;
  const fence=await downstreamRequest(binding,'strategy/fence','GET');
  const request={requestId:'recover-original',action:'RECOVER',clientId:'client',accountId:'native-paper',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision};
  const result=await downstreamRequest(binding,'strategy/commands','POST',request);assert.equal(result.status,'COMPLETED');assert.equal((result.result as any).recovery_required,true);
  assert.doesNotMatch(await readFile(marker,'utf8'),/CONTROL|STRATEGY_START|CANCEL/);
  await manager.close();const before=await readFile(marker,'utf8');config.nativeProcess={...config.nativeProcess!,binary:'/unselected/not-installed',id:'other-environment'};manager=await processAccounts(config);
  const restored=(await manager.list())[0]!;assert.equal(restored.accountSourceId,created.accountSourceId);
  const reopened=config.connections.find(c=>c.kind==='RUST_PROCESS')!;assert.equal(reopened.processBinding?.runner.binary,binary,'Original account is never rebound by global runner selection');await downstreamRequest(reopened,'strategy/commands','POST',request);
  assert.equal(await readFile(marker,'utf8'),before,'Original recovery replay is a stored receipt, never a reconnect.');
 }finally{await manager.close();await rm(dir,{recursive:true,force:true});}
});

 test('legacy account directories and physical identities are not silently adopted by process creation',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-account-historical-'));
 const configuration={id:'original',clientId:'client',accountId:'native-paper',mode:'paper' as const,instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS' as const,trade_size:'1',fast_period:2,slow_period:3}}};
 const original=JSON.stringify({requestId:'original-create',configuration});
 await mkdir(join(dir,'accounts','original'),{recursive:true});await writeFile(join(dir,'accounts','original','configuration.json'),original);
 const evidence='{"originalRequest":"pending-original","status":"UNKNOWN"}';await writeFile(join(dir,'accounts','original','original-receipt.json'),evidence);
 const config=configDefaults({dataDir:dir,nativeProcess:{kind:'RUST_PROCESS',id:'owner',binary:'/not-running',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}}});
 const manager=await processAccounts(config);
 try{
 await assert.rejects(manager.create({requestId:'new-id',configuration:{...configuration,id:'alias'}}),/historical_account_identity_requires_explicit_reconciliation/);
 assert.equal(config.connections.length,0);assert.deepEqual(await manager.list(),[]);
 assert.equal(await readFile(join(dir,'accounts','original','configuration.json'),'utf8'),original);assert.equal(await readFile(join(dir,'accounts','original','original-receipt.json'),'utf8'),evidence);
 await assert.rejects(readFile(join(dir,'accounts','alias','process-binding.json')),/ENOENT/);
 }finally{await manager.close();await rm(dir,{recursive:true,force:true});}
 });

 test('failed pre-publication creation retries the original ID and failed startup releases prior handles',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-account-partial-create-'));
 const configuration={id:'a-first',clientId:'client',accountId:'native-paper-a',mode:'paper' as const,instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS' as const,trade_size:'1',fast_period:2,slow_period:3}}};
 const target={kind:'RUST_PROCESS' as const,id:'owner',binary:'/not-running',capabilities:{schemaVersion:1 as const,protocol:'quazonai-native-process-v1' as const,identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime' as const,requiresExplicitStart:true as const},running:false as const}};
 const config=configDefaults({dataDir:dir,nativeProcess:target});const fail=true;
 let manager=await processAccounts(config,async(a,b)=>{if(fail)throw new Error('injected_publish_failure');await rename(a,b);});
 const original={requestId:'original-create',configuration};
 try{
 await assert.rejects(manager.create(original),/injected_publish_failure/);await assert.rejects(readFile(join(dir,'accounts','a-first','process-binding.json')),/ENOENT/);
 await manager.close();manager=await processAccounts(config);
 const saved=await manager.create(original);assert.ok(saved.accountSourceId);
 const second={requestId:'second-create',configuration:{...configuration,id:'b-second',accountId:'native-paper-b'}};await manager.create(second);
 const blocked=join(dir,'accounts','a-first','capital-receipts.json.next');await mkdir(blocked);
 await assert.rejects(manager.close(),/EISDIR/);assert.equal(config.connections.length,0,'All bindings detach even when the first account close fails');await rm(blocked,{recursive:true});
 manager=await processAccounts(config);assert.equal((await manager.list()).length,2);await manager.close();
 const path=join(dir,'accounts','b-second','configuration.json');const text=await readFile(path,'utf8');await writeFile(path,'{}');
 await assert.rejects(processAccounts(config),/account_creation_request_invalid/);assert.equal(config.connections.length,0,'Prior attached handles are removed after later startup failure');
 await writeFile(path,text);manager=await processAccounts(config);assert.equal((await manager.list()).length,2);assert.equal((await manager.list())[0]!.accountSourceId,saved.accountSourceId);
 }finally{await manager.close();await rm(dir,{recursive:true,force:true});}
 });

 test('post-rename directory sync is retried under original request and concurrent physical aliases cannot both publish',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-account-sync-retry-'));
 const configuration={id:'first',clientId:'client',accountId:'native-paper',mode:'paper' as const,instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS' as const,trade_size:'1',fast_period:2,slow_period:3}}};
 const config=configDefaults({dataDir:dir,nativeProcess:{kind:'RUST_PROCESS',id:'owner',binary:'/not-running',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}}});
 let syncs=0;const manager=await processAccounts(config,rename,async path=>{const h=await open(path,'r');try{if(++syncs===1)throw new Error('injected_root_sync_failure');await h.sync();}finally{await h.close();}});
 const creation={requestId:'original',configuration};
 try{
 await assert.rejects(manager.create(creation),/injected_root_sync_failure/);assert.equal(config.connections.length,0);
 assert.equal(JSON.parse(await readFile(join(dir,'accounts','first','process-binding.json'),'utf8')).creation.requestId,'original');
 const outcomes=await Promise.allSettled([manager.create(creation),manager.create({requestId:'alias',configuration:{...configuration,id:'alias'}})]);
 assert.equal(outcomes[0]!.status,'fulfilled');assert.equal(outcomes[1]!.status,'rejected');assert.ok(syncs>=2,'Success requires actual root sync retry');assert.equal(config.connections.length,1);
 await assert.rejects(readFile(join(dir,'accounts','alias','process-binding.json')),/ENOENT/);
 }finally{await manager.close();await rm(dir,{recursive:true,force:true});}
 });

import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,writeFile,readFile,rm} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {processAccounts,savedAccountRunnerReport} from '../src/process-accounts.ts';
import {configDefaults} from '../src/config.ts';

test('paper parameters save and reopen unchanged without starting the native process',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-paper-values-')),binary=join(dir,'runner'),marker=join(dir,'started');
 await writeFile(binary,`#!${process.execPath}\nimport{writeFileSync}from'node:fs';writeFileSync(${JSON.stringify(marker)},'unexpected');`,{mode:0o755});
 const config=configDefaults({dataDir:dir,nativeProcess:{kind:'RUST_PROCESS',id:'owner',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'0.69.0',nautilusRevision:'634abcd',nautilusSource:'path+file:///private/source',privateValue:'not-for-display',precision:'high'},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}}});
 const paperExecution={prob_fill_on_limit:0.75,prob_slippage:0.125,random_seed:'18446744073709551615',base_latency_ns:'9007199254740993',insert_latency_ns:'7',update_latency_ns:'11',cancel_latency_ns:'13',queue_position:true};
 const creation={requestId:'paper-save-original',configuration:{id:'paper',clientId:'client',accountId:'paper-native',mode:'paper' as const,instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',paperExecution,nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS' as const,trade_size:'1',fast_period:2,slow_period:3}}}};
 let manager=await processAccounts(config);
 try{
  await manager.create(creation);await manager.create(creation);
  assert.deepEqual(((await manager.list())[0]!.accountConfiguration as any).paperExecution,paperExecution);
  const path=join(dir,'accounts','paper','configuration.json');const saved=await readFile(path,'utf8');
  assert.deepEqual(JSON.parse(saved).configuration.paperExecution,paperExecution);
  const report=((await manager.list())[0]!).savedRunner;
  assert.deepEqual(report,{source:'SAVED_ACCOUNT_BINDING',nautilusVersion:'0.69.0',nautilusRevision:'634abcd',precision:'high',compatibilityDeclaration:null,compatibilityVerified:null});
  assert.equal(JSON.stringify(report).includes('/private'),false);assert.equal(JSON.stringify(report).includes('not-for-display'),false);
  await manager.close();config.nativeProcess={...config.nativeProcess!,binary:'/not-used/new-runner',capabilities:{...config.nativeProcess!.capabilities,identity:{nautilusVersion:'9.9.9',precision:'standard'}}};manager=await processAccounts(config);
  assert.deepEqual(((await manager.list())[0]!).savedRunner,report);
  assert.deepEqual(((await manager.list())[0]!.accountConfiguration as any).paperExecution,paperExecution);
  assert.equal(await readFile(path,'utf8'),saved);await assert.rejects(readFile(marker),{code:'ENOENT'});
 }finally{await manager.close();await rm(dir,{recursive:true,force:true});}
});

test('saved runner report does not expose malformed or path-like identity fields',()=>{
 const target={capabilities:{identity:{nautilusVersion:'/private/version',nautilusRevision:'token-or-path',precision:'private'},account:{}}} as any;
 assert.deepEqual(savedAccountRunnerReport(target),{source:'SAVED_ACCOUNT_BINDING',nautilusVersion:null,nautilusRevision:null,precision:null,compatibilityDeclaration:null,compatibilityVerified:null});
});

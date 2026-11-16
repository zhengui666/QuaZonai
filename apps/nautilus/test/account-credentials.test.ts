import test from 'node:test';
import assert from 'node:assert/strict';
import {accountCredentialEnvironment,validateAccountCredentialReference} from '../src/account-credentials.ts';
import {accountCreation} from '../src/account-configurations.ts';
const configuration={id:'account-one',clientId:'client-one',accountId:'HYPERLIQUID-001',mode:'live',accountAddress:'0x'+'1'.repeat(40),instruments:['BTC-USD-PERP.HYPERLIQUID'],nativeStrategy:{instrument_id:'BTC-USD-PERP.HYPERLIQUID',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:10,slow_period:20}}};
test('each runner receives only its selected synthetic credential in the official network variable',()=>{
 const environment={QZ_HYPERLIQUID_ONE_PK:'synthetic-one',QZ_HYPERLIQUID_TWO_PK:'synthetic-two',HYPERLIQUID_PK:'legacy-main',HYPERLIQUID_TESTNET_PK:'legacy-test'};
 assert.deepEqual(accountCredentialEnvironment({mode:'live',credentialEnv:'QZ_HYPERLIQUID_ONE_PK'},environment),{HYPERLIQUID_PK:'synthetic-one'});
 assert.deepEqual(accountCredentialEnvironment({mode:'live',network:'testnet',credentialEnv:'QZ_HYPERLIQUID_TWO_PK'},environment),{HYPERLIQUID_TESTNET_PK:'synthetic-two'});
 assert.deepEqual(accountCredentialEnvironment({mode:'live'},environment),{HYPERLIQUID_PK:'legacy-main'});
 assert.deepEqual(accountCredentialEnvironment({mode:'live',network:'testnet'},environment),{HYPERLIQUID_TESTNET_PK:'legacy-test'});
});
test('paper does not read credentials and explicit missing references do not fall back',()=>{
 const unreadable=new Proxy({}, {get(){throw Error('must not read environment');}});
 assert.deepEqual(accountCredentialEnvironment({mode:'paper'},unreadable),{});
 assert.throws(()=>accountCredentialEnvironment({mode:'live',credentialEnv:'QZ_HYPERLIQUID_MISSING_PK'},{HYPERLIQUID_PK:'unrelated'}),/account_credential_reference_unavailable/);
});
test('rejects secrets and arbitrary environment names; configuration validation resolves no secrets',()=>{
 for(const reference of ['actual-private-key','HYPERLIQUID_PK','PATH','QZ_HYPERLIQUID_bad_PK','',1,null]) assert.throws(()=>validateAccountCredentialReference('live',reference),/account_credential_reference_invalid/);
 assert.throws(()=>validateAccountCredentialReference('paper','QZ_HYPERLIQUID_ONE_PK'),/account_credential_reference_invalid/);
 const saved=accountCreation({requestId:'save-reference',configuration:{...configuration,credentialEnv:'QZ_HYPERLIQUID_NOT_INSTALLED_PK'}});
 assert.equal(saved.configuration.credentialEnv,'QZ_HYPERLIQUID_NOT_INSTALLED_PK');
 assert.equal(JSON.stringify(saved).includes('synthetic-one'),false);
});

test('real child spawn isolates synthetic account credentials without putting references or keys in BOOT',async()=>{
 const {mkdtemp,writeFile,rm}=await import('node:fs/promises');
 const {tmpdir}=await import('node:os');const {join}=await import('node:path');
 const {execFile}=await import('node:child_process');const {promisify}=await import('node:util');
 const directory=await mkdtemp(join(tmpdir(),'qz-credential-spawn-'));
 const protocol=join(directory,'protocol.mjs'),runner=join(directory,'runner'),controller=join(directory,'controller.mjs');
 const shellQuote=(v:string)=>"'"+v.replaceAll("'","'\\''")+"'";
 try{
  await writeFile(protocol,`import readline from 'node:readline';
  const lines=readline.createInterface({input:process.stdin});lines.on('line',line=>{const v=JSON.parse(line);
  if(v.operation==='BOOT'){
    if(process.env.HYPERLIQUID_TESTNET_PK!=='synthetic-selected'||process.env.HYPERLIQUID_PK!==undefined||process.env.QZ_HYPERLIQUID_ONE_PK!==undefined||process.env.QZ_HYPERLIQUID_TWO_PK!==undefined)throw Error('child_environment_not_isolated');
    if(JSON.stringify(v).includes('synthetic')||JSON.stringify(v).includes('credentialEnv')||JSON.stringify(v).includes('QZ_HYPERLIQUID'))throw Error('credential_in_boot');
  }
  console.log(JSON.stringify({requestId:v.requestId,result:{ready:true}}));if(v.operation==='SHUTDOWN')process.exit(0);});`);
  await writeFile(runner,`#!/bin/sh\nexec ${shellQuote(process.execPath)} ${shellQuote(protocol)}\n`,{mode:0o700});
  await writeFile(controller,`import {AccountRuntime} from ${JSON.stringify(new URL('../src/account-runtime.ts',import.meta.url).href)};
  const runtime=new AccountRuntime(${JSON.stringify(runner)},${JSON.stringify({...configuration,network:'testnet',credentialEnv:'QZ_HYPERLIQUID_ONE_PK'})},'01900000-0000-7000-8000-000000000001',${JSON.stringify(directory)});
  try{await runtime.start();}finally{await runtime.close();}`);
  await promisify(execFile)(process.execPath,['--experimental-strip-types',controller],{env:{PATH:'/usr/bin:/bin',QZ_HYPERLIQUID_ONE_PK:'synthetic-selected',QZ_HYPERLIQUID_TWO_PK:'synthetic-other',HYPERLIQUID_PK:'synthetic-main',HYPERLIQUID_TESTNET_PK:'synthetic-default'},timeout:15000});
 }finally{await rm(directory,{recursive:true,force:true});}
});

test('receipt reopening rejects an altered credential reference without starting a runner',async()=>{
 const {mkdtemp,rm}=await import('node:fs/promises');const {tmpdir}=await import('node:os');const {join}=await import('node:path');
 const {createCapitalAccount}=await import('../src/capital-account.ts');
 const directory=await mkdtemp(join(tmpdir(),'qz-credential-receipt-'));
 let starts=0;
 const runtime={async start(){starts++;throw Error('must not start');},async request(){throw Error('not connected');},async close(){}};
 const options=accountCreation({requestId:'save',configuration:{...configuration,accountSemantics:'NATIVE_MARGIN',credentialEnv:'QZ_HYPERLIQUID_ONE_PK'}}).configuration;
 try{
  const original=await createCapitalAccount(options,directory,'/unused',runtime);await original.close();
  const reopened=await createCapitalAccount(options,directory,'/unused',runtime);await reopened.close();
  await assert.rejects(createCapitalAccount({...options,credentialEnv:'QZ_HYPERLIQUID_TWO_PK'},directory,'/unused',runtime),/account_receipts_physical_identity_mismatch/);
  await assert.rejects(createCapitalAccount({...options,credentialEnv:undefined},directory,'/unused',runtime),/account_receipts_physical_identity_mismatch/);
  assert.equal(starts,0);
 }finally{await rm(directory,{recursive:true,force:true});}
});

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

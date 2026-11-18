import assert from 'node:assert/strict';
import { test } from 'node:test';
import { accountAdapter, nativeAccountAdapters, validNativeAccountAdapters } from '../../../packages/contracts/src/account-adapters.ts';
import { accountCreation, physicalAccount, sameAccountCreation } from '../src/account-configurations.ts';
import { validateNativeTarget } from '../../research/src/native-execution.ts';
import type { ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
const configuration = { id:'original',clientId:'client',accountId:'HL-001',mode:'live' as const,accountAddress:'0x'+'a'.repeat(40),instruments:['BTC.HYPERLIQUID'],nativeStrategy:{instrument_id:'BTC.HYPERLIQUID',strategy:{kind:'REGISTERED' as const,name:'owner::strategy',config:{original:'9007199254740993.00001'}}} };

test('explicit Hyperliquid identity is an alias for legacy bindings without changing their original JSON', () => {
 const original=accountCreation({requestId:'original',configuration});
 const explicit=accountCreation({requestId:'original',configuration:{...configuration,adapter:'HYPERLIQUID'}});
 assert.equal(accountAdapter(original.configuration),'HYPERLIQUID');
 assert.equal(original.configuration.adapter,undefined);
 assert.equal(explicit.configuration.adapter,'HYPERLIQUID');
 assert.ok(sameAccountCreation(original,explicit));
 assert.equal(physicalAccount(original.configuration),physicalAccount(explicit.configuration));
 assert.equal(physicalAccount(configuration),physicalAccount({...configuration,accountSemantics:'NATIVE_MARGIN',adapter:'HYPERLIQUID'}));
 assert.notEqual(physicalAccount(configuration),physicalAccount({...configuration,network:'testnet'}));
 assert.notEqual(physicalAccount(configuration),physicalAccount({...configuration,accountAddress:'0x'+'b'.repeat(40)}));
 assert.equal(sameAccountCreation(original,{...explicit,requestId:'different'}),false);
 assert.equal(sameAccountCreation(original,{...explicit,configuration:{...explicit.configuration,accountId:'other'}}),false);
 for(const adapter of ['IBKR','COINBASE','UNKNOWN','',null,42]) assert.throws(()=>accountCreation({requestId:'new',configuration:{...configuration,adapter}}),/account_adapter_unsupported/);
});

test('native adapter capability lists are preserved and not invented for uncompiled runners', () => {
 assert.deepEqual(nativeAccountAdapters({compiled:true}),['HYPERLIQUID']);
 assert.deepEqual(nativeAccountAdapters({compiled:false}),[]);
 assert.deepEqual(nativeAccountAdapters({compiled:true,adapters:[]}),[]);
 const runner:ProcessNativeTarget={kind:'RUST_PROCESS',id:'test',binary:'/not-executed',capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{configuredRunFactory:false,singleStrategyRegistration:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true,adapters:['HYPERLIQUID']},running:false}};
 assert.deepEqual((validateNativeTarget(runner) as ProcessNativeTarget).capabilities.account.adapters,['HYPERLIQUID']);
 for(const adapters of [null,'HYPERLIQUID',['HYPERLIQUID','HYPERLIQUID'],['bad id'],[42]]) {
  assert.equal(validNativeAccountAdapters({compiled:true,adapters} as never),false);
  assert.throws(()=>validateNativeTarget({...runner,capabilities:{...runner.capabilities,account:{...runner.capabilities.account,adapters}}}),/native_process_target_invalid/);
 }
 assert.equal(validNativeAccountAdapters({compiled:false,adapters:['HYPERLIQUID']}),false);
});

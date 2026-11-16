import assert from 'node:assert/strict';
import {test} from 'node:test';
import {accountAdapterCapabilities} from '../../../packages/contracts/src/account-adapters.ts';
import {accountCreation} from '../src/account-configurations.ts';

test('adapter capabilities preserve only explicit build facts and older runners remain unspecified',()=>{
 assert.equal(accountAdapterCapabilities(undefined),undefined);
 const row={id:'HYPERLIQUID',compiled:true,modes:['paper','live'],accountTypes:['CASH','MARGIN'],unsupported:['HOT_LEVERAGE_UPDATE'],reason:'test',secret:'not-propagated'};
 const result=accountAdapterCapabilities([row])!;
 assert.deepEqual(result,[{id:'HYPERLIQUID',compiled:true,modes:['paper','live'],accountTypes:['CASH','MARGIN'],unsupported:['HOT_LEVERAGE_UPDATE'],reason:'test'}]);
 row.modes.push('unknown');assert.equal(result[0]!.modes.length,2);
 assert.throws(()=>accountAdapterCapabilities([row,row]),/capabilities_invalid/);
 assert.throws(()=>accountAdapterCapabilities([{...row,compiled:'true'}]),/capabilities_invalid/);
});
test('only Hyperliquid is selectable; omission preserves existing saved configuration',()=>{
 const configuration={id:'test',clientId:'client',accountId:'paper',mode:'paper',instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
 assert.equal(accountCreation({requestId:'test',configuration}).configuration.adapter,undefined);
 assert.equal(accountCreation({requestId:'test',configuration:{...configuration,adapter:'HYPERLIQUID'}}).configuration.adapter,'HYPERLIQUID');
 assert.throws(()=>accountCreation({requestId:'test',configuration:{...configuration,adapter:'COINBASE'}}),/adapter_not_supported/);
 assert.throws(()=>accountCreation({requestId:'test',configuration:{...configuration,adapterConfig:{private_key:'synthetic'}}}),/configuration_invalid/);
});

import assert from 'node:assert/strict';
import {test} from 'node:test';
import {remoteNativeCapabilities,validateNativeTarget} from '../src/native-execution.ts';
import type {ProcessNativeTarget} from '../../../packages/contracts/src/nautilus.ts';

function target(preparation?: {marketImport:boolean}):ProcessNativeTarget {
  return {kind:'RUST_PROCESS',id:'capability-contract',binary:'/not-executed',capabilities:{
    schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'0.69.0'},
    research:{singleStrategyRegistration:false,configuredRunFactory:true},
    ...(preparation===undefined?{}:{preparation}),
    account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false,
  }};
}
test('data preparation requires an explicit original runner capability, not research support',async()=>{
  for(const preparation of [undefined,{marketImport:false}]) {
    const original=validateNativeTarget(target(preparation));
    const caps=await remoteNativeCapabilities(original);
    assert.equal(caps.commands.includes('native-configured-backtest'),true);
    assert.equal(caps.commands.includes('native-market-import'),false);
  }
  const original=validateNativeTarget(target({marketImport:true})) as ProcessNativeTarget;
  assert.deepEqual(original.capabilities.preparation,{marketImport:true});
  assert.equal((await remoteNativeCapabilities(original)).commands.includes('native-market-import'),true);
});
test('invalid preparation declarations are rejected rather than coerced',()=>{
  for(const value of [null,{}, {marketImport:'true'}, {marketImport:1}]) {
    const bad=target();Object.assign(bad.capabilities,{preparation:value});
    assert.throws(()=>validateNativeTarget(bad),/native_process_target_invalid/);
  }
});

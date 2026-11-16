import assert from 'node:assert/strict';
import {test} from 'node:test';
import {accountCreation} from '../src/account-configurations.ts';

test('paper leverage is an explicit native Margin scenario, never live configuration', () => {
  const base={id:'paper-margin',clientId:'test',accountId:'SIM-001',instruments:['BTC-USDC-PERP.HYPERLIQUID'],nativeStrategy:{instrument_id:'BTC-USDC-PERP.HYPERLIQUID',strategy:{kind:'EMA_CROSS' as const,trade_size:'1',fast_period:2,slow_period:3}},accountAddress:'0x'+'1'.repeat(40),mode:'paper' as const,startingBalance:'1000',paperFeeRate:'0.001',accountSemantics:'NATIVE_MARGIN' as const,paperLeverage:'5.125'};
  assert.equal(accountCreation({requestId:'paper-margin',configuration:base}).configuration.paperLeverage,'5.125');
  for(const change of [{mode:'live'}, {accountSemantics:undefined}, {paperLeverage:'0'}, {paperLeverage:'-2'}, {paperLeverage:5}]) {
    assert.throws(()=>accountCreation({requestId:'invalid',configuration:{...base,...change}}),/paper_leverage_requires_margin/);
  }
  assert.equal(accountCreation({requestId:'legacy',configuration:{...base,accountSemantics:undefined,paperLeverage:undefined}}).configuration.paperLeverage,undefined);
});

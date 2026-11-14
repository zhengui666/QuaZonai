import {describe,it,expect} from 'vitest';
import {cancellationInput} from './account-orders';
describe('original paper order cancellation input',()=>{
 const binding={sourceId:'source',buildId:'build',environmentId:'Sandbox',nativeSessionId:'session'};
 const page={available:true,reason:null,mode:'paper',binding,capability:{supported:true},items:[],offset:0,nextOffset:null,total:1};
 const order={trader_id:'T-001',strategy_id:'ORIGINAL-001',account_id:'PAPER-001',instrument_id:'TEST.SIM',client_order_id:'original',venue_order_id:'venue',status:'PARTIALLY_FILLED',quantity:'0.25',filled_quantity:'0.01',is_active_local:false,emulation_trigger:null,exec_algorithm_id:null};
 it('uses original order identities and no current strategy substitution',()=>{const input=cancellationInput(page,order)!;expect(input.strategyId).toBe('ORIGINAL-001');expect(input.binding).toEqual(binding);expect(input.order).toEqual({traderId:'T-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original'});expect(input).not.toHaveProperty('requestId');});
 it('refuses live, missing capability/session/identity, closed and emulator/algo orders',()=>{
  for(const patch of [{mode:'live'},{available:false},{binding:null},{capability:{supported:false}}])expect(cancellationInput({...page,...patch},order)).toBeNull();
  for(const patch of [{strategy_id:undefined},{trader_id:undefined},{account_id:null},{status:'CANCELED'},{venue_order_id:null},{is_active_local:undefined},{is_active_local:true},{emulation_trigger:'BID_ASK'},{exec_algorithm_id:'ALG'}])expect(cancellationInput(page,{...order,...patch})).toBeNull();
 });
});

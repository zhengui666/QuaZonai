import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createCapitalAccount} from '../src/capital-account.ts';
import type {AccountOptions,NativeAccountRuntime} from '../src/account-runtime.ts';
import type {NativeSessionBinding} from '../../../packages/contracts/src/native-strategy-control.ts';
test('order pages retain original identities and require the same session without connecting or sending',async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-order-page-'));let binding:NativeSessionBinding|null=null,starts=0;const operations:string[]=[];
 const orders=Array.from({length:51},(_,i)=>({trader_id:'T-001',strategy_id:'ORIGINAL-001',account_id:'PAPER-001',client_order_id:`order-${i}`,instrument_id:'TEST.SIM',quantity:'0.123456789012345678',filled_quantity:'0.000000000000000001',status:'PARTIALLY_FILLED'}));
 let session='original';const runtime:NativeAccountRuntime={async start(){starts++;},async close(){},paperCancellationBinding(){return binding;},paperCancellationCapability(){return {supported:!!binding,compatibilityVerified:false};},async request(op,payload){operations.push(op);if(op==='SNAPSHOT')return {};assert.equal(op,'ORDER_PAGE');const offset=payload!.offset as number;assert.equal(payload!.expected_native_session_id,'original');return {native_session_id:session,items:orders.slice(offset,offset+50),offset,nextOffset:offset+50<orders.length?offset+50:null,total:null};}};
 const options:AccountOptions={id:'paper',clientId:'client',accountId:'PAPER-001',mode:'paper',startingBalance:'1000',paperFeeRate:'0.001',instruments:['TEST.SIM'],nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
 const account=await createCapitalAccount(options,dir,'/not-started',runtime);
 const get=async(path:string)=>(await account.request(path,'GET','client')).value as any;
 try{
  const previousCalls=operations.length;const unavailable=await get('/strategy/orders/0');assert.equal(unavailable.available,false);assert.equal(unavailable.total,null);assert.equal(operations.length,previousCalls);
  const config=await get('/strategy/config');binding={sourceId:config.accountSourceId,buildId:'/not-started',environmentId:'Sandbox',nativeSessionId:'original'};
  const first=await get('/strategy/orders/0');assert.equal(first.items.length,50);assert.equal(first.nextOffset,50);assert.deepEqual(first.items[0],orders[0]);assert.deepEqual(first.binding,binding);
  const second=await get('/strategy/orders/50');assert.deepEqual(second.items,[orders[50]]);assert.equal(second.nextOffset,null);
  session='replacement';await assert.rejects(get('/strategy/orders/0'),/native_order_session_changed/);assert.equal(starts,0);assert(operations.includes('ORDER_PAGE'));
 }finally{await account.close();await rm(dir,{recursive:true,force:true});}
});

import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp,writeFile,readFile,rm } from 'node:fs/promises';
import {tmpdir} from 'node:os';import {join} from 'node:path';
import {createCapitalAccount} from '../src/capital-account.ts';
import type {AccountOptions,NativeAccountRuntime} from '../src/account-runtime.ts';
const options:AccountOptions={id:'retired',clientId:'client',accountId:'account',mode:'paper',startingBalance:'1000',paperFeeRate:'0.001',instruments:['HYPE-USDC'],nativeStrategy:{instrument_id:'HYPE-USDC',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:10,slow_period:50}}};
const sleep=(ms:number)=>new Promise(r=>setTimeout(r,ms));
test('unsupported legacy cancellation preserves original pending slice and UNKNOWN across HTTP reopen',{timeout:15000},async()=>{
 const dir=await mkdtemp(join(tmpdir(),'qz-capital-retired-'));const calls:{op:string;payload:any}[]=[];let tick:(()=>void)|undefined;let terminal=false;
 const runtime:NativeAccountRuntime={async request(op,payload={}){calls.push({op,payload});if(op==='SNAPSHOT')return {ready:true,recovery_required:false,sequence:'1',observation_id:'observation',observed_at_ns:'1',native_version:'native',orders:terminal?[{client_order_id:'original-order',status:'CANCELED'}]:[],positions:[],instruments:[],fills:[],cash:'1000',equity:'1000'};if(op==='CONTROL')return {applied:true};if(op==='CANCEL_EXIT')return {cancellation_requested:terminal?0:1,settled:terminal};throw Error('new_operation_forbidden:'+op);},subscribeTicks(fn){tick=fn;return()=>{};},async close(){}};
 const funds={requested_amount:{amount:'123',currency:'USDC'},reserved_amount:{amount:'123',currency:'USDC'},staged_for_withdrawal_amount:{amount:'4',currency:'USDC'},actual_fees:{amount:'0.25',currency:'USDC'}};
 const view={id:'original-exit',revision:'1',state:'REDUCING',policy:{kind:'BOUNDED_LIMIT',pacing:{minimum_interval_seconds:5},deadline:'2030-01-01T00:00:00Z'},funds,events:[],progress:[],created_at:'2026-01-01T00:00:00Z'};
 const original={requestId:'original-start',clientId:'client',accountId:'account',command:{preview_id:'original-preview'}};
 const initial={schema_version:2,sourceId:'source',identity:{mode:'paper',network:'mainnet',accountId:'account',accountAddress:null,currency:'USDC'},epoch:'7',revision:'7',strategyRevision:'1',previews:{},targets:{},ledgerClaims:{},reverseTransferPending:false,commands:{'original-start':{original,state:'UNCERTAIN',error:'original_unknown',exitId:'original-exit'}},exits:{'original-exit':{view,childIds:['original-order'],pendingSlice:{id:'original-order'},fillCosts:{original:{fee:'0.25'}},sliceBasis:{original:{reference_bid:'100'}},executionCost:'0.25'}}};
 await writeFile(join(dir,'capital-receipts.json'),JSON.stringify(initial));
 let account=await createCapitalAccount(options,dir,'unused',runtime);
 const server=createServer((req,res)=>{void account.handle(req.url!,req,async()=>{let s='';for await(const part of req)s+=part;return JSON.parse(s);}).then(result=>{res.statusCode=result.status;res.setHeader('content-type','application/json');res.end(JSON.stringify(result.value));},error=>{res.statusCode=error.statusCode??500;res.end(JSON.stringify({error:error.message}));});});
 await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));const addr=server.address();assert.ok(addr&&typeof addr!=='string');
 const call=(path:string,body?:unknown)=>fetch(`http://127.0.0.1:${addr.port}${path}`,{method:body?'POST':'GET',headers:{'x-qz-client-id':'client','content-type':'application/json'},...(body?{body:JSON.stringify(body)}:{})});
 try{
  await sleep(40);tick?.();await sleep(40);
  assert.equal((await call('/capital-exits/preview',{clientId:'client',accountId:'account'})).status,404);
  assert.equal((await call('/capital-exits',{requestId:'new',clientId:'client',accountId:'account',command:{}})).status,404);
  assert.equal((await call('/capital-exits/commands/new')).status,404);
  const before=JSON.parse(await readFile(join(dir,'capital-receipts.json'),'utf8'));assert.deepEqual(before.exits['original-exit'].view.funds,funds);assert.equal(before.commands['original-start'].state,'UNCERTAIN');assert.ok(before.exits['original-exit'].pendingSlice);
  const body={requestId:'stop-original',clientId:'client',accountId:'account',command:{schema_version:2,expected_revision:before.exits['original-exit'].view.revision,action:'CANCEL'}};
  assert.equal((await call('/capital-exits/original-exit/actions',body)).status,202);
  await sleep(30);tick?.();await sleep(30);
  let saved=JSON.parse(await readFile(join(dir,'capital-receipts.json'),'utf8'));assert.equal(saved.exits['original-exit'].view.state,'REDUCING');assert.deepEqual(saved.exits['original-exit'],before.exits['original-exit']);assert.equal(saved.epoch,before.epoch);assert.equal(saved.revision,before.revision);
  const rejected=await (await call('/capital-exits/commands/stop-original')).json();assert.equal(rejected.status,'REJECTED');assert.equal(rejected.error,'capital_original_control_evidence_unavailable');
  assert.equal((await call('/capital-exits/original-exit/actions',body)).status,202);
  await account.close();account=await createCapitalAccount(options,dir,'unused',runtime);await sleep(30);
  assert.equal((await (await call('/capital-exits/commands/original-start')).json()).status,'UNCERTAIN');
  terminal=true;tick?.();await sleep(60);
  saved=JSON.parse(await readFile(join(dir,'capital-receipts.json'),'utf8'));assert.equal(saved.exits['original-exit'].view.state,'REDUCING');assert.deepEqual(saved.exits['original-exit'].pendingSlice,initial.exits['original-exit'].pendingSlice);assert.deepEqual(saved.commands['original-start'],before.commands['original-start']);assert.equal(saved.epoch,before.epoch);assert.deepEqual(saved.exits['original-exit'].view.funds,funds);
  assert.deepEqual(saved.exits['original-exit'].fillCosts,initial.exits['original-exit'].fillCosts);
  assert.deepEqual(await (await call('/capital-exits/commands/stop-original')).json(),rejected);
  assert.ok(calls.length>0);assert.ok(calls.every(c=>c.op==='SNAPSHOT')); // A terminal order observation never invents a cancelled exit or releases its facts.
 }finally{await account.close();server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));await rm(dir,{recursive:true,force:true});}
});

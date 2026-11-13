import assert from 'node:assert/strict';
import {test} from 'node:test';
import {writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {setup,context} from './support.ts';
import {Application} from '../src/facts.ts';

test('saved account routes use original Pi intent for explicit RECOVER without an observation or automatic START',async()=>{
 const state=await setup();const binary=join(state.directory,'account-peer.mjs'),marker=join(state.directory,'account-calls');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';import{createInterface}from'node:readline';if(process.argv[2]!=='account-runtime')throw Error('mode');createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);appendFileSync(${JSON.stringify(marker)},v.operation+'\\n');const result=v.operation==='SNAPSHOT'?{ready:true,recovery_required:true,sequence:'1',observation_id:'original',observed_at_ns:'1',native_version:'controlled-peer',orders:[],positions:[],instruments:[],fills:[],cash:null,equity:null}:v.operation==='RECOVERY_STATUS'?{ready:true,recovery_required:true,recovery_only:true,native_session_id:'original',native_version:'controlled-peer'}:{ready:true};console.log(JSON.stringify({requestId:v.requestId,result}));if(v.operation==='SHUTDOWN')process.exit(0);});`,{mode:0o755});
 state.options.config.nativeProcess={kind:'RUST_PROCESS',id:'owner',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const creation={requestId:'create-original',configuration:{id:'paper',clientId:'client',accountId:'account',mode:'paper',instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}}};
 try{
 await state.restart();const url='/api/v3/accounts/configurations';
 assert.equal((await state.service.app.inject({method:'POST',url,payload:creation})).statusCode,401);
 const saved=await state.service.app.inject({method:'POST',url,headers:state.headers,payload:creation});assert.equal(saved.statusCode,200,saved.body);
 const strategy=url+'/paper/strategy';const fence=await state.service.app.inject({url:strategy+'/fence',headers:state.headers});assert.equal(fence.statusCode,200,fence.body);
 await state.service.app.inject({url,headers:state.headers});
 const spotUrl=url+'/paper/spot-balance-observation';
 assert.equal((await state.service.app.inject({url:spotUrl})).statusCode,401);
 const offline=await state.service.app.inject({url:spotUrl,headers:state.headers});assert.notEqual(offline.statusCode,200);
 await assert.rejects(readFile(marker),/ENOENT/);
 assert.deepEqual((await state.service.research.harness.snapshot(Application,context))?.observations,{});
 const request={requestId:'recover-original',action:'RECOVER',expectedAccountSourceId:fence.json().accountSourceId,expectedStrategyRevision:fence.json().strategyRevision};
 const sent=await state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:request});assert.equal(sent.statusCode,202,sent.body);
 const app=(await state.service.research.harness.snapshot(Application,context))!;const original=Object.values(app.strategyCommands!)[0]!;
 await state.service.research.harness.waitForTask(original.taskId as never,context);
 const receipt=await state.service.app.inject({url:strategy+'/commands/recover-original',headers:state.headers});assert.equal(receipt.json().status,'COMPLETED',receipt.body);
 const spot=await state.service.app.inject({url:spotUrl,headers:state.headers});assert.equal(spot.statusCode,200,spot.body);assert.equal(spot.json().observation,null);
 assert.doesNotMatch(await readFile(marker,'utf8'),/STRATEGY_START|CONTROL|CANCEL/);
 await state.restart();const before=await readFile(marker,'utf8');
 const repeat=await state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:request});assert.equal(repeat.statusCode,202,repeat.body);
 assert.equal(await readFile(marker,'utf8'),before);assert.equal(Object.keys((await state.service.research.harness.snapshot(Application,context))!.strategyCommands!).length,1);
 }finally{await state.cleanup();}
});

test('saved paper cancel routes use original Task, retain UNKNOWN and never BOOT or resend after Service reopen',async()=>{
 const state=await setup();const binary=join(state.directory,'cancel-peer.mjs'),marker=join(state.directory,'cancel-calls');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';import{createInterface}from'node:readline';let canceled=false;createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);appendFileSync(${JSON.stringify(marker)},v.operation+'\\n');const reply=result=>console.log(JSON.stringify({requestId:v.requestId,result}));
 if(v.operation==='BOOT')return reply({ready:true,native_session_id:'original-session',ordinary_venue_cancel:true});
 if(v.operation==='SNAPSHOT')return reply({ready:true,recovery_required:true,sequence:'1',observation_id:'original',observed_at_ns:'1',native_version:'controlled-peer',orders:[],positions:[],instruments:[],fills:[],cash:null,equity:null});
 if(v.operation==='RECOVERY_STATUS')return reply({ready:true,recovery_required:true,recovery_only:true,native_session_id:'original-session',native_version:'controlled-peer'});
 if(v.operation==='ORDER_CANCEL_ATTEMPT'){canceled=true;return console.log(JSON.stringify({requestId:v.requestId,error:'test_reply_lost_after_controlled_consumption'}));}
 if(v.operation==='ORDER_OBSERVE')return reply({command:v.command,status:'UNKNOWN',orderObservation:{status:canceled?'CANCELED':'PARTIALLY_FILLED',filledQuantity:'0.25',lastEvent:{kind:'controlled-test-only'}}});
 if(v.operation==='SHUTDOWN'){reply({});return process.exit(0);}reply({ready:true});});`,{mode:0o755});
 state.options.config.nativeProcess={kind:'RUST_PROCESS',id:'owner',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const configuration={id:'paper',clientId:'client',accountId:'PAPER-001',mode:'paper',instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}};
 const url='/api/v3/accounts/configurations',strategy=url+'/paper/strategy';
 try{
  await state.restart();
  assert.equal((await state.service.app.inject({method:'POST',url,headers:state.headers,payload:{requestId:'create-paper',configuration}})).statusCode,200);
  let fence=(await state.service.app.inject({url:strategy+'/fence',headers:state.headers})).json();
  assert.equal(fence.ordinaryVenueCancel,null);await assert.rejects(readFile(marker),/ENOENT/);
  const recover={requestId:'recover-paper',action:'RECOVER',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision};
  assert.equal((await state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:recover})).statusCode,202);
  let app=(await state.service.research.harness.snapshot(Application,context))!;
  await state.service.research.harness.waitForTask(Object.values(app.strategyCommands!)[0]!.taskId as never,context);
  fence=(await state.service.app.inject({url:strategy+'/fence',headers:state.headers})).json();
  const original={schemaVersion:1,requestId:'cancel-original',action:'CANCEL',strategyId:'ORIGINAL-001',order:{traderId:'TRADER-001',accountId:'PAPER-001',instrumentId:'TEST.SIM',clientOrderId:'original-order'},binding:fence.ordinaryVenueCancel};
  const send=()=>state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:original});
  const post=(payload:Record<string,unknown>)=>state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload});
  assert.equal((await post({...original,order:null})).statusCode,422);
  for(const field of ['sourceId','buildId','nativeSessionId','environmentId']){
   const denied=await post({...original,requestId:'wrong-'+field,binding:{...original.binding,[field]:'wrong'}});
   assert.equal(denied.statusCode,409,denied.body);
  }
  assert.equal((await post({...original,requestId:'wrong-account',order:{...original.order,accountId:'OTHER'}})).statusCode,409);
  assert.equal(Object.keys((await state.service.research.harness.snapshot(Application,context))!.strategyCommands!).length,1);
  assert.doesNotMatch(await readFile(marker,'utf8'),/ORDER_CANCEL_ATTEMPT/);
  const admitted=await send();assert.equal(admitted.statusCode,202,admitted.body);
  app=(await state.service.research.harness.snapshot(Application,context))!;
  const command=Object.values(app.strategyCommands!).find(v=>(v.original as any).requestId==='cancel-original')!;
  await state.service.research.harness.waitForTask(command.taskId as never,context);
  let receipt=await state.service.app.inject({url:strategy+'/commands/cancel-original',headers:state.headers});
  assert.equal(receipt.statusCode,200,receipt.body);assert.equal(receipt.json().status,'UNCERTAIN');
  assert.equal(receipt.json().receipt.orderObservation.status,'CANCELED');assert.equal(receipt.json().receipt.orderObservation.filledQuantity,'0.25');assert.equal(receipt.json().receipt.settled,undefined);
  assert.equal((await readFile(marker,'utf8')).split('\n').filter(v=>v==='ORDER_CANCEL_ATTEMPT').length,1);
  assert.equal((await send()).statusCode,202);
  assert.equal((await post({...original,order:{...original.order,clientOrderId:'other-order'}})).statusCode,409);
  await state.restart();const before=await readFile(marker,'utf8');
  assert.equal((await send()).statusCode,202);
  receipt=await state.service.app.inject({url:strategy+'/commands/cancel-original',headers:state.headers});
  assert.equal(receipt.json().status,'UNCERTAIN');assert.equal(receipt.json().receipt.error,'native_cancel_original_session_unavailable');
  assert.equal((await post({...original,requestId:'new-after-reopen'})).statusCode,409);
  assert.equal(await readFile(marker,'utf8'),before,'Reopen, duplicate and GET must not BOOT or dispatch');
  assert.equal(Object.keys((await state.service.research.harness.snapshot(Application,context))!.strategyCommands!).length,2);
 }finally{await state.cleanup();}
});

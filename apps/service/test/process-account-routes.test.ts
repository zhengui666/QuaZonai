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

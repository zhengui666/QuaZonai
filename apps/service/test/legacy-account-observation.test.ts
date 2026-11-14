import assert from 'node:assert/strict';
import {test} from 'node:test';
import {writeFile,readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {setup,context} from './support.ts';
import {Application} from '../src/facts.ts';

test('legacy full snapshot is explicit authenticated same-session read, not automatic observation or restart',async()=>{
 const state=await setup(),binary=join(state.directory,'legacy-peer.mjs'),marker=join(state.directory,'legacy-calls');
 await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';import{createInterface}from'node:readline';createInterface({input:process.stdin}).on('line',line=>{const v=JSON.parse(line);appendFileSync(${JSON.stringify(marker)},v.operation+'\\n');const reply=result=>console.log(JSON.stringify({requestId:v.requestId,result}));
 if(v.operation==='BOOT')return reply({ready:true,native_session_id:'original'});
 if(v.operation==='RECOVERY_STATUS')return reply({recovery_only:true,native_session_id:'original',native_version:'legacy-controlled'});
 if(v.operation==='SNAPSHOT')return reply({native_session_id:'original',native_account_id:'account',observed_at_ns:'1794605191057890704',orders:[{client_order_id:'old',events:['not-forwarded']}],fills:[],cash:'9007199254740993.00001',equity:null,native_version:'legacy-controlled'});
 if(v.operation==='SHUTDOWN'){reply({});return process.exit(0);}throw Error('unexpected:'+v.operation);});`,{mode:0o755});
 state.options.config.nativeProcess={kind:'RUST_PROCESS',id:'owner',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{},research:{singleStrategyRegistration:false,configuredRunFactory:false},account:{compiled:true,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
 const url='/api/v3/accounts/configurations',strategy=url+'/paper/strategy',path=strategy+'/legacy-snapshot';
 const payload={mode:'LEGACY_FULL_SNAPSHOT',expectedNativeSessionId:'original',acknowledgeFullHistoryCost:true};
 try{
  await state.restart();
  const saved=await state.service.app.inject({method:'POST',url,headers:state.headers,payload:{requestId:'save',configuration:{id:'paper',clientId:'client',accountId:'account',mode:'paper',instruments:['TEST.SIM'],startingBalance:'1000',paperFeeRate:'0.001',nativeStrategy:{instrument_id:'TEST.SIM',strategy:{kind:'EMA_CROSS',trade_size:'1',fast_period:2,slow_period:3}}}}});
  assert.equal(saved.statusCode,200,saved.body);
  assert.equal((await state.service.app.inject({method:'POST',url:path,payload})).statusCode,401);
  assert.notEqual((await state.service.app.inject({method:'POST',url:path,headers:state.headers,payload})).statusCode,200);
  await assert.rejects(readFile(marker),/ENOENT/);
  const fence=(await state.service.app.inject({url:strategy+'/fence',headers:state.headers})).json();
  const sent=await state.service.app.inject({method:'POST',url:strategy+'/commands',headers:state.headers,payload:{requestId:'recover',action:'RECOVER',expectedAccountSourceId:fence.accountSourceId,expectedStrategyRevision:fence.strategyRevision}});assert.equal(sent.statusCode,202,sent.body);
  const app=(await state.service.research.harness.snapshot(Application,context))!;const task=Object.values(app.strategyCommands!)[0]!;
  await state.service.research.harness.waitForTask(task.taskId as never,context);
  assert.equal((await state.service.app.inject({url:strategy+'/commands/recover',headers:state.headers})).json().status,'COMPLETED');
  assert.doesNotMatch(await readFile(marker,'utf8'),/SNAPSHOT/);
  for(const extra of [{acknowledgeFullHistoryCost:false},{expectedNativeSessionId:'wrong'}])assert.notEqual((await state.service.app.inject({method:'POST',url:path,headers:state.headers,payload:{...payload,...extra}})).statusCode,200);
  const result=await state.service.app.inject({method:'POST',url:path,headers:state.headers,payload});assert.equal(result.statusCode,200,result.body);
  assert.equal(result.json().observation.cash,'9007199254740993.00001');assert.equal(result.json().observedAtNs,'1794605191057890704');assert.equal(result.json().tradePermission,false);assert.equal(result.json().historyIncluded,false);assert.doesNotMatch(result.body,/not-forwarded/);
  await state.service.app.inject({url:strategy+'/config',headers:state.headers});
  assert.equal((await readFile(marker,'utf8')).split('\n').filter(x=>x==='SNAPSHOT').length,1);
  assert.equal(Object.keys((await state.service.research.harness.snapshot(Application,context))!.strategyCommands!).length,1,'read creates no trading intent');
  await state.restart();const before=await readFile(marker,'utf8');
  assert.notEqual((await state.service.app.inject({method:'POST',url:path,headers:state.headers,payload})).statusCode,200);
  assert.equal(await readFile(marker,'utf8'),before,'no BOOT or read after reopen');
 }finally{await state.cleanup();}
});

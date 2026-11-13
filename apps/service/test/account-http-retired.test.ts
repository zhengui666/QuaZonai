import assert from 'node:assert/strict';
import {test} from 'node:test';
import {Application,json} from '../src/facts.ts';
import {accountKey} from '../src/capital-exit.ts';
import {accountSourceFixture} from './pm-account-fixture.ts';
import {context,setup} from './support.ts';

test('HTTP account new commands are refused before Pi task or UNKNOWN admission; original identity remains readable',async()=>{
 const state=await setup();const source=accountSourceFixture();
 try{
  state.options.config.connections=[source.connection];await state.restart();
  const original={requestId:'historical-original',action:'START',expectedAccountSourceId:'source-original',expectedStrategyRevision:'4'};
  await state.service.research.harness.commit(async tx=>{const app=await tx.doc(Application);app.observations[source.connection.id]=json({...source.observation,connection:source.connection});Object.assign(app.accountControls??={},source.application.accountControls);
   const key=`${accountKey(source.connection,app.accountControls)}:${original.requestId}`;
   app.strategyCommands??={};app.strategyCommands[key]=json({original,connection:source.connection,currency:'USDC',observationId:source.connection.id,status:'UNCERTAIN',receipt:null,error:'original_response_lost',taskId:123});
  },context);
  const url=`/api/v3/observations/${source.connection.id}/strategy/commands`;
  const before=await state.service.research.harness.snapshot(Application,context);
  for(const action of ['START','RECOVER','STOP']){
   const payload={requestId:`new-${action}`,action,...(action==='STOP'?{}:{expectedAccountSourceId:'source-original',expectedStrategyRevision:'4'})};
   const response=await state.service.app.inject({method:'POST',url,headers:state.headers,payload});assert.equal(response.statusCode,410,response.body);
  }
  for(const action of ['PAUSE','CANCEL']){const response=await state.service.app.inject({method:'POST',url:`/api/v3/observations/${source.connection.id}/capital-exits/commands`,headers:state.headers,payload:{requestId:`capital-${action}`,action,exitId:'old-exit',expectedRevision:'4'}});assert.equal(response.statusCode,410,response.body);}
  const after=await state.service.research.harness.snapshot(Application,context);
  assert.deepEqual(after!.strategyCommands,before!.strategyCommands);assert.deepEqual(after!.accountControls,before!.accountControls);
  const tasks=(await state.service.research.harness.commit(tx=>tx.scanTasks({kind:'qz.account-strategy-command'},10),context)).items;assert.equal(tasks.length,0);
  const retry=await state.service.app.inject({method:'POST',url,headers:state.headers,payload:original});assert.equal(retry.statusCode,202,retry.body);assert.equal(retry.json().status,'UNCERTAIN');
  assert.deepEqual((await state.service.research.harness.snapshot(Application,context))!.strategyCommands,before!.strategyCommands);
 }finally{await state.cleanup();}
});

import assert from 'node:assert/strict';
import {test} from 'node:test';
import {setup} from './support.ts';
test('dataset summary avoids coverage payload; original collection and selected detail retain it verbatim',async()=>{
 const state=await setup();
 try{
  const coverage={event_start_ns:'9007199254740993',missing_paths:['original-missing-path'],source_evidence:{files:Array.from({length:1000},(_,i)=>({path:`original-file-${i}`,rows:'9007199254740995'}))}};
  state.options.config.datasets[0]!.inputCoverage=coverage;
  const get=async(path:string,status=200)=>{const response=await state.service.app.inject({url:`/api/v3${path}`,headers:state.headers});assert.equal(response.statusCode,status,response.body);return response.json();};
  const full=await get('/datasets');const summary=await get('/datasets?view=summary');const detail=await get('/datasets/controlled');
  assert.deepEqual(detail,full[0]);assert.deepEqual(detail.inputCoverage,coverage);
  const {inputCoverage:omitted,...metadata}=full[0];assert.deepEqual(omitted,coverage);assert.deepEqual(summary,[metadata]);
  assert.ok(JSON.stringify(summary).length<JSON.stringify(full).length/10);
  assert.deepEqual(state.options.config.datasets[0]!.inputCoverage,coverage);
  await get('/datasets?view=typo',400);await get('/datasets/absent',404);
  assert.equal((await state.service.app.inject({url:'/api/v3/datasets/controlled'})).statusCode,401);
 }finally{await state.cleanup();}
});

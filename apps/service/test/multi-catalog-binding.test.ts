import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdir,realpath} from 'node:fs/promises';
import {join} from 'node:path';
import {setup,context,fauxAssistantMessage} from './support.ts';
import {Requests} from '../../research/src/documents.ts';

test('additional registered catalogs are captured once and retries preserve original roots and identities',async()=>{
 const state=await setup();
 try{
  const first=state.options.config.datasets[0]!;
  first.data={catalog:join(state.directory,'catalog')};delete first.strategy;
  const extra=join(state.directory,'book');await mkdir(extra);
  state.options.config.datasets.push({...first,id:'book',name:'Original book',source:'book-source',data:{catalog:extra}}, {...first,id:'not-ready',ready:false}, {...first,id:'same-root'});
  await state.restart();state.faux.setResponses([fauxAssistantMessage('Metadata only, no execution.')]);
  const payload={requestId:'multi',goal:'Review bound metadata only',model:state.model,datasetId:'controlled',additionalDatasetIds:['book']};
  const send=(body:unknown)=>state.service.app.inject({method:'POST',url:'/api/v3/research',headers:state.headers,payload:body as any});
  const response=await send(payload);assert.equal(response.statusCode,202,response.body);
  const receipt=response.json();await (await state.service.research.harness.submission(receipt.submissionId,context))!.wait(context);
  const request=(await state.service.research.harness.snapshot(Requests,context))!.requests['request:multi']!;
  const bindings=(request.input as any).data.catalogBindings as any[];
  assert.deepEqual(bindings.map(x=>[x.datasetId,x.catalog]),[['controlled',await realpath(first.data.catalog as string)],['book',await realpath(extra)]]);
  assert.equal(bindings[1].dataset.source,'book-source');
  state.faux.setResponses([fauxAssistantMessage('Single root unchanged.')]);
  const single=await send({...payload,requestId:'single-empty',additionalDatasetIds:[]});assert.equal(single.statusCode,202,single.body);
  await (await state.service.research.harness.submission(single.json().submissionId,context))!.wait(context);
  assert.equal(((await state.service.research.harness.snapshot(Requests,context))!.requests['request:single-empty']!.input as any).data.catalogBindings,undefined);
  state.options.config.datasets[1]!.data.catalog='/nonexistent/new-root';state.options.config.datasets[1]!.source='changed';await state.restart();
  assert.deepEqual((await send(payload)).json(),receipt);
  assert.deepEqual(((await state.service.research.harness.snapshot(Requests,context))!.requests['request:multi']!.input as any).data.catalogBindings,bindings);
  assert.equal((await send({...payload,additionalDatasetIds:[]})).statusCode,409);
  for(const additionalDatasetIds of [['missing'],['not-ready'],['same-root'],['controlled'],['book','book'],[2],'book']){
   const result=await send({...payload,requestId:JSON.stringify(additionalDatasetIds),additionalDatasetIds});assert.ok(result.statusCode>=400,result.body);
  }
  assert.equal((await send({...payload,requestId:'literature',mode:'LITERATURE_ONLY'})).statusCode,400);
  const direct={requestId:'direct-catalog',goal:payload.goal,model:state.model,catalog:extra};
  const conflicting=await send({...direct,datasetId:'controlled'});
  assert.equal(conflicting.statusCode,400); assert.equal(conflicting.json().error,'research_catalog_binding_invalid');
  const noRunner=await send(direct);
  assert.equal(noRunner.statusCode,422); assert.equal(noRunner.json().error,'research_catalog_requires_local_runner');
 }finally{await state.cleanup();}
});

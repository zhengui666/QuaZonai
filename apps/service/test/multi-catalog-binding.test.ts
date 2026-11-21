import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdir,realpath,symlink} from 'node:fs/promises';
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

  const target: import('../../../packages/contracts/src/nautilus.ts').ProcessNativeTarget={
   kind:'RUST_PROCESS',id:'direct-catalog-test',binary:state.options.config.jobBinary,
   capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-only',precision:'high'},
    research:{singleStrategyRegistration:false,configuredRunFactory:true},
    account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false},
  };
  state.options.config.nativeProcess=target;await state.restart();
  const hf={...direct,requestId:'hf-catalog',catalog:'hf://datasets/a/native@'+'A'.repeat(40)};
  const readSettings=()=>state.service.app.inject({method:'GET',url:'/api/v3/settings',headers:state.headers});
  assert.equal((await readSettings()).json().nativeHfCatalogs,undefined);
  assert.equal((await send(hf)).json().error,'research_hf_catalog_runner_unsupported');
  for (const catalog of ['hf://datasets/a/native@main','hf://datasets/a/native@'+'a'.repeat(40)+'/data','hf://datasets/a/native@'+'a'.repeat(40)+'?token=x','hf://datasets/a/../native@'+'a'.repeat(40)]) {
   assert.equal((await send({...hf,catalog})).json().error,'research_hf_catalog_requires_single_pinned_dataset');
  }
  assert.equal((await send({...hf,catalog:'https://huggingface.co/datasets/a/native'})).json().error,'research_catalog_local_path_required');
  assert.equal((await send({...hf,additionalCatalogs:[extra]})).json().error,'research_hf_catalog_requires_single_pinned_dataset');
  const hfTarget={...target,capabilities:{...target.capabilities,research:{...target.capabilities.research,hfCatalogs:true}}};
  state.options.config.nativeProcess=hfTarget;await state.restart();
  assert.equal((await readSettings()).json().nativeHfCatalogs,true);
  state.faux.setResponses([fauxAssistantMessage('Pinned catalog admission only. No native invocation.')]);
  const hfResponse=await send(hf);assert.equal(hfResponse.statusCode,202,hfResponse.body);
  const hfReceipt=hfResponse.json();
  await (await state.service.research.harness.submission(hfReceipt.submissionId,context))!.wait(context);
  const hfSaved=(await state.service.research.harness.snapshot(Requests,context))!.requests['request:hf-catalog']!;
  assert.deepEqual(JSON.parse(JSON.stringify((hfSaved.input as {data:unknown}).data)),{catalog:hf.catalog.toLowerCase(),nautilusTarget:hfTarget});
  assert.deepEqual(JSON.parse(JSON.stringify(hfSaved.metadata?.original)),hf);
  state.options.config.nativeProcess=target;await state.restart();
  const hfRepeat=await send(hf);assert.equal(hfRepeat.statusCode,202,hfRepeat.body);assert.deepEqual(hfRepeat.json(),hfReceipt);
  assert.equal((await send({...hf,catalog:'hf://datasets/a/native@'+'b'.repeat(40)})).statusCode,409);
  const relative=await send({...direct,requestId:'relative-catalog',catalog:'book'});
  assert.equal(relative.statusCode,422,relative.body);
  assert.equal(relative.json().error,'research_catalog_local_path_required');
  state.faux.setResponses([fauxAssistantMessage('Catalog admission only. No native invocation.')]);
  const accepted=await send(direct);assert.equal(accepted.statusCode,202,accepted.body);
  const directReceipt=accepted.json();
  await (await state.service.research.harness.submission(directReceipt.submissionId,context))!.wait(context);
  const saved=(await state.service.research.harness.snapshot(Requests,context))!.requests['request:direct-catalog']!;
  assert.deepEqual(JSON.parse(JSON.stringify((saved.input as {data:unknown}).data)),{catalog:await realpath(extra),nautilusTarget:target});
  assert.deepEqual(JSON.parse(JSON.stringify(saved.metadata?.original ?? null)),direct);
  assert.equal(saved.metadata?.dataset,undefined);

  const directMulti={...direct,requestId:'direct-multiple',additionalCatalogs:[first.data.catalog as string]};
  assert.equal((await send(directMulti)).json().error,'native_multi_catalog_binding_not_available_rebuild_runner');
  const multiTarget={...target,capabilities:{...target.capabilities,research:{...target.capabilities.research,multiCatalogBindings:true}}};
  state.options.config.nativeProcess=multiTarget;await state.restart();
  const alias=join(state.directory,'book-alias');await symlink(extra,alias,'dir');
  assert.equal((await send({...directMulti,requestId:'duplicate-root',additionalCatalogs:[alias]})).json().error,'research_duplicate_catalog_binding');
  assert.equal((await send({...directMulti,requestId:'outside-root',additionalCatalogs:[join(state.directory,'..')]})).json().error,'research_catalog_outside_data_root');
  for (const body of [{...directMulti,catalog:state.directory},{...directMulti,additionalCatalogs:[state.directory]}]) {
   const broad=await send({...body,requestId:'broad-root'});
   assert.equal(broad.statusCode,400,broad.body);
   assert.equal(broad.json().error,'research_catalog_binding_invalid');
  }
  assert.equal((await send({...payload,requestId:'unpaired-additional-catalogs',additionalCatalogs:[extra]})).json().error,'research_catalog_binding_invalid');
  state.faux.setResponses([fauxAssistantMessage('Local catalog binding only. No native invocation.')]);
  const multiAccepted=await send(directMulti);assert.equal(multiAccepted.statusCode,202,multiAccepted.body);
  const multiReceipt=multiAccepted.json();
  await (await state.service.research.harness.submission(multiReceipt.submissionId,context))!.wait(context);
  const multiSaved=(await state.service.research.harness.snapshot(Requests,context))!.requests['request:direct-multiple']!;
  assert.deepEqual(JSON.parse(JSON.stringify((multiSaved.input as {data:unknown}).data)),{
   catalog:await realpath(extra),nautilusTarget:multiTarget,
   catalogBindings:[{catalogRef:'catalog-1',catalog:await realpath(extra)},{catalogRef:'catalog-2',catalog:await realpath(first.data.catalog as string)}],
  });
  assert.equal(multiSaved.metadata?.dataset,undefined);

  delete state.options.config.nativeProcess;await state.restart();
  const multiRepeated=await send(directMulti);assert.equal(multiRepeated.statusCode,202,multiRepeated.body);
  assert.deepEqual(multiRepeated.json(),multiReceipt);
  assert.equal((await send({...directMulti,additionalCatalogs:[alias]})).statusCode,409);
  const repeated=await send(direct);assert.equal(repeated.statusCode,202,repeated.body);
  assert.deepEqual(repeated.json(),directReceipt);
  const changed=await send({...direct,catalog:state.directory});
  assert.equal(changed.statusCode,409,changed.body);
  assert.equal(changed.json().error,'request_id_conflict');
 }finally{await state.cleanup();}
});

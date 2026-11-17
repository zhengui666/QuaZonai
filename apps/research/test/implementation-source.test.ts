import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { registerImplementationSources, ImplementationSources } from '../src/implementation-source.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { ResearchAssessment } from '../plugins/research-assessment.ts';
const request = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }], event_start_ns: '1', event_end_ns: '100', available_end_ns: '100',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['5000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' }, strategy: { kind: 'EMA_CROSS', trade_size: '1', fast_period: 2, slow_period: 3 } };
const judgment = { stars: null, rationale: 'Partial owner excerpts, missing dependencies and build correspondence.', risks: [], uncertainties: ['Unreviewed dependencies'] };
test('authorized source pages use an isolated official Pi child, record actual ranges, and retain historical reports', async () => {
 const previousBinary=process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY='/nonexistent-native-binary';
 const directory = await mkdtemp(join(tmpdir(), 'qz-source-review-'));
 const faux = fauxProvider({ tokensPerSecond: 100000 }); const models = createModels(); models.setProvider(faux.provider);
 const model = { provider: faux.provider.id, modelId: faux.getModel().id };
 const options = { database: join(directory,'research.sqlite'), models }; let host = await openResearch(options);
 try {
  faux.setResponses([fauxAssistantMessage('Ready for explicit owner material.')]);
  const started = await host.start('source-study',{goal:'PRIVATE_PARENT_SENTINEL',model,data:{catalog:directory,native_strategy_backtest_request:request},policy:{}});
  await (await host.harness.submission(started.submissionId,context))!.wait(context);
  const body = {requestId:'source-one',shareWithResearchModel:true,model,files:[{id:'strategy',name:'strategy.rs',revision:'owner-rev',content:'pub fn strategy() { hidden_dependency(); }'}]};
  const register = (value:unknown)=>host.harness.commit(tx=>registerImplementationSources(tx,started.conversationId,value),context);
  await assert.rejects(register({...body,shareWithResearchModel:false}),/explicit_sharing_required/);
  await assert.rejects(register({...body,model:{...model,modelId:'other'}}),/model_mismatch/);
  await assert.rejects(register({...body,path:'/home/owner/.env'}),/explicit_sharing_required/);
  const manifest = await register(body); assert.ok(!JSON.stringify(manifest).includes('hidden_dependency'));
  assert.deepEqual(await register(body),manifest);
  await assert.rejects(register({...body,files:[{...body.files[0],content:'changed'}]}),/request_conflict/);
  await register({...body,requestId:'not-selected',files:[{id:'private',name:'not-selected.rs',revision:'r',content:'UNSELECTED_SOURCE_SENTINEL'}]});
  faux.setResponses([
   fauxAssistantMessage(fauxToolCall('native_strategy_logic_review',{hypothesis:'Review visible implementation',sourceRequestId:'source-one'}),{stopReason:'toolUse'}),
   input=>{ const wire=JSON.stringify(input); assert.ok(!wire.includes('hidden_dependency')); assert.ok(!wire.includes('PRIVATE_PARENT_SENTINEL')); assert.ok(!wire.includes('UNSELECTED_SOURCE_SENTINEL')); return fauxAssistantMessage(fauxToolCall('native_assessment_read_source',{fileId:'strategy',offset:body.files[0]!.content.length,length:10}),{stopReason:'toolUse'}); },
   input=>{assert.ok(!JSON.stringify(input).includes('hidden_dependency'));return fauxAssistantMessage(fauxToolCall('native_assessment_read_source',{fileId:'strategy',offset:0,length:10}),{stopReason:'toolUse'});},
   input=>{assert.ok(JSON.stringify(input).includes('pub fn str')); return fauxAssistantMessage(fauxToolCall('native_assessment_read_source',{fileId:'private',offset:0,length:4096}),{stopReason:'toolUse'});},
   input=>{assert.ok(!JSON.stringify(input).includes('UNSELECTED_SOURCE_SENTINEL'));return fauxAssistantMessage(JSON.stringify(judgment));},
   fauxAssistantMessage('Only the first ten source code units were read; dependencies remain unreviewed.'),
  ]);
  const submission=await (await host.harness.conversation(started.conversationId,context))!.submit({type:'input',content:'Review registered source-one',requestId:'review-source'},context); await submission.wait(context);
  const assessment=await host.harness.snapshot(ResearchAssessment,started.conversationId,context); assert.ok(assessment?.logicTaskId);
  const task=await host.harness.getTask(assessment.logicTaskId,context); assert.equal(task!.state.outcome?.status,'completed');
  if(task!.state.outcome?.status!=='completed')assert.fail('review failed');
  const result=task!.state.outcome.result as any;
  assert.equal(result.claim,'SCOPED_SOURCE_LOGIC_REVIEW'); assert.equal(result.implementationSourceScope,'OWNER_SUPPLIED_EXCERPTS_READ_NOT_BUILD_VERIFIED');
  assert.equal(result.sourceReads.length,1); assert.equal(result.sourceReads[0].endExclusive,10); assert.equal(result.sourceReads[0].nextOffset,10);
  assert.equal(result.evidence.implementationSourceInspected,true); assert.ok(result.reviewConversationId); assert.equal(result.tradingPermission,false);
  const conversation=(await host.harness.conversation(started.conversationId,context))!;
  const execution=await conversation.commit(tx=>tx.createTask(NativeStrategyBacktestTask,{request,catalog:directory,target:'/nonexistent-native-binary'},{ownership:{kind:'conversation'}}),context);
  await host.harness.waitForTask(execution,context);
  const oldExecution=await host.harness.getTask(execution,context);
  faux.setResponses([
   fauxAssistantMessage(fauxToolCall('native_strategy_logic_review',{hypothesis:'Later source review',sourceRequestId:'source-one',reviewTiming:'POST_EXECUTION'}),{stopReason:'toolUse'}),
   input=>{const wire=JSON.stringify(input);assert.ok(wire.includes('post-execution logic review'));assert.ok(wire.includes('NO_TRANSCRIPT_OR_NATIVE_RESULTS'));assert.ok(!wire.includes('PRIVATE_PARENT_SENTINEL'));return fauxAssistantMessage(fauxToolCall('native_assessment_read_source',{fileId:'strategy',offset:0,length:4096}),{stopReason:'toolUse'});},
   input=>{assert.ok(JSON.stringify(input).includes('hidden_dependency'));return fauxAssistantMessage(JSON.stringify({...judgment,reviewTiming:'PRE_EXECUTION'}));},
   fauxAssistantMessage('Separate retrospective review recorded.'),
  ]);
  await (await conversation.submit({type:'input',content:'Review after execution without rerunning',requestId:'post-source'},context)).wait(context);
  const reviews=await host.harness.commit(tx=>tx.scanTasks({conversationId:started.conversationId,kind:'qz.research.assessment'},20),context);
  const post=reviews.items.find(t=>(t.input as any).evidence.reviewTiming==='POST_EXECUTION')!;
  assert.ok(post);assert.equal(post.state.outcome?.status,'completed');
  const postResult=post.state.outcome!.status==='completed'?post.state.outcome!.result as any:null;
  assert.equal(postResult.reviewTiming,'POST_EXECUTION');assert.equal(postResult.claim,'POST_EXECUTION_SOURCE_LOGIC_REVIEW');
  assert.equal(postResult.sourceReads[0].nextOffset,null);assert.deepEqual(postResult.evidence.priorExecutionTaskIds,[execution]);
  assert.deepEqual(await host.harness.snapshot(ResearchAssessment,started.conversationId,context),assessment);
  assert.deepEqual((await host.harness.getTask(assessment.logicTaskId,context))!.state.outcome!.result,result);
  assert.deepEqual(await host.harness.getTask(execution,context),oldExecution);
  await host.close(); host=await openResearch(options);
  assert.deepEqual((await host.harness.getTask(assessment.logicTaskId,context))!.state.outcome!.result,result);
  assert.equal(Object.keys((await host.harness.snapshot(ImplementationSources,started.conversationId,context))!.bundles).length,2);
 } finally {await host.close();await rm(directory,{recursive:true,force:true});if(previousBinary===undefined)delete process.env.QZ_JOB_BINARY;else process.env.QZ_JOB_BINARY=previousBinary;}
});

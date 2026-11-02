import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import type { ResearchInput } from '../src/documents.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import { fact } from '../src/pm-campaign.ts';
import type { ResearchCampaignBinding } from '../src/pm-campaign.ts';
import { ReviewLedger, latestTrialReview } from '../src/review-documents.ts';
import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
const call = (name:string,args:JsonObject)=>fauxAssistantMessage(fauxToolCall(name,args),{stopReason:'toolUse'});
function last(transcript:{messages:readonly unknown[]},name:string):JsonObject {
  const entry=transcript.messages.filter(value=>(value as {toolName?:string}).toolName===name).at(-1) as {content:{text?:string}[]};
  return JSON.parse(entry.content.map(item=>item.text??'').join('')) as JsonObject;
}
function toolText(transcript:{messages:readonly unknown[]},name:string):string { const entry=transcript.messages.filter(value=>(value as {toolName?:string}).toolName===name).at(-1) as {content:{text?:string}[]}; return entry.content.map(item=>item.text??'').join(''); }
function binding(maximumTrials:number):ResearchCampaignBinding {
  const definition={id:'finish.ic/1',key:'factor.pearson_ic',definitionVersion:'1',label:'TEST_ONLY Pearson IC',unit:'DIMENSIONLESS' as const,valueEncoding:'FLOAT64' as const,direction:'MAXIMIZE' as const,aggregation:'PEARSON',calendarId:null,annualizationDays:null,horizonRef:'one-bar',requiredInputs:['prediction','mature-label']};
  return {campaignId:'finish-campaign',mandateRevisionId:'finish-mandate',protocolRef:'finish-protocol',datasetRevisionIds:['finish-dataset'],independentConsumer:'PM_PORTFOLIO',
    policy:{...fact('finish-policy','0'),mandateRevisionId:'finish-mandate',methodFamily:'TEST_ONLY_FINISH_PROTOCOL',nativeImplementationVersion:'TEST_ONLY',learnedParametersRef:null,trainingProtocolRef:'finish-protocol',validationEvidenceRefs:[],applicableCapabilityIds:[],validDomain:{},numericalToleranceRef:'TEST_ONLY_EXACT',
      selectionRule:{mode:'LEXICOGRAPHIC',objectives:[{metricKey:definition.key,metricDefinitionId:definition.id,direction:'MAXIMIZE',horizonRef:'one-bar',scope:'FORECAST'}],utilityModelRef:null,establishedBy:'OWNER_GOAL',rationale:'TEST_ONLY software protocol, no market assertion'},limitations:['TEST_ONLY']},
    budget:{...fact('finish-budget','0'),maximumTrials,nativeComputeSeconds:null,modelSpend:null,dataBytes:null,reservationForClosureRef:'finish-closure',usageEvidenceRefs:[]},
    scopes:[{id:'finish-scope',allowedUniverseRef:'finish-universe',datasetRevisionIds:['finish-dataset'],interval:{startNs:'10',endExclusiveNs:'20'},role:'SELECTION',calendarId:'UTC',labelDefinitionId:'one-bar',accountingPolicyRef:null,costModelRef:null,startingStateRef:null,capital:null}],definitions:[definition],frontierDefinitionIds:[definition.id]};
}
const hypothesis=()=>call('research_hypothesis',{economicFamilyId:'TEST_ONLY',mechanism:'TEST_ONLY controlled evidence',prediction:'Original scalar compares candidates',falsification:'The source may fail',requiredDataCapabilities:[],incrementalQuestion:'Preserve the explicit finish producer boundary'});
async function fixture(pm=false,sealed=false) {
  const directory=await mkdtemp(join(tmpdir(),'qz-research-finish-review-'));const faux=fauxProvider({tokensPerSecond:100000});const models=createModels();models.setProvider(faux.provider);
  let computed=0; let pendingId:TaskId<JsonObject>|null=null; let releasePending!:()=>void;
  const pendingGate=new Promise<void>(resolve=>{releasePending=resolve;});
  const Pending=defineTask<JsonObject,{phase:'wait'},JsonObject>({name:'test.finish-pending',version:1,initial:()=>({phase:'wait'}),
    phases:{async wait(task,runtime,ctx){await awaitWithContext(pendingGate,ctx);await runtime.commit(async tx=>{const trial=(await tx.doc(Study,runtime.conversationId)).trials.find(row=>row.taskId===task.id)!;trial.status='FAILED';trial.error='TEST_ONLY_PENDING_RECEIPT';return{status:'terminal',outcome:{status:'completed',result:{settled:true}}};},ctx);}},
    async abort(task,runtime,ctx){await runtime.commit(async tx=>{const trial=(await tx.doc(Study,runtime.conversationId)).trials.find(row=>row.taskId===task.id);if(trial)trial.status='FAILED';return{status:'terminal',outcome:{status:'aborted'}};},ctx);}});

  const science=defineExtension({name:'test.finish-science',tools:[defineTool({name:'finish_trial',description:'TEST_ONLY evaluation receipt; not market/PIT evidence',
    parameters:Type.Object({value:Type.Number(),hypothesisId:Type.Optional(Type.String()),portfolio:Type.Optional(Type.Boolean())}),replay:'safe',executionMode:'sequential',
    async execute(args,api,ctx) {return trialFeedback(api,ctx,()=>evaluateTrial(api,ctx,{hypothesis:'TEST_ONLY original candidate',...(args.portfolio?{members:[{model:'(module)',weight:'1'}]}:{model:'(module)'}),purpose:'FEASIBILITY',...(args.hypothesisId?{hypothesisId:args.hypothesisId}:{})},async()=>{
      computed++;return {partitions:[{partition:'VALIDATION',start_ns:'10',end_ns:'20',metric:{pearson_ic:{status:'AVAILABLE',value:args.value},counts:{observations:'10',included:'10'}}}]};
    }));},
  }),defineTool({name:'finish_mark_pending',description:'TEST_ONLY owned pending task boundary',parameters:Type.Object({}),replay:'safe',executionMode:'sequential',
    async execute(_args,api,ctx){pendingId=await api.commit(async tx=>{const id=await tx.createTask(Pending,{}, {ownership:{kind:'conversation'}});(await tx.doc(Study,api.conversationId)).trials.push({taskId:id,status:'STARTED',proposal:{model:'(module)',hypothesis:'TEST_ONLY_PENDING'},result:null});return id;},ctx);return{content:[{type:'text',text:JSON.stringify({taskId:pendingId})}]};}}),
    defineTool({name:'finish_release_pending',description:'TEST_ONLY settle the original owned task',parameters:Type.Object({}),replay:'safe',executionMode:'sequential',
      async execute(_args,api,ctx){releasePending();if(pendingId!==null)await api.waitForTask(pendingId,ctx);return{content:[{type:'text',text:'Original pending task settled.'}]};}})],tasks:[Pending]});
  const options={database:join(directory,'research.sqlite'),models,extensions:[research,science]};let host=await openResearch(options);
  const input:ResearchInput={goal:'TEST_ONLY explicit selected research finish',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:sealed?{sealed:{marker:'OWNER_ONLY_UNREAD_SUFFIX'}}:{},policy:{max_trials:4},...(pm?{campaign:binding(4)}:{})};
  return {faux,input,get host(){return host;},get computed(){return computed;},async reopen(){await host.close();host=await openResearch(options);},async close(){await host.close();await rm(directory,{recursive:true,force:true});}};
}
function evaluate(pm:boolean,value:number) { return (transcript:{messages:readonly unknown[]})=>call('finish_trial',{value,...(pm?{hypothesisId:last(transcript,'research_hypothesis').id!}:{})}); }
function reviewer(ref:()=>string) {return [...['context','provenance','factor','selection','stress','portfolio'].map(kind=>call(`review_${kind}`,{})),()=>call('review_finish',{narrative:'TEST_ONLY actual review producer, missing scientific sources remain explicit.',findings:[],mechanism:['mechanism','availability','direction','implementation','complexity'].map(criterion=>({criterion,supported:false,evidenceRefs:[ref()]}))})];}
async function noReviewYet(f:Awaited<ReturnType<typeof fixture>>,id:number,expectedTrials:number) {
  const state=await f.host.state(id);assert.equal(state.research!.final,null);assert.equal(state.research!.reviewSelection,undefined);assert.equal(state.research!.trials.length,expectedTrials);
  const reviews=await f.host.harness.snapshot(ReviewLedger,id as any,context);assert.equal(Object.keys(reviews?.records??{}).length,0);
  const tasks=await f.host.harness.commit(tx=>tx.scanTasks({kind:'qz.review.independent'},100),context);assert.equal(tasks.items.length,0,'invalid selection cannot admit a reviewer or its budget');
  if(state.campaignResearch?.binding)assert.equal(state.campaignResearch.budgetMovements.filter(row=>row.resource==='TRIAL'&&row.kind==='CONSUME').length,expectedTrials);
}

// Actual official Harness/provider/tool execution; reports are never uploaded or pre-scored.
for(const pm of [false,true])test(`explicit research_finish produces a real review for ${pm?'PM_PORTFOLIO':'standalone without sealed'} selection`,{timeout:30000},async()=>{
  const f=await fixture(pm,pm);let id=0;let selected=0;
  try {
    f.faux.setResponses([...(pm?[hypothesis()]:[]),evaluate(pm,0.7),transcript=>{selected=last(transcript,'finish_trial').trialId as number;return call('research_finish',{trialId:selected,reason:'TEST_ONLY explicit selected finish'});},...reviewer(()=>`research:${id}:trial:${selected}:validation`)]);
    const receipt=await f.host.start(`selected-${pm}`,f.input);id=receipt.conversationId;
    await (await f.host.harness.submission(receipt.submissionId,context))!.wait(context);await f.host.harness.waitForIdle(context);
    const state=await f.host.state(id);assert.ok(state.research!.input!.policy.reviewPolicies,'modern default review policies stay frozen');
    const report=latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),selected);assert.ok(report,JSON.stringify(state.research));
    assert.equal(report.decision,'INCONCLUSIVE','missing true source evidence cannot be invented by the real reviewer');assert.equal(state.research!.final!.trialId,selected);assert.equal(state.research!.final!.result,null);
    assert.equal(state.research!.final!.reviewReportId,report.id);assert.equal(state.research!.final!.reason,'TEST_ONLY explicit selected finish');
    const tasks=await f.host.harness.commit(tx=>tx.scanTasks({kind:'qz.review.independent'},100),context);assert.equal(tasks.items.length,1);assert.equal(tasks.items[0]!.state.status,'terminal');
    const calls=f.faux.state.callCount;const ledger=await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context);const budget=state.campaignResearch?.budgetMovements;
    await f.reopen();assert.deepEqual(await f.host.start(`selected-${pm}`,f.input),receipt);assert.equal(f.faux.state.callCount,calls);assert.equal(f.computed,1);
    assert.deepEqual(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),ledger);assert.deepEqual((await f.host.state(id)).campaignResearch?.budgetMovements,budget);
    assert.equal(JSON.stringify((await f.host.view(id)).entries).includes('OWNER_ONLY_UNREAD_SUFFIX'),false);
  }finally{await f.close();}
});

test('an invalid explicit trial is rejected before review freezes search and a later valid trial can finish',{timeout:30000},async()=>{
  const f=await fixture();let id=0;let selected=0;
  try {
    f.faux.setResponses([evaluate(false,0.2),call('research_finish',{trialId:999999,reason:'invalid original selection'}),async transcript=>{
      assert.match(toolText(transcript,'research_finish'),/selected_trial_not_computed/);await noReviewYet(f,id,1);return evaluate(false,0.8)(transcript);
    },transcript=>{selected=last(transcript,'finish_trial').trialId as number;return call('research_finish',{trialId:selected,reason:'valid after rejection'});},...reviewer(()=>`research:${id}:trial:${selected}:validation`)]);
    const receipt=await f.host.start('invalid-then-valid',f.input);id=receipt.conversationId;await (await f.host.harness.submission(receipt.submissionId,context))!.wait(context);await f.host.harness.waitForIdle(context);
    const report=latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),selected);assert.ok(report);assert.equal(f.computed,2);assert.equal((await f.host.state(id)).research!.final!.reviewReportId,report.id);
  }finally{await f.close();}
});

test('non-PM sealed selection keeps its registered finalizer requirement and leaves search admissible', {timeout:30000},async()=>{
  const f=await fixture(false,true);let id=0;
  try {
    f.faux.setResponses([evaluate(false,0.2),transcript=>call('research_finish',{trialId:last(transcript,'finish_trial').trialId!,reason:'wrong unseen entry'}),async transcript=>{
      assert.match(toolText(transcript,'research_finish'),/registered_final_evaluator_required/);await noReviewYet(f,id,1);return evaluate(false,0.8)(transcript);
    },call('research_finish',{reason:'stop without selecting or reading unseen data'})]);
    const receipt=await f.host.start('sealed-wrong-entry',f.input);id=receipt.conversationId;await (await f.host.harness.submission(receipt.submissionId,context))!.wait(context);await f.host.harness.waitForIdle(context);
    assert.equal(f.computed,2);const state=await f.host.state(id);assert.equal(state.research!.final!.trialId,null);assert.equal(Object.keys((await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context))?.records??{}).length,0);
  }finally{await f.close();}
});

test('comparison-refused explicit selection does not start review and the PM campaign can continue', {timeout:30000},async()=>{
  const f=await fixture(true,true);let id=0;let first=0;let selected=0;
  try {
    f.faux.setResponses([hypothesis(),evaluate(true,0.2),transcript=>{first=last(transcript,'finish_trial').trialId as number;return evaluate(true,0.8)(transcript);},()=>call('research_finish',{trialId:first,reason:'not the registered winner'}),async transcript=>{
      assert.match(toolText(transcript,'research_finish'),/selected_trial_not_supported_by_comparison/);await noReviewYet(f,id,2);return evaluate(true,0.9)(transcript);
    },transcript=>{selected=last(transcript,'finish_trial').trialId as number;return call('research_finish',{trialId:selected,reason:'registered winner after rejected selection'});},...reviewer(()=>`research:${id}:trial:${selected}:validation`)]);
    const receipt=await f.host.start('comparison-then-valid',f.input);id=receipt.conversationId;await (await f.host.harness.submission(receipt.submissionId,context))!.wait(context);await f.host.harness.waitForIdle(context);
    assert.equal(f.computed,3);const report=latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),selected);assert.ok(report);assert.equal((await f.host.state(id)).research!.final!.reviewReportId,report.id);
  }finally{await f.close();}
});

test('an explicit profile mismatch is refused before review and a matching factor can still finish', {timeout:30000},async()=>{
  const f=await fixture();let id=0;let selected=0;
  f.input.policy.reviewPolicy=JSON.parse(JSON.stringify(createReviewPolicy('FACTOR','TEST_ONLY_EXPLICIT_FACTOR_POLICY','90'))) as JsonObject;
  try {
    f.faux.setResponses([call('finish_trial',{value:0.2,portfolio:true}),transcript=>call('research_finish',{trialId:last(transcript,'finish_trial').trialId!,reason:'wrong target kind'}),async transcript=>{
      assert.match(toolText(transcript,'research_finish'),/review_policy_target_mismatch/);await noReviewYet(f,id,1);return evaluate(false,0.8)(transcript);
    },transcript=>{selected=last(transcript,'finish_trial').trialId as number;return call('research_finish',{trialId:selected,reason:'matching original policy kind'});},...reviewer(()=>`research:${id}:trial:${selected}:validation`)]);
    const receipt=await f.host.start('profile-kind-then-valid',f.input);id=receipt.conversationId;await (await f.host.harness.submission(receipt.submissionId,context))!.wait(context);await f.host.harness.waitForIdle(context);
    assert.equal(f.computed,2);const report=latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),selected);assert.ok(report);assert.equal(report.target.kind,'FACTOR');assert.equal((await f.host.state(id)).research!.final!.reviewReportId,report.id);
  }finally{await f.close();}
});

test('a live owned pending trial blocks explicit finish before review and search resumes after its settlement', {timeout:30000},async()=>{
  const f=await fixture();let id=0;let first=0;let selected=0;
  try {
    f.faux.setResponses([evaluate(false,0.2),transcript=>{first=last(transcript,'finish_trial').trialId as number;return call('finish_mark_pending',{});},()=>call('research_finish',{trialId:first,reason:'pending trials cannot freeze'}),async transcript=>{
      assert.match(toolText(transcript,'research_finish'),/research_trials_unsettled/);await noReviewYet(f,id,2);return call('finish_release_pending',{});
    },evaluate(false,0.8),transcript=>{selected=last(transcript,'finish_trial').trialId as number;return call('research_finish',{trialId:selected,reason:'valid after owned pending settled'});},...reviewer(()=>`research:${id}:trial:${selected}:validation`)]);
    const receipt=await f.host.start('pending-then-valid',f.input);id=receipt.conversationId;await (await f.host.harness.submission(receipt.submissionId,context))!.wait(context);await f.host.harness.waitForIdle(context);
    const state=await f.host.state(id);assert.equal(f.computed,2);assert.equal(state.research!.trials.length,3);assert.equal(state.research!.trials.filter(row=>row.status==='FAILED').length,1);
    const report=latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),selected);assert.ok(report);assert.equal(state.research!.final!.reviewReportId,report.id);
  }finally{await f.close();}
});

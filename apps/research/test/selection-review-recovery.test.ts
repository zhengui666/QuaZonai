import { obj, opt, str } from '@earendil-works/pi-schema';
import { closeResearch } from '../src/closure.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import type { ResearchInput } from '../src/documents.ts';
import { evaluateTrial, trialFeedback } from '../src/trials.ts';
import { fact } from '../src/pm-campaign.ts';
import type { ResearchCampaignBinding } from '../src/pm-campaign.ts';
import { ReviewLedger, ReviewSession, latestTrialReview } from '../src/review-documents.ts';
import { reviewSourceDataDigest } from '../src/review-source.ts';
import type { CapturedReviewSource } from '../src/review-source.ts';
import { ensureTrialReview } from '../src/review-workflow.ts';
import { researchRuntime } from '../src/research-runtime.ts';
import research from '../plugins/research.ts';

const context=BACKGROUND_CONTEXT;
const call=(name:string,args:JsonObject)=>fauxAssistantMessage(fauxToolCall(name,args),{stopReason:'toolUse'});
function last(transcript:{messages:readonly unknown[]},name:string):JsonObject {const entry=transcript.messages.filter(value=>(value as {toolName?:string}).toolName===name).at(-1) as {content:{text?:string}[]};return JSON.parse(entry.content.map(item=>item.text??'').join('')) as JsonObject;}
function binding():ResearchCampaignBinding {
  const definition={id:'recovery.ic/1',key:'factor.pearson_ic',definitionVersion:'1',label:'TEST_ONLY IC',unit:'DIMENSIONLESS' as const,valueEncoding:'FLOAT64' as const,direction:'MAXIMIZE' as const,aggregation:'PEARSON',calendarId:null,annualizationDays:null,horizonRef:'one-bar',requiredInputs:['prediction','mature-label']};
  return {campaignId:'recovery-campaign',mandateRevisionId:'recovery-mandate',protocolRef:'recovery-protocol',datasetRevisionIds:['recovery-dataset'],independentConsumer:'PM_PORTFOLIO',
    policy:{...fact('recovery-policy','0'),mandateRevisionId:'recovery-mandate',methodFamily:'TEST_ONLY',nativeImplementationVersion:'TEST_ONLY',learnedParametersRef:null,trainingProtocolRef:'recovery-protocol',validationEvidenceRefs:[],applicableCapabilityIds:[],validDomain:{},numericalToleranceRef:'TEST_ONLY',selectionRule:{mode:'LEXICOGRAPHIC',objectives:[{metricKey:definition.key,metricDefinitionId:definition.id,direction:'MAXIMIZE',horizonRef:'one-bar',scope:'FORECAST'}],utilityModelRef:null,establishedBy:'OWNER_GOAL',rationale:'TEST_ONLY recovery protocol'},limitations:['TEST_ONLY']},
    budget:{...fact('recovery-budget','0'),maximumTrials:1,nativeComputeSeconds:null,modelSpend:null,dataBytes:null,reservationForClosureRef:'recovery-closure',usageEvidenceRefs:[]},
    scopes:[{id:'recovery-scope',allowedUniverseRef:'recovery-universe',datasetRevisionIds:['recovery-dataset'],interval:{startNs:'10',endExclusiveNs:'20'},role:'SELECTION',calendarId:'UTC',labelDefinitionId:'one-bar',accountingPolicyRef:null,costModelRef:null,startingStateRef:null,capital:null}],definitions:[definition],frontierDefinitionIds:[definition.id]};
}
const CacheProbe=defineTask<{trialId:number},{phase:'read'},JsonObject>({name:'test.selection-review-cache',version:1,initial:()=>({phase:'read'}),
  phases:{async read(task,runtime,ctx){const report=await ensureTrialReview(researchRuntime(runtime),ctx,task.input.trialId);await runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result:{reportId:report?.id??null}}}),ctx);}},
  async abort(_task,runtime,ctx){await runtime.commit(()=>({status:'terminal',outcome:{status:'aborted'}}),ctx);}});
async function fixture(pm=false) {
  const directory=await mkdtemp(join(tmpdir(),'qz-selection-review-recovery-'));const log=join(directory,'native.log');const binary=join(directory,'job');const script=join(directory,'source.mjs');
  // Actual source-validation child process; this wire fixture does not certify real source/PIT quality.
  await writeFile(script,"import {readFileSync,appendFileSync} from 'node:fs';const input=JSON.parse(readFileSync(0,'utf8'));const command=process.argv[3];appendFileSync(process.argv[2],JSON.stringify({command,input})+'\\n');if(command!=='review-source')throw new Error('TEST_ONLY_UNSUPPORTED:'+command);console.log(JSON.stringify({schema_version:1,method_version:'qz.review-source.v1',validated:true,metadata:input.metadata}));");
  await writeFile(binary,`#!/bin/sh\nexec '${process.execPath}' '${script}' '${log}' "$@"\n`,{mode:0o700});
  const previous=process.env.QZ_JOB_BINARY;process.env.QZ_JOB_BINARY=binary;
  const faux=fauxProvider({tokensPerSecond:100000});const models=createModels();models.setProvider(faux.provider);let computed=0;let selected=0;let root=0;
  const science=defineExtension({name:'test.selection-source',tasks:[CacheProbe],tools:[defineTool({name:'close_selection',description:'TEST_ONLY original selection closure',parameters:obj({}),replay:'safe',async execute(_args,api,ctx){await closeResearch(api,ctx,'owner_selection_complete');return {control:{terminate:true}};}}),defineTool({name:'selection_trial',description:'TEST_ONLY controlled trial receipt',parameters:obj({hypothesisId:opt(str())}),replay:'safe',executionMode:'sequential',
    async execute(args,api,ctx){selected=api.taskId;return trialFeedback(api,ctx,()=>evaluateTrial(api,ctx,{hypothesis:'TEST_ONLY selected candidate',model:'(module)',purpose:'FEASIBILITY',...(args.hypothesisId?{hypothesisId:args.hypothesisId}:{})},async()=>{computed++;return{partitions:[{partition:'VALIDATION',start_ns:'10',end_ns:'20',metric:{pearson_ic:{status:'AVAILABLE',value:0.7},counts:{observations:'10',included:'10'}}}]};}));}})]});
  const options={database:join(directory,'research.sqlite'),models,extensions:[research,science]};let host=await openResearch(options);
  let entered!:()=>void;const pending=new Promise<void>(resolve=>{entered=resolve;});
  const input:ResearchInput={goal:'TEST_ONLY modern automatic selection closure recovery',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:pm?{sealed:{marker:'OWNER_ONLY_NEVER_READ'}}:{},policy:{selection_metric:'validation.pearson_ic'},...(pm?{campaign:binding()}:{})};
  const source:CapturedReviewSource={schemaVersion:1,partition:'VALIDATION',datasetId:'TEST_ONLY_SOURCE',sourceId:'TEST_ONLY_OWNER',sourceRevision:'TEST_ONLY_REVISION',snapshotDigest:'TEST_ONLY_SOURCE_BYTES',inputDataDigest:reviewSourceDataDigest(input.data),metadataDigest:'TEST_ONLY_METADATA',licenseDigest:'TEST_ONLY_GRANT',
    metadata:{partition:'VALIDATION',origin:'REAL',pit_status:'VERIFIED',revision_policy:'AS_KNOWN_THEN',provenance_reference:'TEST_ONLY_REFERENCE',availability_provenance:'TEST_ONLY_CLOCK_ASSERTION'},
    license:{schemaVersion:1,datasetId:'TEST_ONLY_SOURCE',sourceId:'TEST_ONLY_OWNER',sourceRevision:'TEST_ONLY_REVISION',datasetRevisionId:'TEST_ONLY_NATIVE',grantId:'TEST_ONLY_GRANT',issuer:'TEST_ONLY_ISSUER',reference:'TEST_ONLY_AUTHORIZATION',status:'ACTIVE',purposes:['RESEARCH'],validFromNs:'0',validUntilNs:'9223372036854775807'},receivedAt:'2026-11-02T00:00:00Z',validationStartNs:'10',validationEndNs:'20'};
  const hypothesis=()=>call('research_hypothesis',{economicFamilyId:'TEST_ONLY',mechanism:'TEST_ONLY receipt',prediction:'Original scalar selects the candidate',falsification:'Source may be missing',requiredDataCapabilities:[],incrementalQuestion:'Retain automatic closure identity'});
  faux.setResponses([...(pm?[hypothesis()]:[]),transcript=>call('selection_trial',pm?{hypothesisId:last(transcript,'research_hypothesis').id!}:{}),call('close_selection',{}),
    ...['context','provenance','factor','selection','stress','portfolio'].map(kind=>call(`review_${kind}`,{})),async(_transcript,options)=>{
      entered();
      await new Promise<never>((_resolve,reject)=>{const signal=options?.signal;if(signal?.aborted)reject(signal.reason);else signal?.addEventListener('abort',()=>reject(signal.reason),{once:true});});
      throw new Error('TEST_ONLY interrupted model response ends only on host close or owner abort');
    }]);
  const finish=()=>call('review_finish',{narrative:'TEST_ONLY recovered review with scientific gaps preserved.',findings:[],mechanism:['mechanism','availability','direction','implementation','complexity'].map(criterion=>({criterion,supported:false,evidenceRefs:[`research:${root}:trial:${selected}:validation`]}))});
  return {input,source,faux,pending,get host(){return host;},get selected(){return selected;},get computed(){return computed;},
    async waitForPause(){let timer:ReturnType<typeof setTimeout>|undefined;try{await Promise.race([pending,new Promise<never>((_resolve,reject)=>{timer=setTimeout(()=>reject(new Error(`Reviewer did not reach the interrupted response; provider calls=${faux.state.callCount}`)),4000);})]);}finally{if(timer)clearTimeout(timer);}},set root(value:number){root=value;},
    async reopen(){await host.close();faux.setResponses([finish()]);host=await openResearch(options);},
    async nativeCalls(){return(await readFile(log,'utf8').catch(()=>'')).trim().split('\n').filter(Boolean).map(row=>JSON.parse(row) as JsonObject);},
    async close(){await host.close();if(previous===undefined)delete process.env.QZ_JOB_BINARY;else process.env.QZ_JOB_BINARY=previous;await rm(directory,{recursive:true,force:true});}};
}
async function tasks(host:Awaited<ReturnType<typeof openResearch>>,kind:string){return host.harness.commit(tx=>tx.scanTasks({kind},100),context);}

for(const pm of [false,true])test(`automatic ${pm?'PM_PORTFOLIO':'standalone no-sealed'} selection resumes its original STOPPED review after host reopen`,{timeout:30000},async()=>{
  const f=await fixture(pm);try{
    const receipt=await f.host.start(`selection-recovery-${pm}`,f.input,false,f.source);f.root=receipt.conversationId;await f.waitForPause();
    const before=await f.host.state(receipt.conversationId);assert.equal(before.research!.final!.status,'STOPPED');assert.equal(before.research!.final!.scientificClaim,'SELECTION_ONLY_NOT_INDEPENDENT_QUALIFICATION');assert.equal(before.campaignResearch!.state,'CLOSING');
    const reviewBefore=await tasks(f.host,'qz.review.independent');assert.equal(reviewBefore.items.length,1);const closureBefore=await tasks(f.host,'qz.research.close');assert.equal(closureBefore.items.length,1);
    assert.equal(before.research!.final!.taskId,closureBefore.items[0]!.id);assert.equal(before.campaignResearch!.closureTaskId,closureBefore.items[0]!.id);
    const conversations=await f.host.harness.commit(tx=>tx.scanConversations({},100),context);const reviewer=conversations.items.find(row=>row.owner?.taskId===reviewBefore.items[0]!.id)!;assert.ok(reviewer);
    const session=await f.host.harness.snapshot(ReviewSession,reviewer.id,context);assert.deepEqual(session!.reads,['context','provenance','factor','selection','stress','portfolio']);
    const originalLedger=await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context);assert.equal(Object.values(originalLedger!.records)[0]!.status,'STARTED');
    const count=f.faux.state.callCount;const nativeBefore=await f.nativeCalls();assert.equal(nativeBefore.length,1);assert.equal(nativeBefore[0]!.command,'review-source');
    await f.reopen();await f.host.harness.waitForIdle(context);
    const after=await f.host.state(receipt.conversationId);const report=latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),f.selected);assert.ok(report,JSON.stringify(after.research));
    assert.equal(report.decision,'INCONCLUSIVE');assert.equal(after.campaignResearch!.state,'CONCLUDED');assert.equal(after.research!.final!.trialId,before.research!.final!.trialId);assert.equal(after.research!.final!.taskId,before.research!.final!.taskId);
    assert.deepEqual((await tasks(f.host,'qz.review.independent')).items.map(row=>row.id),reviewBefore.items.map(row=>row.id));assert.deepEqual((await tasks(f.host,'qz.research.close')).items.map(row=>row.id),closureBefore.items.map(row=>row.id));
    assert.equal(f.faux.state.callCount,count+1,'only the interrupted model response is retried; completed responses are retained');
    const recoveredLedger=await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context);assert.equal(Object.values(recoveredLedger!.records)[0]!.createdAt,Object.values(originalLedger!.records)[0]!.createdAt);assert.deepEqual(await f.nativeCalls(),nativeBefore);assert.equal(f.computed,1);
    assert.deepEqual(after.campaignResearch!.budgetMovements,before.campaignResearch!.budgetMovements);
    assert.equal((await tasks(f.host,'qz.factor.wasm')).items.length,0);assert.equal((await tasks(f.host,'qz.strategy-portfolio.evaluate')).items.length,0,'selection-only does not read sealed market data');
    assert.equal((await f.host.harness.snapshot(ReviewSession,reviewer.id,context))!.finished,true);
    const modelCalls=f.faux.state.callCount;const cache=await f.host.harness.commit(tx=>tx.createTask(CacheProbe,{trialId:f.selected},{conversationId:receipt.conversationId,ownership:{kind:'conversation'}}),context);
    const cached=await f.host.harness.waitForTask(cache,context);assert.equal(cached.state.outcome.status,'completed');if(cached.state.outcome.status==='completed')assert.equal(cached.state.outcome.result.reportId,report.id);
    assert.equal(f.faux.state.callCount,modelCalls);assert.deepEqual(await f.nativeCalls(),nativeBefore);assert.equal((await tasks(f.host,'qz.review.independent')).items.length,1,'a saved report does not create another reviewer');
    await f.reopen();assert.deepEqual(await f.host.start(`selection-recovery-${pm}`,f.input,false,f.source),receipt);assert.equal(f.faux.state.callCount,modelCalls);assert.deepEqual(await f.nativeCalls(),nativeBefore);
  }finally{await f.close();}
});

test('standalone owner stop persists its intent before abort and startup cannot revive the pending reviewer', {timeout:30000},async()=>{
  const f=await fixture();let release!:()=>void;let entered!:()=>void;
  const gate=new Promise<void>(resolve=>{release=resolve;});const boundary=new Promise<void>(resolve=>{entered=resolve;});
  try{
    const receipt=await f.host.start('selection-owner-stop-gap',f.input,false,f.source);f.root=receipt.conversationId;await f.waitForPause();
    const before=await f.host.state(receipt.conversationId);const reviewer=await tasks(f.host,'qz.review.independent');assert.equal(reviewer.items.length,1);
    const calls=f.faux.state.callCount;const native=await f.nativeCalls();
    const original=f.host.harness.conversation.bind(f.host.harness);
    f.host.harness.conversation=async(id,ctx)=>{const handle=await original(id,ctx);if(!handle||id!==receipt.conversationId)return handle;
      const abort=handle.abort.bind(handle);handle.abort=async(inner,options)=>{entered();await awaitWithContext(gate,inner);return abort(inner,options);};return handle;};
    // Use the real owner command, then crash after its marker commits but before the official abort.
    const stopping=f.host.stop(receipt.conversationId).catch(()=>null);await boundary;
    const admitted=await f.host.state(receipt.conversationId);assert.equal(admitted.campaignResearch!.state,'CLOSING');assert.equal(admitted.campaignResearch!.closingReason,'owner_stop');assert.deepEqual(admitted.research!.final,before.research!.final);
    await f.host.close();release();await stopping;await f.reopen();await f.host.harness.waitForIdle(context);
    const after=await f.host.state(receipt.conversationId);assert.equal(after.campaignResearch!.closingReason,'owner_stop');assert.equal(after.campaignResearch!.state,'CONCLUDED');assert.deepEqual(after.research!.final,before.research!.final);
    const task=await f.host.harness.getTask(reviewer.items[0]!.id,context);assert.equal(task!.state.status,'terminal');if(task!.state.status==='terminal')assert.equal(task!.state.outcome.status,'aborted');
    assert.equal(latestTrialReview(await f.host.harness.snapshot(ReviewLedger,receipt.conversationId,context),f.selected),null);assert.equal(f.faux.state.callCount,calls);assert.deepEqual(await f.nativeCalls(),native);assert.equal(f.computed,1);
    assert.deepEqual(after.campaignResearch!.budgetMovements,before.campaignResearch!.budgetMovements);assert.equal((await tasks(f.host,'qz.factor.wasm')).items.length,0);
    await f.reopen();await f.host.harness.waitForIdle(context);assert.equal(f.faux.state.callCount,calls);assert.equal((await tasks(f.host,'qz.review.independent')).items.length,1);assert.deepEqual(await f.nativeCalls(),native);
  }finally{release();await f.close();}
});

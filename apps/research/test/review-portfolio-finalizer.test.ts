import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import strategy from '../plugins/strategy-portfolio.ts';
import research from '../plugins/research.ts';
import { reviewSourceDataDigest } from '../src/review-source.ts';
import type { CapturedReviewSource } from '../src/review-source.ts';
import { ReviewLedger, latestTrialReview } from '../src/review-documents.ts';
import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';
import type { ResearchInput } from '../src/documents.ts';

const context = BACKGROUND_CONTEXT;
const DAY = 86_400_000_000_000n;
const wat = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
// TEST ONLY: native process receipts exercise actual evaluation/review/finalizer wiring,
// not the native statistical estimator, matching engine or a real market/PIT claim.
const protocol = String.raw`
import { appendFileSync, readFileSync } from 'node:fs';
const [log,command]=process.argv.slice(2);const input=JSON.parse(readFileSync(0,'utf8'));const DAY=86400000000000n;
appendFileSync(log,JSON.stringify({command,input})+'\n');let result;
if(command==='review-source')result={schema_version:1,method_version:'qz.review-source.v1',validated:true,metadata:input.metadata};
else if(command==='evaluate-factor') { const counts={observations:'0',included:'0',excluded:'0',indicator_warmup:'0',incomplete_labels:'0',cross_boundary_labels:'0',prediction_outside_partition:'0'}; result={schema_version:1,request:input,partitions:['TRAIN','VALIDATION','HELDOUT'].map(partition=>({partition,metric:{counts,pearson_ic:{value:null,status:'INSUFFICIENT_SAMPLES'}}}))}; }
else if(command==='forecast') { const start=BigInt(input.selection.event_start_ns),end=BigInt(input.selection.event_end_ns);result={points:Array.from({length:Number((end-start)/DAY)},(_,i)=>({instrument_id:'TEST.SIM',ordinal:i,event_ns:String(start+BigInt(i)*DAY),available_ns:String(start+BigInt(i)*DAY),forecast:1}))}; }
else if(command==='blend-targets')result={target_points:input.rows.map(row=>row[0])};
else if(command==='simulate') { const start=BigInt(input.selection.event_start_ns),end=BigInt(input.selection.event_end_ns);result={returns_kind:'PORTFOLIO_DAILY',returns_status:'OK',returns:Array.from({length:Number((end-start)/DAY)},(_,i)=>({timestamp_ns:String(start+BigInt(i)*DAY),value:0.01+(i%3)*0.001,reason_code:null})),statistics:[{native_key:start>=160n*DAY?'HELDOUT_OWNER_ONLY_999':'TEST_ONLY_VALIDATION_ACCOUNT',value:start>=160n*DAY?999:1}],summary:{}}; }
else if(command==='review-science') { const selection=input.analysis.kind==='SELECTION',complete=!selection||input.analysis.ledger_complete;const metric=(code,value)=>({code,value:complete?value:null,status:complete?'AVAILABLE':'INSUFFICIENT_EVIDENCE',reason:complete?null:'failed_cohort_reference_returns_unavailable'});result={schema_version:1,source_digest:input.source_digest,method_version:'qz.review-science.v1',observations:input.analysis.timestamps_ns.length,scope:selection?'SELECTION_COHORT':'SHARED_ACCOUNT',limitations:['TEST_ONLY_PROTOCOL_NOT_MARKET_EVIDENCE'],metrics:selection?[metric('dsr',0.99),metric('pbo',0.1)]:[metric('candidate_net_return_lower',0.05),metric('candidate_net_return_mean',0.01),metric('return_coverage',1),metric('positive_slice_fraction',1),metric('net_return_delta_lower',0.01)]}; }
else throw new Error('TEST_ONLY_UNSUPPORTED:'+command);
console.log(JSON.stringify(result));
`;
function capture(partition:'VALIDATION'|'HELDOUT',data:JsonObject):CapturedReviewSource {
  return {schemaVersion:1,partition,datasetId:'TEST_ONLY_REGISTERED_BARS',sourceId:'TEST_ONLY_OWNER',sourceRevision:`TEST_ONLY_${partition}`,snapshotDigest:`TEST_ONLY_BYTES_${partition}`,inputDataDigest:reviewSourceDataDigest(data),metadataDigest:`TEST_ONLY_METADATA_${partition}`,licenseDigest:`TEST_ONLY_GRANT_${partition}`,
    metadata:{partition:partition==='HELDOUT'?'SEALED':'VALIDATION',origin:'REAL',pit_status:'VERIFIED',revision_policy:'AS_KNOWN_THEN',provenance_reference:'TEST_ONLY_SOURCE_OWNER',availability_provenance:'TEST_ONLY_RECEIVER_CLOCK_ASSERTION'},
    license:{schemaVersion:1,datasetId:'TEST_ONLY_REGISTERED_BARS',sourceId:'TEST_ONLY_OWNER',sourceRevision:`TEST_ONLY_${partition}`,datasetRevisionId:`TEST_ONLY_NATIVE_${partition}`,grantId:`TEST_ONLY_GRANT_${partition}`,issuer:'TEST_ONLY_ISSUER',reference:'TEST_ONLY_REFERENCE',status:'ACTIVE',purposes:['RESEARCH'],validFromNs:'0',validUntilNs:'9223372036854775807'},
    receivedAt:'2026-11-02T00:00:00Z',validationStartNs:partition==='VALIDATION'?String(10n*DAY):String(160n*DAY),validationEndNs:partition==='VALIDATION'?String(160n*DAY):String(200n*DAY)};
}
function last(transcript:{messages:readonly unknown[]},name:string):JsonObject {
  const message=transcript.messages.filter(value=>(value as {toolName?:string}).toolName===name).at(-1) as {content:{text?:string}[]};
  return JSON.parse(message.content.map(item=>item.text??'').join('')) as JsonObject;
}
const call=(name:string,args:JsonObject)=>fauxAssistantMessage(fauxToolCall(name,args),{stopReason:'toolUse'});
for(const mode of ['COMPLETE','FAILED_COHORT','MISSING_INDEPENDENT_SOURCE'] as const) test(`portfolio finalizer ${mode} executes the real review gate before any heldout account`,{timeout:60000},async()=>{
  const directory=await mkdtemp(join(tmpdir(),'qz-portfolio-finalizer-'));const log=join(directory,'native-calls.jsonl');const binary=join(directory,'job');const script=join(directory,'protocol.mjs');
  const before=process.env.QZ_JOB_BINARY;process.env.QZ_JOB_BINARY=binary;
  const faux=fauxProvider({tokensPerSecond:100000});const models=createModels();models.setProvider(faux.provider);const model={provider:faux.provider.id,modelId:faux.getModel().id};
  await writeFile(script,protocol);await writeFile(binary,`#!/bin/sh\nexec '${process.execPath}' '${script}' '${log}' "$@"\n`,{mode:0o700});
  const catalog=join(directory,'validation');const heldout=join(directory,'sealed');await mkdir(catalog);await mkdir(heldout);
  const request={schema_version:1,train_end_ns:String(10n*DAY),validation_end_ns:String(160n*DAY),forecast:{schema_version:1,selection:{schema_version:1,bar_types:['TEST.SIM-1-DAY-LAST-EXTERNAL'],event_start_ns:'0',event_end_ns:String(200n*DAY),decision_cutoff_ns:String(200n*DAY),maximum_rows:1000},parameters:{schema_version:1,fast_period:2,slow_period:3,label_horizon_observations:1}}};
  const strategyPolicy={name:'TEST_ONLY_REFERENCE',kind:'factor-threshold',instrumentId:'TEST.SIM',threshold:0,investedWeight:'1',targetTtlNs:String(DAY),simulationSettings:{base_currency:'USD',starting_capital:'1000',fill_model:{adapter_kind:'NAUTILUS_DEFAULT_FILL',parameters:{prob_slippage:'0'}},latency_model:{adapter_kind:'NAUTILUS_STATIC_LATENCY',parameters:{base_latency_ns:'1'}}}};
  const review=createReviewPolicy('PORTFOLIO','TEST_ONLY_FROZEN_REVIEW','100');review.calibration={rootCount:30,endNs:'90',evidenceRefs:['TEST_ONLY_FROZEN_REFERENCE_COHORT']};
  const data:JsonObject={catalog,request,sealed:{catalog:heldout,request}};
  const input:ResearchInput={goal:'TEST_ONLY full portfolio review to heldout protocol',model,data,policy:{strategy:strategyPolicy,reviewPolicy:review as unknown as JsonObject}};
  const source=capture('VALIDATION',data);if(mode!=='MISSING_INDEPENDENT_SOURCE')source.independent=capture('HELDOUT',data.sealed as JsonObject);
  const host=await openResearch({database:join(directory,'research.sqlite'),models,extensions:[research,strategy]});let selected=0;let conversationId=0;
  try {
    const membersA=[{model:mode==='FAILED_COHORT'?'(module malformed syntax)':wat,weight:'0.5'},{model:wat.replace('f64.const 1','f64.const 2'),weight:'0.5'}];
    const membersB=[{model:wat,weight:'0.7'},{model:wat.replace('f64.const 1','f64.const 2'),weight:'0.3'}];
    faux.setResponses([call('strategy_portfolio_evaluate',{hypothesis:'TEST_ONLY original A',members:membersA}),call('strategy_portfolio_evaluate',{hypothesis:'TEST_ONLY original B',members:membersB}),
      transcript=>{selected=last(transcript,'strategy_portfolio_evaluate').trialId as number;return call('strategy_portfolio_finalize',{trialId:selected,reason:'TEST_ONLY predeclared selection'});},
      ...['context','provenance','factor','selection','stress','portfolio'].map(kind=>call(`review_${kind}`,{})),
      ()=>call('review_finish',{narrative:'TEST_ONLY complete native protocol review, no market claim.',findings:[],mechanism:['mechanism','availability','direction','implementation','complexity'].map(criterion=>({criterion,supported:true,evidenceRefs:[`research:${conversationId}:trial:${selected}:validation`]}))})]);
    const receipt=await host.start(`portfolio-finalizer-${mode}`,input,false,source);conversationId=receipt.conversationId;
    await (await host.harness.submission(receipt.submissionId,context))!.wait(context);await host.harness.waitForIdle(context);
    const state=await host.state(receipt.conversationId);const ledger=await host.harness.snapshot(ReviewLedger,receipt.conversationId,context);const report=latestTrialReview(ledger,selected) as ReviewReport|null;
    assert.ok(report,JSON.stringify(state.research));
    const calls=(await readFile(log,'utf8')).trim().split('\n').map(row=>JSON.parse(row) as {command:string;input:JsonObject});
    const independent=calls.filter(row=>row.command==='simulate'&&BigInt((row.input.selection as JsonObject).event_start_ns as string)>=160n*DAY);
    if(mode==='COMPLETE') {
      assert.equal(report.decision,'PASS',JSON.stringify(report.reasonCodes));assert.equal(state.research!.final!.status,'COMPUTED',JSON.stringify(state.research!.final));assert.equal(independent.length,1);
      assert.equal(report.metrics.find(metric=>metric.key==='account.net_return_lower')?.scope,'SHARED_ACCOUNT');
      assert.equal(report.metrics.find(metric=>metric.key==='robustness.positive_fraction')?.scope,'SHARED_ACCOUNT');
      assert.ok(report.metrics.some(metric=>metric.key==='execution.stressed_net_return'&&metric.status==='AVAILABLE'));
      assert.ok(report.metrics.some(metric=>metric.key==='incremental.net_return_delta_lower'&&metric.status==='AVAILABLE'));
      const replays=calls.filter(row=>row.command==='blend-targets');assert.ok(replays.some(row=>JSON.stringify(row.input.weights)===JSON.stringify(['0.7','0.3'])));assert.ok(replays.some(row=>JSON.stringify(row.input.weights)===JSON.stringify(['0.5','0.5'])));assert.deepEqual(replays.at(-1)!.input.weights,['0.7','0.3'],'the actual heldout finalizer preserves the selected nonuniform ensemble');
      assert.equal(JSON.stringify((await host.view(receipt.conversationId)).entries).includes('HELDOUT_OWNER_ONLY_999'),false,'no final statistic is returned to either model context');
    } else {
      assert.equal(report.decision,'INCONCLUSIVE',JSON.stringify(report.reasonCodes));assert.equal(independent.length,0,'failed/missing proof must not grant a heldout read');assert.equal(state.research!.final!.status,'STOPPED');
      if(mode==='FAILED_COHORT') {
        assert.ok(report.missingEvidence.some(reason=>reason.startsWith('selection.dsr:')),'failed original candidate remains in the selection denominator');
        const selectedAnalysis=calls.find(row=>row.command==='review-science'&&(row.input.analysis as JsonObject).kind==='SELECTION');
        assert.ok(selectedAnalysis);assert.equal((selectedAnalysis.input.analysis as JsonObject).total_trials,2);assert.equal((selectedAnalysis.input.analysis as JsonObject).ledger_complete,false);
      } else assert.ok(report.reasonCodes.some(reason=>reason.startsWith('independent_window:')),'missing independent source has its own explicit gate');
    }
    const count=calls.length;assert.deepEqual(await host.start(`portfolio-finalizer-${mode}`,input,false,source),receipt);assert.equal((await readFile(log,'utf8')).trim().split('\n').length,count);
  } finally {await host.close();if(before===undefined)delete process.env.QZ_JOB_BINARY;else process.env.QZ_JOB_BINARY=before;await rm(directory,{recursive:true,force:true});}
});

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { researchRuntime } from '../src/research-runtime.ts';
import { factorAccountEvidence, factorCohortSelectionEvidence, factorReviewCohort, ReviewFactorAccounts } from '../src/review-factor-account.ts';
import type { ResearchInput } from '../src/documents.ts';
import { ReviewNativeTask } from '../src/review-native-task.ts';
import type { ReviewNativePlan } from '../src/review-native-task.ts';
import { originalValidationAccount } from '../src/review-plans.ts';
import type { ThresholdPolicy } from '../src/strategy.ts';

const context = BACKGROUND_CONTEXT;
const DAY = 86_400_000_000_000n;
const model = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
const request = { schema_version: 1, train_end_ns: String(10n*DAY), validation_end_ns: String(160n*DAY), forecast: { schema_version: 1,
  selection: { schema_version: 1, bar_types: ['TEST.SIM-1-DAY-LAST-EXTERNAL'], event_start_ns: '0', event_end_ns: String(200n*DAY), decision_cutoff_ns: String(200n*DAY), maximum_rows: 1000 },
  parameters: { schema_version: 1, fast_period: 2, slow_period: 3, label_horizon_observations: 1 } } };
const policy: ThresholdPolicy = { name: 'TEST_ONLY_REFERENCE', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '1', targetTtlNs: String(2n*DAY),
  simulationSettings: { base_currency: 'USD', starting_capital: '1000', fill_model: { adapter_kind: 'NAUTILUS_DEFAULT_FILL', parameters: { prob_slippage: '0' } },
    latency_model: { adapter_kind: 'NAUTILUS_STATIC_LATENCY', parameters: { base_latency_ns: '1' } } } };
const trial = (id: number, status = 'COMPUTED', changed: JsonObject = {}): JsonObject => ({ taskId: id, status,
  proposal: { model: model.replace('f64.const 1', `f64.const ${id}`), request, hypothesis: `TEST_ONLY_${id}`, ...changed },
  result: status === 'COMPUTED' ? { request, partitions: [] } : null });

// Controlled process receipts validate actual Durable/native transport and evidence provenance,
// never the numerical DSR estimator, market profitability or real historical PIT.
const protocol = String.raw`
import { appendFileSync, existsSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs';
import { setTimeout } from 'node:timers/promises';
const [log, command] = process.argv.slice(2); const input = JSON.parse(readFileSync(0,'utf8')); const DAY=86400000000000n;
appendFileSync(log,JSON.stringify({command,input})+'\n'); let result;
if(command==='evaluate-factor')result={schema_version:1,request:input,partitions:[]};
else if(command==='forecast') { const start=BigInt(input.selection.event_start_ns),end=BigInt(input.selection.event_end_ns); result={points:Array.from({length:Number((end-start)/DAY)},(_,i)=>({instrument_id:'TEST.SIM',ordinal:i,event_ns:String(start+BigInt(i)*DAY),available_ns:String(start+BigInt(i)*DAY),forecast:1}))}; }
else if(command==='blend-targets')result={target_points:input.rows.map(row=>row[0])};
else if(command==='simulate') { if(existsSync(log+'.pause')){unlinkSync(log+'.pause');writeFileSync(log+'.entered','reserved missing receipt');while(!existsSync(log+'.release'))await setTimeout(10);} const start=BigInt(input.selection.event_start_ns),end=BigInt(input.selection.event_end_ns);result={returns_kind:'PORTFOLIO_DAILY',returns_status:'OK',returns:Array.from({length:Number((end-start)/DAY)},(_,i)=>({timestamp_ns:String(start+BigInt(i)*DAY),value:0.01+(i%3)*0.001,reason_code:null})),statistics:[]}; }
else if(command==='review-science') { const selection=input.analysis.kind==='SELECTION'; const complete=!selection||input.analysis.ledger_complete; const metric=(code,value)=>({code,value:complete?value:null,status:complete?'AVAILABLE':'INSUFFICIENT_EVIDENCE',reason:complete?null:'failed_trial_return_missing'});result={schema_version:1,source_digest:input.source_digest,method_version:'qz.review-science.v1',observations:input.analysis.timestamps_ns.length,scope:selection?'SELECTION_COHORT':'SHARED_ACCOUNT',limitations:['TEST_ONLY_WIRE_NOT_SCIENTIFIC_EVIDENCE'],metrics:selection?[metric('dsr',0.99),metric('pbo',0.1)]:[metric('candidate_net_return_lower',0.05),metric('candidate_net_return_mean',0.01),metric('return_coverage',1),metric('positive_slice_fraction',1),metric('net_return_delta_lower',0.01)]}; }
else throw new Error('TEST_ONLY_UNSUPPORTED:'+command);
console.log(JSON.stringify(result));
`;

interface ProbeInput { input: ResearchInput; selected: JsonObject; trials: JsonObject[]; mode: 'ACCOUNT' | 'COHORT' | 'FULL_COHORT' }
const Probe = defineTask<ProbeInput, { phase: 'probe' }, JsonObject>({
  name: 'test.review-factor-cohort', version: 1, initial: () => ({ phase: 'probe' }),
  async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  phases: { async probe(task,runtime,ctx) {
    const api = researchRuntime(runtime);
    if(task.input.mode==='ACCOUNT') {
      const first=await factorAccountEvidence(api,ctx,task.input.input.data,model,policy,'TEST_ONLY:factor:first');
      const second=await factorAccountEvidence(api,ctx,task.input.input.data,model,policy,'TEST_ONLY:factor:first');
      await runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result:{first,second}}}),ctx);
    } else {
      const result=await factorCohortSelectionEvidence(api,ctx,task.input.input,task.input.selected,task.input.trials,'TEST_ONLY:factor:cohort');
      let native: JsonObject | null = null;
      if(task.input.mode==='FULL_COHORT' && result.plans[0]) {
        let child!:TaskId<JsonObject>;
        await runtime.commit(async tx=>{child=await tx.createTask(ReviewNativeTask,result.plans[0] as ReviewNativePlan,{ownership:{kind:'task',taskId:task.id}});return undefined;},ctx);
        const settled=await runtime.waitForTask(child,ctx); if(settled.state.outcome.status==='completed')native=settled.state.outcome.result;
      }
      await runtime.commit(()=>({status:'terminal',outcome:{status:'completed',result:JSON.parse(JSON.stringify({...result,native})) as JsonObject}}),ctx);
    }
  } },
});
async function fixture() {
  const directory=await mkdtemp(join(tmpdir(),'qz-factor-review-cohort-')); const log=join(directory,'calls.jsonl'); const binary=join(directory,'job');
  const script=join(directory,'protocol.mjs');await writeFile(script,protocol);await writeFile(binary,`#!/bin/sh\nexec '${process.execPath}' '${script}' '${log}' "$@"\n`,{mode:0o700});
  const before=process.env.QZ_JOB_BINARY;process.env.QZ_JOB_BINARY=binary;
  const faux=fauxProvider({tokensPerSecond:100000});const models=createModels();models.setProvider(faux.provider);
  const input:ResearchInput={goal:'TEST_ONLY immutable factor cohort',model:{provider:faux.provider.id,modelId:faux.getModel().id},data:{catalog:directory,request},policy:{strategy:policy as unknown as JsonObject}};
  const options={database:join(directory,'research.sqlite'),models,extensions:[defineExtension({name:'test.factor-cohort',tasks:[Probe]})]};
  const calls=async()=>{const data=await readFile(log,'utf8').catch(()=>'');return data.trim()?data.trim().split('\n').map(row=>JSON.parse(row) as {command:string;input:JsonObject}):[];};
  return {directory,log,input,options,calls,async close(){if(before===undefined)delete process.env.QZ_JOB_BINARY;else process.env.QZ_JOB_BINARY=before;await rm(directory,{recursive:true,force:true});}};
}
async function admit(host:Awaited<ReturnType<typeof openResearch>>, input:ProbeInput) {
  return host.harness.commit(async tx=>{const conversation=await tx.createConversation({ownership:{kind:'ownerless'}});return {conversationId:conversation.id,taskId:await tx.createTask(Probe,input,{conversationId:conversation.id,ownership:{kind:'conversation'}})};},context);
}

test('factor cohort retains failed attempts and excludes different registered scope, ensembles and budget rejections',()=>{
  const selected=trial(1);const failed=trial(2,'FAILED');const rejected=trial(3,'REJECTED');const ensemble=trial(4,'COMPUTED',{members:[{model,weight:'1'}]});
  const other=trial(5,'COMPUTED',{request:{...request,validation_end_ns:'99'}});
  assert.deepEqual(factorReviewCohort(selected,[selected,failed,rejected,ensemble,other]).map(row=>row.taskId),[1,2]);
});

test('factor labels are not accepted as original strategy net returns',async()=>{
  const api={conversationId:1,getTask:()=>{throw new Error('must not read labels as a strategy task');}};
  const original=trial(1);original.result={...original.result as JsonObject,points:[{label_return:99,realized_return:99}],returns:[99]};
  assert.equal(await originalValidationAccount(api as never,context,original),null);
});

test('computed factor accounts persist after native task settlement and reopen without scientific calls', {timeout:30000},async()=>{
  const f=await fixture();let host=await openResearch(f.options);
  try {
    const receipt=await admit(host,{input:f.input,selected:trial(1),trials:[trial(1)],mode:'ACCOUNT'});const settled=await host.harness.waitForTask(receipt.taskId as TaskId<JsonObject>,context);
    assert.equal(settled.state.outcome.status,'completed',JSON.stringify(settled.state));if(settled.state.outcome.status!=='completed')return;
    assert.deepEqual(settled.state.outcome.result.first,settled.state.outcome.result.second);
    const nativeCalls=await f.calls();assert.equal(nativeCalls.length,8,'one fixed 8-call account review, including actual cash/stress diagnostics');
    const before=await host.harness.snapshot(ReviewFactorAccounts,receipt.conversationId,context);assert.ok(before&&Object.values(before.records).some(row=>row.status==='COMPUTED'));
    await host.close();host=await openResearch(f.options);assert.deepEqual(await host.harness.snapshot(ReviewFactorAccounts,receipt.conversationId,context),before);
    assert.deepEqual((await host.harness.waitForTask(receipt.taskId as TaskId<JsonObject>,context)).state.outcome,settled.state.outcome);assert.equal((await f.calls()).length,nativeCalls.length);
  } finally {await host.close();await f.close();}
});

test('complete factor cohort produces aligned actual account return series while failed attempts keep the denominator', {timeout:30000},async()=>{
  const f=await fixture();const host=await openResearch(f.options);
  try {
    const selected=trial(1),failed=trial(2,'FAILED');
    const receipt=await admit(host,{input:f.input,selected,trials:[selected,failed],mode:'COHORT'});const settled=await host.harness.waitForTask(receipt.taskId as TaskId<JsonObject>,context);
    assert.equal(settled.state.outcome.status,'completed',JSON.stringify(settled.state));if(settled.state.outcome.status!=='completed')return;
    const plans=settled.state.outcome.result.plans as JsonObject[];assert.equal(plans.length,1);
    const analysis=plans[0]!.analysis as JsonObject;assert.equal(analysis.total_trials,2);assert.equal(analysis.ledger_complete,false);
    const rows=analysis.trials as JsonObject[];assert.equal(rows.length,1);assert.equal(rows[0]!.id,'1');assert.notDeepEqual(rows[0]!.returns,[99]);
    assert.equal((rows[0]!.returns as number[]).length,150);assert.ok((rows[0]!.returns as number[]).every(value=>value>=0.01&&value<0.02));
    const calls=await f.calls();assert.equal(calls.filter(call=>call.command==='simulate').length,3,'selected actual candidate, cash and adverse replay only; failed source remains failed');
  } finally {await host.close();await f.close();}
});

test('an interrupted reserved native call is not a free rerun after the owned task resumes', {timeout:30000},async()=>{
  const f=await fixture();let host=await openResearch(f.options);
  try {
    await writeFile(f.log+'.pause','pause first account simulation');
    const receipt=await admit(host,{input:f.input,selected:trial(1),trials:[trial(1)],mode:'ACCOUNT'});
    let entered=false;for(let n=0;n<300;n++){try{await readFile(f.log+'.entered');entered=true;break;}catch{await delay(10);}}
    assert.equal(entered,true,'original native operation reached its reserved boundary');const count=(await f.calls()).length;
    await host.close();await writeFile(f.log+'.release','resume cannot authorize another invocation');host=await openResearch(f.options);
    const settled=await host.harness.waitForTask(receipt.taskId as TaskId<JsonObject>,context);assert.equal(settled.state.outcome.status,'completed',JSON.stringify(settled.state));
    assert.equal((await f.calls()).length,count,'a reservation with no original outcome requires reconciliation, never a new native call');
    const ledger=await host.harness.snapshot(ReviewFactorAccounts,receipt.conversationId,context);assert.ok(ledger&&Object.values(ledger.records).some(row=>row.status==='FAILED'));
  } finally {await host.close();await f.close();}
});

test('two original computed factors reach native DSR analysis and retain their account cohort for a later parent', {timeout:30000},async()=>{
  const f=await fixture();let host=await openResearch(f.options);
  try {
    const selected=trial(1), second=trial(2);
    // Large original labels must never become the strategy returns matrix.
    (selected.result as JsonObject).points=[{label_return:99}];(second.result as JsonObject).points=[{label_return:-99}];
    const input:ProbeInput={input:f.input,selected,trials:[selected,second],mode:'FULL_COHORT'};
    const receipt=await admit(host,input);const settled=await host.harness.waitForTask(receipt.taskId as TaskId<JsonObject>,context);
    assert.equal(settled.state.outcome.status,'completed',JSON.stringify(settled.state));if(settled.state.outcome.status!=='completed')return;
    const output=settled.state.outcome.result;const analysis=(output.plans as JsonObject[])[0]!.analysis as JsonObject;
    assert.equal(analysis.total_trials,2);assert.equal(analysis.ledger_complete,true);assert.equal((analysis.trials as JsonObject[]).length,2);
    for(const row of analysis.trials as JsonObject[])assert.ok((row.returns as number[]).every(value=>value>=0.01&&value<0.02),'only original native net daily account returns');
    const metrics=(output.native as JsonObject).metrics as JsonObject[];const dsr=metrics.find(metric=>metric.key==='selection.dsr');
    assert.equal(dsr?.status,'AVAILABLE');assert.equal(dsr?.scope,'SELECTION_COHORT');
    const originalCalls=await f.calls();assert.equal(originalCalls.filter(call=>call.command==='simulate').length,6,'each factor has candidate, cash and stress accounts');
    assert.equal(originalCalls.length,17,'two bounded eight-call accounts plus one selected-cohort analysis');
    assert.ok(originalCalls.filter(call=>call.command==='evaluate-factor').every(call=>((call.input.forecast as JsonObject).selection as JsonObject).event_end_ns===request.validation_end_ns),'never read sealed suffix during reference replay');
    const source=await host.harness.snapshot(ReviewFactorAccounts,receipt.conversationId,context);assert.ok(source&&Object.values(source.records).filter(row=>row.status==='COMPUTED').length===2);
    await host.close();host=await openResearch(f.options);
    // A different owning task in the same study reads original settled account evidence.
    const repeated=await host.harness.commit(tx=>tx.createTask(Probe,{...input,mode:'COHORT' as const},{conversationId:receipt.conversationId,ownership:{kind:'conversation'}}),context);
    const restored=await host.harness.waitForTask(repeated,context);assert.equal(restored.state.outcome.status,'completed',JSON.stringify(restored.state));
    assert.equal((await f.calls()).length,originalCalls.length,'later consumers do not rerun an existing source account or its budget');
    assert.deepEqual(await host.harness.snapshot(ReviewFactorAccounts,receipt.conversationId,context),source);
  }finally{await host.close();await f.close();}
});

test('a 33-factor cohort computes every original member without the former quota', {timeout:60000},async()=>{
  const f=await fixture();const host=await openResearch(f.options);
  try {
    const trials=Array.from({length:33},(_,index)=>trial(index+1));const selected=trials[0]!;
    const receipt=await admit(host,{input:f.input,selected,trials,mode:'COHORT'});const settled=await host.harness.waitForTask(receipt.taskId as TaskId<JsonObject>,context);
    assert.equal(settled.state.outcome.status,'completed',JSON.stringify(settled.state));if(settled.state.outcome.status!=='completed')return;
    const result=settled.state.outcome.result;const cohort=result.cohort as JsonObject;
    assert.equal((result.plans as JsonObject[]).length,1);assert.ok(result.selectedAccount);assert.equal(cohort.totalTrials,33);
    assert.equal(cohort.ledgerComplete,true);assert.equal(cohort.comparableReturnSeries,33);
    assert.equal((await f.calls()).length,264,'all 33 reference accounts run their full eight stages');
    const facts=await host.harness.snapshot(ReviewFactorAccounts,receipt.conversationId,context);assert.equal(Object.keys(facts!.records).length,33);
  }finally{await host.close();await f.close();}
});

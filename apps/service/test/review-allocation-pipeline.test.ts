import { createServer } from 'node:http';
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { canonical } from '../../research/src/documents.ts';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import { setup, context, fauxAssistantMessage, fauxToolCall, wat } from './support.ts';
import { prepareReviewSourceEvidence } from '../src/review-source.ts';
import { ReviewSourceEvidence } from '../../research/src/review-source.ts';
import { Application, json } from '../src/facts.ts';
import type { PortfolioFact } from '../src/facts.ts';
import { ReviewPolicies } from '../src/review-policy.ts';
import { ReviewAllocations, portfolioReviewVersion } from '../src/review-allocation.ts';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import type { ReviewPolicy, ReviewMetric } from '../../../packages/contracts/src/review.ts';
import type { TaskId } from '@earendil-works/pi-durable';
const DAY = 86400000000000n;
function calibrated(kind: 'PORTFOLIO' | 'ALLOCATION', id: string): ReviewPolicy {
  const policy = createReviewPolicy(kind, id, '100');
  policy.calibration = { rootCount: 30, endNs: '90', evidenceRefs: ['fixture:reference-cohort'] }; return policy;
}
// Controlled receipts establish the real Durable/HTTP/tool/data flow only. Native scientific
// estimators and Clarabel are separately checked by the Rust tests, not by this executable.
for (const mode of ['COMPLETE', 'MISSING_INDEPENDENT_PROOF', 'FAILED_INDEPENDENT', 'REMOTE_RESERVATION_GAP'] as const) test(`allocation ${mode}: reserved native recovery, evidence gate and original reviewer identity`, { timeout: 60_000 }, async () => {
  let afterRestart = false;
  const state = await setup(); let remote: ReturnType<typeof createServer> | undefined; const remotePosts: any[] = []; const log = join(state.directory,'allocation-protocol.log');
  try {
    const binary = join(state.directory,'allocation-job'); const fixture = new URL('./review-allocation-job.mjs',import.meta.url).pathname;
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${fixture}' '${log}' "$@"\n`, {mode:0o700});
    state.options.config.jobBinary = binary; state.options.config.defaultModel = state.model;
    const request = { train_end_ns: String(150n*DAY), validation_end_ns: String(300n*DAY), forecast: { schema_version:1,
      selection:{schema_version:1,bar_types:['TEST.SIM-1-DAY-LAST-EXTERNAL'],event_start_ns:'0',event_end_ns:String(350n*DAY),decision_cutoff_ns:String(350n*DAY),maximum_rows:1000},
      parameters:{schema_version:1,fast_period:2,slow_period:3,label_horizon_observations:1,total_fuel:'1'} } };
    const settings = {base_currency:'USD',starting_capital:'1000',fill_model:{adapter_kind:'NAUTILUS_DEFAULT_FILL',parameters:{prob_slippage:'0'}},latency_model:{adapter_kind:'NAUTILUS_STATIC_LATENCY',parameters:{base_latency_ns:'0'}}};
    const strategy = {name:'TEST_ONLY',kind:'factor-threshold',instrumentId:'TEST.SIM',threshold:0,investedWeight:'1',targetTtlNs:String(2n*DAY),simulationSettings:settings};
    const dataset = state.options.config.datasets[0]!;
    dataset.data.request = request; dataset.strategy = strategy as any;
    const independentParameters = request.forecast.parameters;
    state.options.config.datasets.push({...dataset,id:'independent-test-only',data:{...dataset.data,request:{...request,validation_end_ns:String(400n*DAY),forecast:{...request.forecast,parameters:independentParameters,selection:{...request.forecast.selection,event_end_ns:String(600n*DAY),decision_cutoff_ns:String(600n*DAY)}}}}});
    // A configured source-owner snapshot fixture exercises the authority boundary only;
    // its TEST_ONLY native validator is not evidence of actual historical market PIT.
    const independentDataset = state.options.config.datasets.at(-1)!;
    const independentRequest = independentDataset.data.request as any;
    const metadata = { registered_ref: independentDataset.id, native_snapshot_ref: 'TEST_ONLY_REVISION', partition: 'SEALED', origin: 'REAL', pit_status: 'VERIFIED', revision_policy: 'AS_KNOWN_THEN', provenance_reference: 'TEST_ONLY_SOURCE_OWNER', availability_provenance: 'TEST_ONLY_SOURCE_CLOCK_CLAIM', quality: { datasets: [{ dataset_revision_id: 'TEST_ONLY_NATIVE_DATASET', instrument_ids: ['TEST.SIM'], selection: { ...independentRequest.forecast.selection, event_start_ns: independentRequest.validation_end_ns } }] } };
    const license = { schemaVersion:1,datasetId:independentDataset.id,sourceId:independentDataset.source,sourceRevision:'TEST_ONLY_REVISION',datasetRevisionId:'TEST_ONLY_NATIVE_DATASET',grantId:'TEST_ONLY_GRANT',issuer:'TEST_ONLY_OWNER',reference:'TEST_ONLY_PROOF',status:'ACTIVE',purposes:['RESEARCH'],validFromNs:'0',validUntilNs:'9223372036854775807' };
    const digest=(text:string)=>createHash('sha256').update(text).digest('hex');const files:any[]=[];
    for(const [path,value]of Object.entries({'catalog/bars.parquet':'TEST_ONLY_BYTES_NOT_MARKET_DATA','heldout-metadata.json':JSON.stringify(metadata),'license.json':JSON.stringify(license)})) { await writeFile(join(state.directory,path),value);files.push({path,sha256:digest(value),bytes:String(Buffer.byteLength(value))}); }
    const manifest={schemaVersion:1,datasetId:independentDataset.id,sourceRevision:'TEST_ONLY_REVISION',availableAtNs:'0',revisionKind:'APPEND',data:{...independentDataset.data,catalog:'catalog'},files};
    await writeFile(join(state.directory,'revision.json'),JSON.stringify({...manifest,contentDigest:digest(canonical(json(manifest)))}));
    independentDataset.revisionManifest=join(state.directory,'revision.json');independentDataset.reviewSourceEvidence={metadata:{path:'heldout-metadata.json',sha256:digest(JSON.stringify(metadata))},heldoutMetadata:{path:'heldout-metadata.json',sha256:digest(JSON.stringify(metadata))},license:{path:'license.json',sha256:digest(JSON.stringify(license))}};
    const sourceMetadata = { ...metadata, registered_ref: dataset.id, native_snapshot_ref: 'TEST_ONLY_VALIDATION', partition: 'VALIDATION', quality: { datasets: [{ dataset_revision_id: 'TEST_ONLY_SOURCE_DATASET', instrument_ids: ['TEST.SIM'], selection: { ...request.forecast.selection, event_start_ns: request.train_end_ns, event_end_ns: request.validation_end_ns } }] } };
    const sourceLicense = { ...license, datasetId: dataset.id, sourceRevision: 'TEST_ONLY_VALIDATION', datasetRevisionId: 'TEST_ONLY_SOURCE_DATASET' };
    const sourceFiles = [files[0]];
    for (const [path,value] of Object.entries({'validation-metadata.json':JSON.stringify(sourceMetadata),'validation-license.json':JSON.stringify(sourceLicense)})) { await writeFile(join(state.directory,path),value);sourceFiles.push({path,sha256:digest(value),bytes:String(Buffer.byteLength(value))}); }
    const sourceManifest={schemaVersion:1,datasetId:dataset.id,sourceRevision:'TEST_ONLY_VALIDATION',availableAtNs:'0',revisionKind:'APPEND',data:{...dataset.data,catalog:'catalog'},files:sourceFiles};
    await writeFile(join(state.directory,'validation-revision.json'),JSON.stringify({...sourceManifest,contentDigest:digest(canonical(json(sourceManifest)))}));
    dataset.revisionManifest=join(state.directory,'validation-revision.json');dataset.reviewSourceEvidence={metadata:{path:'validation-metadata.json',sha256:digest(JSON.stringify(sourceMetadata))},license:{path:'validation-license.json',sha256:digest(JSON.stringify(sourceLicense))}};
    const captured = await prepareReviewSourceEvidence(dataset); assert.ok(captured);
    if(mode==='REMOTE_RESERVATION_GAP') {
      remote=createServer(async(request,response)=>{
        const send=(body:unknown)=>{response.writeHead(200,{'content-type':'application/json'});response.end(JSON.stringify(body));};
        if(request.url==='/v1/capabilities')return send({schemaVersion:1,implementationDigest:`sha256:${'a'.repeat(64)}`,commands:['forecast','simulate']});
        if(request.method==='GET'&&request.url?.startsWith('/v1/executions/'))return send({schemaVersion:1,requestId:request.url.split('/').at(-1),status:afterRestart?'UNKNOWN':'RUNNING',...(afterRestart?{error:'controlled_original_receipt_unavailable'}:{})});
        if(request.method==='DELETE')return send({schemaVersion:1,requestId:request.url!.split('/').at(-1),status:'CANCELLED'});
        if(request.method==='POST'&&request.url==='/v1/execute'){
          let body='';for await(const part of request)body+=String(part);const execution=JSON.parse(body);remotePosts.push(execution);
          if(execution.command==='forecast'){
            const selection=execution.input.selection;const start=BigInt(selection.event_start_ns),end=BigInt(selection.event_end_ns);
            return send({schemaVersion:1,requestId:execution.requestId,status:'COMPLETED',result:{points:Array.from({length:Number((end-start)/DAY)},(_,i)=>({instrument_id:'TEST.SIM',ordinal:i,event_ns:String(start+BigInt(i)*DAY),available_ns:String(start+BigInt(i)*DAY),forecast:1}))}});
          }
          if(execution.command==='simulate'){await writeFile(log+'.entered','remote POST admitted; original receipt retained');return send({schemaVersion:1,requestId:execution.requestId,status:'RUNNING'});}
        }
        response.writeHead(404);response.end();
      });
      await new Promise<void>(resolve=>remote!.listen(0,'127.0.0.1',resolve));const address=remote.address() as import('node:net').AddressInfo;
      state.options.config.nautilus={backtest:{id:'TEST_ONLY_REMOTE',url:`http://127.0.0.1:${address.port}`}};
    }
    if(mode==='MISSING_INDEPENDENT_PROOF')delete independentDataset.reviewSourceEvidence;
    if(mode==='FAILED_INDEPENDENT')await writeFile(log+'.negative','controlled negative independent receipt');
    await state.restart();
    await state.service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ownership:{kind:'ownerless'}});
      const application = await tx.doc(Application);
      (await tx.doc(ReviewPolicies)).records['TEST_ONLY_STORED_CALIBRATION_RECEIPT']={policy:json(calibrated('ALLOCATION','allocation-policy'))};
      const sourceDoc=await tx.doc(ReviewSourceEvidence,conversation.id);sourceDoc.captured=json(captured);sourceDoc.native={schema_version:1,method_version:'qz.review-source.v1',validated:true,metadata:json(sourceMetadata)};
      for (const id of ['portfolio-a','portfolio-b']) {
        const fact = {input:{requestId:id,name:id,datasetId:'controlled',capital:'1000',researchIds:[conversation.id],capacityPolicy:{participation:'0.1',maximumAgeSeconds:3600,scenarios:['1000']}},
          evidence:[{researchId:conversation.id,input:{goal:'TEST_ONLY',data:dataset.data,policy:{strategy},model:state.model},selected:{taskId:1,proposal:{model:wat}}}],weights:['1'],policy:strategy,native:null,target:null,
          view:{id,name:id,researchIds:[conversation.id],datasetId:'controlled',status:'COMPUTED',capital:'1000',currency:'USD',createdAt:'2026-11-02T00:00:00Z',taskId:null,metrics:[],curve:[],targets:[],error:null}} as unknown as PortfolioFact;
        const policy=calibrated('PORTFOLIO',`policy:${id}`);const evidenceRef=`fixture:${id}:validation`;
        const metrics:ReviewMetric[]=policy.metricRules.map(rule=>({key:rule.key,value:rule.high,status:'AVAILABLE',observationCount:100,method:rule.methods[0]!,scope:rule.scope,unit:rule.unit,frequency:rule.frequency,evidenceRef}));
        fact.view.review=scoreReview({id:`review:${id}`,target:{kind:'PORTFOLIO',id,version:portfolioReviewVersion(fact),sourceEvidenceRefs:[evidenceRef],evaluationStartNs:String(150n*DAY)},policy,metrics,checks:[...policy.requiredChecks,'independent_outcome'].map(id=>({id,status:'PASS',reason:'TEST_ONLY',evidenceRefs:[evidenceRef]})),createdAt:'2026-11-02T00:00:00Z'});
        application.portfolios[id]=json(fact);
      }
    },context);
    const call=(name:string,args:Record<string,unknown>)=>fauxAssistantMessage(fauxToolCall(name,args as any),{stopReason:'toolUse'});
    state.faux.setResponses([...['context','provenance','factor','selection','stress','portfolio'].map(kind=>call(`review_${kind}`,{})),async()=>{
      const records=(await state.service.research.harness.snapshot(ReviewAllocations,context))!.records;
      const id=Object.keys(records)[0]!;const evidenceRefs=[`allocation:${id}:shared-validation`];
      return call('review_finish',{narrative:'Controlled complete evidence protocol.',findings:[],mechanism:['mechanism','availability','direction','implementation','complexity'].map(criterion=>({criterion,supported:true,evidenceRefs}))});
    }]);
    await writeFile(log+'.pause','pause first native simulation');
    const body={requestId:'allocation-test',name:'TEST_ONLY',portfolioIds:['portfolio-a','portfolio-b'],memberCaps:[{id:'portfolio-a',maximumWeight:'0.8'},{id:'portfolio-b',maximumWeight:'0.8'}],clusterCaps:[],currentWeights:[{id:'portfolio-a',weight:'0'},{id:'portfolio-b',weight:'0'}],maximumTurnover:'1',reviewPolicy:calibrated('ALLOCATION','allocation-policy'),independentDatasetId:'independent-test-only'};
    const forged=structuredClone(body);forged.requestId='forged-policy';forged.reviewPolicy.calibration!.rootCount=999;
    const refused=await state.service.app.inject({method:'POST',url:'/api/v3/review-allocations',headers:state.headers,payload:forged});assert.equal(refused.statusCode,422,refused.body);assert.equal(refused.json().error,'review_policy_not_registered');
    const response=await state.service.app.inject({method:'POST',url:'/api/v3/review-allocations',headers:state.headers,payload:body});assert.equal(response.statusCode,202,response.body);const admitted=response.json();
    let entered=false;for(let n=0;n<200;n++){try{await readFile(log+'.entered');entered=true;break;}catch{await setTimeout(10);}}assert.equal(entered,true,'native operation started');
    afterRestart = true;
    await state.restart();
    const settled=await state.service.research.harness.waitForTask(admitted.taskId as TaskId,context);
    if(mode==='REMOTE_RESERVATION_GAP'){
      assert.equal(settled.state.outcome.status,'failed');
      const read=await state.service.app.inject({url:`/api/v3/review-allocations/${admitted.id}`,headers:state.headers});const failed=read.json();
      assert.match(failed.error,/^nautilus_execution_unknown:[a-f0-9]{64}$/);assert.equal(failed.consumedScientificCalls,2);
      assert.equal(remotePosts.filter(row=>row.command==='simulate').length,1,'unknown original admission cannot create a replacement invocation');
      assert.equal(new Set(remotePosts.map(row=>row.invocationId)).size,2);assert.equal(state.faux.state.callCount,0);
      assert.ok(remotePosts.filter(row=>row.command==='forecast').every(row=>row.input.selection.event_end_ns===String(300n*DAY)&&row.input.selection.decision_cutoff_ns===String(300n*DAY)));
      return;
    }
    assert.equal(settled.state.outcome.status,'completed',JSON.stringify(settled.state));
    const result=await state.service.app.inject({url:`/api/v3/review-allocations/${admitted.id}`,headers:state.headers});assert.equal(result.statusCode,200,result.body);const view=result.json();
    assert.equal(view.status,'COMPUTED',view.error??'');assert.equal(view.consumedScientificCalls,mode==='MISSING_INDEPENDENT_PROOF'?14:21);assert.equal(view.review.decision,mode==='MISSING_INDEPENDENT_PROOF'?'INCONCLUSIVE':mode==='FAILED_INDEPENDENT'?'REJECT':'PASS',JSON.stringify(view.review.reasonCodes));
    assert.equal(view.weights.candidateId,admitted.id);assert.deepEqual(view.weights.members.map((row:any)=>row.weight),['0.5','0.5']);
    assert.equal(view.review.createdAt,admitted.createdAt);const modelCalls=state.faux.state.callCount;
    await state.restart();assert.equal((await state.service.research.harness.waitForTask(admitted.taskId as TaskId,context)).state.outcome.status,'completed');assert.equal(state.faux.state.callCount,modelCalls);
    const replay=await state.service.app.inject({method:'POST',url:'/api/v3/review-allocations',headers:state.headers,payload:body});assert.equal(replay.statusCode,202,replay.body);assert.equal(replay.json().id,admitted.id);
    const commands=(await readFile(log,'utf8')).trim().split('\n').map(line=>JSON.parse(line));
    assert.ok(commands.filter(row=>row.command==='forecast'&&BigInt(row.input.selection.event_start_ns)===0n&&BigInt(row.input.selection.event_end_ns)<=350n*DAY).every(row=>row.input.selection.event_end_ns===String(300n*DAY)&&row.input.selection.decision_cutoff_ns===String(300n*DAY)),'Validation native input must end at its own cutoff before any worker read');
    if(mode==='MISSING_INDEPENDENT_PROOF'){assert.equal(commands.some(row=>row.command==='review-source'),false);assert.equal(commands.some(row=>row.command==='simulate'&&BigInt(row.input.selection.event_start_ns)>=400n*DAY),false);}
    if(mode==='FAILED_INDEPENDENT')assert.ok(view.review.hardFailures.some((reason:string)=>reason.startsWith('independent_outcome:')));
    assert.ok(commands.some(row=>row.command==='assess-capacity'));assert.ok(commands.some(row=>row.command==='simulate'&&row.input.settings.fill_model.parameters.prob_slippage==='1'));
  }finally{await state.cleanup();if(remote){remote.closeAllConnections();await new Promise<void>(resolve=>remote!.close(()=>resolve()));}}
});

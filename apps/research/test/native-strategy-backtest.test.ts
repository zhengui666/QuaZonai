import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineDocFamily, type JsonObject } from '@earendil-works/pi-durable';
import { canonicalJsonDigest } from '../src/native-json.ts';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask, normalizedNativeStrategyRequest, nativeStrategyResult, nativeEquityMetric, nativeEquityPerformance } from '../plugins/native-strategy-backtest.ts';
import { nativeCommands, NativeRemoteUncertainty, NativeExecutionTask, remoteNativeExecutionStatus } from '../src/native-execution.ts';
import type { NativeExecutionRequest, ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';

// Existing official task state uses legacy GET receipts; new tools use real processes.
// These are protocol tests, not numerical backtest evidence.
const Calls=defineDocFamily<any,null>({kind:'qz.native-call',version:1,scope:'task',family:true,initial:()=>({taskId:null}),checkpointWhen:()=>true});
for (const behavior of ['recover', 'stop', 'mismatch', 'tool', 'tool-finish'] as const) test(`native upstream strategy ${behavior} retains a single original task and invocation`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-indicator-batch-'));
  const posts: NativeExecutionRequest[] = []; const deletes: string[] = [], gets:string[]=[]; let completed = behavior === 'mismatch' || behavior === 'tool' || behavior === 'tool-finish';
  const request: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM',
    selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
    event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000101', available_end_ns: '1790000000000000101',
    venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.0001' },
    strategy: { kind: 'EMA_CROSS', base: { strategy_id: 'OWNER-001', order_id_tag: 'native' }, trade_size: '0.001', fast_period: 2, slow_period: 3 } };
  const original:NativeExecutionRequest={schemaVersion:1,requestId:'b'.repeat(64),invocationId:'retained-native-invocation',files:{},implementationDigest:`sha256:${'a'.repeat(64)}`,command:'native-strategy-backtest',args:['--catalog',directory],input:request};
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') {
      let body = ''; for await (const chunk of incoming) body += chunk;
      posts.push(JSON.parse(body));
    }
    if(incoming.method==='DELETE')deletes.push(incoming.url!);
    assert.equal(incoming.method,'GET','historical receipt reconciliation may not send POST/DELETE');
    assert.equal(incoming.url,`/v1/executions/${original.requestId}`);gets.push(incoming.url!);
    // Protocol-only payload: verifies binding/recovery, not numerical engine behavior.
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed ? (behavior==='stop'?'CANCELLED':'COMPLETED') : 'UNKNOWN',
      ...(completed ? { result: { schema_version: 1, native_version: '0.69.0',
        request: behavior === 'mismatch' ? { ...original.input as JsonObject, instrument_id: 'OTHER.SIM' } : original.input,
        instrument_definition: { mode: 'HISTORICAL_AS_OF_START', ts_init_ns: '0', ts_event_ns: '0', scope: 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START' }, equity_performance: { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'UNAVAILABLE', reason: 'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE', currency: null, returns: [], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: `${name} (${annualization_days} days)`, annualization_days, value: null }))), first_snapshot_ns: null, last_snapshot_ns: null, snapshot_count: '0', observed_utc_days: '0', unpriced_snapshot_count: '0', return_days: '0', notes: ['Protocol fixture, not measured performance'] }, canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' },
          accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], statistics: {}, components: {} },
        data_rows_by_type: { QuoteTick: '2' }, notes: [] } } : { error: 'original_receipt_temporarily_unknown' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const binary=join(directory,'controlled-native.mjs'),marker=join(directory,'calls.jsonl');
  const processResult={schema_version:1,native_version:'0.69.0',request:behavior==='mismatch'?{...request,instrument_id:'OTHER.SIM'}:request,
    instrument_definition:{mode:'HISTORICAL_AS_OF_START',ts_init_ns:'0',ts_event_ns:'0',scope:'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START'},
    equity_performance:{source:'NAUTILUS_PORTFOLIO_SNAPSHOTS',status:'UNAVAILABLE',reason:'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE',currency:null,returns:[],statistics:[252,365].flatMap(annualization_days=>['CAGR','Sharpe Ratio'].map(name=>({native_key:`${name} (${annualization_days} days)`,annualization_days,value:null}))),first_snapshot_ns:null,last_snapshot_ns:null,snapshot_count:'0',observed_utc_days:'0',unpriced_snapshot_count:'0',return_days:'0',notes:['Protocol fixture, not measured performance']},
    canonical_result:{schema:'nautilus-backtest-result/v1',run:{outcome:'completed'},accounts:[],diagnostics:[],fills:[],orders:[],portfolio_snapshots:[],position_snapshots:[],positions:[],statistics:{},components:{}},data_rows_by_type:{QuoteTick:'2'},notes:[]};
  await writeFile(binary,`#!${process.execPath}\nimport{appendFileSync}from'node:fs';let text='';for await(const part of process.stdin)text+=part;appendFileSync(${JSON.stringify(marker)},JSON.stringify({command:process.argv[2],args:process.argv.slice(3),input:JSON.parse(text)})+'\\n');process.stdout.write(${JSON.stringify(JSON.stringify(processResult))});`,{mode:0o755});
  const processTarget:ProcessNativeTarget={kind:'RUST_PROCESS',id:'strategy-process',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-only',precision:'high'},research:{singleStrategyRegistration:true,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
  const processCalls=async()=> (await readFile(marker,'utf8')).trim().split('\n').map(line=>JSON.parse(line));
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  const historical=behavior==='recover'||behavior==='stop';
  const options = { database: join(directory, 'research.sqlite'), models, deferResume:historical };
  if (behavior === 'tool' || behavior === 'tool-finish') faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', behavior === 'tool-finish' ? { finishResearch: true } : {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Historical result saved; no independent validation was requested.')]);
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    if (behavior === 'tool' || behavior === 'tool-finish') {
      const { Study } = await import('../src/documents.ts');
      const admission = await host.start('exact-upstream', { goal: 'Evaluate this bound upstream strategy', model: { provider: faux.provider.id, modelId: faux.getModel().id },
        data: { catalog: directory, native_strategy_backtest_request: request, nautilusTarget: processTarget }, policy: {} });
      await (await host.harness.submission(admission.submissionId, context))!.wait(context);
      const study = (await host.harness.snapshot(Study, admission.conversationId, context))!;
      assert.equal(study.final?.status, 'COMPUTED');
      assert.equal(study.final?.scientificClaim, 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION');
      assert.equal(study.trials.length, 0);
      assert.equal(faux.state.callCount, behavior === 'tool-finish' ? 1 : 2, 'default continues the official loop; explicit standalone finish terminates');
      const launches=await processCalls();
      assert.equal(launches.length,1);assert.deepEqual(launches[0].input,request);assert.equal(launches[0].command,'native-strategy-backtest');assert.deepEqual(launches[0].args,['--catalog',directory]);
      const originalPosts = await processCalls(); await host.close(); host = await openResearch(options);
      assert.deepEqual(await host.start('exact-upstream', study.input!), admission);
      assert.deepEqual(await processCalls(),originalPosts);
      assert.deepEqual(posts,[],'new execution cannot send legacy HTTP POST');
      return;
    }
    const target=behavior==='mismatch'?processTarget:{id:'batch-protocol',url:`http://127.0.0.1:${address.port}`};
    const id = await root.commit(async tx=>{
      const owner=await tx.createTask(NativeStrategyBacktestTask,{request,catalog:directory,target},{ownership:{kind:'conversation'}});
      if(historical){
        const basis=canonicalJsonDigest({target,command:original.command,input:request,args:original.args,files:{}});
        const child=await tx.createTask(NativeExecutionTask,{target,requestId:original.requestId,basis,command:original.command,request:{request:original as unknown as JsonObject},ownerTaskId:owner,receiptRequired:true},{ownership:{kind:'task',taskId:owner}});
        (await tx.doc(Calls,owner,basis,null)).taskId=child;
        (await tx.doc(NativeRemoteUncertainty,root.id,String(owner),null)).requests.push({taskId:owner,target,requestId:original.requestId,status:'UNKNOWN',error:'retained_original_outcome_unknown'});
      }
      return owner;
    }, context);
    host.harness.resume();
    if (behavior === 'mismatch') {
      const task = await host.harness.waitForTask(id, context);
      assert.equal(task.state.outcome.status, 'failed');
      assert.equal(task.state.outcome.status === 'failed' && task.state.outcome.error.message, 'native_strategy_request_mismatch');
    } else {
      const deadline = Date.now() + 5000;
      for (;;) {
        const task = await host.harness.getTask(id, context); assert.ok(task);
        if (task.state.status === 'sleeping' && (task.state.checkpoint as { phase: string }).phase === 'reconcile') break;
        assert.notEqual(task.state.status, 'terminal', JSON.stringify(task.state));
        assert.ok(Date.now() < deadline, 'batch did not retain original receipt'); await delay(5);
      }
      if (behavior === 'recover') {
        const originalPosts = structuredClone(posts); await host.close(); completed = true;
        host = await openResearch(options);host.harness.resume();
        const task = await host.harness.waitForTask(id, context);
        assert.equal(task.state.outcome.status, 'completed');
        assert.deepEqual(posts, originalPosts, 'restart must GET the original receipt, not POST another batch');
        assert.equal(deletes.length, 0);
      } else {
        await host.harness.abortTask(id, context); await host.harness.waitForTask(id, context);
        assert.deepEqual(deletes,[],'local abort is not authority to send retired HTTP DELETE');
        const retained = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
        assert.equal(retained!.requests[0]!.status,'UNKNOWN');
        assert.equal(retained!.requests[0]!.requestId,original.requestId);
        await host.close();host=await openResearch(options);
        assert.deepEqual(await host.harness.snapshot(NativeRemoteUncertainty,root.id,String(id),context),retained);
        completed=true;assert.equal((await remoteNativeExecutionStatus(target,original.requestId)).status,'CANCELLED');
        assert.equal((await host.harness.getTask(id,context))!.state.outcome?.status,'aborted','late remote receipt cannot resurrect the locally aborted owner');
      }
    }
    if(behavior==='mismatch'){const launches=await processCalls();assert.equal(launches.length,1);assert.equal(launches[0].command,original.command);assert.deepEqual(launches[0].args,original.args);assert.deepEqual(launches[0].input,request);}
    else {assert.ok(gets.length>0);assert.ok(gets.every(url=>url===`/v1/executions/${original.requestId}`));const tasks=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items;assert.equal(tasks.length,1);assert.deepEqual((tasks[0]!.input as any).request.request,original);}
    assert.deepEqual(posts,[]);assert.deepEqual(deletes,[]);assert.equal(faux.state.callCount,0);
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

test('bound upstream research exposes only its native tool and never translates a sealed or mixed binding', async () => {
  const { fauxAssistantMessage, fauxToolCall } = await import('@earendil-works/pi-ai');
  const { Study } = await import('../src/documents.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-strategy-intake-'));
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Cannot use a sealed historical binding.')]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    await assert.rejects(host.start('bound-upstream', { goal: 'Reject mixed historical binding without translating it', model: { provider: faux.provider.id, modelId: faux.getModel().id },
      data: { catalog: '/registered/catalog', native_strategy_backtest_request: {}, indicator_strategy_request: {} }, policy: {} }), /custom_indicator_strategy_removed/);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});


test('Grid absent optional quantities and expiry match official None echo without changing explicit values', () => {
  const request = { venue: {}, strategy: { kind: 'GRID_MARKET_MAKER', num_levels: 2 } };
  assert.deepEqual(normalizedNativeStrategyRequest(request), { venue: { base_currency: null }, strategy: { kind: 'GRID_MARKET_MAKER', num_levels: 2, trade_size: null, expire_time_secs: null } });
  const explicit = { venue: { base_currency: null }, strategy: { kind: 'GRID_MARKET_MAKER', trade_size: '0.00000001', expire_time_secs: 2, base: { strategy_id: 'OWNER-001' } } };
  assert.deepEqual(normalizedNativeStrategyRequest(explicit), explicit);
  assert.deepEqual(request.strategy, { kind: 'GRID_MARKET_MAKER', num_levels: 2 });
});

test('instrument definition evidence cannot silently upgrade static assumptions into historical rules', () => {
  const request: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }], event_start_ns: '10', event_end_ns: '20', available_end_ns: '20', venue: { base_currency: null }, strategy: { kind: 'EMA_CROSS' } };
  const basis = { mode: 'HISTORICAL_AS_OF_START', ts_init_ns: '0', ts_event_ns: '0', scope: 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START' };
  const result: JsonObject = { schema_version: 1, native_version: '0.69.0', request, instrument_definition: basis, equity_performance: { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'UNAVAILABLE', reason: 'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE', currency: null, returns: [], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: `${name} (${annualization_days} days)`, annualization_days, value: null }))), first_snapshot_ns: null, last_snapshot_ns: null, snapshot_count: '0', observed_utc_days: '0', unpriced_snapshot_count: '0', return_days: '0', notes: ['Protocol fixture, not measured performance'] }, canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], statistics: {}, components: {} }, data_rows_by_type: { QuoteTick: '1' }, notes: [] };
  assert.deepEqual(nativeStrategyResult(result, request).instrument_definition, basis);
  const countNotes = ['CATALOG_ROW_COUNTS: pre-run validation scan, not consumed events', 'REPLAY_READER_ROWS_INCLUDING_PREFETCH: {"QuoteTick":"9007199254740993"}', 'CATALOG_READ_CONSISTENCY_UNVERIFIED: equal counts do not prove identical inputs'];
  assert.deepEqual(nativeStrategyResult({ ...result, notes: countNotes }, request).notes, countNotes);
  assert.deepEqual(nativeStrategyResult(result, request).notes, [], 'Older report notes must not acquire an inferred streaming mode');
  assert.throws(() => nativeStrategyResult({ ...result, instrument_definition: null }, request), /native_instrument_definition/);
  assert.throws(() => nativeStrategyResult({ ...result, instrument_definition: { ...basis, ts_init_ns: '11' } }, request), /binding_mismatch/);
  const scenario = { ...request, instrument_definition: { mode: 'STATIC_SCENARIO', ts_init_ns: '30', ts_event_ns: '29' } };
  const evidence = { ...result, request: scenario, instrument_definition: { ...scenario.instrument_definition, scope: 'HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED' } };
  assert.deepEqual(nativeStrategyResult(evidence, scenario).instrument_definition, evidence.instrument_definition);
  assert.throws(() => nativeStrategyResult({ ...evidence, instrument_definition: basis }, scenario), /scope_mismatch/);
  assert.throws(() => nativeStrategyResult({ ...evidence, instrument_definition: { ...evidence.instrument_definition, ts_init_ns: '31' } }, scenario), /binding_mismatch/);
  assert.throws(() => nativeStrategyResult(evidence, request), /request_mismatch/);

});

test('optional official BAR execution preserves explicit false and normalizes only absent semantics', () => {
  const strategy = { kind: 'HURST_VPIN_DIRECTIONAL', bar_type: 'BTC.SIM-100-VALUE-LAST-INTERNAL' };
  assert.deepEqual(normalizedNativeStrategyRequest({ venue: { bar_execution: false }, strategy }).venue, { base_currency: null, bar_execution: false });
  assert.deepEqual(normalizedNativeStrategyRequest({ venue: { bar_execution: null }, strategy }).venue, { base_currency: null });
});

test('official equity metrics keep frequencies separate and unavailable evidence never falls back to positions', () => {
  const evidence: JsonObject = { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'AVAILABLE', reason: null, currency: 'USDC', returns: [{ ts_event_ns: '10', value: 0.01 }], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: `${name} (${annualization_days} days)`, annualization_days, value: annualization_days === 252 ? 2 : 3 }))), first_snapshot_ns: '0', last_snapshot_ns: '10', snapshot_count: '2', observed_utc_days: '1', unpriced_snapshot_count: '0', return_days: '1', notes: ['Wire-only test, not measured returns'] };
  const objective = { native_key: 'Sharpe Ratio (365 days)', annualization_days: 365, currency: 'USDC' };
  assert.equal(nativeEquityMetric(evidence, objective), 3);
  assert.equal(nativeEquityMetric(evidence, { ...objective, currency: 'USDT' }), null);
  assert.equal(nativeEquityMetric(evidence, { ...objective, annualization_days: 252 }), null);
  assert.equal(nativeEquityMetric({ ...evidence, status: 'UNAVAILABLE', reason: 'UNPRICED_PORTFOLIO_SNAPSHOTS', unpriced_snapshot_count: '1' }, objective), null);
  assert.throws(() => nativeEquityPerformance({ ...evidence, unpriced_snapshot_count: '1' }), /evidence_incomplete/);
  assert.throws(() => nativeEquityPerformance({ ...evidence, return_days: '2' }), /count_invalid/);
  assert.throws(() => nativeEquityPerformance({ ...evidence, statistics: [] }), /statistics_incomplete/);
  assert.throws(() => nativeEquityPerformance({ ...evidence, statistics: [...evidence.statistics as JsonObject[], (evidence.statistics as JsonObject[])[0]!] }), /duplicate/);

});

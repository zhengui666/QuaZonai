import assert from 'node:assert/strict';
import test from 'node:test';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { obj } from '@earendil-works/pi-schema';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import { evaluateTrial } from '../src/trials.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { researchInput } from '../src/input.ts';
import { legacySelection } from '../src/pm-comparison.ts';
import { validationReviewContext } from '../src/review-workflow.ts';
import heldout from '../plugins/heldout-wasm.ts';
import { runTickEconomic, tickEconomicFeedback } from '../plugins/tick-economic.ts';
import factor from '../plugins/factor-wasm.ts';
import research from '../plugins/research.ts';
import { Exposures } from '../plugins/native-exposure.ts';
import { startLegacyResearch } from './legacy-study.ts';

const context = BACKGROUND_CONTEXT;
const day = 86_400_000_000_000n;
const tool = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function feedback(transcript: { messages: readonly unknown[] }, name: string): JsonObject {
  const message = transcript.messages.filter(value => (value as { role: string; toolName?: string }).role === 'toolResult'
    && (value as { toolName?: string }).toolName === name).at(-1) as { content: { text?: string }[] };
  return JSON.parse(message.content.map(item => item.text ?? '').join('')) as JsonObject;
}

for (const mode of ['completed', 'failed', 'stopped', 'recovered', 'financial', 'financial-final-failed'] as const) test(`tick economic research preserves primary/model binding and original ${mode} child`, { timeout: 30_000, concurrency: false }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-research-tick-economic-'));
  const catalog = join(directory, 'primary'); const sealed = join(directory, 'sealed');
  await mkdir(catalog); await mkdir(sealed);
  const calls = join(directory, 'calls'); const allow = join(directory, 'allow'); const binary = join(directory, 'controlled-job');
  await writeFile(binary, `#!/usr/bin/python3
import json, os, sys, time
request=json.load(sys.stdin)
if sys.argv[1]=='evaluate-tick-factor':
 print(json.dumps({'request':request})); sys.exit(0)
assert sys.argv[1]=='replay-tick-economic'
heldout=request['research']['input']['event_end_ns']==str(8*86400000000000)
assert sys.argv[sys.argv.index('--catalog')+1]==(${JSON.stringify(sealed)} if heldout else ${JSON.stringify(catalog)})
assert sys.argv[sys.argv.index('--funding-catalog')+1]==(${JSON.stringify(sealed)} if heldout else ${JSON.stringify(catalog)})
assert sys.argv[sys.argv.index('--book-cores')+1]=='3'
assert request['research']['input']['event_end_ns']==str((8 if heldout else 5)*86400000000000)
assert request['research']['input']['decision_cutoff_ns']==str((8 if heldout else 5)*86400000000000)
assert request['scenario']['initial_usdt']=='6000'
assert request['scenario']['decision_interval_ns']=='1000000000'
assert request['small_capital_policy']['decision_clock']=='FIXED_INTERVAL_V1'
model=open(sys.argv[sys.argv.index('--model')+1],'rb').read()
assert model==bytes([0,97,115,109,1,0,0,0])
with open(${JSON.stringify(calls)},'a') as log: log.write('run\\n')
if (${JSON.stringify(mode)}=='failed' and len(open(${JSON.stringify(calls)}).readlines())==1) or (${JSON.stringify(mode)}=='financial-final-failed' and heldout): raise RuntimeError('CONTROLLED_ECONOMIC_FAILURE')
while ${JSON.stringify(mode)} in ['recovered','stopped'] and not os.path.exists(${JSON.stringify(allow)}): time.sleep(0.01)
print(json.dumps({'schema_version':1,'request':request,'model':'CONTROLLED_PROTOCOL_ONLY','methods':{'scope':'protocol only'},'coverage':{},'assets':[],
'days':[{'day_start_ns':'0','equity_usdt':None,'net_return':None,'unknown_reason':'CONTROLLED_MISSING_MARK'}, {'day_start_ns':str(6*86400000000000),'equity_usdt':'999','net_return':99,'unknown_reason':None}],
'partition_metrics':[{'partition':p,'days':'1','valid_return_days':'1','sharpe_365':6 if p!='HELDOUT' else 12345,'annualized_return_365':4} for p in ['TRAIN','VALIDATION','HELDOUT']],
'metrics':[{'partition':p,'regime':'UNKNOWN','days':'0','valid_return_days':'0','sharpe_365':None,'conditional_annualized_return_365':None} for p in ['TRAIN','VALIDATION','HELDOUT']],
'conversion_ns':None,'initial_usdc_after_cost':None,'funding_usdc':'0','fees_usdc_equivalent':None,'fees_by_currency':{},'initial_conversion_cost_usdc':'0','nominal_usd_settled_usdc':'0',
'native_fills':'0','native_rejections':'0','native_perp_pnl_rounding_events':'0','perp_pnl_rounding_bound_usdc':'0','funding_missing_instruments':[],'targets':[],'target_asof_ns':None,'target_cash_weight':None,
'skipped_cash':'0','skipped_minimum':'0','consumed_fuel':None,'limitations':['CONTROLLED_PROTOCOL_NOT_MARKET_EVIDENCE']}))
`, { mode: 0o700 });
  const worker = await createNautilusWorker({ binary, tickBinary: binary, dataDir: join(directory, 'worker'), catalogRoots: [catalog, sealed] });
  worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening');
  const target = { id: 'original-tick-worker', url: `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}` };
  const full = { schema_version: 1, input: { modality: 'TRADE_TICK', instrument_ids: ['BTC.SIM'], event_start_ns: '0', event_end_ns: String(8n * day), decision_cutoff_ns: String(8n * day),
    representation: 'EVENT_TIME_COHORT_FLOW_CACHE_V1', source_revision: 'controlled-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
    sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100 }, target: { kind: 'FIXED_EVENT_TIME', horizon_ns: '1000000000' },
    parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 }, train_end_ns: String(3n * day), validation_end_ns: String(5n * day) };
  const data = { catalog, nautilusTarget: target, nativeInput: { modality: 'TRADE_TICK', request: full }, sealed: { catalog: sealed, nautilusTarget: target, nativeInput: { modality: 'TRADE_TICK', request: full } } };
  const policy = { tickEconomic: { scenario: { initial_usdt: '6000', usdt0_per_usdt: '1', spot_fraction: '0.5', gross_fraction: '1', maximum_asset_weight: '0.02', participation_fraction: '0.01',
    perp_taker_fee_bps: '4.5', spot_taker_fee_bps: '7', extra_cost_bps: '1.5', initial_margin_fraction: '0.1', maintenance_margin_fraction: '0.05', decision_interval_ns: '1000000000',
    base_latency_ns: '1000000', jitter_min_ns: '100000', jitter_max_ns: '1500000', random_seed: '0', maximum_mark_age_ns: '60000000000' },
    fx_instrument_ids: ['USDT0-USDC.HYPERLIQUID'], funding_source: 'funding-rates.jsonl',
    small_capital_policy: { maximum_entry_notional_usdc: '30', maximum_holding_ns: '60000000000', decision_clock: 'FIXED_INTERVAL_V1' }, bookCores: 3 } };
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const register = defineExtension({ name: 'test.tick-economic-source', tools: [defineTool({
    name: 'record_source', description: 'Protocol-only original computed trial', parameters: obj({}), replay: 'safe',
    async execute(_args, api, ctx) {
      const result = await evaluateTrial(api, ctx, { model: '(module)', hypothesis: 'Controlled original trial', request: full }, async () => ({ request: iterationRequest(data) }));
      return { output: [{ type: 'text', text: JSON.stringify({ trialId: api.taskId, result }) }] };
    },
  }), defineTool({
    name: 'economic_pair', description: 'Concurrent calls retain one economic attempt', parameters: obj({}), replay: 'safe',
    async execute(_args, api, ctx) {
      const [left, right] = await Promise.all([runTickEconomic(api, ctx, trialId), runTickEconomic(api, ctx, trialId)]);
      assert.deepEqual(left, right);
      return { output: [{ type: 'text', text: JSON.stringify({ trialId, economic: left }) }] };
    },
  })] });
  let trialId = 0; let first: JsonObject; let repeated: JsonObject; let readback: JsonObject;
  faux.setResponses([
    mode.startsWith('financial') ? tool('factor_evaluate', { hypothesis: 'Controlled original trial', model: '(module)' }) : tool('record_source'),
    transcript => { trialId = feedback(transcript, mode.startsWith('financial') ? 'factor_evaluate' : 'record_source').trialId as number; return tool('factor_economic_evaluate', { trialId, model: '(module invalid)', catalog: sealed }); },
    transcript => {
      const rejected = transcript.messages.filter(value => (value as { role: string; toolName?: string }).role === 'toolResult'
        && (value as { toolName?: string }).toolName === 'factor_economic_evaluate').at(-1) as { isError: boolean };
      assert.equal(rejected.isError, true, 'model and catalog overrides are rejected before execution');
      return tool('factor_economic_evaluate', { trialId: trialId + 100000 });
    },
    transcript => {
      const rejected = transcript.messages.filter(value => (value as { role: string; toolName?: string }).role === 'toolResult'
        && (value as { toolName?: string }).toolName === 'factor_economic_evaluate').at(-1) as { isError: boolean };
      assert.equal(rejected.isError, true, 'only a computed trial in the original conversation can be replayed');
      return tool('factor_economic_evaluate', { trialId });
    },
    transcript => { first = feedback(transcript, 'factor_economic_evaluate'); return mode === 'failed' ? tool('research_finish', { reason: 'Retain failed economic attempt before recovery.' }) : tool('economic_pair'); },
    transcript => { repeated = feedback(transcript, 'economic_pair'); return tool('research_trial', { trialId }); },
    transcript => { readback = feedback(transcript, 'research_trial'); return mode.startsWith('financial') ? tool('factor_finalize', { trialId, reason: 'Controlled protocol completed' }) : tool('research_finish', { reason: 'Controlled protocol completed' }); },
  ]);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [factor, research, register, heldout] };
  let host = await openResearch(options);
  try {
    const specification = { version: 1, name: 'Economic goal', question: 'Check primary-only economics protocol', rationale: 'After-cost account objective', evaluation: { metric: 'Sharpe Ratio (365 days)', direction: 'maximize', target: 5, scheme: 'registered-partitions' }, search: { scope: 'single-factor', maxMembers: 1, stop: 'selection' } };
    if (mode.startsWith('financial')) {
      assert.throws(() => researchInput({ goal: specification.question, specification, model: { provider: 'test', modelId: 'test' }, data, policy: {} }), /tick_economic_policy/);
      assert.throws(() => researchInput({ goal: specification.question, specification, model: { provider: 'test', modelId: 'test' }, data: {}, policy }), /tick_economic_tick_input_required/);
    }
    const receipt = await startLegacyResearch(host, `economic-${mode}`, { goal: 'Check primary-only economics protocol', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy, ...(mode.startsWith('financial') ? { specification } : {}) });
    const originalInput = structuredClone((await host.state(receipt.conversationId)).research!.input);
    let submissionId = receipt.submissionId;
    let previousAttempt: Awaited<ReturnType<typeof host.harness.getTask>>;
    let previousExposures = await host.harness.snapshot(Exposures, context);
    if (mode === 'recovered' || mode === 'stopped') {
      const deadline = Date.now() + 10_000;
      for (;;) {
        try { if ((await readFile(calls, 'utf8')).includes('run')) break; } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
        assert.ok(Date.now() < deadline, 'native economic child did not start'); await new Promise(resolve => setTimeout(resolve, 10));
      }
      const active = (await host.state(receipt.conversationId)).research!.trials[0]!;
      assert.equal((active.economic as JsonObject).status, 'STARTED');
      const running = await tickEconomicFeedback(host.harness, context, active.economic as JsonObject);
      assert.equal(running.status, 'STARTED'); assert.notEqual(running.taskStatus, 'terminal');
      const originalChildId = (active.economic as JsonObject).taskId;
      if (mode === 'stopped') {
        previousExposures = await host.harness.snapshot(Exposures, context);
        await host.stop(receipt.conversationId);
        const stopped = (await host.state(receipt.conversationId)).research!;
        assert.equal(stopped.trials[0]!.status, 'COMPUTED');
        first = { trialId, economic: stopped.trials[0]!.economic! };
        assert.equal((first.economic as JsonObject).status, 'STOPPED');
        previousAttempt = await host.harness.getTask(originalChildId as TaskId<unknown>, context);
        assert.equal(previousAttempt!.state.status, 'terminal');
        assert.deepEqual(await host.harness.snapshot(Exposures, context), previousExposures);
      }
      await host.close(); await writeFile(allow, 'continue'); host = await openResearch(options);
      const recovered = (await host.state(receipt.conversationId)).research!.trials[0]!;
      assert.equal((recovered.economic as JsonObject).taskId, originalChildId);
    }
    if (mode === 'failed') {
      await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      previousExposures = await host.harness.snapshot(Exposures, context);
      previousAttempt = await host.harness.getTask((first!.economic as JsonObject).taskId as TaskId<unknown>, context);
      await host.close(); host = await openResearch(options);
    }
    if (mode === 'stopped' || mode === 'failed') {
      faux.setResponses([tool('economic_pair'),
        transcript => { repeated = feedback(transcript, 'economic_pair'); return tool('factor_economic_evaluate', { trialId }); },
        transcript => { assert.deepEqual(feedback(transcript, 'factor_economic_evaluate'), repeated!); return tool('research_trial', { trialId }); },
        transcript => { readback = feedback(transcript, 'research_trial'); return tool('research_finish', { reason: 'Same computed factor retained after owner resume.' }); },
      ]);
      submissionId = (await host.resume(receipt.conversationId, receipt.submissionId, 'Retry economic evaluation of the original computed factor.')).submissionId;
    }
    await (await host.harness.submission(submissionId, context))!.wait(context);
    const source = (await host.harness.snapshot(Study, receipt.conversationId, context))!.trials[0]!;
    assert.deepEqual((await host.state(receipt.conversationId)).research!.input, originalInput);
    assert.equal(source.status, 'COMPUTED'); assert.equal(source.taskId, trialId);
    assert.deepEqual(source.proposal, { model: '(module)', hypothesis: 'Controlled original trial', request: full });
    assert.deepEqual(source.result, { request: iterationRequest(data) });
    const economic = first!.economic as JsonObject;
    const retry = mode === 'failed' || mode === 'stopped';
    assert.equal(economic.status, mode === 'failed' ? 'FAILED' : mode === 'stopped' ? 'STOPPED' : 'COMPUTED');
    if (retry) {
      assert.notEqual((repeated!.economic as JsonObject).taskId, economic.taskId);
      assert.equal((repeated!.economic as JsonObject).status, 'COMPUTED');
      const original = await host.harness.getTask(economic.taskId as TaskId<unknown>, context);
      assert.equal(original!.state.status, 'terminal');
      assert.deepEqual(original, previousAttempt);
      assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows.slice(0, previousExposures!.windows.length), previousExposures!.windows);
      assert.deepEqual(readback!.economic, repeated!.economic);
      const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.tick-economic' }, 100), context);
      assert.deepEqual(tasks.items[0]!.input, tasks.items[1]!.input);
      assert.equal((await host.state(receipt.conversationId)).research!.trials.length, 1);
    } else {
      assert.deepEqual(repeated!, first!); assert.deepEqual(readback!.economic, economic);
    }
    if (mode.startsWith('financial')) {
      const current = (await host.state(receipt.conversationId)).research!;
      assert.equal(current.final?.status, mode === 'financial-final-failed' ? 'FAILED' : 'COMPUTED', JSON.stringify(current.final));
    }
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, mode.startsWith('financial') || retry ? 2 : 1, 'only failed or stopped economic attempts create another native execution');
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.tick-economic' }, 100), context);
    assert.equal(tasks.items.length, mode.startsWith('financial') || retry ? 2 : 1); assert.ok(tasks.items.some(task => task.id === economic.taskId));
    assert.equal(tasks.items[0]!.conversationId, receipt.conversationId);
    const native = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 100), context);
    assert.equal(native.items.length, mode.startsWith('financial') ? 3 : retry ? 2 : 1);
    if (mode.startsWith('financial')) {
      const current = (await host.state(receipt.conversationId)).research!;
      if (mode === 'financial-final-failed') {
        assert.equal(current.final!.status, 'FAILED');
        assert.match(String(current.final!.error), /CONTROLLED_ECONOMIC_FAILURE/);
        assert.deepEqual(current.trials[0]!.economic, source.economic);
        return;
      }
      assert.equal(current.final!.status, 'COMPUTED');
      const finalEconomic = (current.final!.result as JsonObject).economic as JsonObject;
      assert.notEqual(finalEconomic.taskId, economic.taskId);
      assert.deepEqual((finalEconomic.summary as JsonObject).partition_metrics, [{ partition: 'HELDOUT', days: '1', valid_return_days: '1', sharpe_365: 12345, annualized_return_365: 4 }]);
      assert.equal(JSON.stringify(first!).includes('12345'), false);
      assert.equal(legacySelection(current.input!, current.trials), trialId);
      assert.equal(legacySelection(current.input!, [source, { ...source, taskId: trialId + 1 }]), null);
      assert.equal(legacySelection(current.input!, [{ ...source, economic: { ...economic, summary: { ...economic.summary as JsonObject, partition_metrics: [] } } }]), null);
      const missing = { ...source, economic: { status: 'FAILED', summary: (economic.summary as JsonObject) } };
      assert.equal(legacySelection(current.input!, [missing]), null);
      const review = validationReviewContext(current.input!, source, current.trials);
      assert.equal(JSON.stringify(review).includes('12345'), false);
      assert.equal(((review.candidate as JsonObject).economic as JsonObject).taskId, economic.taskId);
    }
    if (mode === 'failed') assert.match(String(economic.error), /CONTROLLED_ECONOMIC_FAILURE/);
    else if (mode !== 'stopped') {
      const summary = economic.summary as JsonObject;
      assert.deepEqual((summary.metrics as JsonObject[]).map(row => row.partition), ['TRAIN', 'VALIDATION']);
      assert.equal((summary.days as JsonObject[]).length, 1);
      assert.equal((summary.days as JsonObject[])[0]!.net_return, null);
      assert.equal(summary.fees_usdc_equivalent, null);
    }
  } finally { await host.close(); await worker.close(); await rm(directory, { recursive: true, force: true }); }
});

import assert from 'node:assert/strict';
import test from 'node:test';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTool, type JsonObject } from '@earendil-works/pi-durable';
import { obj } from '@earendil-works/pi-schema';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import { evaluateTrial } from '../src/trials.ts';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { tickEconomicFeedback } from '../plugins/tick-economic.ts';
import factor from '../plugins/factor-wasm.ts';
import research from '../plugins/research.ts';
import { startLegacyResearch } from './legacy-study.ts';

const context = BACKGROUND_CONTEXT;
const day = 86_400_000_000_000n;
const tool = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function feedback(transcript: { messages: readonly unknown[] }, name: string): JsonObject {
  const message = transcript.messages.filter(value => (value as { role: string; toolName?: string }).role === 'toolResult'
    && (value as { toolName?: string }).toolName === name).at(-1) as { content: { text?: string }[] };
  return JSON.parse(message.content.map(item => item.text ?? '').join('')) as JsonObject;
}

for (const mode of ['completed', 'failed', 'recovered'] as const) test(`tick economic research preserves primary/model binding and original ${mode} child`, { timeout: 30_000, concurrency: false }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-research-tick-economic-'));
  const catalog = join(directory, 'primary'); const sealed = join(directory, 'sealed');
  await mkdir(catalog); await mkdir(sealed);
  const calls = join(directory, 'calls'); const allow = join(directory, 'allow'); const binary = join(directory, 'controlled-job');
  await writeFile(binary, `#!/usr/bin/python3
import json, os, sys, time
request=json.load(sys.stdin)
assert sys.argv[1]=='replay-tick-economic'
assert sys.argv[sys.argv.index('--catalog')+1]==${JSON.stringify(catalog)}
assert sys.argv[sys.argv.index('--funding-catalog')+1]==${JSON.stringify(catalog)}
assert sys.argv[sys.argv.index('--book-cores')+1]=='3'
assert request['research']['input']['event_end_ns']==str(5*86400000000000)
assert request['research']['input']['decision_cutoff_ns']==str(5*86400000000000)
assert request['scenario']['initial_usdt']=='6000'
assert request['scenario']['decision_interval_ns']=='1000000000'
assert request['small_capital_policy']['decision_clock']=='FIXED_INTERVAL_V1'
model=open(sys.argv[sys.argv.index('--model')+1],'rb').read()
assert model==bytes([0,97,115,109,1,0,0,0])
with open(${JSON.stringify(calls)},'a') as log: log.write('run\\n')
if ${JSON.stringify(mode)}=='failed': raise RuntimeError('CONTROLLED_ECONOMIC_FAILURE')
while ${JSON.stringify(mode)}=='recovered' and not os.path.exists(${JSON.stringify(allow)}): time.sleep(0.01)
print(json.dumps({'schema_version':1,'request':request,'model':'CONTROLLED_PROTOCOL_ONLY','methods':{'scope':'protocol only'},'coverage':{},'assets':[],
'days':[{'day_start_ns':'0','equity_usdt':None,'net_return':None,'unknown_reason':'CONTROLLED_MISSING_MARK'}, {'day_start_ns':str(6*86400000000000),'equity_usdt':'999','net_return':99,'unknown_reason':None}],
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
  const data = { catalog, nautilusTarget: target, nativeInput: { modality: 'TRADE_TICK', request: full }, sealed: { catalog: sealed, nativeInput: { modality: 'TRADE_TICK', request: full } } };
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
  })] });
  let trialId = 0; let first: JsonObject; let repeated: JsonObject; let readback: JsonObject;
  faux.setResponses([
    tool('record_source'),
    transcript => { trialId = feedback(transcript, 'record_source').trialId as number; return tool('factor_economic_evaluate', { trialId, model: '(module invalid)', catalog: sealed }); },
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
    transcript => { first = feedback(transcript, 'factor_economic_evaluate'); return tool('factor_economic_evaluate', { trialId }); },
    transcript => { repeated = feedback(transcript, 'factor_economic_evaluate'); return tool('research_trial', { trialId }); },
    transcript => { readback = feedback(transcript, 'research_trial'); return tool('research_finish', { reason: 'Controlled protocol completed' }); },
  ]);
  const options = { database: join(directory, 'research.sqlite'), models, extensions: [factor, research, register] };
  let host = await openResearch(options);
  try {
    const receipt = await startLegacyResearch(host, `economic-${mode}`, { goal: 'Check primary-only economics protocol', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy });
    if (mode === 'recovered') {
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
      await host.close(); await writeFile(allow, 'continue'); host = await openResearch(options);
      const recovered = (await host.state(receipt.conversationId)).research!.trials[0]!;
      assert.equal((recovered.economic as JsonObject).taskId, originalChildId);
    }
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const source = (await host.harness.snapshot(Study, receipt.conversationId, context))!.trials[0]!;
    assert.equal(source.status, 'COMPUTED'); assert.equal(source.taskId, trialId);
    assert.deepEqual(source.proposal, { model: '(module)', hypothesis: 'Controlled original trial', request: full });
    assert.deepEqual(source.result, { request: iterationRequest(data) });
    const economic = first!.economic as JsonObject;
    assert.equal(economic.status, mode === 'failed' ? 'FAILED' : 'COMPUTED');
    assert.deepEqual(repeated!, first!); assert.deepEqual(readback!.economic, economic);
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 1, 'invalid overrides, repeat reads and host recovery never start another native execution');
    const tasks = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.tick-economic' }, 100), context);
    assert.equal(tasks.items.length, 1); assert.equal(tasks.items[0]!.id, economic.taskId);
    assert.equal(tasks.items[0]!.conversationId, receipt.conversationId);
    const native = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 100), context);
    assert.equal(native.items.length, 1);
    if (mode === 'failed') assert.match(String(economic.error), /CONTROLLED_ECONOMIC_FAILURE/);
    else {
      const summary = economic.summary as JsonObject;
      assert.deepEqual((summary.metrics as JsonObject[]).map(row => row.partition), ['TRAIN', 'VALIDATION']);
      assert.equal((summary.days as JsonObject[]).length, 1);
      assert.equal((summary.days as JsonObject[])[0]!.net_return, null);
      assert.equal(summary.fees_usdc_equivalent, null);
    }
  } finally { await host.close(); await worker.close(); await rm(directory, { recursive: true, force: true }); }
});

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import type { JsonObject } from '@earendil-works/pi-durable';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import research from '../plugins/research.ts';
import { modelFeedback } from '../src/feedback.ts';
import { object } from '../src/trials.ts';

const ids = Array.from({ length: 2253 }, (_, index) => String(index + 1).padStart(76, '0') + '.POLYMARKET');
const request = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
  selection: { schema_version: 1, event_start_ns: '0', event_end_ns: '20', decision_cutoff_ns: '20', maximum_rows: 10000,
    bar_types: ids.map((id) => id + '-1-MINUTE-LAST-EXTERNAL') }, parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 } } };
const rates = ids.map((instrument_id) => ({ instrument_id, maker: '0.001234567890123456', taker: '0.005000000000000000' }));

test('cross-market model feedback keeps original account scalars deliverable without repeating a thousands-market request', () => {
  const result: JsonObject = { request, portfolio: { partition: 'VALIDATION', strategy_kind: 'cross-market-threshold',
    statistics: [{ native_key: 'Calendar Geometric Daily Return', value: null, reason_code: 'TEST_ONLY' }],
    provenance: { account: 'ONE_NATIVE_SHARED_ACCOUNT', selection: request.forecast.selection } }, components: [] };
  const before = structuredClone(result); const projected = modelFeedback(result);
  assert.ok(Buffer.byteLength(JSON.stringify(projected)) < 50 * 1024);
  assert.deepEqual(object(projected.portfolio, 'portfolio').statistics, object(result.portfolio, 'portfolio').statistics);
  assert.deepEqual(result, before);
});

test('large cross-market brief and all original training-market pages read completely without trials, compute or final leakage', async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-market-pages-'));
  // This test covers all market pages, not the Faux model's compaction response script.
  const faux = fauxProvider({ tokensPerSecond: 100000, models: [{ id: 'market-paging-test-only', contextWindow: 1000000 }] }); const models = createModels(); models.setProvider(faux.provider);
  const input = { goal: 'TEST ONLY original training-market paging; not market strategy evidence.', model: { provider: faux.provider.id, modelId: faux.getModel().id },
    data: { catalog: '/test-only-primary', request, trainingMarkets: ids.map((instrumentId, index) => ({ instrumentId, observedCalendarDays: 70, barRows: String(index + 1), final_private_marker: 'NEVER_RETURN_FINAL_MARKET_FIELDS' })),
      sealed: { catalog: '/test-only-final', secret: 'FINAL_VALUES_NEVER_RETURN' } },
    policy: { max_trials: 1, strategy: { name: 'original-wide-market-pool', kind: 'cross-market-threshold', instrumentIds: ids, threshold: 0, investedWeight: '1', targetTtlNs: '5',
      simulationSettings: { base_currency: 'pUSD', starting_capital: '3000', fee_rates: rates } } } };
  const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
  const read = (transcript: { messages: readonly unknown[] }, name: string): JsonObject => {
    const message = transcript.messages.filter((m) => (m as { role?: string; toolName?: string }).role === 'toolResult' && (m as { toolName?: string }).toolName === name).at(-1) as { content: { type: string; text?: string }[] };
    const text = message.content.filter((c) => c.type === 'text').map((c) => c.text).join('');
    assert.ok(Buffer.byteLength(text) < 50 * 1024);
    assert.equal(text.includes('FINAL_VALUES_NEVER_RETURN'), false);
    assert.equal(text.includes('NEVER_RETURN_FINAL_MARKET_FIELDS'), false);
    return JSON.parse(text);
  };
  const seen: string[] = [];
  faux.setResponses([
    call('research_brief', {}),
    (transcript) => { const brief = read(transcript, 'research_brief'); assert.equal(brief.totalTrials, 0); return call('research_markets', { offset: 0, limit: 50 }); },
    ...Array.from({ length: Math.ceil(ids.length / 50) }, (_, page) => (transcript: { messages: readonly unknown[] }) => {
      const result = read(transcript, 'research_markets');
      assert.equal(result.totalMarkets, ids.length);
      const markets = result.markets as JsonObject[];
      seen.push(...markets.map((market) => String(market.instrumentId)));
      assert.deepEqual(markets.map((market) => market.rank), markets.map((_, index) => page * 50 + index + 1));
      assert.ok(markets.every((market) => object(market.training, 'training').observedCalendarDays === 70));
      assert.ok(markets.every((market) => object(market.feeReserve, 'reserve').maker === '0.001234567890123456'));
      if (result.nextOffset === null) return call('research_finish', { reason: 'Completed read-only paging protocol, no scientific result.' });
      return call('research_markets', { offset: result.nextOffset, limit: 50 });
    }),
  ]);
  const host = await openResearch({ database: join(directory, 'test-only.sqlite'), models, extensions: [research] });
  try {
    const receipt = await host.start('test-only-original-market-pages', input);
    const settled = await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
    assert.equal(settled.status, 'done', JSON.stringify({ calls: faux.state.callCount, readMarkets: seen.length }));
    const facts = (await host.view(receipt.conversationId)).research!;
    assert.deepEqual(facts.input, input);
    assert.deepEqual(facts.trials, []);
    assert.deepEqual(seen, ids);
    assert.equal(facts.final!.status, 'STOPPED');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

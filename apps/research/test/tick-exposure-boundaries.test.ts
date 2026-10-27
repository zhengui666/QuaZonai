import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { studyInput } from '../src/trials.ts';
import { checkExposure, Exposures } from '../plugins/native-exposure.ts';
import research from '../plugins/research.ts';

const context = BACKGROUND_CONTEXT;
const second = 1_000_000_000n;
const ns = (value: number) => (BigInt(value) * second).toString();
function barData(catalog: string, instrument: string, aggregation = '1-SECOND'): JsonObject {
  return { catalog, request: { schema_version: 1, train_end_ns: ns(1), validation_end_ns: ns(10),
    forecast: { schema_version: 1, parameters: { schema_version: 1, fast_period: 1, slow_period: 2, label_horizon_observations: 1 },
      selection: { schema_version: 1, bar_types: [`${instrument}-${aggregation}-LAST-EXTERNAL`],
        event_start_ns: '0', event_end_ns: ns(12), decision_cutoff_ns: ns(12), maximum_rows: 100 } } } };
}
function tickData(catalog: string, instrument: string, start = 9, end = 10): JsonObject {
  return { catalog, nativeInput: { modality: 'TRADE_TICK', request: { schema_version: 1,
    input: { modality: 'TRADE_TICK', instrument_ids: [instrument], event_start_ns: ns(start - 2), event_end_ns: ns(end),
      decision_cutoff_ns: ns(end), maximum_rows: 100, source_revision: 'original-source', availability: 'EVENT_TIME_PROXY_UNVERIFIED' },
    sampling: { kind: 'EVENT_TIME_COHORT_V1', maximum_cohort_rows: 100 }, target: { kind: 'FIXED_TRADE_COHORTS', horizon_cohorts: 1 },
    parameters: { fast_period_cohorts: 1, slow_period_cohorts: 2 }, train_end_ns: ns(start - 1), validation_end_ns: ns(start) } } };
}

// Real SQLite/Durable transactions verify source-exposure admission only. This
// fixture neither evaluates a numerical factor nor qualifies historical data.
test('cross-modality exposure includes completed BAR source intervals while BAR admission retains exact granularity', { timeout: 30_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-source-exposure-'));
  const catalog = join(directory, 'catalog'); const copied = join(directory, 'copied');
  await mkdir(catalog); await mkdir(copied);
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const failures: string[] = [];
  const reservation = defineExtension({ name: 'test.exposure-reservation', tools: [defineTool({
    name: 'reserve_original_input', description: 'Protocol-only original final window reservation; no calculation.',
    parameters: Type.Object({}), replay: 'safe', executionMode: 'sequential',
    async execute(_args, api, ctx) {
      try { await checkExposure(api, ctx, (await studyInput(api, ctx)).data, true); }
      catch (error) { failures.push((error as Error).message); throw error; }
      return { content: [{ type: 'text', text: 'Original source exposure reserved; no numerical evidence.' }] };
    },
  })] });
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [research, reservation] });
  let sequence = 0;
  async function reserve(data: JsonObject): Promise<void> {
    faux.setResponses([fauxAssistantMessage(fauxToolCall('reserve_original_input', {}), { stopReason: 'toolUse' }),
      fauxAssistantMessage(fauxToolCall('research_finish', { reason: 'Complete bounded exposure protocol without scientific conclusions.' }), { stopReason: 'toolUse' })]);
    const receipt = await host.start(`exposure-${sequence++}`, { goal: 'Source interval admission protocol', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
  }
  try {
    await reserve(barData(catalog, 'BAR_FIRST.SIM'));
    const original = (await host.harness.snapshot(Exposures, context))!.windows[0]!;
    assert.equal(original.start, ns(10));
    await reserve(tickData(copied, 'BAR_FIRST.SIM'));
    assert.deepEqual(failures, ['heldout_already_exposed']);
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 1);
    assert.deepEqual((await host.harness.snapshot(Exposures, context))!.windows[0], original);

    await reserve(tickData(catalog, 'TICK_FIRST.SIM'));
    await reserve(barData(copied, 'TICK_FIRST.SIM'));
    assert.deepEqual(failures, ['heldout_already_exposed', 'heldout_already_exposed']);
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 2);

    await reserve(barData(catalog, 'BAR_COMPAT.SIM'));
    await reserve(barData(copied, 'BAR_COMPAT.SIM', '1-MINUTE'));
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 4);
    assert.equal(failures.length, 2);
    await reserve(barData(copied, 'BAR_COMPAT.SIM'));
    assert.equal(failures.at(-1), 'heldout_already_exposed');
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 4);

    const perInstrument = barData(catalog, 'SHORT.SIM');
    const request = perInstrument.request as JsonObject;
    ((request.forecast as JsonObject).selection as JsonObject).bar_types = ['SHORT.SIM-1-SECOND-LAST-EXTERNAL', 'LONG.SIM-1-DAY-LAST-EXTERNAL'];
    await reserve(perInstrument);
    await reserve(tickData(copied, 'SHORT.SIM', 3, 4));
    assert.equal(failures.length, 3);
    assert.equal((await host.harness.snapshot(Exposures, context))!.windows.length, 6);

    await reserve(barData(catalog, 'UNKNOWN_SPAN.SIM', '1-VOLUME'));
    await reserve(tickData(copied, 'UNKNOWN_SPAN.SIM', 3, 4));
    assert.equal(failures.length, 4);
    assert.equal(failures.at(-1), 'heldout_already_exposed');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

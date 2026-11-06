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
import { checkExposure, Exposures, footprint } from '../plugins/native-exposure.ts';
import { CatalogScopes } from '../plugins/native-catalog-scope.ts';
import { Study } from '../src/documents.ts';
import { clipTickRegimeEvaluation, validateNativeTickFactorRequest } from '../../../packages/contracts/src/tick-input.ts';
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

test('unscoped final catalog boundaries cannot erase an existing final exposure', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-final-catalog-boundary-'));
  const data = barData(directory, 'ORIGINAL.SIM');
  // Controlled transaction protocol checks admission only; native calculation
  // and real Durable scope consumption are covered by the service native suite.
  const ledger: { windows: JsonObject[] } = { windows: [{ kind: 'FINAL', taskId: 1, conversationId: 1,
    barTypes: ['ORIGINAL.SIM-1-SECOND-LAST-EXTERNAL'], start: ns(10), end: ns(12) }] };
  const original = structuredClone(ledger);
  const api = { taskId: 2, conversationId: 2,
    async commit(action: (tx: { doc(): Promise<typeof ledger> }) => Promise<void>) { return action({ async doc() { return ledger; } }); },
  } as unknown as Parameters<typeof checkExposure>[0];
  try {
    for (const kind of ['catalog_scan_request', 'catalog_ensemble_request']) {
      const finalData: JsonObject = { catalog: directory, train_end_ns: ns(1), validation_end_ns: ns(10),
        [kind]: { event_start_ns: '0', train_end_ns: ns(1), validation_end_ns: ns(10), event_end_ns: '0' } };
      await assert.rejects(checkExposure(api, context, data, false, finalData), /native_partition_boundaries/);
      (finalData[kind] as JsonObject).event_end_ns = ns(12);
      await assert.rejects(checkExposure(api, context, data, false, finalData), /heldout_already_exposed/);
      // A valid but shorter final binding cannot narrow the current source read.
      finalData.validation_end_ns = ns(2);
      Object.assign(finalData[kind] as JsonObject, { validation_end_ns: ns(2), event_end_ns: ns(5) });
      await assert.rejects(checkExposure(api, context, data, false, finalData), /heldout_already_exposed/);
      finalData.request = data.request!;
      await assert.rejects(checkExposure(api, context, data, false, finalData), /ambiguous_factor_request/);
      assert.deepEqual(ledger, original);
    }
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('later source events bypass an unavailable BAR scope while receive delay preserves overlap', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-exposure-received-'));
  const ledger = { windows: [{ kind: 'ITERATION', taskId: 1, conversationId: 1, catalog: directory,
    barTypesRef: { conversationId: 1, taskId: 99, count: '1' }, start: ns(1), end: ns(10) }] };
  const scopes: { roots: JsonObject[] } = { roots: [] };
  const original = structuredClone(ledger); let scopeReads = 0;
  const api = { taskId: 2, conversationId: 2,
    async commit(action: (tx: unknown) => Promise<void>) {
      return action({ async doc(type: unknown) { return type === CatalogScopes ? scopes : ledger; }, async task() { scopeReads++; return null; } });
    },
  } as unknown as Parameters<typeof checkExposure>[0];
  try {
    const data = tickData(directory, 'LATER.SIM', 11, 12);
    await checkExposure(api, context, data, true, undefined, undefined, false);
    const request = (data.nativeInput as JsonObject).request as JsonObject;
    const input = request.input as JsonObject;
    input.availability = 'RECEIVED_TIME_OBSERVED'; input.maximum_receive_delay_ns = ns(1);
    (request.sampling as JsonObject).kind = 'RECEIVED_TIME_COHORT_V1';
    data.source_event_start_ns = ns(9); data.source_event_end_ns = ns(12);
    await checkExposure(api, context, data, true, undefined, undefined, false);
    assert.equal(scopeReads, 0);
    input.maximum_receive_delay_ns = ns(2);
    await assert.rejects(checkExposure(api, context, data, true, undefined, undefined, false), /native_catalog_scope_missing/);
    assert.equal(scopeReads, 1);
    scopes.roots.push({ catalog: directory, reference: ledger.windows[0]!.barTypesRef, instrumentVenues: ['POLYMARKET'] });
    await checkExposure(api, context, data, true, undefined, undefined, false);
    assert.equal(scopeReads, 1);
    input.instrument_ids = ['ORIGINAL.POLYMARKET'];
    await assert.rejects(checkExposure(api, context, data, true, undefined, undefined, false), /native_catalog_scope_missing/);
    assert.equal(scopeReads, 2);
    assert.deepEqual(ledger, original);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('received venue bounds preserve ahead-clock exposure and require an explicit source extent', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-received-source-extent-'));
  const ledger: { windows: JsonObject[] } = { windows: [] };
  const api = { taskId: 2, conversationId: 2,
    async commit(action: (tx: unknown) => Promise<void>) { return action({ async doc() { return ledger; } }); },
  } as unknown as Parameters<typeof checkExposure>[0];
  try {
    const data = tickData(directory, 'CLOCK.SIM', 11, 12);
    const request = (data.nativeInput as JsonObject).request as JsonObject;
    const input = request.input as JsonObject;
    input.availability = 'RECEIVED_TIME_OBSERVED'; input.maximum_receive_delay_ns = ns(1);
    (request.sampling as JsonObject).kind = 'RECEIVED_TIME_COHORT_V1';
    await assert.rejects(footprint(data), /native_received_source_bounds_required/);
    data.source_event_start_ns = ns(9); data.source_event_end_ns = ns(13);
    const window = await footprint(data);
    assert.deepEqual([window.start, window.end, window.receivedStart, window.receivedEnd], [ns(10), ns(13), ns(11), ns(12)]);
    await checkExposure(api, context, data, false);
    assert.deepEqual([ledger.windows[0]!.end, ledger.windows[0]!.receivedEnd], [ns(13), ns(11)]);
    ledger.windows.push({ kind: 'FINAL', taskId: 1, conversationId: 1,
      modality: 'TRADE_TICK', instrumentIds: ['CLOCK.SIM'], start: ns(12), end: ns(13) });
    const original = structuredClone(ledger);
    await assert.rejects(checkExposure(api, context, data, true), /heldout_already_exposed/);
    assert.deepEqual(ledger, original);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('only an admitted received-time study can finalize beside its own iteration delay envelope', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-own-received-final-'));
  const primaryCatalog = join(directory, 'primary'); const sealedCatalog = join(directory, 'sealed'); const changedCatalog = join(directory, 'changed');
  await Promise.all([primaryCatalog, sealedCatalog, changedCatalog].map(path => mkdir(path)));
  const full = tickData(sealedCatalog, 'RECEIVED.SIM');
  const request = (full.nativeInput as JsonObject).request as JsonObject;
  Object.assign(request.input as JsonObject, { event_start_ns: '0', event_end_ns: '80', decision_cutoff_ns: '80',
    availability: 'RECEIVED_TIME_OBSERVED', maximum_receive_delay_ns: '2' });
  Object.assign(request, { train_end_ns: '30', validation_end_ns: '50', target: { kind: 'FIXED_RECEIVED_TIME', horizon_ns: '1' } });
  (request.sampling as JsonObject).kind = 'RECEIVED_TIME_COHORT_V1';
  full.source_event_start_ns = '0'; full.source_event_end_ns = '90';
  const primary = { catalog: primaryCatalog, source_event_start_ns: '0', source_event_end_ns: '55',
    nativeInput: { modality: 'TRADE_TICK', request: clipTickRegimeEvaluation(validateNativeTickFactorRequest(request), '50') } };
  const admitted: JsonObject = { ...primary, sealed: full };
  const study = { input: { data: admitted } }; const ledger: { windows: JsonObject[] } = { windows: [] };
  const api = (taskId: number, conversationId = 7) => ({ taskId, conversationId,
    async commit(action: (tx: unknown) => Promise<void>) { return action({ async doc(type: unknown) { return type === Study ? study : ledger; } }); },
  }) as unknown as Parameters<typeof checkExposure>[0];
  const rejected = (data = full) => assert.rejects(checkExposure(api(2), context, data, true, undefined, undefined, false), /heldout_already_exposed/);
  try {
    ledger.windows.push({ kind: 'FINAL', taskId: 99, conversationId: 8,
      modality: 'TRADE_TICK', instrumentIds: ['RECEIVED.SIM'], start: '85', end: '90' });
    await assert.rejects(checkExposure(api(1), context, admitted, false, full), /heldout_already_exposed/);
    ledger.windows.pop();
    await checkExposure(api(1), context, admitted, false, full);
    assert.deepEqual([ledger.windows[0]!.start, ledger.windows[0]!.end, ledger.windows[0]!.receivedStart, ledger.windows[0]!.receivedEnd], ['0', '55', '0', '50']);
    const iteration = structuredClone(ledger.windows[0]!);
    await assert.rejects(checkExposure(api(2, 8), context, full, true, undefined, undefined, false), /heldout_already_exposed/);
    for (const [field, value] of [['source_revision', 'different-source'], ['maximum_receive_delay_ns', '1']] as const) {
      const changed = structuredClone(full);
      (((changed.nativeInput as JsonObject).request as JsonObject).input as JsonObject)[field] = value;
      await rejected(changed);
    }
    study.input.data = { ...admitted, catalog: changedCatalog }; await rejected();
    study.input.data = { ...admitted, source_event_end_ns: '54' }; await rejected();
    study.input.data = structuredClone(admitted);
    (((study.input.data.nativeInput as JsonObject).request as JsonObject).parameters as JsonObject).slow_period_cohorts = 3;
    await rejected();
    study.input.data = { ...admitted }; delete study.input.data.sealed; await rejected();
    study.input.data = admitted;
    delete ledger.windows[0]!.receivedStart; await rejected(); ledger.windows[0] = iteration;
    ledger.windows[0] = { ...iteration, end: '50' }; await rejected(); ledger.windows[0] = iteration;
    ledger.windows.push({ kind: 'ITERATION', taskId: 99, conversationId: 7, barTypes: ['RECEIVED.SIM-1-SECOND-LAST-EXTERNAL'], start: '49', end: '51' });
    await rejected(); ledger.windows.pop();
    await checkExposure(api(2), context, full, true);
    assert.deepEqual(ledger.windows[0], iteration);
    assert.deepEqual([ledger.windows[1]!.start, ledger.windows[1]!.end, ledger.windows[1]!.receivedStart, ledger.windows[1]!.receivedEnd], ['48', '90', '50', '80']);
    await assert.rejects(checkExposure(api(3), context, full, true), /heldout_already_exposed/);
    assert.equal(ledger.windows.length, 2);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

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

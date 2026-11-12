import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { setup } from './support.ts';

const settings: JsonObject = { schema_version: 1, base_currency: 'USDC', starting_capital: '3000', account_kind: 'MARGIN', leverage: '1',
  exposure_tolerance: '0.000001', snapshot_interval_ms: 1000,
  fee_model: { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.69.0', parameters: {} },
  fill_model: { schema_version: 1, adapter_kind: 'NAUTILUS_DEFAULT_FILL', upstream_class: 'nautilus_execution::models::fill::DefaultFillModel', upstream_version: '0.69.0',
    parameters: { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '1' } },
  latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'nautilus_execution::models::latency::StaticLatencyModel', upstream_version: '0.69.0',
    parameters: { base_latency_ns: '1', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' } },
  fee_rates: [{ instrument_id: 'A.SIM', maker: '0', taker: '0' }] };
const request = { schema_version: 1, selection: { schema_version: 1, bar_types: ['A.SIM-1-MINUTE-LAST-EXTERNAL'], event_start_ns: '1790000000000000001', event_end_ns: '1790001000000000001', decision_cutoff_ns: '1790001000000000001' },
  settings, signal: { kind: 'EMA_CROSS', fast_period: 2, slow_period: 3 }, decision_start_ns: '1790000300000000001', long_weight: '0.5', target_ttl_seconds: 300 };

test('custom indicator single and batch bindings are rejected without non-equivalent translation', { timeout: 30_000 }, async () => {
  const state = await setup();
  try {
    const configured = state.options.config.datasets[0]!;
    const catalog = configured.data.catalog!;
    delete configured.strategy;
    async function start(data: JsonObject, requestId: string) {
      configured.data = { catalog, ...data }; await state.restart();
      return state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers,
        payload: { requestId, goal: 'Explicit native historical strategy', datasetId: configured.id, model: state.model } });
    }
    for (const [key, value] of Object.entries({ indicator_strategy_request: request, indicator_strategy_batch_request: { schema_version: 1, requests: [request] } })) {
      const rejected = await start({ [key]: value }, key);
      assert.equal(rejected.statusCode, 410, rejected.body);
      assert.match(rejected.body, /custom_indicator_strategy_removed/);
    }
  } finally { await state.cleanup(); }
});

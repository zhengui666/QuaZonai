import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { forecastMembers } from '../plugins/native-portfolio.ts';
import { resolveStrategyPolicy } from '../src/strategy.ts';

const model = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
const observation = (instrument_id: string, event_ns: string, available_ns: string, forecast = 1) => ({ instrument_id, event_ns, available_ns, forecast });
const forecasts = [
  observation('BASE.SIM', '9', '11'),
  observation('BASE.SIM', '10', '12'),
  observation('TEST.SIM', '19', '20'),
  observation('BASE.SIM', '19', '21', -1),
  observation('TEST.SIM', '20', '22'),
  observation('TEST.SIM', '19', '23', -1),
  observation('BASE.SIM', '29', '31'),
  observation('BASE.SIM', '29', '40', -1),
];

for (const partition of ['VALIDATION', 'HELDOUT'] as const) {
  test(`scoped ${partition} partitions observations by event while retaining permitted receipt clocks`, async () => {
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-partition-protocol-'));
    try {
      const log = join(directory, 'calls.jsonl'); const binary = join(directory, 'job');
      await writeFile(join(directory, 'binding.json'), JSON.stringify({ end: '30', forecast_points: forecasts }));
      await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('./portfolio-protocol.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
      const request: JsonObject = { schema_version: 1, train_end_ns: '10', validation_end_ns: '20', forecast: { schema_version: 1,
        selection: { schema_version: 1, bar_types: ['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'TEST.SIM-1-MINUTE-LAST-EXTERNAL'],
          event_start_ns: '0', event_end_ns: '30', decision_cutoff_ns: '40', maximum_rows: 50 }, parameters: {} } };
      const policy = resolveStrategyPolicy({ allocations: [{ instrumentId: 'BASE.SIM', weight: '0.4' }, { instrumentId: 'TEST.SIM', weight: '0.6' }] }, {
        name: 'event-partition-protocol', kind: 'cross-market-threshold', instrumentIds: ['BASE.SIM', 'TEST.SIM'],
        threshold: 0, investedWeight: '1', targetTtlNs: '50', forecastMode: 'price-volume', evaluationPartition: 'HELDOUT',
        simulationSettings: { starting_capital: '3000', base_currency: 'pUSD', fee_rates: [
          { instrument_id: 'BASE.SIM', maker: '0', taker: '0' }, { instrument_id: 'TEST.SIM', maker: '0', taker: '0' },
        ] },
      });
      const before = structuredClone(request);
      const result = await forecastMembers(binary, directory, [{ model, weight: '1' }], request, policy, 'pUSD', directory, BACKGROUND_CONTEXT, false, partition);
      assert.deepEqual(result.points.map((point) => point.asof_ns), partition === 'VALIDATION' ? ['12', '20'] : ['22', '31', '40']);
      assert.deepEqual(result.points.map((point) => point.rebalance_instrument_ids), partition === 'VALIDATION'
        ? [['BASE.SIM'], ['TEST.SIM']] : [['TEST.SIM'], ['BASE.SIM'], ['BASE.SIM']]);
      assert.deepEqual((result.points[0]!.targets as JsonObject[]).map((target) => target.weight), partition === 'VALIDATION'
        ? ['0.400000000000000000', '0.000000000000000000'] : ['0.000000000000000000', '0.600000000000000000']);
      assert.deepEqual((result.components[0]!.forecasts as JsonObject).points, forecasts);
      assert.deepEqual(request, before);
      const calls = (await readFile(log, 'utf8')).trim().split('\n').map((line) => JSON.parse(line) as JsonObject);
      assert.deepEqual(calls.map((call) => call.command), ['forecast-factor']);
      assert.deepEqual(calls[0]!.input, request.forecast);
    } finally { await rm(directory, { recursive: true, force: true }); }
  });
}

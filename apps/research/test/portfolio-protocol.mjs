// TEST ONLY: wire/ownership/isolation probe. No scientific calculations or market-performance evidence.
import assert from 'node:assert/strict';
import { appendFileSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
assert.equal(process.env.QZ_TEST_PROVIDER_SECRET, undefined);
const [log, command, ...args] = process.argv.slice(2);
if (command === 'catalog-factor-scope') {
  appendFileSync(log, JSON.stringify({ command, args }) + '\n');
  console.log(JSON.stringify(['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'TEST.SIM-1-MINUTE-LAST-EXTERNAL']));
  process.exit(0);
}
const input = JSON.parse(readFileSync(0, 'utf8'));
const catalog = args.includes('--catalog') ? args[args.indexOf('--catalog') + 1] : null;
const binding = catalog ? JSON.parse(readFileSync(join(catalog, 'binding.json'), 'utf8')) : null;
let prediction = null;
if (command !== 'replay-catalog-ensemble' && args.includes('--model')) {
  const module = new WebAssembly.Module(readFileSync(args[args.indexOf('--model') + 1]));
  prediction = new WebAssembly.Instance(module).exports.predict(1, 2, 3, 4, 5, 6, 7, 8);
}
const scope = args.includes('--scope') ? JSON.parse(readFileSync(args[args.indexOf('--scope') + 1], 'utf8')) : null;
appendFileSync(log, JSON.stringify({ command, args, input, binding, prediction, scope }) + '\n');
const selection = input.forecast?.selection ?? input.selection ?? input;
if (binding) assert.ok(BigInt(selection.event_end_ns) <= BigInt(binding.end), 'requested suffix is absent from this original catalog');
if (command === 'replay-catalog-ensemble') {
  assert.ok(args.includes('--partitions'));
  assert.deepEqual(scope, ['BASE.SIM-1-MINUTE-LAST-EXTERNAL', 'TEST.SIM-1-MINUTE-LAST-EXTERNAL']);
  assert.equal(input.settings.starting_capital, '3000');
  assert.equal(input.event_start_ns, '0');
  assert.ok(['10', '20'].includes(input.evaluation_start_ns));
  const modelPaths = args.flatMap((arg, index) => arg === '--model' ? [args[index + 1]] : []);
  assert.equal(modelPaths.length, input.members.length);
  for (const path of modelPaths) {
    const model = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(path))).exports;
    model.qz_set_feature_v2(0, 1, 0, 0n, 0n);
    assert.equal(model.qz_predict_v2(0n, 0n, 0, 0, 7), 1);
  }
  const final = input.evaluation_start_ns === '20';
  console.log(JSON.stringify({ schema_version: 1, source: input.source, revision: input.revision,
    catalog_series: '2', observed_markets: '2', bars: '18', source_records: '50', consumed_fuel: input.members.map(() => null),
    assumptions: ['TEST_ONLY_CONDITIONAL_PROXY_NOT_VERIFIED_PIT'],
    simulation: { native_version: 'TEST_ONLY', iterations: '1', events: '18', orders: '0', positions: '0', consumed_target_points: '4',
      statistics: [{ native_key: 'Sharpe Ratio (252 days)', value: binding?.account_replay_protocol ? 1 : null, reason_code: 'TEST_ONLY_NO_SCIENCE' }],
      summary: { protocol: final ? 'HELDOUT_OWNER_ONLY_PROTOCOL' : 'VALIDATION_PROTOCOL' },
      returns: [{ value: 12345.6789 }], canonical_result: { private_protocol_marker: 'CATALOG_RAW_ACCOUNT_OWNER_ONLY',
        ...(binding?.account_replay_protocol ? { portfolio_snapshots: [{ ts_event: input.evaluation_start_ns, total_equity: [`${input.settings.starting_capital} pUSD`] }] } : {}) } } }));
} else if (command === 'evaluate-factor') {
  assert.ok(args.includes('--summary-only'));
  const full = BigInt(selection.event_end_ns) > BigInt(input.validation_end_ns);
  console.log(JSON.stringify({ request: input, native_versions: { protocol: 'TEST_ONLY' }, consumed_fuel: null,
    diagnostic_prediction: prediction,
    partitions: ['TRAIN', 'VALIDATION', 'HELDOUT'].map((partition) => ({ partition, metric: {
      counts: { observations: partition === 'HELDOUT' && !full ? '0' : '2', included: partition === 'HELDOUT' && !full ? '0' : '2', excluded: '0', indicator_warmup: '0', incomplete_labels: '0', cross_boundary_labels: '0', prediction_outside_partition: '0' },
      pearson_ic: { value: null, status: 'TEST_ONLY_NO_SCIENCE' },
    }, assets: Array.from({ length: binding?.diagnostic_assets ?? 0 }, (_, index) => ({
      instrument_id: `${String(index).padStart(76, '0')}.SIM`,
      metric: { counts: { observations: '2', included: '2', excluded: '0', indicator_warmup: '0', incomplete_labels: '0', cross_boundary_labels: '0', prediction_outside_partition: '0' }, pearson_ic: { value: null, status: 'TEST_ONLY_NO_SCIENCE' } },
    })) })) }));
} else if (command === 'forecast-factor' || command === 'forecast') {
  assert.equal(selection.event_start_ns, '0', 'forecast must retain its warmup prefix');
  const clocks = binding?.cross_market ? [
    ...['1', '10', '15', '20', '25', '29'].map((at) => ({ instrument_id: 'BASE.SIM', at })),
    ...['2', '11', '15', '21', '26', '29'].map((at) => ({ instrument_id: 'TEST.SIM', at })),
  ] : ['1', '9', '10', '12', '15', '19', '20', '23', '29'].map((at) => ({ instrument_id: 'TEST.SIM', at }));
  const points = binding?.forecast_points ?? clocks.map(({ at, instrument_id }) => ({ instrument_id, event_ns: at, available_ns: at,
    forecast: at === '15' ? null : prediction, label_return: 12345.6789, test_raw_marker: 'RAW_FORECAST_AND_LABELS_PROTOCOL_ONLY' }));
  console.log(JSON.stringify({ native_versions: { protocol: 'TEST_ONLY' }, points: points
    .filter(({ event_ns, available_ns, instrument_id }) => selection.bar_types.some((bar) => bar.startsWith(instrument_id + '-'))
      && BigInt(event_ns) < BigInt(selection.event_end_ns) && BigInt(available_ns) <= BigInt(selection.decision_cutoff_ns)) }));
} else if (command === 'blend-targets') {
  // Deliberately no alternative numerical blend engine: echo one row only to test orchestration/provenance.
  assert.equal(input.rows[0].length, input.weights.length);
  console.log(JSON.stringify({ target_points: input.rows.map((row) => row[0]), protocol: 'TEST_ONLY_NOT_A_NUMERICAL_BLEND' }));
} else if (command === 'simulate') {
  assert.ok(['10', '20'].includes(selection.event_start_ns), 'never simulate TRAIN');
  assert.equal(input.settings.starting_capital, '3000');
  for (const point of input.target_points) assert.ok(BigInt(point.asof_ns) >= BigInt(selection.event_start_ns) && BigInt(point.asof_ns) < BigInt(selection.event_end_ns));
  if (selection.event_start_ns === '10') assert.equal(selection.decision_cutoff_ns, '20');
  console.log(JSON.stringify({ native_version: 'TEST_ONLY', iterations: '1', events: '2', orders: '0', positions: '0', consumed_target_points: String(input.target_points.length),
    statistics: [{ native_key: 'Sharpe Ratio (252 days)', value: null, reason_code: 'TEST_ONLY_NO_SCIENCE' }],
    summary: { protocol: selection.event_start_ns === '20' ? 'HELDOUT_OWNER_ONLY_PROTOCOL' : 'VALIDATION_PROTOCOL' },
    returns: [{ value: 12345.6789 }], canonical_result: { portfolio_snapshots: [{ ts_event: selection.event_start_ns, total_equity: ['3000 USD'] }] } }));
} else throw new Error('unsupported_test_command');

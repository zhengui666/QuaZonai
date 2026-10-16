// TEST ONLY: native wire protocol probe, never a backtest or a production scientific module.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
assert.equal(process.env.QZ_TEST_PROVIDER_SECRET, undefined);
const input = JSON.parse(readFileSync(0, 'utf8'));
const [command, ...args] = process.argv.slice(2);
const at = (BigInt(Date.now()) * 1_000_000n).toString();
if (command === 'forecast') {
  const module = new WebAssembly.Module(readFileSync(args[args.indexOf('--model') + 1]));
  const model = new WebAssembly.Instance(module).exports.predict;
  console.log(JSON.stringify({ native_versions: { protocol: 'TEST_ONLY' }, points: [{ instrument_id: 'TEST.SIM', event_ns: at, available_ns: at, forecast: model(0, 0, 0, 0, 0, 0, 0, 0) }] }));
} else if (command === 'simulate') {
  assert.equal(input.target_points[0].targets[0].weight, '0.250000000000000001');
  console.log(JSON.stringify({ native_version: 'TEST_ONLY', statistics: [{ native_key: 'unavailable_test_metric', value: null }], canonical_result: { portfolio_snapshots: [{ ts_event: at, total_equity: [`${input.settings.starting_capital} USD`] }] } }));
} else process.exit(1);

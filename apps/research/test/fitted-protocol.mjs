// TEST ONLY: native wire/causality probe. These numbers are not fitted science or market evidence.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
const [log, command, ...args] = process.argv.slice(2);
const input = JSON.parse(readFileSync(0, 'utf8'));
appendFileSync(log, JSON.stringify({ command, input, args }) + '\n');
if (command === 'catalog-prefix-digest') {
  const content = process.env.QZ_TEST_PREFIX_FILE ? readFileSync(process.env.QZ_TEST_PREFIX_FILE, 'utf8') : 'controlled protocol prefix';
  const hash = createHash('sha256').update(content).digest('hex');
  console.log(JSON.stringify({ schema_version: 1, method: 'NAUTILUS_SELECTED_PREFIX_SHA256_V2', selection: input, series_count: 1, row_count: '3', content_sha256: hash, series: [{ instrument_id: 'TEST.SIM', bar_type: input.bar_types[0], row_count: '3', instrument_version_count: 1, content_sha256: hash }] }));
} else if (command === 'evaluate-factor') {
  if (process.env.QZ_TEST_PREFIX_MUTATE_DURING_FIT && !args.includes('--summary-only')) writeFileSync(process.env.QZ_TEST_PREFIX_FILE, 'changed while reading full panels');
  const report = { request: input, partitions: [{ partition: 'VALIDATION', metric: { pearson_ic: { status: 'AVAILABLE', value: 0.1 } } }] };
  if (!args.includes('--summary-only')) report.points = [{ protocol: 'RAW_TRAIN_PANEL_NOT_MODEL_VISIBLE' }];
  console.log(JSON.stringify(report));
} else if (command === 'fit-ensemble') {
  assert.equal(input.fit_end_ns, '10'); assert.equal(input.members.length, 2);
  for (const member of input.members) { assert.ok(member.report.points.length); assert.match(member.source_evidence_ref, /^native-task:.*:panel:/); }
  for (const fold of input.folds) assert.ok(BigInt(fold.validation_end_ns) <= 10n);
  console.log(JSON.stringify({ schema_version: 1, method: 'TEST_ONLY_PROTOCOL', fit_end_ns: '10', forecast_contract: input.members[0].report.request.forecast,
    members: input.members.map((member, index) => ({ factor_version_id: member.factor_version_id, source_evidence_ref: member.source_evidence_ref, weight: index === 0 ? '0' : '1', calibrations: [{ instrument_id: 'TEST.SIM', intercept: -100, slope: 1, training_observations: '3' }] })) }));
} else if (command === 'forecast-factor' || command === 'forecast') {
  const model = new WebAssembly.Instance(new WebAssembly.Module(readFileSync(args[args.indexOf('--model') + 1])));
  const score = model.exports.predict(1, 2, 3, 4, 5, 6, 7, 8);
  console.log(JSON.stringify({ points: ['5', '10', '15', '20', '25'].filter(at => BigInt(at) < BigInt(input.selection.event_end_ns)).map(at => ({ instrument_id: 'TEST.SIM', event_ns: at, available_ns: at, forecast: score })) }));
} else if (command === 'apply-ensemble') {
  assert.deepEqual(input.fit.members.map(member => member.weight), ['0', '1']);
  assert.ok(input.members.every(member => member.forecast.points.every(point => point.forecast > 0)));
  console.log(JSON.stringify({ schema_version: 1, forecast: { points: input.members[0].forecast.points.filter(point => BigInt(point.event_ns) >= 10n).map(point => ({ ...point, forecast: -0.4 })) } }));
} else if (command === 'simulate') {
  if (input.settings.multi_currency_spot_cash != null) {
    assert.equal(input.settings.multi_currency_spot_cash.mode, 'MULTI_CURRENCY_CASH');
    assert.equal(input.settings.account_kind, 'CASH');
    assert.match(args[args.indexOf('--dataset-revision-id') + 1] ?? '', /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
  }
  if (process.env.QZ_TEST_SIMULATE_PAUSE_FILE && existsSync(process.env.QZ_TEST_SIMULATE_PAUSE_FILE)) await new Promise(() => { setInterval(() => {}, 1000); });
  assert.ok(input.target_points.length > 0);
  assert.ok(input.target_points.every(point => point.targets.every(target => target.weight === '0')), 'must apply frozen calibration before threshold instead of blending positive raw scores');
  console.log(JSON.stringify({ native_version: 'TEST_ONLY', statistics: [], returns_status: 'UNAVAILABLE', summary: { protocolOnly: true },
    ...(input.settings.multi_currency_spot_cash ? { spot_cash_report: { statistics: [{ native_key: 'Calendar Geometric Daily Return', value: 0.01, status: 'AVAILABLE' }],
      daily_returns: { kind: 'REPORT_CURRENCY_DAILY', report_currency: input.settings.base_currency, days: [{ value: 0.01 }] }, fee_acceptance: { fixtureOnly: true } } } : {}) }));
} else throw new Error(`unsupported protocol command: ${command}`);

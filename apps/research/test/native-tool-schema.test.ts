import assert from 'node:assert/strict';
import test from 'node:test';
import { readFile } from 'node:fs/promises';
import { compile, type Schema } from '@earendil-works/pi-schema';
import { nativeToolSchema } from '../src/native-tool-schema.ts';
import { nativeStrategyConfigureTool, nativeStrategyCatalogTool, nativeParameterGridTool, nativeParameterBatchTool, nativeConfiguredConfigureTool, nativeConfiguredBacktestTool, nativeStrategyRequestSchema } from '../plugins/native-strategy-backtest.ts';
import { nativeMarketImportTool, nativeFillObservationImportTool, nativeRunnerMarketImportTool } from '../plugins/native-market-import.ts';

const request = {
  schema_version: 1, instrument_id: 'BTC-USDC.HYPERLIQUID',
  selectors: [{ data_type: 'OrderBookDelta', identifier: 'BTC-USDC.HYPERLIQUID' }],
  event_start_ns: '0', event_end_ns: '9223372036854775807', available_end_ns: '9223372036854775807',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L2_MBP', starting_balances: ['5000 USDC'], base_currency: null, leverage: '1', maker_fee: '0.00015', taker_fee: '0.00045' },
  strategy: { kind: 'ORDER_BOOK_IMBALANCE', max_trade_size: '0.001', trigger_min_size: '0.002', trigger_imbalance_ratio: '0.2', min_seconds_between_triggers: 1, time_in_force: 'IOC', dry_run: false },
};
const grid = (value: typeof request) => ({ configured: false, request: value, axes: [{ path: ['venue', 'leverage'], values: ['1'] }], offset: 0, pageSize: 1 });
const patternList = (value: unknown): string[] => {
  if (Array.isArray(value)) return value.flatMap(patternList);
  if (!value || typeof value !== 'object') return [];
  return Object.entries(value).flatMap(([key, child]) => key === 'pattern' && typeof child === 'string' ? [child] : patternList(child));
};

test('native tool parameters expose provider-compatible scalar endings without changing original validation schemas', () => {
  const original = JSON.stringify(nativeStrategyRequestSchema);
  const tools = [nativeStrategyConfigureTool, nativeStrategyCatalogTool, nativeParameterGridTool, nativeParameterBatchTool, nativeConfiguredConfigureTool, nativeConfiguredBacktestTool, nativeMarketImportTool, nativeFillObservationImportTool,
    nativeRunnerMarketImportTool(async () => { throw new Error('not executed'); })];
  for (const tool of tools) {
    for (const pattern of patternList(tool.parameters)) assert.doesNotMatch(pattern, /\(\?[=!<]/, tool.name);
  }
  assert.ok(patternList(nativeStrategyRequestSchema).some(pattern => pattern.includes('(?!')));
  assert.equal(JSON.stringify(nativeStrategyRequestSchema), original);
  assert.equal(compile(nativeToolSchema(nativeStrategyRequestSchema)).check(request), true);
  assert.equal(compile(nativeStrategyRequestSchema).check(request), true);
});

test('projection only changes schema patterns with the exact generated suffix, not opaque examples or descriptions', () => {
  const pattern = String.raw`^[0-9]+(?![\s\S])`;
  const schema = { type: 'object', description: pattern, properties: { amount: { type: 'string', pattern, examples: [{ pattern }] } }, additionalProperties: { type: 'string', pattern }, examples: [{ pattern }], default: { pattern } } as unknown as Schema<unknown>;
  const projected = nativeToolSchema(schema) as any;
  assert.equal(projected.properties.amount.pattern, '^[0-9]+$');
  assert.equal(projected.additionalProperties.pattern, '^[0-9]+$');
  assert.equal(projected.description, pattern);
  assert.deepEqual(projected.examples, [{ pattern }]);
  assert.deepEqual(projected.default, { pattern });
  assert.deepEqual(projected.properties.amount.examples, [{ pattern }]);
  assert.equal((schema as any).properties.amount.pattern, pattern);
  const unrelated = { type: 'string', pattern: '^(?!forbidden).*' } as Schema<string>;
  assert.deepEqual(nativeToolSchema(unrelated), unrelated);
});

test('actual configure/grid/batch prepareArguments still reject line endings and exact numeric overflows', () => {
  const prepare = (value: typeof request) => {
    nativeStrategyConfigureTool.prepareArguments!({ request: value });
    nativeParameterGridTool.prepareArguments!(grid(value));
    nativeParameterBatchTool.prepareArguments!({ configured: false, requests: [value] });
  };
  prepare(request);
  for (const suffix of ['\n', '\r', '\u2028', '\u2029']) {
    for (const value of [{ ...request, event_start_ns: `1${suffix}` }, { ...request, available_end_ns: `1${suffix}` }]) {
      assert.throws(() => nativeStrategyConfigureTool.prepareArguments!({ request: value }), /schema_invalid/);
      assert.throws(() => nativeParameterGridTool.prepareArguments!(grid(value)), /schema_invalid/);
      assert.throws(() => nativeParameterBatchTool.prepareArguments!({ configured: false, requests: [value] }), /schema_invalid/);
    }
  }
  for (const event_start_ns of ['-1', '9223372036854775808', '01']) assert.throws(() => prepare({ ...request, event_start_ns }), /schema_invalid/);
  // The current pi-schema runtime also rejects this projected pattern; the original
  // argument boundary remains authoritative regardless of provider regex semantics.
  assert.equal(compile(nativeToolSchema(nativeStrategyRequestSchema)).check({ ...request, event_start_ns: '1\n' }), false);
  assert.throws(() => prepare({ ...request, event_start_ns: '1\n' }), /schema_invalid/);
});


test('scalar projection preserves numeric bounds while original decimal validators reject all line endings', async () => {
  const schemas = JSON.parse(await readFile(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8')).components.schemas;
  const original = compile(schemas.DecimalValue);
  const projected = compile(nativeToolSchema(schemas.DecimalValue));
  for (const amount of ['0', '-0', '99999999999999999999.999999999999999999', '-99999999999999999999.999999999999999999', '0.000000000000000001']) {
    assert.equal(original.check(amount), true);
    assert.equal(projected.check(amount), true);
  }
  for (const amount of ['100000000000000000000', '-100000000000000000000', '0.0000000000000000001']) {
    assert.equal(original.check(amount), false);
    assert.equal(projected.check(amount), false);
  }
  for (const suffix of ['\n', '\r', '\u2028', '\u2029']) assert.equal(original.check(`1${suffix}`), false);
});

test('catalog tool preparation retains strict original cursor counter validation', () => {
  const cursor = { offset: '0', total_items: '9223372036854775807', instrument_ids: [], data_types: [] };
  nativeStrategyCatalogTool.prepareArguments!({ cursor });
  for (const offset of ['-1', '9223372036854775808', '1\n', '1\r', '1\u2028', '1\u2029']) {
    assert.throws(() => nativeStrategyCatalogTool.prepareArguments!({ cursor: { ...cursor, offset } }), /native_catalog_query_schema_invalid/);
  }
});

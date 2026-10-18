import assert from 'node:assert/strict';
import { constants } from 'node:buffer';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { executeNative } from '../plugins/native-portfolio.ts';

// Transport-only process: no catalog, Wasm inference, market simulation or owner trial.
async function transportProcess(directory: string) {
  const binary = join(directory, 'job.mjs');
  await writeFile(binary, `#!${process.execPath}
const command = process.argv[2];
if (command === 'simulate') {
  let bytes = 0; let prefix = ''; let suffix = '';
  for await (const chunk of process.stdin) {
    bytes += chunk.length;
    if (prefix.length < 32) prefix = (prefix + chunk.toString()).slice(0, 32);
    suffix = (suffix + chunk.toString()).slice(-32);
  }
  process.stdout.write(JSON.stringify({ bytes, prefix, suffix }));
} else {
  for await (const chunk of process.stdin) {}
  const count = Number(process.argv[3]);
  const row = JSON.stringify({ marker: 'x'.repeat(16384), clock: '1791849600000000000' });
  async function write(text) {
    if (!process.stdout.write(text)) await new Promise(resolve => process.stdout.once('drain', resolve));
  }
  await write('{"points":[');
  for (let i = 0; i < count; i++) await write((i ? ',' : '') + row);
  await write('],"complete":"original-tail"}');
}
`, { mode: 0o700 });
  return binary;
}

const scratch = () => mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-large-json-'));

test('native transport sends the complete 64-market vector beyond one V8 string without truncation', async () => {
  const directory = await scratch();
  try {
    const binary = await transportProcess(directory);
    const frame = { schema_version: 1, asof_ns: '1782864000000000001', valid_until_ns: '1782950400000000001', cash_weight: '1.000000000000000000',
      targets: Array.from({ length: 64 }, (_, index) => ({ instrument_id: `${String(index).padStart(77, '0')}.POLYMARKET`, currency: 'pUSD', weight: '0.000000000000000000' })),
      rebalance_instrument_ids: ['0'.repeat(77) + '.POLYMARKET'] };
    const encoded = JSON.stringify(frame);
    const count = Math.ceil(constants.MAX_STRING_LENGTH / (encoded.length + 1)) + 2;
    const input: JsonObject = { target_points: Array(count).fill(frame), original_tail: 'unchanged' };
    const expected = Buffer.byteLength('{"target_points":[') + count * Buffer.byteLength(encoded) + count - 1 + Buffer.byteLength('],"original_tail":"unchanged"}');
    assert.ok(expected > constants.MAX_STRING_LENGTH);
    const result = await executeNative(binary, 'simulate', input, [], directory, BACKGROUND_CONTEXT);
    assert.equal(result.bytes, expected);
    assert.equal(result.prefix, ('{"target_points":[' + encoded).slice(0, 32));
    assert.equal(result.suffix, (encoded + '],"original_tail":"unchanged"}').slice(-32));
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('native transport reads complete JSON output beyond one V8 string and retains the final record', async () => {
  const directory = await scratch();
  try {
    const binary = await transportProcess(directory);
    const row = JSON.stringify({ marker: 'x'.repeat(16384), clock: '1791849600000000000' });
    const count = Math.ceil(constants.MAX_STRING_LENGTH / (row.length + 1)) + 2;
    const result = await executeNative(binary, 'forecast-factor', {}, [String(count)], directory, BACKGROUND_CONTEXT);
    const points = result.points as JsonObject[];
    assert.equal(points.length, count);
    assert.deepEqual(points[0], { marker: 'x'.repeat(16384), clock: '1791849600000000000' });
    assert.deepEqual(points.at(-1), points[0]);
    assert.equal(result.complete, 'original-tail');
  } finally { await rm(directory, { recursive: true, force: true }); }
});

import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { JsonReader, jsonChunks } from '../src/native-json.ts';

test('streamed native JSON matches standard JSON spelling and complete parsed values', () => {
  const value: JsonObject = { schema_version: 1, exact: '0.000000000000000001', clock: '1791849600000000000',
    'escaped"key': '原生🙂\n\t\\"\ud800', values: [null, true, false, 0, -0, 1.2345678901234567, 1e-20], nested: [{ a: 1 }, {}] };
  const text = JSON.stringify(value);
  assert.equal([...jsonChunks(value, 7)].join(''), text);
  const reader = new JsonReader();
  for (const character of ' \n' + text + '\t ') reader.write(character);
  assert.deepEqual(reader.finish(), JSON.parse(text));
});

test('streamed native JSON preserves standard missing object values and sparse array nulls', () => {
  const value = { present: 1, omitted: undefined, values: [undefined, , 3], nonfinite: NaN } as unknown as JsonObject;
  assert.equal([...jsonChunks(value, 1)].join(''), JSON.stringify(value));
});

for (const text of ['', '{"a":', '{"a":1}junk', '{} {}', '1e', '{"a":"unterminated']) {
  test(`streamed native JSON refuses incomplete or trailing content ${JSON.stringify(text)}`, () => {
    const reader = new JsonReader();
    assert.throws(() => { reader.write(text); reader.finish(); });
  });
}

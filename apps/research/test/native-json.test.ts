import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { JsonReader, jsonChunks } from '../src/native-json.ts';

// Independent pre-optimization encoder, including its non-JSON edge behavior.
function* originalFragments(value: JsonObject[string]): Generator<string> {
  if (value === null || typeof value !== 'object') { yield JSON.stringify(value) ?? 'null'; return; }
  if (Array.isArray(value)) {
    yield '[';
    for (let index = 0; index < value.length; index++) {
      if (index) yield ',';
      yield* originalFragments(value[index]!);
    }
    yield ']';
  } else {
    yield '{'; let first = true;
    for (const key of Object.keys(value)) {
      if (value[key] === undefined) continue;
      if (!first) yield ',';
      first = false;
      yield JSON.stringify(key); yield ':'; yield* originalFragments(value[key]!);
    }
    yield '}';
  }
}
function* originalChunks(value: JsonObject[string], chunkSize: number): Generator<string> {
  let pending = '';
  for (const fragment of originalFragments(value)) {
    if (pending && pending.length + fragment.length > chunkSize) { yield pending; pending = ''; }
    pending += fragment;
  }
  if (pending) yield pending;
}
function consume(iterator: Generator<string>) {
  const chunks: string[] = [];
  try { for (const chunk of iterator) chunks.push(chunk); return { chunks, error: undefined }; }
  catch (error) { return { chunks, error }; }
}

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

test('iterative native JSON keeps original chunk boundaries and standard bytes for large nested evidence', () => {
  const shared = { amount: '3000.000000000000000001', clock: '1791849600000000001', text: '原生🐋\ud800' };
  const rows = Array.from({ length: 4096 }, (_, index) => ({ index, snapshot: { balances: [{ total: shared, free: shared }],
    events: [{ metadata: { source: { native: { value: [index, null, true, -0, 1e-20] } } } }] } }));
  let nested: JsonObject = { rows };
  for (let depth = 0; depth < 32; depth++) nested = { original: nested };
  const value: JsonObject = { '10': 'ten', '2': 'two', nested, scalar: 'x'.repeat(65_535) + '🐋',
    keyOrder: { z: 1, a: 2 }, nonfinite: [NaN, Infinity, -Infinity] };
  const expected = JSON.stringify(value);
  for (const chunkSize of [7, 64 * 1024, 1024 * 1024]) {
    const actual = [...jsonChunks(value, chunkSize)];
    assert.deepEqual(actual, [...originalChunks(value, chunkSize)]);
    assert.equal(actual.join(''), expected);
  }
});

test('iterative native JSON preserves legacy handling outside the plain JSON type', () => {
  const sparse = new Array(4); sparse[2] = undefined; sparse[3] = 7;
  const values = [undefined, Symbol('missing'), () => 1,
    { omitted: undefined, symbol: Symbol('value'), function: () => 1, sparse, nonfinite: NaN },
    { toJSON: () => { throw new Error('legacy encoder does not invoke toJSON'); } }, new Date(0)];
  for (const value of values) {
    const input = value as unknown as JsonObject;
    assert.deepEqual([...jsonChunks(input, 3)], [...originalChunks(input, 3)]);
  }
});

test('chunk pulls preserve one-token lookahead, getter order and early consumer cancellation', () => {
  function fixture() {
    const reads: string[] = [];
    const input = { head: 'x'.repeat(30), get tail() { reads.push('tail'); return { value: '原值🐋' }; } };
    return { reads, input };
  }
  const actual = fixture(); const expected = fixture();
  const a = jsonChunks(actual.input, 8); const b = originalChunks(expected.input, 8);
  for (let index = 0; index < 1; index++) {
    assert.deepEqual(a.next(), b.next()); assert.deepEqual(actual.reads, expected.reads);
  }
  assert.deepEqual(actual.reads, [], 'consumer backpressure must not inspect the tail early');
  a.return(undefined); b.return(undefined);
  assert.deepEqual(actual.reads, [], 'closing an iterator must not traverse the remaining evidence');

  const complete = fixture(); const reference = fixture();
  assert.deepEqual([...jsonChunks(complete.input, 1)], [...originalChunks(reference.input, 1)]);
  assert.deepEqual(complete.reads, reference.reads);
});

test('late BigInt and getter failures retain the original prefix and error without a completed JSON value', () => {
  const failure = new Error('original-late-getter-failure');
  const fixtures = [
    () => ({ head: 'x'.repeat(128), tail: 1n }),
    () => { let calls = 0; return { head: 'x'.repeat(128), get tail() { if (++calls === 2) throw failure; return true; } }; },
  ];
  for (const fixture of fixtures) for (const size of [1, 32, 1024]) {
    const actual = consume(jsonChunks(fixture() as unknown as JsonObject, size));
    const expected = consume(originalChunks(fixture() as unknown as JsonObject, size));
    assert.deepEqual(actual.chunks, expected.chunks);
    assert.ok(actual.error instanceof Error && expected.error instanceof Error);
    assert.equal(actual.error.constructor, expected.error.constructor);
    assert.equal(actual.error.message, expected.error.message);
    if (actual.error === failure) assert.equal(expected.error, failure);
  }
});

test('cycles remain errors and shared references do not become cycles', () => {
  const object: JsonObject = {}; object.self = object;
  const array: JsonObject[string][] = []; array.push(array);
  for (const value of [object, array]) {
    assert.throws(() => [...originalChunks(value, 64 * 1024)], RangeError);
    assert.throws(() => [...jsonChunks(value, 64 * 1024)], RangeError);
  }
  const shared = { exact: '0.000000000000000001' };
  assert.equal([...jsonChunks({ a: shared, b: shared })].join(''), JSON.stringify({ a: shared, b: shared }));
});

test('finite dynamic getters can revisit an ancestor with the original reads and bytes', () => {
  function fixture(limit: number) {
    let reads = 0;
    const value: JsonObject = {};
    Object.defineProperty(value, 'self', { enumerable: true, configurable: true, get() { return ++reads <= limit ? value : undefined; } });
    return { value, reads: () => reads };
  }
  for (const limit of [0, 1, 2, 3, 8, 127, 128]) for (const size of [1, 7, 64 * 1024]) {
    const actual = fixture(limit); const expected = fixture(limit);
    const chunks = [...jsonChunks(actual.value, size)];
    assert.deepEqual(chunks, [...originalChunks(expected.value, size)]);
    assert.equal(actual.reads(), expected.reads());
    if (limit === 2) { assert.equal(chunks.join(''), '{"self":{}}'); assert.equal(actual.reads(), 3); }
  }
});

test('consumer pauses can change an ancestor getter or array before its next traversal step', () => {
  function objectFixture() {
    let enabled = true; let reads = 0;
    const value: JsonObject = {};
    Object.defineProperty(value, 'self', { enumerable: true, configurable: true, get() { reads++; return enabled ? value : undefined; } });
    return { value, reads: () => reads, stop: () => { enabled = false; } };
  }
  function arrayFixture() {
    let reads = 0; const value: JsonObject[string][] = [];
    Object.defineProperty(value, '0', { get() { reads++; return value; }, configurable: true, enumerable: true });
    return { value, reads: () => reads, stop: () => { value.length = 0; } };
  }
  for (const [factory, pulls, text] of [[objectFixture, 3, '{"self":{}}'], [arrayFixture, 1, '[[]]']] as const) {
    const actual = factory(); const expected = factory();
    const a = jsonChunks(actual.value, 1); const b = originalChunks(expected.value, 1); const prefix: string[] = [];
    for (let index = 0; index < pulls; index++) {
      const step = a.next(); assert.deepEqual(step, b.next()); assert.equal(actual.reads(), expected.reads());
      assert.equal(step.done, false); prefix.push(step.value!);
    }
    actual.stop(); expected.stop();
    const rest = [...a]; assert.deepEqual(rest, [...b]); assert.equal(actual.reads(), expected.reads());
    assert.equal([...prefix, ...rest].join(''), text);
  }
});

test('ancestor traversal keeps late getter failures and stops reading when the consumer closes', () => {
  const failure = new Error('original-ancestor-getter-failure');
  function fixture() {
    let reads = 0;
    const value: JsonObject = { get self() { if (++reads > 2) throw failure; return value; } };
    return { value, reads: () => reads };
  }
  for (const close of [false, true]) {
    const actual = fixture(); const expected = fixture();
    const a = jsonChunks(actual.value, 1); const b = originalChunks(expected.value, 1);
    for (let index = 0; index < 3; index++) { assert.deepEqual(a.next(), b.next()); assert.equal(actual.reads(), expected.reads()); }
    if (close) { a.return(undefined); b.return(undefined); assert.equal(actual.reads(), 2); }
    else { assert.deepEqual(consume(a), consume(b)); assert.equal(actual.reads(), 3); }
    assert.equal(actual.reads(), expected.reads());
  }
});

test('array mutations observed during traversal keep the original stopping condition', () => {
  function fixture() {
    const values = [1, 2, 3];
    Object.defineProperty(values, '0', { get() { values.length = 1; return 9; }, enumerable: true });
    return values;
  }
  assert.deepEqual([...jsonChunks(fixture(), 1)], [...originalChunks(fixture(), 1)]);
  const nonfiniteLength = new Proxy([1, 2, 3], { get(target, key, receiver) {
    return key === 'length' ? NaN : Reflect.get(target, key, receiver);
  } });
  assert.deepEqual([...jsonChunks(nonfiniteLength)], [...originalChunks(nonfiniteLength, 64 * 1024)]);
});

for (const text of ['', '{"a":', '{"a":1}junk', '{} {}', '1e', '{"a":"unterminated']) {
  test(`streamed native JSON refuses incomplete or trailing content ${JSON.stringify(text)}`, () => {
    const reader = new JsonReader();
    assert.throws(() => { reader.write(text); reader.finish(); });
  });
}

import { createHash } from 'node:crypto';
import type { NativeJson } from '../../../packages/contracts/src/nautilus.ts';
import { JSONParser } from '@streamparser/json';
import type { JsonObject } from '@earendil-works/pi-durable';

type JsonValue = JsonObject[string];

/** SHA-256 of the original sorted-key native wire JSON, without a whole-request string. */
export function canonicalJsonDigest(value: NativeJson): string {
  const hash = createHash('sha256'); let pending = '';
  function write(fragment: string) {
    // Keep complete JSON tokens together, including surrogate pairs. The hash
    // consumes the original canonical bytes without an additional whole-request string.
    if (pending.length + fragment.length > 64 * 1024) { hash.update(pending); pending = ''; }
    if (fragment.length >= 64 * 1024) hash.update(fragment);
    else pending += fragment;
  }
  function visit(item: NativeJson) {
    if (item === null || typeof item !== 'object') { write(JSON.stringify(item)); return; }
    if (Array.isArray(item)) {
      write('[');
      for (let index = 0; index < item.length; index++) {
        if (index) write(',');
        visit(item[index]!);
      }
      write(']');
    } else {
      write('{'); let first = true;
      for (const key of Object.keys(item).sort()) {
        if (!first) write(','); first = false;
        write(JSON.stringify(key)); write(':'); visit(item[key]!);
      }
      write('}');
    }
  }
  visit(value);
  if (pending) hash.update(pending);
  return hash.digest('hex');
}
/** Same JSON values and property order, without creating a whole-result string. */
function* fragments(value: JsonValue): Generator<string> {
  if (value === null || typeof value !== 'object') { yield JSON.stringify(value) ?? 'null'; return; }
  if (Array.isArray(value)) {
    yield '[';
    for (let index = 0; index < value.length; index++) {
      if (index) yield ',';
      yield* fragments(value[index]!);
    }
    yield ']';
  } else {
    yield '{'; let first = true;
    for (const key of Object.keys(value)) {
      if (value[key] === undefined) continue;
      if (!first) yield ',';
      first = false;
      yield JSON.stringify(key); yield ':'; yield* fragments(value[key]!);
    }
    yield '}';
  }
}

export function* jsonChunks(value: JsonValue, chunkSize = 64 * 1024): Generator<string> {
  let pending = '';
  for (const fragment of fragments(value)) {
    if (pending && pending.length + fragment.length > chunkSize) { yield pending; pending = ''; }
    pending += fragment;
  }
  if (pending) yield pending;
}

/** Complete native JSON, including values split across process-output chunks. */
export class JsonReader {
  private parser = new JSONParser({ paths: ['$'], keepStack: true });
  private value: JsonValue | undefined;
  private count = 0;
  constructor() {
    this.parser.onValue = ({ value }) => {
      if (this.count++) throw new SyntaxError('native_json_multiple_values');
      this.value = value as JsonValue;
    };
  }
  write(text: string) { this.parser.write(text); }
  finish(): JsonValue {
    if (!this.parser.isEnded) this.parser.end();
    if (this.count !== 1) throw new SyntaxError('native_json_value_missing');
    return this.value!;
  }
}

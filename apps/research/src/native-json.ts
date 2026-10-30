import { createHash } from 'node:crypto';
import type { NativeJson } from '../../../packages/contracts/src/nautilus.ts';
import { JSONParser } from '@streamparser/json';
import type { JsonObject } from '@earendil-works/pi-durable';

type JsonValue = JsonObject[string];

/** Batch complete JSON tokens before calling the native UTF-8 SHA-256 writer. */
export function createBufferedJsonHash() {
  const hash = createHash('sha256'); let pending = '';
  return {
    update(fragment: string) {
      // The target is 64 Ki UTF-16 code units, not UTF-8 bytes. Never split a
      // token's surrogate pair; a larger scalar goes directly to the hash.
      if (pending && pending.length + fragment.length > 64 * 1024) { hash.update(pending); pending = ''; }
      if (fragment.length >= 64 * 1024) hash.update(fragment);
      else pending += fragment;
    },
    digest() { if (pending) hash.update(pending); return hash.digest('hex'); },
  };
}

/** SHA-256 of the original sorted-key native wire JSON, without a whole-request string. */
export function canonicalJsonDigest(value: NativeJson): string {
  const hash = createBufferedJsonHash();
  function visit(item: NativeJson) {
    if (item === null || typeof item !== 'object') { hash.update(JSON.stringify(item)); return; }
    if (Array.isArray(item)) {
      hash.update('[');
      for (let index = 0; index < item.length; index++) {
        if (index) hash.update(',');
        visit(item[index]!);
      }
      hash.update(']');
    } else {
      hash.update('{'); let first = true;
      for (const key of Object.keys(item).sort()) {
        if (!first) hash.update(','); first = false;
        hash.update(JSON.stringify(key)); hash.update(':'); visit(item[key]!);
      }
      hash.update('}');
    }
  }
  visit(value);
  return hash.digest();
}

type JsonFrame =
  // state 0 selects an entry; 1 reads an array value or emits an object key;
  // object states 2 and 3 emit ':' and then read its value, respectively.
  | { array: true; value: JsonValue[]; index: number; state: number }
  | { array: false; value: JsonObject; keys: string[] | null; index: number; state: number; first: boolean };

// A getter or consumer mutation can make an ancestor visit finite. Retain the
// original token traversal for this rare case instead of declaring it a cycle.
function* ancestorTokens(value: JsonValue): Generator<string> {
  if (value === null || typeof value !== 'object') { yield JSON.stringify(value) ?? 'null'; return; }
  if (Array.isArray(value)) {
    yield '[';
    for (let index = 0; index < value.length; index++) {
      if (index) yield ',';
      yield* ancestorTokens(value[index]!);
    }
    yield ']';
  } else {
    yield '{'; let first = true;
    for (const key of Object.keys(value)) {
      if (value[key] === undefined) continue;
      if (!first) yield ',';
      first = false;
      yield JSON.stringify(key); yield ':'; yield* ancestorTokens(value[key]!);
    }
    yield '}';
  }
}

/** Same JSON tokens and chunk boundaries, with one frame per active container.
 * chunkSize counts UTF-16 code units; a complete scalar token may exceed it. */
export function* jsonChunks(value: JsonValue, chunkSize = 64 * 1024): Generator<string> {
  const stack: JsonFrame[] = []; const active = new Set<object>();
  let ancestor: Generator<string> | undefined;
  let item = value; let enter = true;
  function nextToken(): string | undefined {
    for (;;) {
      if (ancestor) {
        const token = ancestor.next();
        if (!token.done) return token.value;
        ancestor = undefined;
      }
      if (enter) {
        enter = false;
        if (item === null || typeof item !== 'object') return JSON.stringify(item) ?? 'null';
        // True cycles still reach the original recursion RangeError, while
        // dynamic getters retain their exact original read order and output.
        if (active.has(item)) { ancestor = ancestorTokens(item); continue; }
        active.add(item);
        if (Array.isArray(item)) { stack.push({ array: true, value: item, index: 0, state: 0 }); return '['; }
        stack.push({ array: false, value: item, keys: null, index: 0, state: 0, first: true }); return '{';
      }
      const frame = stack[stack.length - 1];
      if (!frame) return undefined;
      if (frame.array) {
        if (frame.state === 0) {
          if (!(frame.index < frame.value.length)) { stack.pop(); active.delete(frame.value); return ']'; }
          frame.state = 1;
          if (frame.index) return ',';
        }
        frame.state = 0; item = frame.value[frame.index++]!; enter = true;
      } else {
        if (frame.state === 0) {
          frame.keys ??= Object.keys(frame.value);
          while (frame.index < frame.keys.length && frame.value[frame.keys[frame.index]!] === undefined) frame.index++;
          if (frame.index === frame.keys.length) { stack.pop(); active.delete(frame.value); return '}'; }
          frame.state = 1;
          if (!frame.first) return ',';
          frame.first = false;
        }
        if (frame.state === 1) { frame.state = 2; return JSON.stringify(frame.keys![frame.index]!); }
        if (frame.state === 2) { frame.state = 3; return ':'; }
        frame.state = 0; item = frame.value[frame.keys![frame.index++]!]!; enter = true;
      }
    }
  }
  let pending = '';
  try {
    for (let fragment = nextToken(); fragment !== undefined; fragment = nextToken()) {
      if (pending && pending.length + fragment.length > chunkSize) { yield pending; pending = ''; }
      pending += fragment;
    }
    if (pending) yield pending;
  } finally { ancestor?.return(undefined); }
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

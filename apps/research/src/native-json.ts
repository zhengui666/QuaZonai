import { JSONParser } from '@streamparser/json';
import type { JsonObject } from '@earendil-works/pi-durable';

type JsonValue = JsonObject[string];

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

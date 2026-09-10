import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import { describe, expect, it } from 'vitest';
import { validateResponse } from './generated/responses.cjs';

// Compare every generated route/status/media validator with an independently
// compiled reference to its ORIGINAL Rust schema, not a hand-maintained DTO.
type NativeSchema = { $ref?: string; [keyword: string]: unknown };
type NativeResponse = { $ref?: string; content?: Record<string, { schema: NativeSchema }> };
type NativeOperation = { responses: Record<string, NativeResponse> };
const document = JSON.parse(readFileSync(new URL('../../../contracts/generated/api-v2.openapi.json', import.meta.url), 'utf8')) as {
  paths: Record<string, Partial<Record<string, NativeOperation>>>;
};
const root = 'urn:quazonai:test-native-response';
const ajv = new Ajv2020({ strict: false, inlineRefs: false });
addFormats(ajv);
ajv.addSchema(document, root);
const generated = createRequire(import.meta.url)('./generated/responses.cjs') as Record<string, unknown>;
const escapePointer = (value: string) => value.replaceAll('~', '~0').replaceAll('/', '~1');
const repeated = new Map<string, unknown>();
const entries: { path: string; method: string; status: number; media: string; pointer: string; name: string; pureRef?: string }[] = [];
let sequence = 0;
for (const [path, item] of Object.entries(document.paths) as [string, Record<string, any>][]) {
  for (const method of ['get', 'post', 'patch', 'put', 'delete', 'head', 'options']) {
    const operation = item[method];
    if (!operation) continue;
    for (const [status, response] of Object.entries(operation.responses) as [string, any][]) {
      if (!/^[2-5][0-9]{2}$/.test(status) || status === '204' || status === '205') continue;
      // The current Rust emitter uses inline Response Objects. Fail rather than
      // silently skipping coverage if that upstream shape changes.
      expect(response.$ref).toBeUndefined();
      for (const [media, content] of Object.entries(response.content ?? {}) as [string, any][]) {
        if (media !== 'application/json' && !/^application\/[a-z0-9.+-]+\+json$/.test(media)) continue;
        const pureRef = typeof content.schema.$ref === 'string' && Object.keys(content.schema).length === 1
          ? content.schema.$ref : undefined;
        entries.push({ path, method, status: Number(status), media,
          pointer: `#/paths/${escapePointer(path)}/${method}/responses/${status}/content/${escapePointer(media)}/schema`,
          name: `response${sequence++}`, pureRef });
      }
    }
  }
}

const id = '01990000-0000-7000-8000-000000000001';
const problem = {
  type: 'urn:quazonai:problem:validation-error', title: 'Validation failed',
  status: 422, code: 'VALIDATION_ERROR', detail: 'Check the request.', request_id: id,
  retryable: false, current_revision: null, field_errors: [], safe_next_actions: [],
};
const corpus: unknown[] = [
  undefined, null, false, true, -1, 0, 1, 1.25, NaN, Infinity,
  '', id, '0', '+000.0100', '1\n', [], {}, { schema_version: 1 },
  { schema_version: 1, items: [], next_cursor: null },
  { schema_version: 1, items: [], next_cursor: id },
  { schema_version: 1, items: [], next_cursor: 'not-a-uuid' },
  { schema_version: 1, replayed: false, resource: {} },
  problem, { ...problem, unexpected: true }, { ...problem, status: '422' },
  { ...problem, retryable: 'false' }, { ...problem, request_id: 'invalid' },
];

describe('native standalone response generation', () => {
  it('passes the native module syntax check that rejects duplicate function declarations', () => {
    const check = (source: string) => spawnSync(process.execPath, ['--input-type=module', '--check'], {
      input: source, encoding: 'utf8', env: {}, timeout: 15000, maxBuffer: 8192,
    });
    const original = readFileSync(new URL('./generated/responses.cjs', import.meta.url), 'utf8');
    const native = check(original);
    expect(native.error).toBeUndefined();
    expect(native.status, native.stderr).toBe(0);
    const duplicate = check('function nativeRegression() {}\nfunction nativeRegression() {}');
    expect(duplicate.error).toBeUndefined();
    expect(duplicate.status).not.toBe(0);
    expect(duplicate.stderr).toContain('already been declared');
  });

  it('preserves all original route, status and media validation decisions', () => {
    expect(entries.length).toBeGreaterThan(100);
    for (const entry of entries) {
      const native = ajv.compile({ $ref: root + entry.pointer });
      for (const value of corpus) {
        expect(validateResponse(entry.path, entry.method, entry.status, value, entry.media),
          `${entry.method} ${entry.path} ${entry.status} ${entry.media}`)
          .toBe(native(value));
      }
    }
  });

  it('reuses a single native function for repeated pure component references', () => {
    let reused = 0;
    for (const entry of entries) {
      expect(typeof generated[entry.name]).toBe('function');
      if (!entry.pureRef) continue;
      if (repeated.has(entry.pureRef)) {
        expect(generated[entry.name]).toBe(repeated.get(entry.pureRef));
        reused++;
      } else repeated.set(entry.pureRef, generated[entry.name]);
    }
    expect(reused).toBeGreaterThan(100);
  });
});

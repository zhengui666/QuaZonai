import { readFileSync } from 'node:fs';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import { describe, expect, it } from 'vitest';

const domain = JSON.parse(readFileSync(new URL('../../../contracts/generated/domain-v1.openapi.json', import.meta.url), 'utf8'));
const root = 'urn:quazonai:runtime-output-contract-boundaries';
const ajv = new Ajv2020({ strict: false, validateFormats: true });
addFormats(ajv);
ajv.addSchema(domain, root);
function accepts(name: string, value: unknown): boolean {
  const validate = ajv.getSchema(`${root}#/components/schemas/${name}`);
  if (!validate) throw new Error(`Missing native generated schema: ${name}`);
  return !!validate(value);
}
const id = '01991f2a-0000-7000-8000-000000000001';
const output = () => ({ kind: 'DATA_QUALITY', schema: { name: 'qz.data_quality', version: '1' }, storage_ref: id, storage_version: '1', byte_count: '128', media_type: 'application/json' });

describe('native immutable Runtime output contract', () => {
  it('requires exactly native output version one, not an arbitrary positive revision', () => {
    expect(accepts('RuntimeOutputV1', output())).toBe(true);
    for (const storage_version of ['0', '01', '2', '9223372036854775807', '1\n', '+1', '', 1, null]) {
      expect(accepts('RuntimeOutputV1', { ...output(), storage_version })).toBe(false);
    }
    const missing: Record<string, unknown> = output();
    delete missing.storage_version;
    expect(accepts('RuntimeOutputV1', missing)).toBe(false);
  });

  it('binds kind and media to the actual versioned output schema', () => {
    const registered = [
      ['qz.wasm_model', 'MODEL', 'application/wasm'],
      ['qz.model_compilation', 'REPORT', 'application/json'],
      ['qz.data_quality', 'DATA_QUALITY', 'application/json'],
      ['qz.native_forecast', 'REPORT', 'application/json'],
      ['qz.native_allocation', 'REPORT', 'application/json'],
      ['qz.native_simulation', 'REPORT', 'application/json'],
    ];
    for (const [name, kind, media_type] of registered) {
      const original = { ...output(), kind, media_type, schema: { name, version: '1' } };
      expect(accepts('RuntimeOutputV1', original)).toBe(true);
      for (const wrong of ['MODEL', 'SIGNALS', 'TARGETS', 'REPORT', 'METRICS', 'DATA_QUALITY']) {
        if (wrong !== kind) expect(accepts('RuntimeOutputV1', { ...original, kind: wrong })).toBe(false);
      }
      expect(accepts('RuntimeOutputV1', { ...original, schema: { name, version: '2' } })).toBe(false);
      expect(accepts('RuntimeOutputV1', { ...original, media_type: 'text/html' })).toBe(false);
      expect(accepts('RuntimeOutputV1', { ...original, schema: { name, version: '1', fallback: true } })).toBe(false);
    }
    expect(accepts('RuntimeOutputV1', { ...output(), schema: { name: 'qz.unregistered', version: '1' } })).toBe(false);
    expect(accepts('RuntimeOutputV1', { ...output(), qualified: true })).toBe(false);
  });

  it('uses exact canonical string byte bounds for every native output', () => {
    for (const byte_count of ['1', '9', '10', '67108863', '67108864']) {
      expect(accepts('RuntimeOutputV1', { ...output(), byte_count })).toBe(true);
    }
    for (const byte_count of ['0', '01', '67108865', '99999999', '9223372036854775807', '-1', '1.0', '1\n', 128, null]) {
      expect(accepts('RuntimeOutputV1', { ...output(), byte_count })).toBe(false);
    }
  });
});

describe('native requested job output limit', () => {
  const limits = { cpu: 1, cpu_seconds: '1', memory_mib: 64, wall_seconds: 60, output_bytes: '1' };
  it('matches the native one-byte through 64-MiB boundary without floating point', () => {
    for (const output_bytes of ['1', '67108863', '67108864']) {
      expect(accepts('RuntimeJobLimitsV1', { ...limits, output_bytes })).toBe(true);
    }
    for (const output_bytes of ['0', '67108865', '9223372036854775807', '01', '+1', '1\n', '1.0', 1, null]) {
      expect(accepts('RuntimeJobLimitsV1', { ...limits, output_bytes })).toBe(false);
    }
  });
  it('checks decimal prefix transitions and deterministic wide samples against native integer bounds', () => {
    const samples = new Set<bigint>();
    for (let power = 1n; power <= 1_000_000_000n; power *= 10n) {
      for (const delta of [-1n, 0n, 1n]) samples.add(power + delta);
    }
    let state = 17n;
    for (let index = 0; index < 1000; index++) {
      state = (state * 48271n) % 2147483647n;
      samples.add(state);
    }
    for (const value of samples) {
      expect(accepts('RuntimeJobLimitsV1', { ...limits, output_bytes: value.toString() }))
        .toBe(value >= 1n && value <= 67108864n);
    }
  });
});

import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { validateResponse } from '@quazonai/web/response-contract';
import { validateResponseAsync, validateResponseResultAsync } from '@quazonai/web/response-contract/lazy';
import { caseCounts, measured, observedIdentity } from './agent-evaluation-data';
import type { AgentReport } from './agent-evaluation-data';
const fixture = JSON.parse(readFileSync(new URL('../../../tests/fixtures/agent-evaluation/unrun-v1.json', import.meta.url), 'utf8')) as AgentReport;
const path = '/api/v2/artifacts/{id}/agent-evaluation';
describe('Agent evaluation report foundation', () => {
  it('validates the canonical protocol-only fixture and rejects unknown fields and lossy scalars', () => {
    expect(validateResponse(path, 'GET', 200, fixture, 'application/json')).toBe(true);
    expect(validateResponse(path, 'GET', 200, { ...fixture, qualification: 'PASS' }, 'application/json')).toBe(false);
    const lossy = structuredClone(fixture);
    Object.assign(lossy.cases[0]!.measurements, { input_tokens: 9007199254740992 });
    expect(validateResponse(path, 'GET', 200, lossy, 'application/json')).toBe(false);
    expect(validateResponse(path, 'GET', 200, { ...fixture, source_sha256: 'a'.repeat(64) + '\n' }, 'application/json')).toBe(false);
  });
  it('loads the same report contract asynchronously and keeps concurrent error snapshots independent', async () => {
    expect(await validateResponseAsync(path, 'GET', 200, fixture, 'application/json')).toBe(true);
    const extra = { ...fixture, qualification: 'PASS' };
    const invalidHash = { ...fixture, source_sha256: 'not-a-hash' };
    const expectedExtra = await validateResponseResultAsync(path, 'GET', 200, extra, 'application/json');
    const expectedHash = await validateResponseResultAsync(path, 'GET', 200, invalidHash, 'application/json');
    expect(expectedExtra.valid).toBe(false); expect(expectedHash.valid).toBe(false);
    expect(expectedExtra.errors).not.toEqual(expectedHash.errors);
    const actual = await Promise.all([extra, invalidHash, fixture].map(report =>
      validateResponseResultAsync(path, 'GET', 200, report, 'application/json')));
    expect(actual).toEqual([expectedExtra, expectedHash, { valid: true, errors: null }]);
    expect(validateResponse(path, 'GET', 200, invalidHash, 'application/json')).toBe(false);
    expect(actual[0]).toEqual(expectedExtra);
  });
  it('accepts native-normalized RFC3339 dates and rejects incompatible year/leap encodings', () => {
    for (const recorded_at of ['2026-09-30T00:00:00Z', '0000-01-01T00:00:00Z',
      '9999-12-31T23:59:59Z', '2026-09-30T23:59:60.999999999Z', '2026-09-29T23:59:60Z']) {
      expect(validateResponse(path, 'GET', 200, { ...fixture, recorded_at }, 'application/json')).toBe(true);
    }
    for (const recorded_at of ['2026-9-30T00:00:00Z', '+10000-01-01T00:00:00Z',
      '-0001-01-01T00:00:00Z', '2026-09-30T12:00:60Z', '2026-09-30T23:59:60+01:00']) {
      expect(validateResponse(path, 'GET', 200, { ...fixture, recorded_at }, 'application/json')).toBe(false);
    }
  });
  it('uses the native currency capability and rejects an incomplete cost tuple', () => {
    const report = structuredClone(fixture);
    report.cases[0]!.measurements.cost = { amount: '0.000000000000000001', currency: 'EUR' };
    report.cases[0]!.measurements.tool_calls = '9007199254740993';
    expect(validateResponse(path, 'GET', 200, report, 'application/json')).toBe(true);
    Object.assign(report.cases[0]!.measurements.cost, { currency: 'ZZZ' });
    expect(validateResponse(path, 'GET', 200, report, 'application/json')).toBe(false);
    Object.assign(report.cases[0]!.measurements, { cost: { amount: '0' } });
    expect(validateResponse(path, 'GET', 200, report, 'application/json')).toBe(false);
  });
  it('keeps all four outcomes distinct and never assumes an actual model', () => {
    expect(caseCounts(fixture.cases)).toEqual({ PASS: 0, FAIL: 0, BLOCKED: 0, UNRUN: 2 });
    expect(caseCounts(['PASS', 'FAIL', 'BLOCKED', 'UNRUN'].map(status => ({ ...fixture.cases[0]!, status } as AgentReport['cases'][number])))).toEqual({ PASS: 1, FAIL: 1, BLOCKED: 1, UNRUN: 1 });
    expect(observedIdentity(fixture.cases[0]!)).toContain('未知');
  });
  it('distinguishes measured zero from unknown and preserves decimal and counter precision', () => {
    expect(measured(null, 'USD')).toContain('未知');
    expect(measured(undefined, 'tokens')).toContain('未知');
    expect(measured('0', 'tokens')).toBe('0 tokens');
    expect(measured(null, 'calls')).toContain('未知');
    expect(measured('0', 'calls')).toBe('0 calls');
    expect(measured('9007199254740993', 'calls')).toBe('9007199254740993 calls');
    expect(measured('9007199254740993', 'tokens')).toBe('9007199254740993 tokens');
    expect(measured('0.000000000000000001', 'USD')).toBe('0.000000000000000001 USD');
  });
});

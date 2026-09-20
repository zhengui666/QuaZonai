import { describe, expect, it } from 'vitest';
import type { Schema } from './api';
import { checkedEquity, equityCoordinates, equityRange, equityTime, equityTimeText } from './equity-data';

const evaluation: Schema['EvaluationView'] = {
  id: 'e', project_id: 'p', subject_candidate_id: 'c', subject_alpha_version_id: null,
  input_set_id: 'i', policy_id: 'policy', run_id: 'r', evaluation_kind: 'PORTFOLIO',
  execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'REJECT',
  report_artifact_id: 'report', method_versions_artifact_id: 'report', origin: 'FIXTURE',
  concluded_at: '2026-01-02T00:00:00Z', valid_until: '2026-01-03T00:00:00Z',
  checked_at: '2026-01-04T00:00:00Z', unexpired_at_read: false,
};
function response(): Schema['EquityCurveV1'] {
  return { schema_version: 1, evaluation_id: 'e', project_id: 'p', candidate_id: 'c', run_id: 'r', origin: 'FIXTURE',
    curve: { status: 'READY', source_artifact_id: 'source', series: {
      native_version: '0.63.0', base_currency: 'USD', starting_capital: '1000',
      period_start_ns: '1767225600000000000', period_end_ns: '1767312000000000000',
      source_point_count: '3', window_point_count: '3', resolution: 'NATIVE', sampled: false,
      points: [
        { timestamp_ns: '1767225600000000000', value: '0', reason_code: null },
        { timestamp_ns: '1767225600000000001', value: '-0.000000000000000001', reason_code: null },
        { timestamp_ns: '1767312000000000000', value: null, reason_code: 'MISSING_OBSERVATION' },
      ],
    } } };
}

describe('native equity display boundaries', () => {
  it('keeps nanoseconds and raw decimal values independently of plotting precision', () => {
    const raw = '1767225600123456789';
    expect(equityTimeText(raw)).toBe('2026-01-01T00:00:00.123456789Z');
    expect(equityTime(raw)).toBeCloseTo(1767225600123.4568, 3);
    const point = { timestamp_ns: raw, value: '99999999999999999999.999999999999999999', reason_code: null };
    expect(Number.isFinite(equityCoordinates(point)[1])).toBe(true);
    expect(point.value).toBe('99999999999999999999.999999999999999999');
    expect(equityCoordinates({ ...point, value: '0' })[1]).toBe(0);
    expect(equityCoordinates({ ...point, value: '-1' })[1]).toBe(-1);
    expect(equityCoordinates({ ...point, value: null })[1]).toBeNull();
    expect(() => equityCoordinates({ ...point, value: 'Infinity' })).toThrow();
    expect(() => equityTime('9223372036854775808')).toThrow();
  });
  it('preserves the inclusive end millisecond and rejects invalid ranges', () => {
    expect(equityRange(1, 2)).toEqual({ start_ns: '1000000', end_ns: '2999999' });
    for (const [start, end] of [[-1, 2], [2, 1], [1.5, 2], [1, Infinity], [0, Number.MAX_SAFE_INTEGER]]) {
      expect(() => equityRange(start!, end!)).toThrow();
    }
  });
  it('does not confuse REJECT or expired qualification with missing historical data', () => {
    const data = response();
    expect(checkedEquity(data, evaluation)).toBe(data);
    for (const field of ['evaluation_id', 'project_id', 'candidate_id', 'run_id'] as const) {
      expect(() => checkedEquity({ ...data, [field]: 'other' }, evaluation)).toThrow('不一致');
    }
    expect(() => checkedEquity({ ...data, origin: 'REAL' }, evaluation)).toThrow();
  });
  it('preserves explicit gaps and rejects duplicate times, incorrect counts and fabricated zeroes', () => {
    const data = response();
    if (data.curve.status !== 'READY') throw new Error('fixture');
    const series = data.curve.series;
    series.points[2]!.reason_code = null;
    expect(() => checkedEquity(data, evaluation)).toThrow();
    series.points[2]!.reason_code = 'MISSING_OBSERVATION';
    series.points[1]!.timestamp_ns = series.points[0]!.timestamp_ns;
    expect(() => checkedEquity(data, evaluation)).toThrow();
    series.points[1]!.timestamp_ns = '1767225600000000001';
    series.sampled = true;
    expect(() => checkedEquity(data, evaluation)).toThrow();
    series.sampled = false;
    series.resolution = 'AUTO';
    expect(() => checkedEquity(data, evaluation)).toThrow();
  });
});

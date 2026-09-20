import { ApiFailure, isCounter } from './api';
import type { Schema } from './api';

export type EquitySeries = Schema['EquitySeriesV1'];
export type EquityPoint = Schema['EquityPointV1'];
export type EquityResolution = Schema['EquityResolution'];

export const equityResolutions: { value: EquityResolution; label: string }[] = [
  { value: 'AUTO', label: '自动' }, { value: 'NATIVE', label: '原始' },
  { value: 'DAY', label: '日末' }, { value: 'WEEK', label: '周末' }, { value: 'MONTH', label: '月末' },
];

export function equityTime(timestamp: string): number {
  if (!isCounter(timestamp)) throw new ApiFailure('EQUITY_TIME_INVALID', '权益时间无效。');
  const ns = BigInt(timestamp);
  return Number(ns / 1_000_000n) + Number(ns % 1_000_000n) / 1_000_000;
}

export function equityTimeText(timestamp: string): string {
  const ns = BigInt(timestamp);
  const seconds = new Date(Number(ns / 1_000_000n)).toISOString().slice(0, 19);
  return `${seconds}.${(ns % 1_000_000_000n).toString().padStart(9, '0')}Z`;
}

/** Number is a plotting coordinate only; never format money from this value. */
export function equityCoordinates(point: EquityPoint): [number, number | null] {
  const value = point.value === null ? null : Number(point.value);
  if (value !== null && !Number.isFinite(value)) throw new ApiFailure('EQUITY_VALUE_INVALID', '权益金额超出绘图范围。');
  return [equityTime(point.timestamp_ns), value];
}

export function checkedEquity(view: Schema['EquityCurveV1'], evaluation: Schema['EvaluationView']): Schema['EquityCurveV1'] {
  if (view.project_id !== evaluation.project_id || view.candidate_id !== evaluation.subject_candidate_id
      || view.evaluation_id !== evaluation.id || view.run_id !== evaluation.run_id || view.origin !== evaluation.origin) {
    throw new ApiFailure('EQUITY_IDENTITY_MISMATCH', '权益数据与当前回测不一致。');
  }
  if (view.curve.status === 'READY') {
    const series = view.curve.series;
    if (series.resolution === 'AUTO' || series.points.length > 10_000
        || BigInt(series.period_start_ns) > BigInt(series.period_end_ns)
        || BigInt(series.window_point_count) > BigInt(series.source_point_count)
        || BigInt(series.points.length) > BigInt(series.window_point_count)
        || series.sampled !== (BigInt(series.points.length) < BigInt(series.window_point_count))) {
      throw new ApiFailure('EQUITY_SERIES_INVALID', '权益序列不完整。');
    }
    series.points.forEach((point, index) => {
      if (BigInt(point.timestamp_ns) < BigInt(series.period_start_ns)
          || BigInt(point.timestamp_ns) > BigInt(series.period_end_ns)
          || (index > 0 && BigInt(point.timestamp_ns) <= BigInt(series.points[index - 1]!.timestamp_ns))
          || (point.value === null) !== (point.reason_code !== null)) {
        throw new ApiFailure('EQUITY_SERIES_INVALID', '权益观测顺序或缺值状态无效。');
      }
      equityCoordinates(point);
    });
  }
  return view;
}

/** Convert the user's millisecond range without discarding the end millisecond. */
export function equityRange(start: number, end: number): { start_ns: string; end_ns: string } {
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || start > end) {
    throw new ApiFailure('EQUITY_RANGE_INVALID', '请选择有效时间范围。');
  }
  const startNs = (BigInt(start) * 1_000_000n).toString();
  const endNs = (BigInt(end) * 1_000_000n + 999_999n).toString();
  if (!isCounter(startNs) || !isCounter(endNs)) throw new ApiFailure('EQUITY_RANGE_INVALID', '时间超出可用范围。');
  return { start_ns: startNs, end_ns: endNs };
}

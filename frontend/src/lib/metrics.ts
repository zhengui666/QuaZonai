export interface TimeValuePoint { time: string | number; value: number; }
export interface NamedValue { name: string; value: number; }
export interface CalibrationPoint { predicted: number; observed: number; }
export interface MatrixMetric { labels: string[]; values: number[][]; }

type MetricRecord = Record<string, unknown>;

function isRecord(value: unknown): value is MetricRecord {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value);
}

export function metricValue(record: MetricRecord | undefined, keys: string[]): unknown {
  if (!record) return undefined;
  for (const key of keys) {
    const parts = key.split('.');
    let current: unknown = record;
    for (const part of parts) {
      if (!isRecord(current) || !(part in current)) { current = undefined; break; }
      current = current[part];
    }
    if (current !== undefined && current !== null) return current;
  }
  return undefined;
}

export function asTimeSeries(value: unknown): TimeValuePoint[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (Array.isArray(item) && item.length >= 2 && (typeof item[0] === 'string' || typeof item[0] === 'number') && typeof item[1] === 'number') return [{ time: item[0], value: item[1] }];
    if (!isRecord(item)) return [];
    const time = item.time ?? item.date ?? item.timestamp ?? item.as_of;
    const numeric = item.value ?? item.equity ?? item.drawdown ?? item.score ?? item.ic ?? item.return;
    return (typeof time === 'string' || typeof time === 'number') && typeof numeric === 'number' ? [{ time, value: numeric }] : [];
  });
}

export function findTimeSeries(record: MetricRecord | undefined, keys: string[]): TimeValuePoint[] {
  return asTimeSeries(metricValue(record, keys));
}

export function asNamedValues(value: unknown): NamedValue[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => {
      if (!isRecord(item)) return [];
      const name = item.name ?? item.feature ?? item.factor ?? item.universe ?? item.alpha ?? item.label;
      const numeric = item.value ?? item.importance ?? item.weight ?? item.exposure ?? item.contribution;
      return typeof name === 'string' && typeof numeric === 'number' ? [{ name, value: numeric }] : [];
    });
  }
  if (isRecord(value)) return Object.entries(value).filter((entry): entry is [string, number] => typeof entry[1] === 'number').map(([name, numeric]) => ({ name, value: numeric }));
  return [];
}

export function findNamedValues(record: MetricRecord | undefined, keys: string[]): NamedValue[] {
  return asNamedValues(metricValue(record, keys));
}

export function asCalibration(value: unknown): CalibrationPoint[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!isRecord(item)) return [];
    const predicted = item.predicted ?? item.expected ?? item.forecast ?? item.bin_center;
    const observed = item.observed ?? item.realized ?? item.actual;
    return typeof predicted === 'number' && typeof observed === 'number' ? [{ predicted, observed }] : [];
  });
}

export function findCalibration(record: MetricRecord | undefined, keys: string[]): CalibrationPoint[] {
  return asCalibration(metricValue(record, keys));
}

export function asMatrix(value: unknown): MatrixMetric | null {
  if (!isRecord(value) || !Array.isArray(value.labels) || !Array.isArray(value.values)) return null;
  const labels = value.labels.filter((item): item is string => typeof item === 'string');
  const values = value.values.filter(Array.isArray).map((row) => row.filter((item): item is number => typeof item === 'number'));
  return labels.length && values.length === labels.length && values.every((row) => row.length === labels.length) ? { labels, values } : null;
}

export function findMatrix(record: MetricRecord | undefined, keys: string[]): MatrixMetric | null {
  return asMatrix(metricValue(record, keys));
}

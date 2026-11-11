import type { Metric } from '../../../packages/contracts/src/index.ts';

/** Query state selects existing results only; it never starts another scientific run. */
export function scanComparisonOrdinals(value: string | null): string[] {
  const selected: string[] = [];
  for (const part of value?.split(',') ?? []) {
    if (!/^(0|[1-9]\d*)$/.test(part)) continue;
    if (!selected.includes(part)) selected.push(part);
    if (selected.length === 5) break;
  }
  return selected;
}

export function scanFixedParameters(value: string | null): Record<string, string> | undefined {
  if (!value) return {};
  try {
    const parsed: unknown = JSON.parse(value);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed) || Object.values(parsed).some(item => typeof item !== 'string')) return;
    return parsed as Record<string, string>;
  } catch { return; }
}

/** Scanning displays its declared selection partition, never heldout-first summaries. */
export function scanValidationMetrics(metrics: readonly Metric[]): Metric[] {
  return metrics.filter(metric => {
    const prefix = /^(TRAIN|VALIDATION|HELDOUT|TEST)\./.exec(metric.key)?.[1];
    if (metric.partition !== undefined && metric.partition !== 'VALIDATION' || prefix !== undefined && prefix !== 'VALIDATION') return false;
    return metric.partition === 'VALIDATION' || prefix === 'VALIDATION';
  });
}

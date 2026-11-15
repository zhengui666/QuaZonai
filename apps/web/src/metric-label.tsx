import { factMetricKey, metricDefinition } from './metric-glossary';

export function MetricLabel({ metricKey, label }: { metricKey: string; label?: string }) {
  return <span>{label ?? metricDefinition(metricKey).label}</span>;
}
export function FactLabel({ label }: { label: string }) {
  const key = factMetricKey(label);
  return key ? <MetricLabel metricKey={key} label={['输入 Token', '输出 Token', '费用'].includes(label) ? undefined : label} /> : <>{label}</>;
}

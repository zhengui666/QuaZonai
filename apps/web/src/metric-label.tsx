import { useTranslation } from 'react-i18next';
import { factDisplayLabel, factMetricKey, metricDefinition } from './metric-glossary';

export function MetricLabel({ metricKey, label }: { metricKey: string; label?: string }) {
  useTranslation('shared');
  return <span>{label ?? metricDefinition(metricKey).label}</span>;
}
export function FactLabel({ label }: { label: string }) {
  useTranslation('shared');
  const key = factMetricKey(label);
  return key ? <MetricLabel metricKey={key} label={factDisplayLabel(label)} /> : <>{label}</>;
}

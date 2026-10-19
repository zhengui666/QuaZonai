import { Tooltip } from 'antd';
import { InfoIcon } from '@phosphor-icons/react';
import { useState } from 'react';
import { factMetricKey, metricDefinition } from './metric-glossary';

export function MetricLabel({ metricKey, label }: { metricKey: string; label?: string }) {
  const metric = metricDefinition(metricKey); const [open, setOpen] = useState(false);
  const name = label ?? metric.label;
  return <span className="explained-metric"><span>{name}</span><Tooltip open={open} onOpenChange={setOpen} trigger={['hover', 'focus', 'click']} destroyOnHidden classNames={{ root: 'metric-tooltip' }} title={<span className="metric-explanation"><strong>{name}</strong><span>{metric.explanation}</span></span>}><button type="button" className="metric-info" aria-label={`解释：${name}`} onKeyDown={event => { if (event.key === 'Escape') { setOpen(false); event.stopPropagation(); } }}><InfoIcon size={15} aria-hidden /></button></Tooltip></span>;
}
export function FactLabel({ label }: { label: string }) {
  const key = factMetricKey(label);
  return key ? <MetricLabel metricKey={key} label={['输入 Token', '输出 Token', '费用'].includes(label) ? undefined : label} /> : <>{label}</>;
}

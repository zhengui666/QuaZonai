import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';

export interface KpiItem { label: string; value: ReactNode; note?: ReactNode }
const localizedStaticValues = new Set(['YES', 'NO', 'READY', 'NOT READY', 'RUNNING', 'CURRENT', 'IDLE']);

export function KpiStrip({ items }: { items: KpiItem[] }) {
  const { text } = useI18n();
  return <div className="qz-kpi-strip">{items.map((item) => {
    const value = typeof item.value === 'string' && localizedStaticValues.has(item.value) ? text(item.value) : item.value;
    return <div className="qz-kpi" key={item.label}><div className="qz-kpi-label">{text(item.label)}</div><div className="qz-kpi-value qz-number">{value}</div>{item.note ? <div className="qz-kpi-note">{item.note}</div> : null}</div>;
  })}</div>;
}

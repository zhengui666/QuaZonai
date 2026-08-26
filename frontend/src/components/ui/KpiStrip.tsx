import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';

export interface KpiItem { label: string; value: ReactNode; note?: ReactNode }
const localizedStaticValues = new Set(['YES', 'NO', 'READY', 'NOT READY', 'RUNNING', 'CURRENT', 'IDLE']);

export function KpiStrip({ items }: { items: KpiItem[] }) {
  const { locale, text } = useI18n();
  return <div className="qz-kpi-strip">{items.map((item) => {
    const value = typeof item.value === 'number'
      ? new Intl.NumberFormat(locale).format(item.value)
      : typeof item.value === 'string' && localizedStaticValues.has(item.value)
        ? text(item.value)
        : item.value;
    return <div className="qz-kpi" key={item.label}><div className="qz-kpi-label">{text(item.label)}</div><div className="qz-kpi-value qz-number" dir="auto">{value}</div>{item.note ? <div className="qz-kpi-note" dir="auto">{item.note}</div> : null}</div>;
  })}</div>;
}

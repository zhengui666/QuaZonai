import type { ReactNode } from 'react';
import { useI18n } from '../../i18n';

export function Section({ title, meta, actions, children, className = '' }: { title: string; meta?: ReactNode; actions?: ReactNode; children: ReactNode; className?: string }) {
  const { text } = useI18n();
  const localizedMeta = typeof meta === 'string' ? text(meta) : meta;
  return <section className={`qz-section ${className}`}><div className="qz-section-header"><div><h2 className="qz-section-title">{text(title)}</h2>{localizedMeta ? <div className="qz-section-meta">{localizedMeta}</div> : null}</div>{actions}</div>{children}</section>;
}

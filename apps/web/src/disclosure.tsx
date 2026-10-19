import type { ReactNode } from 'react';
import { shortTitle, shouldCollapse } from './titles';

export function Disclosure({ title, children, className = '' }: { title: string; children: ReactNode; className?: string }) {
  return <details className={`provenance-disclosure ${className}`}><summary>{title}</summary>{children}</details>;
}
export function TextDisclosure({ label, value, collapsed = false }: { label: string; value?: string | null; collapsed?: boolean }) {
  if (!value) return null;
  return collapsed || shouldCollapse(value) ? <Disclosure title={label} className="text-disclosure"><p>{value}</p></Disclosure> : <p className="record-description">{value}</p>;
}
export function RecordTitle({ name, text, fallback = '未命名', as: Heading = 'h1' }: { name?: string | null; text?: string | null; fallback?: string; as?: 'h1' | 'h2' | 'h3' }) {
  const original = name?.trim() ? name : text; const title = shortTitle(name, text, fallback);
  return <><Heading>{title}</Heading>{original && original !== title && <TextDisclosure label="完整名称与描述" value={original} collapsed />}</>;
}

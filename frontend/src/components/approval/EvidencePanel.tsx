import { Table } from '@radix-ui/themes';
import { Fragment, type ReactNode } from 'react';
import { useI18n, type Locale } from '../../i18n';
import type { ApprovalSnapshot } from '../../lib/api/types';
import { humanizeIdentifier } from '../../lib/format';

function entries(value?: Record<string, unknown>) { return value ? Object.entries(value).filter(([, item]) => item !== null && item !== undefined).slice(0, 12) : []; }
export function formatEvidenceValue(locale: Locale, value: unknown): string {
  if (typeof value === 'number') {
    return Math.abs(value) <= 2
      ? new Intl.NumberFormat(locale, { style: 'percent', maximumSignificantDigits: 15 }).format(value)
      : new Intl.NumberFormat(locale, { notation: 'compact', maximumFractionDigits: 2 }).format(value);
  }
  if (Array.isArray(value)) return value.map((item) => formatEvidenceValue(locale, item)).join(', ');
  if (typeof value === 'object' && value) {
    return `{ ${Object.entries(value)
      .map(([key, item]) => `${key}: ${formatEvidenceValue(locale, item)}`)
      .join(', ')} }`;
  }
  return String(value ?? '—');
}

function renderEvidenceValue(locale: Locale, value: unknown): ReactNode {
  if (Array.isArray(value)) {
    return value.map((item, index) => (
      <Fragment key={index}>
        {index ? ', ' : null}
        {renderEvidenceValue(locale, item)}
      </Fragment>
    ));
  }
  if (typeof value === 'object' && value) {
    return <>
      {'{ '}
      {Object.entries(value as Record<string, unknown>).map(([key, item], index) => (
        <Fragment key={`${key}-${index}`}>
          {index ? ', ' : null}
          <bdi dir="ltr">{key}</bdi>: {renderEvidenceValue(locale, item)}
        </Fragment>
      ))}
      {' }'}
    </>;
  }
  return <bdi dir="auto">{formatEvidenceValue(locale, value)}</bdi>;
}

export function EvidencePanel({ approval }: { approval: ApprovalSnapshot }) {
  const { locale, t } = useI18n();
  const groups: Array<[string, Record<string, unknown> | undefined]> = [
    [t('alpha.evidence'), approval.evidence_summary],
    [t('evidence.risk'), approval.risk_summary],
    [t('evidence.cost'), approval.cost_summary],
    [t('evidence.capacity'), approval.capacity_summary],
    [t('evidence.changes'), approval.changes_summary],
  ];
  const combined = groups.flatMap(([label, values]) => entries(values).map(([key, value]) => [label, key, value] as const));
  return (
    <div className="qz-panel">
      <Table.Root size="1">
        <Table.Body>
          {combined.length ? combined.map(([label, key, value]) => (
            <Table.Row key={`${label}:${key}`}>
              <Table.RowHeaderCell style={{ color: 'var(--qz-text-muted)', fontSize: 11 }}><span dir="auto">{label}</span> · <bdi dir="ltr">{humanizeIdentifier(key)}</bdi></Table.RowHeaderCell>
              <Table.Cell className="qz-number" dir="auto" style={{ textAlign: 'right', fontSize: 11 }}>{renderEvidenceValue(locale, value)}</Table.Cell>
            </Table.Row>
          )) : <Table.Row><Table.Cell style={{ color: 'var(--qz-text-faint)', fontSize: 11 }}>{t('evidence.empty')}</Table.Cell></Table.Row>}
        </Table.Body>
      </Table.Root>
    </div>
  );
}

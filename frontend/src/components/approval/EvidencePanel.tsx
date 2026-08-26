import { Table } from '@radix-ui/themes';
import { useI18n } from '../../i18n';
import type { ApprovalSnapshot } from '../../lib/api/types';
import { formatCompactNumber, formatPercent, humanizeIdentifier } from '../../lib/format';

function entries(value?: Record<string, unknown>) { return value ? Object.entries(value).filter(([, item]) => item !== null && item !== undefined).slice(0, 12) : []; }
function renderValue(value: unknown) {
  if (typeof value === 'number') return Math.abs(value) <= 2 ? formatPercent(value, 2) : formatCompactNumber(value);
  if (Array.isArray(value)) return value.join(', ');
  if (typeof value === 'object' && value) return JSON.stringify(value);
  return String(value ?? '—');
}

export function EvidencePanel({ approval }: { approval: ApprovalSnapshot }) {
  const { t } = useI18n();
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
              <Table.Cell className="qz-number" dir="auto" style={{ textAlign: 'right', fontSize: 11 }}>{renderValue(value)}</Table.Cell>
            </Table.Row>
          )) : <Table.Row><Table.Cell style={{ color: 'var(--qz-text-faint)', fontSize: 11 }}>{t('evidence.empty')}</Table.Cell></Table.Row>}
        </Table.Body>
      </Table.Root>
    </div>
  );
}

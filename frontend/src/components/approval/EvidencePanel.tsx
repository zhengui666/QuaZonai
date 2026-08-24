import { Table } from '@radix-ui/themes';
import type { ApprovalSnapshot } from '../../lib/api/types';
import { formatCompactNumber, formatPercent } from '../../lib/format';

function entries(value?: Record<string, unknown>) { return value ? Object.entries(value).filter(([, item]) => item !== null && item !== undefined).slice(0, 12) : []; }
function renderValue(value: unknown) {
  if (typeof value === 'number') return Math.abs(value) <= 2 ? formatPercent(value, 2) : formatCompactNumber(value);
  if (Array.isArray(value)) return value.join(', ');
  if (typeof value === 'object' && value) return JSON.stringify(value);
  return String(value ?? '—');
}

export function EvidencePanel({ approval }: { approval: ApprovalSnapshot }) {
  const groups: Array<[string, Record<string, unknown> | undefined]> = [
    ['Evidence', approval.evidence_summary],
    ['Risk', approval.risk_summary],
    ['Cost', approval.cost_summary],
    ['Capacity', approval.capacity_summary],
    ['Changes', approval.changes_summary],
  ];
  const combined = groups.flatMap(([label, values]) => entries(values).map(([key, value]) => [`${label} · ${key}`, value] as const));
  return (
    <div className="qz-panel">
      <Table.Root size="1">
        <Table.Body>
          {combined.length ? combined.map(([key, value]) => (
            <Table.Row key={key}>
              <Table.RowHeaderCell style={{ color: 'var(--qz-text-muted)', fontSize: 11 }}>{key.replaceAll('_', ' ')}</Table.RowHeaderCell>
              <Table.Cell className="qz-number" style={{ textAlign: 'right', fontSize: 11 }}>{renderValue(value)}</Table.Cell>
            </Table.Row>
          )) : <Table.Row><Table.Cell style={{ color: 'var(--qz-text-faint)', fontSize: 11 }}>No structured Level 2 metrics returned by the API.</Table.Cell></Table.Row>}
        </Table.Body>
      </Table.Root>
    </div>
  );
}

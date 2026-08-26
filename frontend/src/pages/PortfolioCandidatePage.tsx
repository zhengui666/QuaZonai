import type { ColumnDef } from '@tanstack/react-table';
import { useParams } from 'react-router-dom';
import { RedundancyGraph, type RedundancyEdge } from '../components/graphs/RedundancyGraph';
import { DataTable } from '../components/ui/DataTable';
import { EmptyState } from '../components/ui/EmptyState';
import { ErrorPanel } from '../components/ui/ErrorPanel';
import { KpiStrip } from '../components/ui/KpiStrip';
import { PageHeader } from '../components/ui/PageHeader';
import { PageSkeleton } from '../components/ui/Skeleton';
import { StateBadge } from '../components/ui/StateBadge';
import { Section } from '../components/ui/Section';
import { useI18n } from '../i18n';
import { useCandidate } from '../lib/api/hooks';
import type { CandidateMember } from '../lib/api/types';
import { formatPercent, humanize, readMetric } from '../lib/format';

const cols: ColumnDef<CandidateMember, unknown>[] = [
  { accessorKey: 'alpha_name', header: 'Alpha', meta: { messageKey: 'alpha.name' }, cell: ({ row }) => row.original.alpha_name ?? row.original.alpha_qualification_id.slice(0, 10) },
  { accessorKey: 'role', header: 'Role', meta: { localizedSort: true }, cell: ({ getValue }) => humanize(String(getValue())) },
  { accessorKey: 'universe', header: 'Universe' },
  { accessorKey: 'target_weight', header: 'Target weight', meta: { searchFormat: 'percent', searchDecimals: 1 }, cell: ({ getValue }) => <span className="qz-number">{formatPercent(getValue() as number | null)}</span> },
  { accessorKey: 'target_contribution', header: 'Contribution', meta: { searchFormat: 'percent', searchDecimals: 1 }, cell: ({ getValue }) => <span className="qz-number">{formatPercent(getValue() as number | null)}</span> },
];

export function PortfolioCandidatePage() {
  const { t } = useI18n();
  const { id } = useParams();
  const q = useCandidate(id);
  if (q.isLoading) return <PageSkeleton />;
  if (q.error) return <ErrorPanel error={q.error} />;
  if (!q.data) return <EmptyState title="Candidate not found" description="The immutable candidate could not be loaded." />;
  const c = q.data;
  const quality = readMetric(c.metrics, ['search_adjusted_quality', 'quality']);
  const searchAdjustedQuality = typeof quality === 'string' && Number.isFinite(Number(quality)) ? Number(quality) : quality ?? '—';
  return <><PageHeader title={t('candidate.title', { id: c.id.slice(0, 8) })} description="Immutable assembly snapshot. Any change to members, policy, risk, cost, capacity, constraints or rebalance semantics creates a new Candidate." /><KpiStrip items={[{ label: 'State', value: <StateBadge state={c.state} /> }, { label: 'Mandate', value: c.mandate_name ?? c.mandate_version_id?.slice(0, 8) ?? '—' }, { label: 'Policy', value: c.policy_version ?? '—' }, { label: 'Search-adjusted quality', value: searchAdjustedQuality }]} /><Section title="Constituent Alpha qualifications" meta="Read-only role and weight map"><DataTable data={c.members ?? []} columns={cols} emptyTitle="No constituent details" emptyDescription="The API did not return member-level details for this Candidate." /></Section><Section title="Redundancy & common-source map" meta="React Flow · correlations, lineage and dependency overlap"><RedundancyGraph members={c.members ?? []} edges={c.metrics?.redundancy_edges as unknown as RedundancyEdge[] | undefined} /></Section><Section title="Frozen model contract"><div className="qz-panel qz-panel-pad qz-grid-3"><div><div className="qz-label">{t('candidate.riskModel')}</div><div className="qz-list-subtitle">{c.risk_model_version ?? '—'}</div></div><div><div className="qz-label">{t('candidate.costModel')}</div><div className="qz-list-subtitle">{c.cost_model_version ?? '—'}</div></div><div><div className="qz-label">{t('candidate.capacityModel')}</div><div className="qz-list-subtitle">{c.capacity_model_version ?? '—'}</div></div><div><div className="qz-label">{t('candidate.constraints')}</div><div className="qz-list-subtitle">{c.constraint_set_version ?? '—'}</div></div><div><div className="qz-label">{t('candidate.rebalance')}</div><div className="qz-list-subtitle">{c.rebalance_policy_version ?? '—'}</div></div><div><div className="qz-label">{t('candidate.evaluation')}</div><div className="qz-list-subtitle qz-mono">{c.evaluation_episode_id ?? '—'}</div></div></div></Section></>;
}
